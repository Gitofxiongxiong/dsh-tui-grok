/**
 * Package-owned invariant companion for the cold-session projection adapter.
 * The adapter has no package-local durable event stream; the host/cache tests
 * cover its public seam instead.
 *
 * @module @dsh-pager-grok/tui-session-projection-recovery/invariant
 */

import type { Context } from '@deepseek-ai/cordis'
import type { InvariantInstaller } from '@deepseek-ai/dsh-invariants'

const PACKAGE_NAME = '@dsh-pager-grok/tui-session-projection-recovery'

export const name = 'tui-session-projection-recovery-invariant'
export const inject = ['invariants']

const install: InvariantInstaller = () => {}

export const apply = (ctx: Context): Promise<() => void> =>
  Promise.resolve(ctx.invariants.register(PACKAGE_NAME, install))
