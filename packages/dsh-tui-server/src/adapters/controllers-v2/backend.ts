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
import { createRequire } from 'node:module'
import { homedir } from 'node:os'
import type {
  ApiResult,
  HostFrame,
  MuxFrame,
  SessionId,
  ToolEventView,
} from '@dsh-pager-grok/tui-protocol'
import type { TuiBackend, TuiBackendInfo, TuiMuxEnvelope } from '../../core/backend.js'
import { assertBackendSelection } from '../../core/backend-selection.js'
import type {
  AgentLike,
  GoalServiceLike,
  HistoryPage,
  RecordLike,
  ToolsLike,
  TuiHarnessContext,
  WorkspaceFollowFrame,
} from './context.js'
import { CONTROLLERS_V2_DSH_VERSION, CONTROLLERS_V2_INFO } from './plugin.js'
import {
  DEFAULT_MAX_MESSAGES,
  backscanArgs,
  paginate,
  recordsToEvents,
  rememberToolCall,
  stableJson,
  type OpeningSnapshot,
} from './history.js'
import {
  apiError,
  asArray,
  asOptionalRecord,
  asRecord,
  failure,
  optionalNumber,
  requireArray,
  requireMode,
  requireString,
  sessionAddedFrame,
} from './normalize.js'
import {
  cloneWorkspaceBaseline,
  updateWorkspaceBaseline,
  type WorkspaceBaseline,
} from './workspace.js'
import {
  AsyncQueue,
} from './streams.js'
import type { PendingInteraction } from './interactions.js'
import { callControllersV2Unary } from './unary.js'
import {
  resolveControllersV2Runtime,
  type ControllersV2Runtime,
} from './runtime.js'

export type { TuiHarnessContext } from './context.js'

/** Compatibility alias retained for existing direct bridge consumers. */
export type BridgeMuxEnvelope = TuiMuxEnvelope

interface FollowerState {
  readonly opening: Promise<OpeningSnapshot>
  readonly resolve: (snapshot: OpeningSnapshot) => void
  readonly reject: (error: unknown) => void
  snapshot?: OpeningSnapshot
}

/**
 * Direct in-process adapter for Harness 0.1.2-alpha.1 controllers.
 */
export class ControllersV2Backend implements TuiBackend {
  readonly info: TuiBackendInfo = CONTROLLERS_V2_INFO

  private readonly attached = new Set<string>()
  private readonly followers = new Map<string, FollowerState>()
  private readonly subagentOpenings = new Map<string, OpeningSnapshot>()
  private readonly muxSubscribers = new Set<AsyncQueue<BridgeMuxEnvelope>>()
  private readonly hostSubscribers = new Set<AsyncQueue<HostFrame>>()
  private readonly pending = new Map<string, PendingInteraction>()
  private readonly disposers: Array<() => void> = []
  private workspaceBaselineValue: WorkspaceBaseline | undefined
  private pushSequence = 0
  private disposed = false

  private readonly decodeStorageRecord: ControllersV2Runtime['decodeStorageRecord']

  constructor(
    private readonly ctx: TuiHarnessContext,
    runtime: ControllersV2Runtime = resolveControllersV2Runtime(createRequire(import.meta.url)),
  ) {
    assertBackendSelection(this.info)
    this.decodeStorageRecord = runtime.decodeStorageRecord
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
    return await callControllersV2Unary(this.ctx, method, params, signal, {
      sessionHistory: (value, abort) => this.sessionHistory(value, abort),
      sessionModels: (sessionId, abort) => this.sessionModels(sessionId, abort),
      subagentHistory: (value, abort) => this.subagentHistory(value, abort),
      hostDescription: () => this.hostDescription(),
      workspaceBaseline: abort => this.workspaceBaseline(abort),
      agentPresetList: () => this.agentPresetList(),
      mutateGoal: (value, mutate, alreadyRef) => this.mutateGoal(value, mutate, alreadyRef),
      providerDirectory: () => this.providerDirectory(),
      resolveAgent: sessionId => this.resolveAgent(sessionId),
      requireService: <Value>(value: Value | undefined, label: string) => this.requireService(value, label),
    })
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
    const events = recordsToEvents(page.records, this.decodeStorageRecord)
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
      version: CONTROLLERS_V2_DSH_VERSION,
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

  private async workspaceBaseline(signal: AbortSignal): Promise<unknown> {
    if (this.workspaceBaselineValue !== undefined) return cloneWorkspaceBaseline(this.workspaceBaselineValue)
    const opening = await this.readWorkspaceOpening(signal)
    this.workspaceBaselineValue = opening
    return cloneWorkspaceBaseline(opening)
  }

  private async readWorkspaceOpening(signal: AbortSignal): Promise<WorkspaceBaseline> {
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
    updateWorkspaceBaseline(this.workspaceBaselineValue, frame)
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

/** @deprecated Use ControllersV2Backend for new adapter-aware code. */
export { ControllersV2Backend as TuiHarnessBridge }

function followerState(): FollowerState {
  const settled = deferred<OpeningSnapshot>()
  // A detach may reject before history starts waiting; mark it observed while
  // preserving rejection for later awaiters.
  void settled.promise.catch(() => undefined)
  return { opening: settled.promise, resolve: settled.resolve, reject: settled.reject }
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
