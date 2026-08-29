/**
 * Thin TUI dispatch entry over the Harness 0.1.2 bridge.
 *
 * @module @dsh-pager-grok/tui-server/dispatch
 */

import type { ApiResult } from '@dsh-pager-grok/tui-protocol'
import type { TuiHarnessBridge } from './bridge.js'

/** Forward one legacy unary call through the unique Harness adapter. */
export function dispatchUnary(
  bridge: TuiHarnessBridge,
  method: string,
  params: unknown,
  operationId: string,
  signal: AbortSignal = new AbortController().signal,
): Promise<ApiResult> {
  return bridge.call(method, params, operationId, signal)
}

/** Resolve one pending approval/question waterfall request. */
export function dispatchRespond(
  bridge: TuiHarnessBridge,
  requestId: string,
  value: unknown,
): Promise<unknown> {
  return bridge.respond(requestId, value)
}
