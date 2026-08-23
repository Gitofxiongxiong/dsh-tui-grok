/**
 * Control-plane errors that become JSON-RPC application errors.
 *
 * @module @dsh-pager-grok/tui-server/errors
 */

import type { TuiErrorData, TuiErrorKind } from '@dsh-pager-grok/tui-protocol'

/** A TUI control failure with a stable `error.data.kind`. */
export class TuiRpcError extends Error {
  /**
   * @param kind - TUI error kind written to JSON-RPC `error.data.kind`.
   * @param message - human-readable message.
   * @param extra - optional generation/session/request correlation.
   */
  constructor(
    readonly kind: TuiErrorKind,
    message: string,
    readonly extra: Omit<TuiErrorData, 'kind'> = {},
  ) {
    super(message)
    this.name = 'TuiRpcError'
  }
}

/** JSON-RPC `-32601` for a method the connection does not serve. */
export class TuiMethodNotFoundError extends Error {
  /**
   * @param method - the JSON-RPC method name.
   */
  constructor(readonly method: string) {
    super(`method not found: ${method}`)
    this.name = 'TuiMethodNotFoundError'
  }
}
