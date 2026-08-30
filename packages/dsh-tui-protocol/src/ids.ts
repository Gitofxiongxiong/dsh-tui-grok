/**
 * Brand factories for TUI wire ids.
 *
 * @module @dsh-pager-grok/tui-protocol/ids
 */

import { SessionId } from './brand.js'
import type { TuiClientId } from './types.js'

export { SessionId }

/**
 * Brand a string as a {@link TuiClientId}.
 * @param id - the raw client id string.
 * @returns the same string, branded (a compile-time cast — no runtime cost).
 */
export function TuiClientId(id: string): TuiClientId {
  return id as TuiClientId
}
