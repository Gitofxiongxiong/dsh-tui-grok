/**
 * Harness 0.1.2-alpha.1 -> native TUI v1 compatibility boundary.
 *
 * The native pager owns a stable JSON-RPC method and frame vocabulary. New
 * Harness controllers remain ordinary in-process services here; no HTTP or
 * browser Remote carrier is mounted for the stdio surface.
 *
 * @module @dsh-pager-grok/tui-server/bridge
 */

import { randomUUID } from 'node:crypto'
import { homedir } from 'node:os'
import { decodeStorageRecord } from '@deepseek-ai/dsh-session/chunk-rows'
import type {
  ApiError,
  ApiResult,
  HostFrame,
  MuxFrame,
  SessionId,
  ToolEventView,
  WorkspaceView,
} from '@dsh-pager-grok/tui-protocol'
import type { TuiBackend, TuiBackendInfo, TuiMuxEnvelope } from './backend.js'

const HARNESS_VERSION = '0.1.2-alpha.1'
const DEFAULT_MAX_MESSAGES = 50
const MESSAGE_TYPES = new Set(['user/message', 'assistant/message'])

type RecordLike = Record<string, unknown>

interface SessionControllerLike {
  list(request: RecordLike, signal: AbortSignal): Promise<{ items: RecordLike[] }>
  search(request: RecordLike, signal: AbortSignal): Promise<unknown>
  create(request: RecordLike): Promise<unknown>
  selectModel(request: RecordLike): Promise<unknown>
  modelCatalog(): Promise<RecordLike>
  canOpenWorkspacePath(): boolean
  openWorkspacePath(request: RecordLike, signal: AbortSignal): Promise<unknown>
  rename(request: RecordLike): Promise<unknown>
  fork(request: RecordLike): Promise<unknown>
  prompt(request: RecordLike, signal: AbortSignal): Promise<unknown>
  attachment(request: RecordLike): Promise<unknown>
  updateQueue(request: RecordLike): unknown
  cancel(request: RecordLike): unknown
  inspect(sessionId: SessionId, signal?: AbortSignal): Promise<{ meta: RecordLike; events: RecordLike[] }>
  page(request: RecordLike, signal: AbortSignal): Promise<HistoryPage>
  follow(request: RecordLike, signal: AbortSignal): AsyncIterable<FollowFrame>
  control(signal: AbortSignal): AsyncIterable<ControlFrame>
  resolveAgent(sessionId: SessionId): Promise<{ agent: AgentLike } | { error: ApiError }>
}

interface WorkspaceControllerLike {
  create(request: RecordLike): Promise<unknown>
  rename(request: RecordLike): Promise<unknown>
  delete(request: RecordLike): Promise<unknown>
  insertBefore(request: RecordLike): Promise<unknown>
  insertSessionBefore(request: RecordLike): Promise<unknown>
  archiveSession(request: RecordLike): Promise<unknown>
  follow(signal: AbortSignal): AsyncIterable<WorkspaceFollowFrame>
}

interface DirectoryPickerControllerLike {
  pick(signal: AbortSignal): Promise<string | null>
  list(path: string | undefined, signal: AbortSignal): Promise<unknown>
  createDirectory(path: string, name: string): Promise<string>
}

interface SettingsControllerLike {
  describe(): unknown
  canOpenAgentPresetDirectory(): boolean
  update(ns: string, patch: RecordLike, expectedRevision: number | undefined): Promise<unknown>
  replace(ns: string, section: RecordLike, expectedRevision: number | undefined): Promise<unknown>
  mutate(ns: string, ops: unknown[], expectedRevision: number | undefined): Promise<unknown>
  openSettingsDocument(signal: AbortSignal): Promise<unknown>
  openAgentPresetDirectory(agentPreset: string, signal: AbortSignal): Promise<unknown>
}

interface CredentialsControllerLike {
  describe(refs: string[]): Promise<Record<string, unknown>>
  set(ref: string, value: string): Promise<void>
  unset(ref: string): Promise<void>
}

interface AgentPresetServiceLike {
  remoteExportList(): Promise<RecordLike>
  readDocument(agentPreset: string): Promise<unknown>
  remoteExportCopy(from: string, id: string, name?: string): Promise<void>
  remoteExportDelete(id: string): Promise<void>
  select(agent: AgentLike, agentPreset: string): Promise<string>
  serviceFor?(agent: AgentLike, name: string): unknown
}

interface GoalServiceLike {
  remoteExportCreate(agent: AgentLike, request: RecordLike): unknown
  edit(agent: AgentLike, ref: unknown, request: RecordLike): RecordLike
  pause(agent: AgentLike, ref: unknown): RecordLike
  resume(agent: AgentLike, ref: unknown): RecordLike
  complete(agent: AgentLike, ref: unknown): RecordLike
  clear(agent: AgentLike, ref: unknown): unknown
}

interface LlmServiceLike {
  listProviders(): RecordLike[]
  listConfigurableProviders(): RecordLike[]
  remoteDiscoverModels(settingsNs: string, request: RecordLike, signal: AbortSignal): Promise<unknown[]>
}

interface SubagentServiceLike {
  remoteExportList(parentSessionId: SessionId, signal: AbortSignal): Promise<unknown>
  prompt(request: RecordLike, signal: AbortSignal): Promise<unknown>
  interruptByParent(childSessionId: SessionId, parentSessionId: SessionId, mode: 'continuable'): unknown
}

interface CommandsServiceLike {
  list(agent: AgentLike): readonly unknown[]
  execute(agent: AgentLike, line: string, images: readonly unknown[], signal: AbortSignal): Promise<unknown>
}

interface SessionFileReferencesLike {
  list(agent: AgentLike, query: string, signal: AbortSignal): Promise<unknown[]>
}

interface SessionSkillCatalogLike {
  list(request: RecordLike, signal: AbortSignal): Promise<unknown>
}

interface AgentLike {
  id: SessionId
  session?: { events?: readonly RecordLike[] }
}

interface AgentsLike {
  get(sessionId: SessionId): AgentLike | undefined
  list?(): readonly AgentLike[]
  roots?(): readonly AgentLike[]
}

interface ToolsLike {
  get(name: string, scope?: unknown): {
    presentCall?: (args: unknown) => unknown
    presentResult?: (args: unknown, result: RecordLike) => unknown
  } | undefined
}

/** Structural Host context kept deliberately independent of generated Remote types. */
export interface TuiHarnessContext {
  sessionController: SessionControllerLike
  workspaceController: WorkspaceControllerLike
  directoryPickerController: DirectoryPickerControllerLike
  settingsController: SettingsControllerLike
  credentialsController: CredentialsControllerLike
  agentPresets?: AgentPresetServiceLike
  goals?: GoalServiceLike
  llm: LlmServiceLike
  subagents: SubagentServiceLike
  commands: CommandsServiceLike
  sessionFileReferences?: SessionFileReferencesLike
  sessionSkillCatalog?: SessionSkillCatalogLike
  agents: AgentsLike
  tools?: ToolsLike
  get?(name: string): unknown
  on(event: string, listener: (...args: any[]) => unknown): () => void
}

interface HistoryRecord {
  type: 'event' | 'chunks'
  event: RecordLike
}

interface HistoryPage {
  records: readonly HistoryRecord[]
  hasMore: boolean
}

type FollowFrame =
  | {
    type: 'snapshot'
    header: RecordLike
    cursor: number
    records: readonly HistoryRecord[]
    hasMore: boolean
    projections: RecordLike
  }
  | { type: 'event'; event: RecordLike }

type ControlFrame =
  | { type: 'baseline'; value: RecordLike }
  | { type: 'queue'; sessionId: SessionId; items: unknown[] }
  | { type: 'jobs'; sessionId: SessionId; jobs: unknown[] }
  | { type: 'projection'; sessionId: SessionId; key: string; value: unknown; seq: number }

type WorkspaceFollowFrame =
  | { type: 'baseline'; value: { items: WorkspaceView[]; archivedSessionIds: SessionId[] } }
  | { type: 'upsert'; workspace: WorkspaceView }
  | { type: 'remove'; workspaceId: string }
  | { type: 'order'; workspaceIds: string[] }
  | { type: 'archived'; archivedSessionIds: SessionId[] }

/** Compatibility alias retained for existing direct bridge consumers. */
export type BridgeMuxEnvelope = TuiMuxEnvelope

interface OpeningSnapshot {
  header: RecordLike
  cursor: number
  records: readonly HistoryRecord[]
  hasMore: boolean
  projections: RecordLike
}

interface FollowerState {
  readonly opening: Promise<OpeningSnapshot>
  readonly resolve: (snapshot: OpeningSnapshot) => void
  readonly reject: (error: unknown) => void
  snapshot?: OpeningSnapshot
}

type PendingInteraction =
  | {
    kind: 'approval'
    id: string
    sessionId: SessionId
    approvalId: string
    request: RecordLike
    resolve: (value: unknown) => void
    reject: (error: unknown) => void
    next: () => unknown
    disposeAbort?: () => void
  }
  | {
    kind: 'question'
    id: string
    sessionId: SessionId
    request: RecordLike
    resolve: (value: unknown) => void
    reject: (error: unknown) => void
    next: () => unknown
    disposeAbort?: () => void
  }

/**
 * Direct in-process adapter for Harness 0.1.2-alpha.1 controllers.
 */
export class TuiHarnessBridge implements TuiBackend {
  readonly info: TuiBackendInfo = {
    adapterFamily: 'controllers-v2',
    dshVersion: HARNESS_VERSION,
    profileSchema: 2,
    capabilities: {
      sessions: true,
      workspaces: true,
      settings: true,
      credentials: true,
      agentPresets: true,
      goals: true,
      subagents: true,
      approvals: true,
      questions: true,
      queue: true,
      jobs: true,
      skills: true,
      fileReferences: true,
      directoryPicker: true,
    },
  }

  private readonly attached = new Set<string>()
  private readonly followers = new Map<string, FollowerState>()
  private readonly subagentOpenings = new Map<string, OpeningSnapshot>()
  private readonly muxSubscribers = new Set<AsyncQueue<BridgeMuxEnvelope>>()
  private readonly hostSubscribers = new Set<AsyncQueue<HostFrame>>()
  private readonly pending = new Map<string, PendingInteraction>()
  private readonly disposers: Array<() => void> = []
  private workspaceBaselineValue: { items: WorkspaceView[]; archivedSessionIds: SessionId[] } | undefined
  private pushSequence = 0
  private disposed = false

  constructor(private readonly ctx: TuiHarnessContext) {
    this.installHostEvents()
    this.installInteractionAnswerers()
  }

  /** Release event listeners and delegate unresolved waterfalls. */
  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    for (const dispose of this.disposers.splice(0)) dispose()
    for (const queue of this.muxSubscribers) queue.close()
    for (const queue of this.hostSubscribers) queue.close()
    for (const interaction of this.pending.values()) {
      interaction.disposeAbort?.()
      Promise.resolve().then(interaction.next).then(interaction.resolve, interaction.reject)
    }
    this.pending.clear()
    this.resetConnection()
  }

  /** Reset only connection-owned state; Host listeners and pending asks survive. */
  resetConnection(): void {
    this.attached.clear()
    for (const state of this.followers.values()) {
      if (state.snapshot === undefined) state.reject(new Error('TUI connection generation ended'))
    }
    this.followers.clear()
    this.subagentOpenings.clear()
  }

  attachSession(sessionId: SessionId): void {
    const key = String(sessionId)
    this.attached.add(key)
    if (!this.followers.has(key)) this.followers.set(key, followerState())
    for (const interaction of this.pending.values()) {
      if (interaction.sessionId === sessionId) this.publishMux(this.interactionFrame(interaction), interaction.id)
    }
  }

  detachSession(sessionId: SessionId): void {
    const key = String(sessionId)
    this.attached.delete(key)
    const state = this.followers.get(key)
    if (state?.snapshot === undefined) state?.reject(new Error(`session "${key}" detached before opening snapshot`))
    this.followers.delete(key)
  }

  /** Uniform legacy ApiResult boundary for every accepted TUI unary method. */
  async call(
    method: string,
    params: unknown,
    operationId: string,
    signal: AbortSignal,
  ): Promise<ApiResult> {
    try {
      const payload = asRecord(params)
      const value = await this.callValue(method, payload, operationId, signal)
      return { ok: true, value }
    } catch (error: unknown) {
      return { ok: false, error: apiError(error, signal) }
    }
  }

  private async callValue(
    method: string,
    params: RecordLike,
    _operationId: string,
    signal: AbortSignal,
  ): Promise<unknown> {
    const session = this.ctx.sessionController
    switch (method) {
      case 'session.list':
        return flattenSessionList(await session.list(params, signal))
      case 'session.search':
        return await session.search(params, signal)
      case 'session.create':
        return await session.create(params)
      case 'session.history':
        return await this.sessionHistory(params, signal)
      case 'session.models':
        return await this.sessionModels(requireString(params, 'sessionId'), signal)
      case 'session.selectModel':
        return await session.selectModel(params)
      case 'session.rename':
        return await session.rename(params)
      case 'session.fork':
        return await session.fork(params)
      case 'session.prompt':
        requireString(params, 'requestId')
        return await session.prompt(params, signal)
      case 'session.attachment':
        return await session.attachment(params)
      case 'session.updateQueue':
        return await session.updateQueue(params)
      case 'session.cancel':
        return await session.cancel(params)
      case 'subagent.list':
        return await this.ctx.subagents.remoteExportList(
          requireString(params, 'parentSessionId') as SessionId,
          signal,
        )
      case 'subagent.history':
        return await this.subagentHistory(params, signal)
      case 'subagent.prompt':
        requireString(params, 'requestId')
        return await this.ctx.subagents.prompt(params, signal)
      case 'subagent.interrupt':
        return this.ctx.subagents.interruptByParent(
          requireString(params, 'childSessionId') as SessionId,
          requireString(params, 'parentSessionId') as SessionId,
          requireContinuable(params),
        )
      case 'host.describe':
        return await this.hostDescription()
      case 'host.pickDirectory':
        return { path: await this.ctx.directoryPickerController.pick(signal) }
      case 'host.listDirectory':
        return await this.ctx.directoryPickerController.list(optionalString(params, 'path'), signal)
      case 'host.createDirectory':
        return {
          path: await this.ctx.directoryPickerController.createDirectory(
            requireString(params, 'path'),
            requireString(params, 'name'),
          ),
        }
      case 'host.openPath':
        return await session.openWorkspacePath({ path: requireString(params, 'path') }, signal)
      case 'workspace.list':
        return await this.workspaceBaseline(signal)
      case 'workspace.create':
        return await this.ctx.workspaceController.create(params)
      case 'workspace.rename':
        return await this.ctx.workspaceController.rename(params)
      case 'workspace.delete':
        return await this.ctx.workspaceController.delete(params)
      case 'workspace.insertBefore':
        return await this.ctx.workspaceController.insertBefore(params)
      case 'workspace.insertSessionBefore':
        return await this.ctx.workspaceController.insertSessionBefore(params)
      case 'workspace.archiveSession':
        return await this.ctx.workspaceController.archiveSession(params)
      case 'skill.list':
        return await this.requireService(this.ctx.sessionSkillCatalog, 'session skill catalog')
          .list({ sessionId: requireString(params, 'sessionId') }, signal)
      case 'fileReferences.list': {
        const agent = await this.resolveAgent(requireString(params, 'sessionId') as SessionId)
        const items = await this.requireService(this.ctx.sessionFileReferences, 'file reference service')
          .list(agent, requireString(params, 'query'), signal)
        return { items }
      }
      case 'commands/list': {
        const agent = await this.resolveAgent(agentId(params))
        return this.ctx.commands.list(agent)
      }
      case 'commands/execute': {
        const agent = await this.resolveAgent(agentId(params))
        const line = requireString(params, 'line')
        const images = Array.isArray(params.images) ? params.images : []
        const execution = await this.ctx.commands.execute(agent, line, images, signal)
        return { matched: execution !== undefined, execution: execution ?? null }
      }
      case 'agentPreset.list':
        return await this.agentPresetList()
      case 'agentPreset.select': {
        const presets = this.requireService(this.ctx.agentPresets, 'agent preset service')
        const agent = await this.resolveAgent(requireString(params, 'sessionId') as SessionId)
        return { agentPreset: await presets.select(agent, requireString(params, 'agentPreset')) }
      }
      case 'agentPreset.read':
        return await this.requireService(this.ctx.agentPresets, 'agent preset service')
          .readDocument(requireString(params, 'agentPreset'))
      case 'agentPreset.copy': {
        const presets = this.requireService(this.ctx.agentPresets, 'agent preset service')
        const id = requireString(params, 'agentPreset')
        await presets.remoteExportCopy(
          requireString(params, 'from'), id, optionalString(params, 'name'),
        )
        return { agentPreset: id }
      }
      case 'agentPreset.openDocument':
        return await this.ctx.settingsController.openAgentPresetDirectory(
          requireString(params, 'agentPreset'), signal,
        )
      case 'agentPreset.remove':
        await this.requireService(this.ctx.agentPresets, 'agent preset service')
          .remoteExportDelete(requireString(params, 'agentPreset'))
        return {}
      case 'goal.create':
        return await this.mutateGoal(params, (goals, agent) => goals.remoteExportCreate(agent, {
          objective: requireString(params, 'objective'),
          ...params.maxGoalRounds === undefined ? {} : { maxGoalRounds: params.maxGoalRounds },
        }), true)
      case 'goal.edit':
        return await this.mutateGoal(params, (goals, agent) => goals.edit(agent, params.ref, {
          ...params.objective === undefined ? {} : { objective: params.objective },
          ...params.maxGoalRounds === undefined ? {} : { maxGoalRounds: params.maxGoalRounds },
        }))
      case 'goal.pause':
        return await this.mutateGoal(params, (goals, agent) => goals.pause(agent, params.ref))
      case 'goal.resume':
        return await this.mutateGoal(params, (goals, agent) => goals.resume(agent, params.ref))
      case 'goal.complete':
        return await this.mutateGoal(params, (goals, agent) => goals.complete(agent, params.ref))
      case 'goal.clear':
        await this.mutateGoal(params, (goals, agent) => goals.clear(agent, params.ref), true)
        return { cleared: true }
      case 'settings.describe':
        return this.ctx.settingsController.describe()
      case 'settings.openDocument':
        return await this.ctx.settingsController.openSettingsDocument(signal)
      case 'settings.update':
        return await this.ctx.settingsController.update(
          requireString(params, 'ns'), requireRecord(params, 'patch'), optionalNumber(params, 'expectedRevision'),
        )
      case 'settings.replace':
        return await this.ctx.settingsController.replace(
          requireString(params, 'ns'), requireRecord(params, 'section'), optionalNumber(params, 'expectedRevision'),
        )
      case 'settings.mutate':
        return await this.ctx.settingsController.mutate(
          requireString(params, 'ns'), requireArray(params, 'ops'), optionalNumber(params, 'expectedRevision'),
        )
      case 'credentials.describe':
        return { credentials: await this.ctx.credentialsController.describe(requireStringArray(params, 'refs')) }
      case 'credentials.set':
        await this.ctx.credentialsController.set(requireString(params, 'ref'), requireString(params, 'value'))
        return {}
      case 'credentials.unset':
        await this.ctx.credentialsController.unset(requireString(params, 'ref'))
        return {}
      case 'llm.providers':
        return { providers: this.providerDirectory() }
      case 'llm.models': {
        const catalog = await session.modelCatalog()
        return { groups: catalog.groups ?? [], failures: catalog.failures ?? [] }
      }
      case 'llm.discoverModels': {
        const settingsNs = requireString(params, 'settingsNs')
        const request = { ...params }
        delete request.settingsNs
        return { models: await this.ctx.llm.remoteDiscoverModels(settingsNs, request, signal) }
      }
      default:
        throw failure('bad-request', `unsupported TUI API method "${method}"`)
    }
  }

  /** Opening snapshot followed by gap-free live event frames. */
  async *followSession(sessionId: SessionId, signal: AbortSignal): AsyncIterable<BridgeMuxEnvelope> {
    const key = String(sessionId)
    const state = this.followers.get(key) ?? followerState()
    this.followers.set(key, state)
    const openCalls = new Map<string, { name: string; args: unknown }>()
    try {
      for await (const frame of this.ctx.sessionController.follow({
        address: { kind: 'session', sessionId },
      }, signal)) {
        if (frame.type === 'snapshot') {
          const snapshot: OpeningSnapshot = {
            header: frame.header,
            cursor: frame.cursor,
            records: frame.records,
            hasMore: frame.hasMore,
            projections: frame.projections,
          }
          state.snapshot = snapshot
          state.resolve(snapshot)
          yield {
            frame: { type: 'session/subscribed', sessionId, lastSeq: frame.cursor },
            requestId: this.nextPushId('follow-open'),
          }
          continue
        }
        const event = frame.event
        rememberToolCall(openCalls, event)
        const view = this.toolView(event, callId => openCalls.get(callId), sessionId)
        yield {
          frame: {
            type: 'session/event',
            sessionId,
            event: event as never,
            ...view === undefined ? {} : { view },
          },
          requestId: this.nextPushId('follow-event'),
        }
      }
      if (!signal.aborted && state.snapshot === undefined) {
        state.reject(new Error(`session "${key}" follow ended before its opening snapshot`))
      }
    } catch (error: unknown) {
      if (state.snapshot === undefined) state.reject(error)
      throw error
    }
  }

  /** Session control snapshots plus pending interaction frames. */
  async *muxFrames(signal: AbortSignal): AsyncIterable<BridgeMuxEnvelope> {
    const queue = new AsyncQueue<BridgeMuxEnvelope>()
    this.muxSubscribers.add(queue)
    for (const interaction of this.pending.values()) {
      if (this.attached.has(String(interaction.sessionId))) {
        queue.push({ frame: this.interactionFrame(interaction), requestId: interaction.id })
      }
    }
    void this.pumpControl(queue, signal).catch(error => queue.fail(error))
    try {
      yield* queue.read(signal)
    } finally {
      this.muxSubscribers.delete(queue)
      queue.close()
    }
  }

  /** Workspace feed plus session lifecycle events. */
  async *hostFrames(signal: AbortSignal): AsyncIterable<HostFrame> {
    const queue = new AsyncQueue<HostFrame>()
    this.hostSubscribers.add(queue)
    void this.pumpWorkspaces(queue, signal).catch(error => queue.fail(error))
    try {
      yield* queue.read(signal)
    } finally {
      this.hostSubscribers.delete(queue)
      queue.close()
    }
  }

  /** Resolve one synthetic waterfall request id at most once. */
  async respond(requestId: string, value: unknown): Promise<{ accepted: boolean; reason?: string }> {
    const interaction = this.pending.get(requestId)
    if (interaction === undefined) return { accepted: false, reason: 'not-pending' }
    this.pending.delete(requestId)
    interaction.disposeAbort?.()
    const payload = asRecord(value)
    if (interaction.kind === 'approval') {
      const outcome = payload.outcome
      if (outcome !== 'allowed-once' && outcome !== 'rejected') {
        this.pending.set(requestId, interaction)
        throw failure('bad-request', 'approval outcome must be allowed-once or rejected')
      }
      interaction.resolve(outcome)
      this.publishMux({
        type: 'approval/resolved',
        sessionId: interaction.sessionId,
        approvalId: interaction.approvalId,
        outcome,
      }, requestId)
    } else {
      const answer = payload.answer
      if (answer === undefined) {
        this.pending.set(requestId, interaction)
        throw failure('bad-request', 'question response requires answer')
      }
      interaction.resolve(answer)
      this.publishMux({
        type: 'question/resolved',
        sessionId: interaction.sessionId,
        questionRpcId: requestId,
        outcome: 'answered',
      }, requestId)
    }
    return { accepted: true }
  }

  private async sessionHistory(params: RecordLike, signal: AbortSignal): Promise<RecordLike> {
    const sessionId = requireString(params, 'sessionId') as SessionId
    const beforeSeq = optionalNumber(params, 'beforeSeq')
    const maxMessages = optionalNumber(params, 'maxMessages')
    const state = this.followers.get(String(sessionId))
    if (state === undefined) return await this.coldSessionHistory(sessionId, beforeSeq, maxMessages, signal)
    const opening = await abortable(state.opening, signal)
    const page = beforeSeq === undefined
      ? { records: opening.records, hasMore: opening.hasMore }
      : await this.ctx.sessionController.page({
        address: { kind: 'session', sessionId },
        throughSeq: opening.cursor,
        beforeSeq,
        ...maxMessages === undefined ? {} : { maxMessages },
      }, signal)
    return this.legacyHistory(page, sessionId, beforeSeq === undefined ? opening.projections : undefined)
  }

  private async coldSessionHistory(
    sessionId: SessionId,
    beforeSeq: number | undefined,
    maxMessages: number | undefined,
    signal: AbortSignal,
  ): Promise<RecordLike> {
    const inspected = await this.ctx.sessionController.inspect(sessionId, signal)
    const page = paginate(inspected.events, beforeSeq, maxMessages ?? DEFAULT_MAX_MESSAGES)
    const tail = inspected.events.at(-1)
    return {
      events: this.historyEntries(page.events, sessionId),
      hasMore: page.hasMore,
      ...beforeSeq === undefined
        ? { projections: { asOfSeq: typeof tail?.seq === 'number' ? tail.seq : -1, values: {} } }
        : {},
    }
  }

  private async subagentHistory(params: RecordLike, signal: AbortSignal): Promise<RecordLike> {
    const address = {
      kind: 'subagent',
      parentSessionId: requireString(params, 'parentSessionId'),
      childSessionId: requireString(params, 'childSessionId'),
      mode: requireMode(params),
    }
    const key = stableJson(address)
    const beforeSeq = optionalNumber(params, 'beforeSeq')
    let opening = this.subagentOpenings.get(key)
    if (opening === undefined) {
      opening = await this.readOpening(address, signal)
      this.subagentOpenings.set(key, opening)
    }
    const page = beforeSeq === undefined
      ? { records: opening.records, hasMore: opening.hasMore }
      : await this.ctx.sessionController.page({
        address,
        throughSeq: opening.cursor,
        beforeSeq,
        ...params.maxMessages === undefined ? {} : { maxMessages: params.maxMessages },
      }, signal)
    return this.legacyHistory(
      page,
      address.childSessionId as SessionId,
      beforeSeq === undefined ? opening.projections : undefined,
    )
  }

  private async readOpening(address: RecordLike, signal: AbortSignal): Promise<OpeningSnapshot> {
    const controller = new AbortController()
    const unlink = linkAbort(signal, controller)
    try {
      for await (const frame of this.ctx.sessionController.follow({ address }, controller.signal)) {
        if (frame.type !== 'snapshot') continue
        return {
          header: frame.header,
          cursor: frame.cursor,
          records: frame.records,
          hasMore: frame.hasMore,
          projections: frame.projections,
        }
      }
      throw new Error('subagent follow ended before its opening snapshot')
    } finally {
      controller.abort()
      unlink()
    }
  }

  private legacyHistory(
    page: HistoryPage,
    sessionId: SessionId,
    projections?: RecordLike,
  ): RecordLike {
    const events = recordsToEvents(page.records)
    return {
      events: this.historyEntries(events, sessionId),
      hasMore: page.hasMore,
      ...projections === undefined ? {} : { projections },
    }
  }

  private historyEntries(events: readonly RecordLike[], sessionId: SessionId): RecordLike[] {
    return events.map((event, index) => {
      const view = this.toolView(
        event,
        callId => backscanArgs(events.slice(0, index + 1), callId),
        sessionId,
      )
      return { event, ...view === undefined ? {} : { view } }
    })
  }

  private toolView(
    event: RecordLike,
    argsFor: (callId: string) => { name: string; args: unknown } | undefined,
    sessionId: SessionId,
  ): ToolEventView | undefined {
    const tools = this.ctx.tools ?? this.ctx.get?.('tools') as ToolsLike | undefined
    if (tools === undefined) return undefined
    try {
      if (event.type === 'tool/call') {
        const data = asRecord(event.data)
        const name = requireString(data, 'name')
        const raw = requireString(data, 'arguments')
        const view = tools.get(name, this.ctx.agents.get(sessionId))?.presentCall?.(JSON.parse(raw))
        return view === undefined ? undefined : { for: 'call', view }
      }
      if (event.type === 'tool/result') {
        const data = asRecord(event.data)
        const message = asRecord(data.message)
        const source = asRecord(message.source)
        const call = argsFor(requireString(source, 'callId'))
        if (call === undefined) return undefined
        const content = requireArray(message, 'content')
        const result = asRecord(content[0])
        const view = tools.get(call.name, this.ctx.agents.get(sessionId))?.presentResult?.(call.args, {
          content: result.content,
          isError: result.isError === true,
          ...data.meta === undefined ? {} : { meta: data.meta },
        })
        return view === undefined ? undefined : { for: 'result', view }
      }
    } catch {
      // Stored data or a presenter may be unavailable across a page boundary;
      // the legacy contract explicitly falls back to the generic card.
    }
    return undefined
  }

  private async sessionModels(sessionId: string, signal: AbortSignal): Promise<RecordLike> {
    const [catalog, list] = await Promise.all([
      this.ctx.sessionController.modelCatalog(),
      this.ctx.sessionController.list({}, signal),
    ])
    const row = list.items.find(item => item.sessionId === sessionId)
    const values = asOptionalRecord(asOptionalRecord(row?.projections)?.values)
    const selection = asOptionalRecord(values?.modelSelection)
    const current = asOptionalRecord(selection?.next)
      ?? asOptionalRecord(selection?.lastUsed)
      ?? asRecord(catalog.default)
    const routable = Array.isArray(catalog.routableProviders)
      && catalog.routableProviders.includes(current.provider)
    return {
      current,
      routable,
      groups: Array.isArray(catalog.groups) ? catalog.groups : [],
      failures: Array.isArray(catalog.failures) ? catalog.failures : [],
    }
  }

  private async hostDescription(): Promise<RecordLike> {
    const catalog = await this.ctx.sessionController.modelCatalog()
    const selection = asOptionalRecord(catalog.default)
    const agents = this.ctx.agents.list?.() ?? this.ctx.agents.roots?.() ?? []
    return {
      version: HARNESS_VERSION,
      cwd: process.cwd(),
      ...typeof selection?.provider === 'string' ? { provider: selection.provider } : {},
      ...typeof selection?.model === 'string' ? { model: selection.model } : {},
      attachedSessions: agents.length,
      home: homedir(),
      canOpenPath: this.ctx.sessionController.canOpenWorkspacePath(),
    }
  }

  private async agentPresetList(): Promise<RecordLike> {
    if (this.ctx.agentPresets === undefined) {
      return { presets: [], authorable: false, hasDocument: false }
    }
    const roster = await this.ctx.agentPresets.remoteExportList()
    return { ...roster, hasDocument: this.ctx.settingsController.canOpenAgentPresetDirectory() }
  }

  private async mutateGoal(
    params: RecordLike,
    mutate: (goals: GoalServiceLike, agent: AgentLike) => unknown,
    alreadyRef = false,
  ): Promise<unknown> {
    const agent = await this.resolveAgent(requireString(params, 'sessionId') as SessionId)
    const fromPreset = this.ctx.agentPresets?.serviceFor?.(agent, 'goals') as GoalServiceLike | undefined
    const goals = this.requireService(fromPreset ?? this.ctx.goals, 'goal service')
    const value = await mutate(goals, agent)
    if (alreadyRef) return value
    const view = asRecord(value)
    return { ref: { id: view.id, revision: view.revision } }
  }

  private providerDirectory(): RecordLike[] {
    const registered = this.ctx.llm.listProviders()
    const active = new Map(registered.map(row => [String(row.id), row]))
    const declared = this.ctx.llm.listConfigurableProviders()
    const rows = declared.map(row => ({
      provider: row.provider,
      displayName: row.displayName,
      settingsNs: row.settingsNs,
      settingsPath: row.settingsPath,
      active: active.has(String(row.provider)),
      ...row.declared === undefined ? {} : { declared: row.declared },
    }))
    const declaredIds = new Set(declared.map(row => String(row.provider)))
    for (const row of registered) {
      if (declaredIds.has(String(row.id))) continue
      rows.push({
        provider: row.id,
        displayName: row.name,
        settingsNs: '',
        settingsPath: [],
        active: true,
      })
    }
    return rows
  }

  private async resolveAgent(sessionId: SessionId): Promise<AgentLike> {
    const resolved = await this.ctx.sessionController.resolveAgent(sessionId)
    if ('error' in resolved) throw { failure: resolved.error }
    return resolved.agent
  }

  private requireService<Value>(value: Value | undefined, label: string): Value {
    if (value === undefined) throw failure('internal', `${label} is not mounted in this TUI profile`)
    return value
  }

  private async workspaceBaseline(signal: AbortSignal): Promise<RecordLike> {
    if (this.workspaceBaselineValue !== undefined) return cloneWorkspaceBaseline(this.workspaceBaselineValue)
    const opening = await this.readWorkspaceOpening(signal)
    this.workspaceBaselineValue = opening
    return cloneWorkspaceBaseline(opening)
  }

  private async readWorkspaceOpening(signal: AbortSignal): Promise<{ items: WorkspaceView[]; archivedSessionIds: SessionId[] }> {
    const controller = new AbortController()
    const unlink = linkAbort(signal, controller)
    try {
      for await (const frame of this.ctx.workspaceController.follow(controller.signal)) {
        if (frame.type === 'baseline') return frame.value
      }
      throw new Error('workspace follow ended before its opening baseline')
    } finally {
      controller.abort()
      unlink()
    }
  }

  private async pumpControl(queue: AsyncQueue<BridgeMuxEnvelope>, signal: AbortSignal): Promise<void> {
    for await (const frame of this.ctx.sessionController.control(signal)) {
      if (frame.type === 'baseline') {
        const queues = asOptionalRecord(frame.value.queues) ?? {}
        for (const [sessionId, items] of Object.entries(queues)) {
          queue.push({
            frame: { type: 'session/queue', sessionId: sessionId as SessionId, items: asArray(items) as never[] },
            requestId: this.nextPushId('control-queue'),
          })
        }
        const jobs = asOptionalRecord(frame.value.jobs) ?? {}
        for (const [sessionId, values] of Object.entries(jobs)) {
          queue.push({
            frame: { type: 'session/jobs', sessionId: sessionId as SessionId, jobs: asArray(values) as never[] },
            requestId: this.nextPushId('control-jobs'),
          })
        }
        const projections = asOptionalRecord(frame.value.projections) ?? {}
        for (const [sessionId, block] of Object.entries(projections)) {
          const baseline = asRecord(block)
          const values = asOptionalRecord(baseline.values) ?? {}
          const seq = typeof baseline.asOfSeq === 'number' ? baseline.asOfSeq : -1
          for (const [key, value] of Object.entries(values)) {
            queue.push({
              frame: { type: 'session/projection', sessionId: sessionId as SessionId, key, value, seq },
              requestId: this.nextPushId('control-projection'),
            })
          }
        }
        continue
      }
      const mapped: MuxFrame = frame.type === 'queue'
        ? { type: 'session/queue', sessionId: frame.sessionId, items: frame.items as never[] }
        : frame.type === 'jobs'
          ? { type: 'session/jobs', sessionId: frame.sessionId, jobs: frame.jobs as never[] }
          : {
            type: 'session/projection',
            sessionId: frame.sessionId,
            key: frame.key,
            value: frame.value,
            seq: frame.seq,
          }
      queue.push({ frame: mapped, requestId: this.nextPushId(`control-${frame.type}`) })
    }
    if (!signal.aborted) queue.close()
  }

  private async pumpWorkspaces(queue: AsyncQueue<HostFrame>, signal: AbortSignal): Promise<void> {
    for await (const frame of this.ctx.workspaceController.follow(signal)) {
      if (frame.type === 'baseline') {
        this.workspaceBaselineValue = cloneWorkspaceBaseline(frame.value)
        for (const workspace of frame.value.items) {
          queue.push({ type: 'host/workspace-changed', workspace })
        }
        queue.push({
          type: 'host/workspace-order-changed',
          workspaceIds: frame.value.items.map(workspace => workspace.workspaceId),
        })
        queue.push({
          type: 'host/archived-sessions-changed',
          archivedSessionIds: [...frame.value.archivedSessionIds],
        })
        continue
      }
      this.updateWorkspaceCache(frame)
      if (frame.type === 'upsert') queue.push({ type: 'host/workspace-changed', workspace: frame.workspace })
      else if (frame.type === 'remove') queue.push({ type: 'host/workspace-removed', workspaceId: frame.workspaceId })
      else if (frame.type === 'order') queue.push({ type: 'host/workspace-order-changed', workspaceIds: [...frame.workspaceIds] })
      else queue.push({ type: 'host/archived-sessions-changed', archivedSessionIds: [...frame.archivedSessionIds] })
    }
    if (!signal.aborted) queue.close()
  }

  private updateWorkspaceCache(frame: Exclude<WorkspaceFollowFrame, { type: 'baseline' }>): void {
    if (this.workspaceBaselineValue === undefined) return
    if (frame.type === 'upsert') {
      const index = this.workspaceBaselineValue.items.findIndex(row => row.workspaceId === frame.workspace.workspaceId)
      if (index < 0) this.workspaceBaselineValue.items.push(frame.workspace)
      else this.workspaceBaselineValue.items[index] = frame.workspace
    } else if (frame.type === 'remove') {
      this.workspaceBaselineValue.items = this.workspaceBaselineValue.items
        .filter(row => row.workspaceId !== frame.workspaceId)
    } else if (frame.type === 'order') {
      const rows = new Map(this.workspaceBaselineValue.items.map(row => [row.workspaceId, row]))
      this.workspaceBaselineValue.items = frame.workspaceIds
        .map(id => rows.get(id)).filter((row): row is WorkspaceView => row !== undefined)
    } else {
      this.workspaceBaselineValue.archivedSessionIds = [...frame.archivedSessionIds]
    }
  }

  private installHostEvents(): void {
    this.disposers.push(
      this.ctx.on('api-session/added', (summary: RecordLike) => {
        const frame = sessionAddedFrame(summary)
        this.publishHost(frame)
      }),
      this.ctx.on('api-session/removed', (sessionId: SessionId) => {
        this.publishHost({ type: 'host/session-removed', sessionId })
      }),
      this.ctx.on('api-session/status', (sessionId: SessionId, running: boolean) => {
        this.publishHost({ type: 'host/session-status', sessionId, running })
      }),
      this.ctx.on('api-session/activity', (sessionId: SessionId, updatedAt: number) => {
        this.publishHost({ type: 'host/remote-event', event: 'api-session/activity', args: [sessionId, updatedAt] })
      }),
      this.ctx.on('api-session/error', (sessionId: SessionId, message: string) => {
        this.publishHost({ type: 'host/agent-error', sessionId, message })
      }),
    )
  }

  private installInteractionAnswerers(): void {
    this.disposers.push(
      this.ctx.on('approval/request', (request: RecordLike, next: () => unknown) =>
        this.claimInteraction('approval', request, next)),
      this.ctx.on('user-questions/request', (request: RecordLike, next: () => unknown) =>
        this.claimInteraction('question', request, next)),
    )
  }

  private claimInteraction(
    kind: 'approval' | 'question',
    request: RecordLike,
    next: () => unknown,
  ): unknown {
    const agent = asOptionalRecord(request.agent) as AgentLike | undefined
    if (agent === undefined || !this.isAttachedRuntimeRoot(agent)) return next()
    const id = `tui_${kind}_${randomUUID()}`
    const settled = deferred<unknown>()
    const base = {
      kind,
      id,
      sessionId: agent.id,
      request,
      resolve: settled.resolve,
      reject: settled.reject,
      next,
    }
    const interaction: PendingInteraction = kind === 'approval'
      ? { ...base, kind, approvalId: id }
      : { ...base, kind }
    const signal = request.signal instanceof AbortSignal ? request.signal : undefined
    if (signal !== undefined) {
      const abort = (): void => this.abortInteraction(id)
      signal.addEventListener('abort', abort, { once: true })
      interaction.disposeAbort = () => signal.removeEventListener('abort', abort)
    }
    this.pending.set(id, interaction)
    this.publishMux(this.interactionFrame(interaction), id)
    return settled.promise
  }

  private isAttachedRuntimeRoot(agent: AgentLike): boolean {
    if (!this.attached.has(String(agent.id))) return false
    if (this.ctx.agents.get(agent.id) !== agent) return false
    const roots = this.ctx.agents.roots?.()
    return roots === undefined || roots.includes(agent)
  }

  private abortInteraction(id: string): void {
    const interaction = this.pending.get(id)
    if (interaction === undefined) return
    this.pending.delete(id)
    interaction.disposeAbort?.()
    if (interaction.kind === 'approval') {
      interaction.resolve('cancelled')
      this.publishMux({
        type: 'approval/resolved',
        sessionId: interaction.sessionId,
        approvalId: interaction.approvalId,
        outcome: 'cancelled',
      }, id)
    } else {
      interaction.reject(new Error('user question was cancelled'))
      this.publishMux({
        type: 'question/resolved',
        sessionId: interaction.sessionId,
        questionRpcId: id,
        outcome: 'cancelled',
      }, id)
    }
  }

  private interactionFrame(interaction: PendingInteraction): MuxFrame {
    if (interaction.kind === 'approval') {
      return {
        type: 'approval/requested',
        sessionId: interaction.sessionId,
        approvalId: interaction.approvalId,
        toolName: requireString(interaction.request, 'toolName'),
        ...typeof interaction.request.callId === 'string' ? { callId: interaction.request.callId } : {},
        ...typeof interaction.request.reason === 'string' ? { reason: interaction.request.reason } : {},
      }
    }
    return {
      type: 'question/requested',
      sessionId: interaction.sessionId,
      questions: requireArray(interaction.request, 'questions'),
    }
  }

  private publishMux(frame: MuxFrame, requestId = this.nextPushId('mux')): void {
    for (const queue of this.muxSubscribers) queue.push({ frame, requestId })
  }

  private publishHost(frame: HostFrame): void {
    for (const queue of this.hostSubscribers) queue.push(frame)
  }

  private nextPushId(prefix: string): string {
    this.pushSequence += 1
    return `tui-${prefix}-${String(this.pushSequence)}`
  }
}

function followerState(): FollowerState {
  const settled = deferred<OpeningSnapshot>()
  // A detach may reject before history starts waiting; mark it observed while
  // preserving rejection for later awaiters.
  void settled.promise.catch(() => undefined)
  return { opening: settled.promise, resolve: settled.resolve, reject: settled.reject }
}

function recordsToEvents(records: readonly HistoryRecord[]): RecordLike[] {
  const events: RecordLike[] = []
  for (const record of records) {
    if (record.type === 'event') {
      events.push(record.event)
      continue
    }
    const event = record.event
    const type = typeof event.type === 'string' ? event.type.replace(/^chunkrow\//, '') : ''
    const expanded = decodeStorageRecord({
      type,
      seq0: event.seq,
      time0: event.time,
      data: event.data,
    })
    events.push(...expanded as unknown as RecordLike[])
  }
  return events
}

function paginate(
  events: readonly RecordLike[],
  beforeSeq: number | undefined,
  maxMessages: number,
): { events: RecordLike[]; hasMore: boolean } {
  const window = beforeSeq === undefined
    ? [...events]
    : events.filter(event => typeof event.seq !== 'number' || event.seq < beforeSeq)
  let count = 0
  let cut = 0
  for (let index = window.length - 1; index >= 0; index -= 1) {
    const event = window[index] as RecordLike
    if (typeof event.type !== 'string' || !MESSAGE_TYPES.has(event.type)) continue
    count += 1
    let groupStart = typeof event.seq === 'number' ? event.seq : 0
    if (Array.isArray(event.sourceEventSeqs)) {
      for (const source of event.sourceEventSeqs) {
        if (typeof source === 'number' && source < groupStart) groupStart = source
      }
    }
    if (count >= maxMessages) {
      cut = groupStart
      break
    }
  }
  return {
    events: window.filter(event => typeof event.seq !== 'number' || event.seq >= cut),
    hasMore: cut > 0,
  }
}

function rememberToolCall(
  calls: Map<string, { name: string; args: unknown }>,
  event: RecordLike,
): void {
  if (event.type === 'turn/end') {
    calls.clear()
    return
  }
  if (event.type !== 'tool/call') return
  try {
    const data = asRecord(event.data)
    calls.set(requireString(data, 'callId'), {
      name: requireString(data, 'name'),
      args: JSON.parse(requireString(data, 'arguments')),
    })
  } catch {
    // Generic tool cards cover malformed stored arguments.
  }
}

function backscanArgs(
  events: readonly RecordLike[],
  callId: string,
): { name: string; args: unknown } | undefined {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index] as RecordLike
    if (event.type !== 'tool/call') continue
    try {
      const data = asRecord(event.data)
      if (data.callId !== callId) continue
      return { name: requireString(data, 'name'), args: JSON.parse(requireString(data, 'arguments')) }
    } catch {
      return undefined
    }
  }
  return undefined
}

function flattenSessionList(value: { items: RecordLike[] }): RecordLike {
  return {
    items: value.items.map((row) => {
      const projections = asOptionalRecord(row.projections)
      const values = asOptionalRecord(projections?.values)
      const projected = values?.agentPreset
      return {
        ...row,
        ...typeof projected === 'string'
          ? { agentPreset: projected }
          : typeof row.agentPreset === 'string' ? { agentPreset: row.agentPreset } : {},
      }
    }),
  }
}

function sessionAddedFrame(summary: RecordLike): HostFrame {
  const projections = asOptionalRecord(summary.projections)
  const values = asOptionalRecord(projections?.values)
  const projected = values?.agentPreset
  return {
    type: 'host/session-added',
    sessionId: requireString(summary, 'sessionId') as SessionId,
    blank: summary.blank === true,
    ...typeof summary.parentSessionId === 'string' ? { parentSessionId: summary.parentSessionId as SessionId } : {},
    ...summary.origin === 'subagent' ? { origin: 'subagent' as const } : {},
    ...typeof summary.cwd === 'string' ? { cwd: summary.cwd } : {},
    ...typeof projected === 'string' ? { agentPreset: projected } : {},
  }
}

function cloneWorkspaceBaseline(
  value: { items: WorkspaceView[]; archivedSessionIds: SessionId[] },
): { items: WorkspaceView[]; archivedSessionIds: SessionId[] } {
  return {
    items: value.items.map(row => ({ ...row, sessionIds: [...row.sessionIds] })),
    archivedSessionIds: [...value.archivedSessionIds],
  }
}

function apiError(error: unknown, signal: AbortSignal): ApiError {
  if (signal.aborted) {
    return { code: 'cancelled', message: 'operation was cancelled', details: {} }
  }
  if (typeof error === 'object' && error !== null) {
    const failureValue = (error as { failure?: unknown }).failure
    if (isFailure(failureValue)) return failureValue
    if (isFailure(error)) return error
    const code = (error as { code?: unknown }).code
    if (typeof code === 'string') {
      return {
        code: code.startsWith('GOAL_') ? 'internal' : code.toLowerCase().replaceAll('_', '-'),
        message: error instanceof Error ? error.message : String(error),
        details: code.startsWith('GOAL_') ? { goalCode: code } : {},
      }
    }
  }
  return {
    code: 'internal',
    message: error instanceof Error ? error.message : String(error),
    details: {},
  }
}

function isFailure(value: unknown): value is ApiError {
  return typeof value === 'object'
    && value !== null
    && typeof (value as RecordLike).code === 'string'
    && typeof (value as RecordLike).message === 'string'
    && Object.hasOwn(value, 'details')
}

function failure(code: string, message: string, details: unknown = {}): { failure: ApiError } {
  return { failure: { code, message, details } }
}

function asRecord(value: unknown): RecordLike {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw failure('bad-request', 'request payload must be an object')
  }
  return value as RecordLike
}

function asOptionalRecord(value: unknown): RecordLike | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as RecordLike
    : undefined
}

function requireRecord(value: RecordLike, key: string): RecordLike {
  try {
    return asRecord(value[key])
  } catch {
    throw failure('bad-request', `${key} must be an object`)
  }
}

function requireString(value: RecordLike, key: string): string {
  const item = value[key]
  if (typeof item !== 'string' || item.length === 0) {
    throw failure('bad-request', `${key} must be a non-empty string`)
  }
  return item
}

function optionalString(value: RecordLike, key: string): string | undefined {
  const item = value[key]
  if (item === undefined) return undefined
  if (typeof item !== 'string') throw failure('bad-request', `${key} must be a string`)
  return item
}

function optionalNumber(value: RecordLike, key: string): number | undefined {
  const item = value[key]
  if (item === undefined) return undefined
  if (typeof item !== 'number' || !Number.isSafeInteger(item)) {
    throw failure('bad-request', `${key} must be a safe integer`)
  }
  return item
}

function requireArray(value: RecordLike, key: string): unknown[] {
  const item = value[key]
  if (!Array.isArray(item)) throw failure('bad-request', `${key} must be an array`)
  return item
}

function requireStringArray(value: RecordLike, key: string): string[] {
  const items = requireArray(value, key)
  if (items.some(item => typeof item !== 'string')) {
    throw failure('bad-request', `${key} must contain only strings`)
  }
  return items as string[]
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
}

function agentId(params: RecordLike): SessionId {
  const value = typeof params.agentId === 'string' ? params.agentId : params.sessionId
  if (typeof value !== 'string' || value.length === 0) {
    throw failure('bad-request', 'agentId is required')
  }
  return value as SessionId
}

function requireMode(params: RecordLike): 'one-shot' | 'continuable' {
  if (params.mode === 'one-shot' || params.mode === 'continuable') return params.mode
  throw failure('bad-request', 'mode must be one-shot or continuable')
}

function requireContinuable(params: RecordLike): 'continuable' {
  if (params.mode === 'continuable') return params.mode
  throw failure('bad-request', 'subagent interrupt requires continuable mode')
}

function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`
  if (value !== null && typeof value === 'object') {
    return `{${Object.entries(value as RecordLike)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`)
      .join(',')}}`
  }
  return JSON.stringify(value)
}

function linkAbort(source: AbortSignal, target: AbortController): () => void {
  const abort = (): void => target.abort(source.reason)
  if (source.aborted) abort()
  else source.addEventListener('abort', abort, { once: true })
  return () => source.removeEventListener('abort', abort)
}

async function abortable<Value>(promise: Promise<Value>, signal: AbortSignal): Promise<Value> {
  signal.throwIfAborted()
  const aborted = deferred<never>()
  const abort = (): void => aborted.reject(signal.reason ?? new Error('operation aborted'))
  signal.addEventListener('abort', abort, { once: true })
  try {
    return await Promise.race([promise, aborted.promise])
  } finally {
    signal.removeEventListener('abort', abort)
  }
}

function deferred<Value>(): {
  promise: Promise<Value>
  resolve: (value: Value | PromiseLike<Value>) => void
  reject: (reason?: unknown) => void
} {
  let resolve!: (value: Value | PromiseLike<Value>) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<Value>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise
    reject = rejectPromise
  })
  return { promise, resolve, reject }
}

class AsyncQueue<Value> {
  private readonly values: Value[] = []
  private waiter: (() => void) | undefined
  private closed = false
  private error: unknown

  push(value: Value): void {
    if (this.closed) return
    this.values.push(value)
    this.waiter?.()
  }

  close(): void {
    if (this.closed) return
    this.closed = true
    this.waiter?.()
  }

  fail(error: unknown): void {
    this.error = error
    this.close()
  }

  async *read(signal: AbortSignal): AsyncIterable<Value> {
    const wake = (): void => this.waiter?.()
    signal.addEventListener('abort', wake, { once: true })
    try {
      while (!signal.aborted) {
        while (this.values.length > 0) yield this.values.shift() as Value
        if (this.closed) {
          if (this.error !== undefined) throw this.error
          return
        }
        await new Promise<void>(resolve => { this.waiter = resolve })
        this.waiter = undefined
      }
    } finally {
      signal.removeEventListener('abort', wake)
    }
  }
}
