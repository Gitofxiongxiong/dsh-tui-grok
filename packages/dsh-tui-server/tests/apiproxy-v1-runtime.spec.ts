import { describe, expect, it, vi } from 'vitest'
import { apiProxyV1Info } from '../src/adapters/apiproxy-v1/plugin.ts'
import { resolveApiProxyV1Runtime } from '../src/adapters/apiproxy-v1/runtime.ts'

describe('apiproxy-v1 profile runtime resolution', () => {
  it('advertises file references only when both profile extensions exist', () => {
    expect(apiProxyV1Info('0.1.1-rc.2', {}).capabilities.fileReferences).toBe(false)
    expect(apiProxyV1Info('0.1.1-rc.2', {
      resolveAgent: async () => ({ error: { code: 'test', message: 'test' } }),
      fileReferences: { list: async () => [] },
    }).capabilities.fileReferences).toBe(true)
  })

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
