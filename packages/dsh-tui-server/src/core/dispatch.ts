/**
 * Thin TUI dispatch entry over the selected DSH backend.
 *
 * @module @dsh-pager-grok/tui-server/dispatch
 */

import type { ApiResult, TuiUnaryMethod } from '@dsh-pager-grok/tui-protocol'
import type { TuiBackend } from './backend.js'

/** Forward one legacy unary call through the selected adapter. */
export function dispatchUnary(
  bridge: TuiBackend,
  method: TuiUnaryMethod,
  params: unknown,
  operationId: string,
  signal: AbortSignal = new AbortController().signal,
): Promise<ApiResult> {
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
