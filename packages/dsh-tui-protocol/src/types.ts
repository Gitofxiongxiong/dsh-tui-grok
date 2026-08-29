/**
 * Named wire types for the native TUI protocol. This module is types only.
 *
 * @module @dsh-pager-grok/tui-protocol/types
 */

import type { Branded } from '@deepseek-ai/dsh-brand'
import type { SessionEvent, SessionId } from '@deepseek-ai/dsh-session/types'

export type { SessionId }

/** Stable business error carried inside a successful JSON-RPC response. */
export interface ApiError {
  code: string
  message: string
  details: unknown
}

/** Harness-independent business result frozen by the native TUI v1 wire. */
export type ApiResult<Value = unknown> =
  | { ok: true; value: Value }
  | { ok: false; error: ApiError }

/** Host-computed render intent for one tool event. */
export type ToolEventView =
  | { for: 'call'; view: unknown }
  | { for: 'result'; view: unknown }

/** One pending inbox occurrence in a complete queue snapshot. */
export interface QueuedInboxItem {
  id: string
  placement: 'queued' | 'steering' | 'context'
  message: unknown
  /** New Harness prompt correlation; old clients safely ignore it. */
  rpcId?: string
}

/** Browser-safe background job projection. */
export interface JobView {
  id: string
  kind: string
  label: string
  status: 'running' | 'stopping' | 'completed' | 'killed' | 'failed'
  detail?: string
  startedAt: number
  finishedAt?: number
}

/** Workspace row frozen at the former ApiProxy wire shape. */
export interface WorkspaceView {
  workspaceId: string
  path: string
  title: string
  sessionIds: SessionId[]
  createdAt: string
  updatedAt: string
}

/**
 * Stable mux vocabulary consumed by the Rust pager. Harness 0.1.2 emits
 * follow/control/waterfall surfaces; the server bridge translates those
 * surfaces back to this union.
 */
export type MuxFrame =
  | { type: 'session/event'; sessionId: SessionId; event: SessionEvent; view?: ToolEventView }
  | { type: 'session/subscribed'; sessionId: SessionId; lastSeq: number }
  | {
    type: 'approval/requested'
    sessionId: SessionId
    approvalId: string
    toolName: string
    callId?: string
    reason?: string
  }
  | { type: 'approval/resolved'; sessionId: SessionId; approvalId: string; outcome: unknown }
  | { type: 'question/requested'; sessionId: SessionId; questions: unknown[] }
  | {
    type: 'question/resolved'
    sessionId: SessionId
    questionRpcId: string
    outcome: 'answered' | 'cancelled'
  }
  | { type: 'session/queue'; sessionId: SessionId; items: QueuedInboxItem[] }
  | { type: 'session/jobs'; sessionId: SessionId; jobs: JobView[] }
  | { type: 'session/projection'; sessionId: SessionId; key: string; value: unknown; seq: number }
  | { type: 'stream/error'; error: ApiError }

/** Stable Host-control vocabulary consumed by the Rust pager. */
export type HostFrame =
  | {
    type: 'host/session-added'
    sessionId: SessionId
    blank: boolean
    parentSessionId?: SessionId
    origin?: 'subagent'
    cwd?: string
    agentPreset?: string
  }
  | { type: 'host/session-removed'; sessionId: SessionId }
  | { type: 'host/session-status'; sessionId: SessionId; running: boolean }
  | { type: 'host/agent-error'; sessionId: SessionId; message: string }
  | { type: 'host/workspace-changed'; workspace: WorkspaceView }
  | { type: 'host/workspace-removed'; workspaceId: string }
  | { type: 'host/workspace-order-changed'; workspaceIds: string[] }
  | { type: 'host/archived-sessions-changed'; archivedSessionIds: SessionId[] }
  | { type: 'host/remote-event'; event: string; args: unknown[] }
  | { type: 'stream/error'; error: ApiError }

/** Compatibility name retained for downstream type imports. */
export type RpcMethodMap = Record<string, unknown>

/** JSON-RPC 2.0 correlation id. */
export type JsonRpcId = string | number

/** Branded TUI client id assigned at `tui.hello`. */
export type TuiClientId = Branded<'tui-client-id'>

/**
 * Connection generation. Increments on every hello and every reconnect of
 * the same client. Stale replies that carry an older generation are discarded.
 */
export type ConnectionGeneration = number

/**
 * How the server classified this connection's event recovery.
 *
 * `baseline-required` means `events.mux` `since` is unavailable: the client
 * must refetch `session.history` and must not treat the reconnect as a
 * lossless cursor resume.
 */
export type ResumeClass = 'resume-accepted' | 'baseline-required'

/**
 * Classification of one buffered mux/host frame against an in-flight attach
 * load. Mirrors the Grok pager load-barrier head kinds.
 */
export type LoadBacklogKind = 'empty' | 'replay-head' | 'live-head' | 'unrelated'

/** Client process kind advertised at hello. */
export type TuiClientType = 'tui' | 'test'

/** Optional operator/observer/image bits reserved for later lease rules. */
export interface TuiClientCapabilities {
  /** When true the client may answer approval/question/plan interactions. */
  operator?: boolean
  /** When true the client may subscribe without becoming session driver. */
  observer?: boolean
  /** When true the client can display inline images. */
  images?: boolean
}

/**
 * Identity fields the server uses to refuse a shared backend that does not
 * match this client's profile, workspace, plugins, or sandbox.
 */
export interface TuiClientIdentity {
  profile?: string
  cwd?: string
  pluginDigest?: string
  sandbox?: string
}

/** `tui.hello` parameters. */
export interface TuiHelloParams {
  protocolVersion: 1
  clientType: TuiClientType
  capabilities?: TuiClientCapabilities
  /** Absent on first connect; a reconnecting client repeats the assigned id. */
  clientId?: TuiClientId
  identity?: TuiClientIdentity
}

/** `tui.hello` result. */
export interface TuiHelloResult {
  protocolVersion: 1
  clientId: TuiClientId
  generation: ConnectionGeneration
  resumeClass: ResumeClass
  serverInfo: {
    name: 'deepseek-harness-tui'
    version: string
    identityDigest?: string
  }
}

/** `tui.attach` parameters. */
export interface TuiAttachParams {
  sessionId: SessionId
  generation: ConnectionGeneration
}

/** Role granted for this session on this client. */
export type TuiAttachRole = 'driver' | 'subscriber'

/** `tui.attach` result. */
export interface TuiAttachResult {
  attached: true
  role: TuiAttachRole
}

/** `tui.detach` parameters. */
export interface TuiDetachParams {
  sessionId: SessionId
  generation: ConnectionGeneration
}

/** Scope of a control-plane subscription. */
export type TuiSubscribeScope = 'session' | 'control-plane' | 'all'

/** `tui.subscribe` parameters. Replay of history is unicast to this client. */
export interface TuiSubscribeParams {
  generation: ConnectionGeneration
  /** Omit for an all-session control-plane subscription. */
  sessionId?: SessionId
  /** Defaults to `session` when `sessionId` is present, otherwise `all`. */
  scope?: TuiSubscribeScope
  /** Optional event watermark; -1 denotes an empty session log. */
  since?: number
}

/** Result returned by `tui.subscribe`. */
export interface TuiSubscribeResult {
  generation: ConnectionGeneration
  resumeClass: ResumeClass
  scope: TuiSubscribeScope
  sessionId?: SessionId
  /** Per-session last event watermarks known by the gateway. */
  watermarks: Record<string, number>
}

/** One bounded control-plane replay item. */
export interface TuiControlPlaneRecord {
  stream: 'mux' | 'host'
  generation: ConnectionGeneration
  sessionId?: SessionId
  sequence?: number
  frame: TuiStampedMuxFrame | HostFrame
  at: number
}

/** Value-backed projection cell in a control-plane baseline. */
export interface TuiSessionProjection {
  seq: number
  value: unknown
}

/** Value-backed session control snapshot. */
export interface TuiSessionControlSnapshot {
  sessionId: SessionId
  generation: ConnectionGeneration
  workspaceId?: string
  /** Host `session.list` activity timestamp, if supplied by the host. */
  updatedAtMs?: number
  lastSeenSeq?: number
  subscribedLastSeq?: number
  projectionWatermark?: number
  projections: Record<string, TuiSessionProjection>
  queue: unknown[]
  jobs: unknown[]
  pendingInteractions: unknown[]
  blank?: boolean
  parentSessionId?: SessionId
  origin?: string
  cwd?: string
  agentPreset?: string
  running?: boolean
  lastError?: unknown
  removed?: boolean
  archived?: boolean
  lastActivityAt?: number
}

/** Server-to-client control-plane baseline. */
export interface TuiControlPlaneBaseline {
  generation: ConnectionGeneration
  resumeClass: ResumeClass
  sessions: TuiSessionControlSnapshot[]
  workspaces: unknown[]
  workspaceOrder: string[]
  archivedSessionIds: SessionId[]
  /** Optional replay records; old gateways may omit this field. */
  records?: TuiControlPlaneRecord[]
}

/** `tui.respond` parameters for a pending approval or question. */
export interface TuiRespondParams {
  sessionId: SessionId
  generation: ConnectionGeneration
  /** rpcId of the original `approval/requested` or `question/requested` frame. */
  requestId: string
  interaction: TuiInteractionResponse
}

/** Receipt returned after forwarding an interaction answer. */
export interface TuiRespondResult {
  accepted: boolean
}

/** Answer payload for one blocking interaction. */
export type TuiInteractionResponse =
  | { type: 'approval'; approvalId: string; outcome: unknown }
  | { type: 'question'; answers: unknown }

/** Client-to-server TUI control methods. */
export interface TuiRequestMap {
  'tui.hello': { params: TuiHelloParams; result: TuiHelloResult }
  'tui.attach': { params: TuiAttachParams; result: TuiAttachResult }
  'tui.detach': { params: TuiDetachParams; result: Record<string, never> }
  'tui.subscribe': { params: TuiSubscribeParams; result: TuiSubscribeResult }
  'tui.respond': { params: TuiRespondParams; result: TuiRespondResult }
}

export type TuiRequestMethod = keyof TuiRequestMap

/** Server-to-client TUI notifications. */
export interface TuiNotificationMap {
  'tui.serverReady': { params: Record<string, never> }
  'tui.serverDraining': { params: Record<string, never> }
  'tui.controlPlaneBaseline': { params: TuiControlPlaneBaseline }
  'events.mux': { params: TuiStampedMuxFrame }
  'events.host': { params: TuiStampedHostFrame }
}

export type TuiNotificationMethod = keyof TuiNotificationMap

/**
 * Mux frames delivered to a native TUI. Answerable frames carry the original
 * server-request id so a client can respond without reconstructing transport
 * state that belongs to ApiProxy.
 */
export type TuiMuxFrame =
  | (Extract<MuxFrame, { type: 'approval/requested' }> & { requestId: string })
  | (Extract<MuxFrame, { type: 'question/requested' }> & { requestId: string })
  | Exclude<MuxFrame, { type: 'approval/requested' | 'question/requested' }>

/** Optional generation stamp added by the TUI carrier to every notification. */
export type TuiStampedMuxFrame = TuiMuxFrame & { generation?: ConnectionGeneration }

/** Optional generation stamp added to host notifications by the carrier. */
export type TuiStampedHostFrame = HostFrame & { generation?: ConnectionGeneration }

/** Application error kinds carried in JSON-RPC `error.data.kind`. */
export type TuiErrorKind =
  | 'protocol-version'
  | 'stale-generation'
  | 'already-resolved'
  | 'unknown-session'
  | 'identity-mismatch'
  | 'baseline-required'
  | 'not-attached'
  | 'capability-denied'

/** Structured `error.data` on a TUI control failure. */
export interface TuiErrorData {
  kind: TuiErrorKind
  generation?: ConnectionGeneration
  sessionId?: SessionId
  requestId?: string
}

/** JSON-RPC error object. */
export interface JsonRpcErrorObject {
  code: number
  message: string
  data?: TuiErrorData
}

/** Parsed JSON-RPC request. */
export interface JsonRpcRequest {
  jsonrpc: '2.0'
  method: string
  id: JsonRpcId
  params?: unknown
}

/** Parsed JSON-RPC notification (no `id`). */
export interface JsonRpcNotification {
  jsonrpc: '2.0'
  method: string
  params?: unknown
}

/** Parsed JSON-RPC success response. */
export interface JsonRpcSuccess {
  jsonrpc: '2.0'
  id: JsonRpcId
  result: unknown
}

/** Parsed JSON-RPC error response. */
export interface JsonRpcFailure {
  jsonrpc: '2.0'
  id: JsonRpcId
  error: JsonRpcErrorObject
}

export type JsonRpcResponse = JsonRpcSuccess | JsonRpcFailure

export type JsonRpcMessage = JsonRpcRequest | JsonRpcNotification | JsonRpcResponse
