/**
 * Compile-time-only brands for TUI wire identifiers.
 *
 * @module @dsh-pager-grok/tui-protocol/brand
 */

declare const BRAND: unique symbol

/** A string distinguished by a compile-time brand with no runtime wrapper. */
export type Branded<Brand extends string> = string & { readonly [BRAND]: Brand }

/** Stable session identifier carried on the TUI wire. */
export type SessionId = Branded<'session-id'>

/**
 * Brand a raw session id without validation or allocation.
 * @param id - the raw session id string.
 * @returns the same string with its compile-time wire brand.
 */
export function SessionId(id: string): SessionId {
  return id as SessionId
}
