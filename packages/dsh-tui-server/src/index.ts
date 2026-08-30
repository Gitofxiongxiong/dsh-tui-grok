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
import { serve } from './core/serve.js'
import {
  ControllersV2Backend,
  type TuiHarnessContext,
} from './adapters/controllers-v2/backend.js'
import { CONTROLLERS_V2_INJECT } from './adapters/controllers-v2/plugin.js'

export { TuiGateway, TUI_SERVER_VERSION } from './core/gateway.js'
export {
  ControllersV2Backend,
  TuiHarnessBridge,
} from './adapters/controllers-v2/backend.js'
export type { TuiHarnessContext } from './adapters/controllers-v2/backend.js'
export { ApiProxyV1Backend } from './adapters/apiproxy-v1/backend.js'
export type {
  ApiProxyV1BackendOptions,
  ApiProxyV1Extensions,
  ApiProxyV1Like,
  ProfileRequireLike,
  ToFetchHandlerLike,
} from './adapters/apiproxy-v1/backend.js'
export { resolveApiProxyV1Runtime } from './adapters/apiproxy-v1/backend.js'
export type {
  TuiBackend,
  TuiBackendInfo,
  TuiCapabilities,
  TuiMuxEnvelope,
} from './core/backend.js'
export {
  ControlPlaneRouter,
  ControlPlaneStore,
} from './core/control-plane.js'
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
} from './core/control-plane.js'
export { TuiLineTransport } from './core/transport.js'
export type { TuiLineTransportOptions } from './core/transport.js'
export type { TuiServeOptions } from './core/serve.js'
export { TuiMethodNotFoundError, TuiRpcError } from './core/errors.js'
export { serve } from './core/serve.js'

export const name = 'tui-server'
export const inject = [...CONTROLLERS_V2_INJECT]

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
    const bridge = new ControllersV2Backend(ctx as unknown as TuiHarnessContext)
    return serve(bridge, input, output, {
      ...config.maxQueuedFrames === undefined ? {} : { maxQueuedFrames: config.maxQueuedFrames },
    })
  }, 'tui.serve')
}
