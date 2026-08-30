import { describe, expect, it, vi } from 'vitest'
import { resolveApiProxyV1Runtime } from '../src/adapters/apiproxy-v1/runtime.ts'

describe('apiproxy-v1 profile runtime resolution', () => {
  it('accepts the required profile-local toFetchHandler export', () => {
    const toFetchHandler = vi.fn(() => ({ fetch: vi.fn() }))
    const runtime = resolveApiProxyV1Runtime((id) => {
      expect(id).toBe('@deepseek-ai/dsh-host-apiproxy')
      return { toFetchHandler }
    })
    expect(runtime.toFetchHandler).toBe(toFetchHandler)
  })

  it('fails closed for missing modules and incompatible exports', () => {
    expect(() => resolveApiProxyV1Runtime(() => { throw new Error('not installed') }))
      .toThrow(/could not resolve.*not installed/)
    expect(() => resolveApiProxyV1Runtime(() => ({})))
      .toThrow(/missing the required toFetchHandler/)
  })
})
