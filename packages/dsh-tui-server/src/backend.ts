/**
 * DSH-neutral backend port consumed by the TUI server core.
 *
 * @module @dsh-pager-grok/tui-server/backend
 */

import type {
  ApiResult,
  HostFrame,
  MuxFrame,
  SessionId,
  TuiUnaryMethod,
} from '@dsh-pager-grok/tui-protocol'

/** Product capabilities implemented by one DSH adapter. */
export interface TuiCapabilities {
  sessions: boolean
  workspaces: boolean
  settings: boolean
  credentials: boolean
  agentPresets: boolean
  goals: boolean
  subagents: boolean
  approvals: boolean
  questions: boolean
  queue: boolean
  jobs: boolean
  skills: boolean
  fileReferences: boolean
  directoryPicker: boolean
}

/** Stable identity and capability evidence for one backend instance. */
export interface TuiBackendInfo {
  adapterFamily: 'apiproxy-v1' | 'controllers-v2'
  dshVersion: string
  profileSchema: number
  capabilities: Readonly<TuiCapabilities>
}

/** One stable mux payload plus its server-owned request identity. */
export interface TuiMuxEnvelope {
  frame: MuxFrame
  requestId: string
}

/**
 * Adapter SPI between the TUI server core and one exact DSH architecture
 * family. Signatures intentionally mirror the running bridge baseline.
 */
export interface TuiBackend {
  readonly info: TuiBackendInfo

  call(
    method: TuiUnaryMethod,
    params: unknown,
    operationId: string,
    signal: AbortSignal,
  ): Promise<ApiResult>

  attachSession(sessionId: SessionId): void
  detachSession(sessionId: SessionId): void

  followSession(
    sessionId: SessionId,
    signal: AbortSignal,
  ): AsyncIterable<TuiMuxEnvelope>

  muxFrames(signal: AbortSignal): AsyncIterable<TuiMuxEnvelope>
  hostFrames(signal: AbortSignal): AsyncIterable<HostFrame>

  respond(
    requestId: string,
    value: unknown,
  ): Promise<{ accepted: boolean; reason?: string }>

  resetConnection(): void
  dispose(): void
}
