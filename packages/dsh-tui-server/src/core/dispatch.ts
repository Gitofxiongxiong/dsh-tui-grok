/**
 * Thin TUI dispatch entry over the selected DSH backend.
 *
 * @module @dsh-pager-grok/tui-server/dispatch
 */

import {
  capabilityForTuiUnaryMethod,
  type ApiResult,
  type TuiUnaryMethod,
} from '@dsh-pager-grok/tui-protocol'
import type { TuiBackend } from './backend.js'

/** Forward one legacy unary call through the selected adapter. */
export function dispatchUnary(
  bridge: TuiBackend,
  method: TuiUnaryMethod,
  params: unknown,
  operationId: string,
  signal: AbortSignal = new AbortController().signal,
): Promise<ApiResult> {
  const capability = capabilityForTuiUnaryMethod(method)
  if (!bridge.info.capabilities[capability]) {
    return Promise.resolve({
      ok: false,
      error: {
        code: 'unsupported-capability',
        message: `${method} requires the ${capability} capability`,
        details: {
          method,
          capability,
          adapterFamily: bridge.info.adapterFamily,
          dshVersion: bridge.info.dshVersion,
        },
      },
    })
  }
  return bridge.call(method, params, operationId, signal)
}

/** Resolve one pending approval/question waterfall request. */
export function dispatchRespond(
  bridge: TuiBackend,
  requestId: string,
  value: unknown,
): Promise<unknown> {
  return bridge.respond(requestId, value)
}
