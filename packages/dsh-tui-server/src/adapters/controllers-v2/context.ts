import type { ApiError, SessionId, WorkspaceView } from '@dsh-pager-grok/tui-protocol'

export type RecordLike = Record<string, unknown>

export interface HistoryRecord {
  type: 'event' | 'chunks'
  event: RecordLike
}

export interface HistoryPage {
  records: readonly HistoryRecord[]
  hasMore: boolean
}

export type FollowFrame =
  | {
    type: 'snapshot'
    header: RecordLike
    cursor: number
    records: readonly HistoryRecord[]
    hasMore: boolean
    projections: RecordLike
  }
  | { type: 'event'; event: RecordLike }

export type ControlFrame =
  | { type: 'baseline'; value: RecordLike }
  | { type: 'queue'; sessionId: SessionId; items: unknown[] }
  | { type: 'jobs'; sessionId: SessionId; jobs: unknown[] }
  | { type: 'projection'; sessionId: SessionId; key: string; value: unknown; seq: number }

export type WorkspaceFollowFrame =
  | { type: 'baseline'; value: { items: WorkspaceView[]; archivedSessionIds: SessionId[] } }
  | { type: 'upsert'; workspace: WorkspaceView }
  | { type: 'remove'; workspaceId: string }
  | { type: 'order'; workspaceIds: string[] }
  | { type: 'archived'; archivedSessionIds: SessionId[] }

export interface SessionControllerLike {
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

export interface WorkspaceControllerLike {
  create(request: RecordLike): Promise<unknown>
  rename(request: RecordLike): Promise<unknown>
  delete(request: RecordLike): Promise<unknown>
  insertBefore(request: RecordLike): Promise<unknown>
  insertSessionBefore(request: RecordLike): Promise<unknown>
  archiveSession(request: RecordLike): Promise<unknown>
  follow(signal: AbortSignal): AsyncIterable<WorkspaceFollowFrame>
}

export interface DirectoryPickerControllerLike {
  pick(signal: AbortSignal): Promise<string | null>
  list(path: string | undefined, signal: AbortSignal): Promise<unknown>
  createDirectory(path: string, name: string): Promise<string>
}

export interface SettingsControllerLike {
  describe(): unknown
  canOpenAgentPresetDirectory(): boolean
  update(ns: string, patch: RecordLike, expectedRevision: number | undefined): Promise<unknown>
  replace(ns: string, section: RecordLike, expectedRevision: number | undefined): Promise<unknown>
  mutate(ns: string, ops: unknown[], expectedRevision: number | undefined): Promise<unknown>
  openSettingsDocument(signal: AbortSignal): Promise<unknown>
  openAgentPresetDirectory(agentPreset: string, signal: AbortSignal): Promise<unknown>
}

export interface CredentialsControllerLike {
  describe(refs: string[]): Promise<Record<string, unknown>>
  set(ref: string, value: string): Promise<void>
  unset(ref: string): Promise<void>
}

export interface AgentPresetServiceLike {
  remoteExportList(): Promise<RecordLike>
  readDocument(agentPreset: string): Promise<unknown>
  remoteExportCopy(from: string, id: string, name?: string): Promise<void>
  remoteExportDelete(id: string): Promise<void>
  select(agent: AgentLike, agentPreset: string): Promise<string>
  serviceFor?(agent: AgentLike, name: string): unknown
}

export interface GoalServiceLike {
  remoteExportCreate(agent: AgentLike, request: RecordLike): unknown
  edit(agent: AgentLike, ref: unknown, request: RecordLike): RecordLike
  pause(agent: AgentLike, ref: unknown): RecordLike
  resume(agent: AgentLike, ref: unknown): RecordLike
  complete(agent: AgentLike, ref: unknown): RecordLike
  clear(agent: AgentLike, ref: unknown): unknown
}

export interface LlmServiceLike {
  listProviders(): RecordLike[]
  listConfigurableProviders(): RecordLike[]
  remoteDiscoverModels(settingsNs: string, request: RecordLike, signal: AbortSignal): Promise<unknown[]>
}

export interface SubagentServiceLike {
  remoteExportList(parentSessionId: SessionId, signal: AbortSignal): Promise<unknown>
  prompt(request: RecordLike, signal: AbortSignal): Promise<unknown>
  interruptByParent(childSessionId: SessionId, parentSessionId: SessionId, mode: 'continuable'): unknown
}

export interface CommandsServiceLike {
  list(agent: AgentLike): readonly unknown[]
  execute(agent: AgentLike, line: string, images: readonly unknown[], signal: AbortSignal): Promise<unknown>
}

export interface SessionFileReferencesLike {
  list(agent: AgentLike, query: string, signal: AbortSignal): Promise<unknown[]>
}

export interface SessionSkillCatalogLike {
  list(request: RecordLike, signal: AbortSignal): Promise<unknown>
}

export interface AgentLike {
  id: SessionId
  session?: { events?: readonly RecordLike[] }
}

export interface AgentsLike {
  get(sessionId: SessionId): AgentLike | undefined
  list?(): readonly AgentLike[]
  roots?(): readonly AgentLike[]
}

export interface ToolsLike {
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
