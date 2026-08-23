/**
 * Runtime protocol constants. Types live in `./types.ts`.
 *
 * @module @dsh-pager-grok/tui-protocol/constants
 */

import type { TuiErrorKind } from './types.js'

/** Wire-stable protocol integer. Handshake rejects any other value. */
export const TUI_PROTOCOL_VERSION = 1 as const

/** Wire-stable `serverInfo.name` returned by `tui.hello`. */
export const TUI_SERVER_INFO_NAME = 'deepseek-harness-tui' as const

/** JSON-RPC application error codes for TUI control failures. */
export const TUI_ERROR_CODES: Record<TuiErrorKind, number> = {
  'protocol-version': -32001,
  'stale-generation': -32002,
  'already-resolved': -32003,
  'unknown-session': -32004,
  'identity-mismatch': -32005,
  'baseline-required': -32006,
  'not-attached': -32007,
  'capability-denied': -32008,
}
