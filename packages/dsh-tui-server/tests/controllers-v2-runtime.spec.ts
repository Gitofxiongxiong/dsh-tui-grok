import { describe, expect, it, vi } from 'vitest'
import { resolveControllersV2Runtime } from '../src/adapters/controllers-v2/runtime.ts'

describe('controllers-v2 profile runtime resolution', () => {
  it('accepts the selected profile chunk-row decoder', () => {
    const decodeStorageRecord = vi.fn(() => [])
    const runtime = resolveControllersV2Runtime((id) => {
      expect(id).toBe('@deepseek-ai/dsh-session/chunk-rows')
      return { decodeStorageRecord }
    })
    expect(runtime.decodeStorageRecord).toBe(decodeStorageRecord)
  })

  it('fails closed for missing modules and incompatible exports', () => {
    expect(() => resolveControllersV2Runtime(() => { throw new Error('not installed') }))
      .toThrow(/could not resolve.*not installed/)
    expect(() => resolveControllersV2Runtime(() => ({})))
      .toThrow(/missing the required decodeStorageRecord/)
  })
})
