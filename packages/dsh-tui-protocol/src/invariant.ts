/**
 * Package-owned invariant companion for `@dsh-pager-grok/tui-protocol`.
 * @module @dsh-pager-grok/tui-protocol/invariant
 */

/* jscpd:ignore-start */
import type { Context } from '@deepseek-ai/cordis'
import type { InvariantInstaller } from '@deepseek-ai/dsh-invariants'

const PACKAGE_NAME = '@dsh-pager-grok/tui-protocol'

/** Cordis companion plugin name. */
export const name = 'tui-protocol-invariant'
/** Service required before the companion can reserve package ownership. */
export const inject = ['invariants']

/**
 * No runtime invariant: a pure wire library (codec + type declarations)
 * with no event stream or mutable data relation of its own; both wire
 * ends own their protocol behavior.
 */
const install: InvariantInstaller = () => {}

/**
 * Register this package's invariant companion.
 * @param ctx - Cordis context carrying the invariant service.
 * @returns the installed registration's disposer after setup succeeds.
 */
export const apply = (ctx: Context): Promise<() => void> =>
  Promise.resolve(ctx.invariants.register(PACKAGE_NAME, install))
/* jscpd:ignore-end */
