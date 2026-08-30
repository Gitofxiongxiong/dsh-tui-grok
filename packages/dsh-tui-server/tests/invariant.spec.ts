import { describe, expect, it } from 'vitest'
import { inject } from '../src/index.ts'
import * as TuiServerInvariant from '../src/invariant.ts'

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

describe('tui-server invariant companion', () => {
  it('declares every Host service read directly by the compatibility bridge', () => {
    expect(inject).toEqual(expect.arrayContaining([
      'agentPresets',
      'goals',
      'sessionFileReferences',
      'sessionSkillCatalog',
      'tools',
    ]))
  })

  it('registers its explained empty runtime invariant', async () => {
    const ctx = { invariants: new FakeInvariantRegistry() }
    const dispose = await TuiServerInvariant.apply(ctx as never)

    expect(() => {
      ctx.invariants.register('@dsh-pager-grok/tui-server', () => {})
    }).toThrow(/already registered/)
    dispose()
  })
})
