/**
 * Stdio JSON-RPC plugin serving the native TUI protocol over `ctx.apiProxy`.
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
import { createApiRemoteAgentResolver } from '@deepseek-ai/dsh-api-remotes'
import type { CommandRuntime } from '@deepseek-ai/dsh-commands'

export { TuiGateway, TUI_SERVER_VERSION } from './gateway.js'
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
// createApiRemoteAgentResolver reads both services directly. Cordis service
// access is not transitively authorized through apiProxy, so declare the
// resolver's own dependencies on this plugin as well.
export const inject = ['apiProxy', 'agents', 'sessions', 'commands']

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
 * @param ctx - Cordis context providing `apiProxy`.
 * @param config - optional stream overrides.
 */
export function apply(ctx: Context, config: TuiServerConfig): void {
  ctx.effect(() => {
    /* v8 ignore next -- production stdio wiring; tests inject streams */
    const input = config.input ?? process.stdin
    /* v8 ignore next -- production stdio wiring; tests inject streams */
    const output = config.output ?? process.stdout
    const fileReferences = ctx.get('fileReferences')
    const resolveAgent = createApiRemoteAgentResolver(ctx, {})
    const commands: Pick<CommandRuntime, 'list' | 'execute'> = ctx.commands
    return serve(ctx.apiProxy, input, output, {
      ...config.maxQueuedFrames === undefined ? {} : { maxQueuedFrames: config.maxQueuedFrames },
      ...fileReferences === undefined ? {} : { fileReferences },
      resolveAgent,
      commands,
    })
  }, 'tui.serve')
}
