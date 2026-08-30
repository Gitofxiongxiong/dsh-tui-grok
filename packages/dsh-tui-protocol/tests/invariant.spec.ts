import { describe, expect, it } from 'vitest'
import * as TuiProtocolInvariant from '../src/invariant.ts'

class FakeInvariantRegistry {
  private readonly installers = new Map<string, () => void>()

  register(name: string, installer: () => void): () => void {
    if (this.installers.has(name)) throw new Error(`${name} is already registered`)
    this.installers.set(name, installer)
    return () => {
      this.installers.delete(name)
    }
  }
}

describe('tui-protocol invariant companion', () => {
  it('registers its explained empty runtime invariant', async () => {
    const ctx = { invariants: new FakeInvariantRegistry() }
    const dispose = await TuiProtocolInvariant.apply(ctx)

    expect(() => {
      ctx.invariants.register('@dsh-pager-grok/tui-protocol', () => {})
    }).toThrow(/already registered/)
    dispose()
  })
})
