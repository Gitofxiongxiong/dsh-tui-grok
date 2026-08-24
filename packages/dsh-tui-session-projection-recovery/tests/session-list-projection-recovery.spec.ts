import { describe, expect, it, vi } from 'vitest'
import { SessionId } from '@deepseek-ai/dsh-session/types'
import type { ApiProxy, RpcRequest, SessionSummary } from '@deepseek-ai/dsh-host-apiproxy/api'
import type { Context } from '@deepseek-ai/cordis'
import { apply, recoverColdSessionList } from '../src/index.ts'

const coldId = SessionId('cold-recovery')
const attachedId = SessionId('attached-idle')
const request: RpcRequest<{ cursor?: string }> = { rpcId: 'test-list' as never, payload: {} }

function row(sessionId: SessionSummary['sessionId'], overrides: Partial<SessionSummary> = {}): SessionSummary {
  return {
    sessionId,
    updatedAt: 10,
    running: false,
    blank: false,
    ...overrides,
  }
}

function response(items: SessionSummary[]) {
  return {
    rpcId: request.rpcId,
    result: { ok: true as const, value: { items } },
  }
}

function context(options: {
  items: SessionSummary[]
  snapshots?: Record<string, unknown>
  attached?: Set<string>
}) {
  const snapshots = options.snapshots ?? {}
  const warnings: string[] = []
  const cache = {
    coldSnapshot: vi.fn(async (id: SessionId) => {
      const snapshot = snapshots[String(id)]
      if (snapshot instanceof Error) throw snapshot
      return snapshot ?? { asOfSeq: -1, values: {} }
    }),
  }
  const original = vi.fn(async (requestValue: typeof request) => response(options.items))
  const fake = {
    apiProxy: { sessions: { list: original } } as unknown as ApiProxy,
    sessions: { get: (id: SessionId) => (options.attached?.has(String(id)) ? {} : undefined) },
    get: (name: string) => name === 'sessionProjectionCache' ? cache : undefined,
    logger: { warn: (message: string) => warnings.push(message) },
  } as unknown as Context
  return { fake, cache, original, warnings }
}

describe('cold session projection recovery plugin', () => {
  it('recovers a cold row and applies list metadata without hiding it', async () => {
    const recovered = {
      asOfSeq: 9,
      values: {
        title: 'Recovered title',
        sessionListMetadata: { blank: false, lastPromptAt: 42 },
      },
    }
    const { fake, cache } = context({
      items: [row(coldId, { blank: true })],
      snapshots: { [coldId]: recovered },
    })
    const result = await recoverColdSessionList(fake, response([row(coldId, { blank: true })]))
    expect(result.result).toEqual({ ok: true, value: { items: [
      expect.objectContaining({
        sessionId: coldId,
        projections: recovered,
        blank: false,
        updatedAt: 42,
      }),
    ] } })
    expect(cache.coldSnapshot).toHaveBeenCalledWith(coldId, undefined)
  })

  it('does not fold attached sessions or rows that already carry a projection', async () => {
    const existing = { asOfSeq: 3, values: { title: 'already there' } }
    const { fake, cache } = context({
      items: [row(attachedId), row(coldId, { projections: existing })],
      snapshots: {
        [attachedId]: { asOfSeq: 7, values: { title: 'must not replace live row' } },
        [coldId]: { asOfSeq: 8, values: { title: 'must not replace cached row' } },
      },
      attached: new Set([String(attachedId)]),
    })
    const result = await recoverColdSessionList(fake, response([
      row(attachedId), row(coldId, { projections: existing }),
    ]))
    expect(result.result).toEqual({ ok: true, value: { items: [
      row(attachedId), row(coldId, { projections: existing }),
    ] } })
    expect(cache.coldSnapshot).not.toHaveBeenCalled()
  })

  it('fails soft per row and propagates cancellation only when requested', async () => {
    const failed = new Error('old log unavailable')
    const { fake, warnings } = context({
      items: [row(coldId)],
      snapshots: { [coldId]: failed },
    })
    await expect(recoverColdSessionList(fake, response([row(coldId)]))).resolves.toEqual(
      response([row(coldId)]),
    )
    expect(warnings[0]).toContain('old log unavailable')

    const abort = new AbortController()
    abort.abort(new Error('cancelled'))
    await expect(recoverColdSessionList(fake, response([row(coldId)]), abort.signal)).rejects.toThrow('cancelled')
  })

  it('keeps the host response unchanged when the optional cache is absent', async () => {
    const { fake, original } = context({ items: [row(coldId)] })
    fake.get = (() => undefined) as never
    const originalResponse = response([row(coldId)])
    await expect(recoverColdSessionList(fake, originalResponse)).resolves.toEqual(originalResponse)
    expect(original).not.toHaveBeenCalled()
  })

  it('restores the original ApiProxy method when the plugin effect is disposed', async () => {
    const { fake, original } = context({
      items: [row(coldId)],
      snapshots: { [coldId]: { asOfSeq: 1, values: { title: 'recovered' } } },
    })
    let dispose: (() => void) | undefined
    fake.effect = ((effect: () => (() => void) | void) => {
      dispose = effect() ?? undefined
    }) as never
    apply(fake, {})
    expect(fake.apiProxy.sessions.list).not.toBe(original)
    const listed = await fake.apiProxy.sessions.list(request)
    expect(listed.result.ok && listed.result.value.items[0]?.projections?.values.title).toBe('recovered')
    dispose?.()
    expect(fake.apiProxy.sessions.list).toBe(original)
  })
})
