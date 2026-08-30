/**
 * Stdio JSON-RPC plugin serving the native TUI protocol over Harness domain controllers.
 * Stdout is reserved for protocol frames. Named plugin exports only — no
 * default export — so Loader `unwrapExports` keeps `name`, `inject`, `Config`,
 * and `apply`.
 *
 * @module @dsh-pager-grok/tui-server
 */

import type { Context } from '@deepseek-ai/cordis'
import type { Readable, Writable } from 'node:stream'
import Schema from '@deepseek-ai/schemastery'
import { serve } from './serve.js'
import { TuiHarnessBridge, type TuiHarnessContext } from './bridge.js'

export { TuiGateway, TUI_SERVER_VERSION } from './gateway.js'
export { TuiHarnessBridge } from './bridge.js'
export type { TuiHarnessContext } from './bridge.js'
export type {
  TuiBackend,
  TuiBackendInfo,
  TuiCapabilities,
  TuiMuxEnvelope,
} from './backend.js'
export {
  ControlPlaneRouter,
  ControlPlaneStore,
} from './control-plane.js'
export type {
  ControlPlaneApplyResult,
  ControlPlaneBaseline,
  ControlPlaneConnectionState,
  ControlPlaneRecord,
  ControlPlaneStoreOptions,
  PendingInteractionSnapshot,
  SessionControlSnapshot,
  SessionProjectionSnapshot,
  WorkspaceControlSnapshot,
} from './control-plane.js'
export { TuiLineTransport } from './transport.js'
export type { TuiLineTransportOptions } from './transport.js'
export type { TuiServeOptions } from './serve.js'
export { TuiMethodNotFoundError, TuiRpcError } from './errors.js'
export { serve } from './serve.js'

export const name = 'tui-server'
export const inject = [
  'sessionController',
  'settingsController',
  'credentialsController',
  'workspaceController',
  'directoryPickerController',
  'agents',
  'commands',
  'llm',
  'subagents',
  'agentPresets',
  'goals',
  'sessionFileReferences',
  'sessionSkillCatalog',
  'tools',
]

/** Runtime stream overrides used by tests. */
export interface TuiServerConfig {
  /** Transport input override; production uses `process.stdin`. */
  input?: Readable
  /** Transport output override; production uses `process.stdout`. */
  output?: Writable
  /** Maximum queued output frames before notifications are dropped. */
  maxQueuedFrames?: number
}

export const Config: Schema<TuiServerConfig> = Schema.object({})

/**
 * Serve TUI requests over the configured streams.
 * @param ctx - Cordis context providing the Harness 0.1.2 controllers.
 * @param config - optional stream overrides.
 */
export function apply(ctx: Context, config: TuiServerConfig): void {
  ctx.effect(() => {
    /* v8 ignore next -- production stdio wiring; tests inject streams */
    const input = config.input ?? process.stdin
    /* v8 ignore next -- production stdio wiring; tests inject streams */
    const output = config.output ?? process.stdout
    const bridge = new TuiHarnessBridge(ctx as unknown as TuiHarnessContext)
    return serve(bridge, input, output, {
      ...config.maxQueuedFrames === undefined ? {} : { maxQueuedFrames: config.maxQueuedFrames },
    })
  }, 'tui.serve')
}
