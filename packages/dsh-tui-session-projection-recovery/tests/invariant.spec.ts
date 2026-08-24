import { Context } from '@deepseek-ai/cordis'
import InvariantRegistry from '@deepseek-ai/dsh-invariants'
import { describe, expect, it } from 'vitest'
import * as RecoveryInvariant from '../src/invariant.ts'

describe('session projection recovery invariant companion', () => {
  it('registers its explained empty runtime invariant and disposes cleanly', async () => {
    const ctx = new Context()
    await ctx.plugin(InvariantRegistry)
    const fiber = await ctx.plugin(RecoveryInvariant)

    expect(() => {
      ctx.invariants.register('@dsh-pager-grok/tui-session-projection-recovery', () => {})
    }).toThrow(/already registered/)
    await fiber.dispose()
    await ctx.fiber.dispose()
  })
})
