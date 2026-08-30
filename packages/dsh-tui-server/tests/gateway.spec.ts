import { once } from 'node:events'
import { PassThrough } from 'node:stream'
import { describe, expect, it } from 'vitest'
import {
  SessionId,
  TUI_PROTOCOL_VERSION,
  TUI_SERVER_INFO_NAME,
  parseJsonRpcLine,
  type ApiResult,
  type HostFrame,
} from '@dsh-pager-grok/tui-protocol'
import type { TuiBackend, TuiBackendInfo, TuiMuxEnvelope } from '../src/core/backend.ts'
import { dispatchUnary } from '../src/core/dispatch.ts'
import { TuiMethodNotFoundError, TuiRpcError } from '../src/core/errors.ts'
import { TuiGateway, TUI_SERVER_VERSION } from '../src/core/gateway.ts'
import { serve } from '../src/core/serve.ts'
import { TuiLineTransport } from '../src/core/transport.ts'

const sessionId = SessionId('sess-1')

async function readResult(stream: PassThrough) {
  for (;;) {
    const [chunk] = await once(stream, 'data') as [Buffer | string]
    for (const line of String(chunk).split('\n')) {
      if (!line.trim()) continue
      const parsed = parseJsonRpcLine(line.trim())
      if (parsed.ok && 'id' in parsed.message) return parsed
    }
  }
}

function hang(signal: AbortSignal): Promise<never> {
  return new Promise((_, reject) => {
    const fail = (): void => reject(signal.reason instanceof Error
      ? signal.reason
      : new Error(String(signal.reason ?? 'aborted')))
    if (signal.aborted) fail()
    else signal.addEventListener('abort', fail, { once: true })
  })
}

function wait(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms))
}

type FollowFactory = (
  sessionId: ReturnType<typeof SessionId>,
  signal: AbortSignal,
) => AsyncIterable<TuiMuxEnvelope>

class FakeBridge implements TuiBackend {
  readonly info: TuiBackendInfo = {
    adapterFamily: 'controllers-v2',
    dshVersion: 'test',
    profileSchema: 2,
    capabilities: {
      sessions: true,
      workspaces: true,
      settings: true,
      credentials: true,
      agentPresets: true,
      goals: true,
      subagents: true,
      approvals: true,
      questions: true,
      queue: true,
      jobs: true,
      skills: true,
      fileReferences: true,
      directoryPicker: true,
    },
  }
  readonly calls: Array<{ method: string; params: unknown; operationId: string }> = []
  readonly attached = new Set<string>()
  resetCount = 0
  disposed = false
  callHandler: (
    method: string,
    params: unknown,
    operationId: string,
    signal: AbortSignal,
  ) => Promise<ApiResult> = async (method) => ({
    ok: true,
    value: method === 'session.list'
      ? { items: [] }
      : method === 'workspace.list'
        ? { items: [], archivedSessionIds: [] }
        : method === 'session.history'
          ? { events: [], hasMore: false }
          : {},
  })
  respondHandler: (
    requestId: string,
    value: unknown,
  ) => Promise<{ accepted: boolean; reason?: string }> = async () => ({ accepted: true })
  followFactory: FollowFactory = (_sessionId, signal) => this.hangingFollow(signal)
  muxFactory: (signal: AbortSignal) => AsyncIterable<TuiMuxEnvelope> = signal => this.hangingMux(signal)
  hostFactory: (signal: AbortSignal) => AsyncIterable<HostFrame> = signal => this.hangingHost(signal)

  async call(method: string, params: unknown, operationId: string, signal: AbortSignal): Promise<ApiResult> {
    this.calls.push({ method, params, operationId })
    return await this.callHandler(method, params, operationId, signal)
  }

  async respond(requestId: string, value: unknown): Promise<{ accepted: boolean; reason?: string }> {
    return await this.respondHandler(requestId, value)
  }

  attachSession(id: ReturnType<typeof SessionId>): void {
    this.attached.add(String(id))
  }

  detachSession(id: ReturnType<typeof SessionId>): void {
    this.attached.delete(String(id))
  }

  resetConnection(): void {
    this.resetCount += 1
    this.attached.clear()
  }

  dispose(): void {
    this.disposed = true
  }

  followSession(id: ReturnType<typeof SessionId>, signal: AbortSignal): AsyncIterable<TuiMuxEnvelope> {
    return this.followFactory(id, signal)
  }

  muxFrames(signal: AbortSignal): AsyncIterable<TuiMuxEnvelope> {
    return this.muxFactory(signal)
  }

  hostFrames(signal: AbortSignal): AsyncIterable<HostFrame> {
    return this.hostFactory(signal)
  }

  private async *hangingFollow(signal: AbortSignal): AsyncIterable<TuiMuxEnvelope> {
    await hang(signal)
  }

  private async *hangingMux(signal: AbortSignal): AsyncIterable<TuiMuxEnvelope> {
    await hang(signal)
  }

  private async *hangingHost(signal: AbortSignal): AsyncIterable<HostFrame> {
    await hang(signal)
  }
}

class Notifications {
  readonly items: Array<{ method: string; params?: unknown }> = []

  notify(method: string, params?: unknown): void {
    this.items.push(params === undefined ? { method } : { method, params })
  }
}

async function hello(gateway: TuiGateway) {
  return await gateway.handleRequest('tui.hello', {
    protocolVersion: TUI_PROTOCOL_VERSION,
    clientType: 'test',
  }, '1') as {
    clientId: string
    generation: number
    resumeClass: string
    serverInfo: { name: string; version: string }
  }
}

describe('TuiGateway', () => {
  it('requires hello and forwards unary calls through the new bridge', async () => {
    const fake = new FakeBridge()
    const notes = new Notifications()
    const gateway = new TuiGateway(fake, notes)
    await expect(gateway.handleRequest('session.list', {}, '1')).rejects.toBeInstanceOf(TuiRpcError)
    const result = await hello(gateway)
    await gateway.handleRequest('session.list', {}, 'list-1')
    expect(result).toMatchObject({
      resumeClass: 'baseline-required',
      serverInfo: { name: TUI_SERVER_INFO_NAME, version: TUI_SERVER_VERSION },
    })
    expect(fake.calls.at(-1)).toEqual({ method: 'session.list', params: {}, operationId: 'list-1' })
    expect(notes.items.some(item => item.method === 'tui.serverReady')).toBe(true)
    gateway.dispose()
  })

  it('rejects a mismatched protocol version and invalid control payloads', async () => {
    const gateway = new TuiGateway(new FakeBridge(), new Notifications())
    await expect(gateway.handleRequest('tui.hello', {
      protocolVersion: TUI_PROTOCOL_VERSION + 1,
      clientType: 'test',
    }, '1')).rejects.toMatchObject({ kind: 'protocol-version' })
    const { generation } = await hello(gateway)
    await expect(gateway.handleRequest('tui.attach', { generation }, '2'))
      .rejects.toBeInstanceOf(TuiRpcError)
    await expect(gateway.handleRequest('tui.subscribe', {
      generation, scope: 'session',
    }, '3')).rejects.toBeInstanceOf(TuiRpcError)
    await expect(gateway.handleRequest('tui.respond', {
      generation, sessionId,
    }, '4')).rejects.toBeInstanceOf(TuiRpcError)
    await expect(gateway.handleRequest('events.mux', {}, '5'))
      .rejects.toBeInstanceOf(TuiMethodNotFoundError)
    gateway.dispose()
  })

  it('cancels an executing bridge request when the connection is disposed', async () => {
    const fake = new FakeBridge()
    let markStarted!: () => void
    const started = new Promise<void>(resolve => { markStarted = resolve })
    fake.callHandler = async (_method, _params, _operationId, signal) => {
      markStarted()
      try {
        await hang(signal)
        throw new Error('unreachable')
      } catch {
        return {
          ok: false,
          error: { code: 'cancelled', message: 'operation was cancelled', details: {} },
        }
      }
    }
    const gateway = new TuiGateway(fake, new Notifications())
    await hello(gateway)
    const pending = gateway.handleRequest('commands/execute', {
      agentId: sessionId,
      line: '/compact',
    }, 'command-cancel')
    await started
    gateway.dispose()
    await expect(pending).resolves.toMatchObject({
      ok: false,
      error: { code: 'cancelled' },
    })
  })

  it('attaches a follower, crosses the history barrier, and filters unrelated frames', async () => {
    const fake = new FakeBridge()
    const notes = new Notifications()
    const other = SessionId('sess-other')
    fake.followFactory = async function* (id, signal) {
      yield {
        frame: { type: 'session/subscribed', sessionId: id, lastSeq: -1 },
        requestId: 'open',
      }
      await wait(35)
      yield {
        frame: { type: 'session/subscribed', sessionId: id, lastSeq: 4 },
        requestId: 'live',
      }
      await hang(signal)
    }
    fake.muxFactory = async function* (signal) {
      yield {
        frame: { type: 'stream/error', error: { code: 'test', message: 'global', details: {} } },
        requestId: 'global',
      }
      yield {
        frame: { type: 'session/queue', sessionId: other, items: [] },
        requestId: 'other',
      }
      await hang(signal)
    }
    fake.hostFactory = async function* (signal) {
      yield { type: 'host/session-status', sessionId, running: true }
      yield { type: 'host/session-status', sessionId: other, running: true }
      await hang(signal)
    }

    const gateway = new TuiGateway(fake, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.attach', { sessionId, generation }, '2')
    await wait(10)
    expect(fake.attached.has(String(sessionId))).toBe(true)
    expect(notes.items.some(item =>
      item.method === 'events.mux'
      && (item.params as { type?: string }).type === 'session/subscribed',
    )).toBe(false)
    expect(notes.items.some(item =>
      item.method === 'events.mux' && (item.params as { type?: string }).type === 'stream/error',
    )).toBe(true)
    expect(notes.items.some(item => item.method === 'events.host')).toBe(true)
    expect(notes.items.some(item =>
      item.method === 'events.host'
      && (item.params as { sessionId?: string }).sessionId === String(other),
    )).toBe(false)

    await gateway.handleRequest('session.history', { sessionId }, 'history')
    expect(notes.items.some(item =>
      item.method === 'events.mux'
      && (item.params as { type?: string; lastSeq?: number }).type === 'session/subscribed'
      && (item.params as { lastSeq?: number }).lastSeq === -1,
    )).toBe(true)
    await wait(40)
    expect(notes.items.some(item =>
      item.method === 'events.mux'
      && (item.params as { type?: string; lastSeq?: number }).type === 'session/subscribed'
      && (item.params as { lastSeq?: number }).lastSeq === 4,
    )).toBe(true)
    expect(notes.items.some(item =>
      (item.params as { sessionId?: string } | undefined)?.sessionId === String(other),
    )).toBe(false)

    await gateway.handleRequest('tui.detach', { sessionId, generation }, '3')
    await expect(gateway.handleRequest('tui.detach', { sessionId, generation }, '4'))
      .rejects.toMatchObject({ kind: 'unknown-session' })
    gateway.dispose()
  })

  it('keeps follower content buffered when history fails', async () => {
    const fake = new FakeBridge()
    fake.followFactory = async function* (id, signal) {
      yield { frame: { type: 'session/subscribed', sessionId: id, lastSeq: 3 }, requestId: 'held' }
      await hang(signal)
    }
    fake.callHandler = async method => method === 'session.history'
      ? { ok: false, error: { code: 'session-not-found', message: 'missing', details: {} } }
      : { ok: true, value: {} }
    const notes = new Notifications()
    const gateway = new TuiGateway(fake, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.attach', { sessionId, generation }, '2')
    await wait(5)
    await gateway.handleRequest('session.history', { sessionId }, '3')
    expect(notes.items.some(item =>
      (item.params as { type?: string; lastSeq?: number } | undefined)?.type === 'session/subscribed'
      && (item.params as { lastSeq?: number }).lastSeq === 3,
    )).toBe(false)
    gateway.dispose()
  })

  it('rejects stale generations, resets attachments on hello, and rejects unknown methods', async () => {
    const fake = new FakeBridge()
    const gateway = new TuiGateway(fake, new Notifications())
    const first = await hello(gateway)
    await expect(gateway.handleRequest('tui.attach', {
      sessionId, generation: first.generation + 1,
    }, '2')).rejects.toMatchObject({ kind: 'stale-generation' })
    await gateway.handleRequest('tui.attach', { sessionId, generation: first.generation }, '3')
    const second = await gateway.handleRequest('tui.hello', {
      protocolVersion: 1,
      clientType: 'test',
      clientId: first.clientId,
    }, '4') as { generation: number; clientId: string }
    expect(second).toMatchObject({ generation: first.generation + 1, clientId: first.clientId })
    await expect(gateway.handleRequest('tui.detach', {
      sessionId, generation: second.generation,
    }, '5')).rejects.toMatchObject({ kind: 'unknown-session' })
    await expect(gateway.handleRequest('nope', {}, '6')).rejects.toBeInstanceOf(TuiMethodNotFoundError)
    expect(fake.resetCount).toBeGreaterThanOrEqual(2)
    gateway.dispose()
  })

  it('aborts follower and control streams on reconnect and dispose', async () => {
    const fake = new FakeBridge()
    const followerSignals: AbortSignal[] = []
    const muxSignals: AbortSignal[] = []
    const hostSignals: AbortSignal[] = []
    fake.followFactory = async function* (_id, signal) {
      followerSignals.push(signal)
      await hang(signal)
    }
    fake.muxFactory = async function* (signal) {
      muxSignals.push(signal)
      await hang(signal)
    }
    fake.hostFactory = async function* (signal) {
      hostSignals.push(signal)
      await hang(signal)
    }

    const gateway = new TuiGateway(fake, new Notifications())
    const first = await hello(gateway)
    await gateway.handleRequest('tui.attach', { sessionId, generation: first.generation }, '2')
    await wait(5)
    expect([followerSignals[0], muxSignals[0], hostSignals[0]].every(signal => !signal.aborted))
      .toBe(true)

    const second = await hello(gateway)
    await wait(5)
    expect([followerSignals[0], muxSignals[0], hostSignals[0]].every(signal => signal.aborted))
      .toBe(true)
    expect(fake.attached.size).toBe(0)

    await gateway.handleRequest('tui.attach', { sessionId, generation: second.generation }, '3')
    await wait(5)
    expect([followerSignals[1], muxSignals[1], hostSignals[1]].every(signal => !signal.aborted))
      .toBe(true)
    gateway.dispose()
    expect([followerSignals[1], muxSignals[1], hostSignals[1]].every(signal => signal.aborted))
      .toBe(true)
    expect(fake.attached.size).toBe(0)
  })

  it('deduplicates accepted interaction replies but permits correction after rejection', async () => {
    const fake = new FakeBridge()
    let attempts = 0
    fake.respondHandler = async () => {
      attempts += 1
      return attempts === 1 ? { accepted: false, reason: 'bad-response' } : { accepted: true }
    }
    const gateway = new TuiGateway(fake, new Notifications())
    const { generation } = await hello(gateway)
    const base = { sessionId, generation, requestId: 'interaction-1' }
    await expect(gateway.handleRequest('tui.respond', {
      ...base,
      interaction: { type: 'question', answers: { answers: [{ id: 'q', selected: ['one'] }] } },
    }, '2')).resolves.toEqual({ accepted: false, reason: 'bad-response' })
    await expect(gateway.handleRequest('tui.respond', {
      ...base,
      interaction: { type: 'question', answers: { answers: [{ id: 'q', selected: ['two'] }] } },
    }, '3')).resolves.toEqual({ accepted: true })
    await expect(gateway.handleRequest('tui.respond', {
      ...base,
      interaction: { type: 'question', answers: { answers: [{ id: 'q', selected: ['three'] }] } },
    }, '4')).rejects.toMatchObject({ kind: 'already-resolved' })
    expect(attempts).toBe(2)
    gateway.dispose()
  })

  it('reuses identical in-flight and completed interaction responses at most once', async () => {
    const fake = new FakeBridge()
    let attempts = 0
    let resolveResponse!: (value: { accepted: boolean }) => void
    const response = new Promise<{ accepted: boolean }>(resolve => {
      resolveResponse = resolve
    })
    fake.respondHandler = async () => {
      attempts += 1
      return await response
    }
    const gateway = new TuiGateway(fake, new Notifications())
    const { generation } = await hello(gateway)
    const params = {
      sessionId,
      generation,
      requestId: 'interaction-identical',
      interaction: { type: 'question', answers: { answers: [{ id: 'q', selected: ['one'] }] } },
    }

    const first = gateway.handleRequest('tui.respond', params, '2')
    const duplicate = gateway.handleRequest('tui.respond', params, '3')
    expect(attempts).toBe(1)
    resolveResponse({ accepted: true })
    await expect(Promise.all([first, duplicate])).resolves.toEqual([
      { accepted: true },
      { accepted: true },
    ])
    await expect(gateway.handleRequest('tui.respond', params, '4'))
      .resolves.toEqual({ accepted: true })
    expect(attempts).toBe(1)
    gateway.dispose()
  })

  it('forwards approval and question responses through the legacy payload shapes', async () => {
    const fake = new FakeBridge()
    const received: Array<{ requestId: string; value: unknown }> = []
    fake.respondHandler = async (requestId, value) => {
      received.push({ requestId, value })
      return { accepted: true }
    }
    const gateway = new TuiGateway(fake, new Notifications())
    const { generation } = await hello(gateway)
    await expect(gateway.handleRequest('tui.respond', {
      sessionId,
      generation,
      requestId: 'approval-1',
      interaction: { type: 'approval', approvalId: 'ap-1', outcome: 'allowed-once' },
    }, '2')).resolves.toEqual({ accepted: true })
    await expect(gateway.handleRequest('tui.respond', {
      sessionId,
      generation,
      requestId: 'question-1',
      interaction: { type: 'question', answers: { answers: [{ id: 'q1', selected: ['one'] }] } },
    }, '3')).resolves.toEqual({ accepted: true })
    expect(received).toEqual([
      {
        requestId: 'approval-1',
        value: { sessionId, approvalId: 'ap-1', outcome: 'allowed-once' },
      },
      {
        requestId: 'question-1',
        value: {
          sessionId,
          answer: { answers: [{ id: 'q1', selected: ['one'] }] },
        },
      },
    ])
    gateway.dispose()
  })

  it('folds an all-session control stream and can resume retained records', async () => {
    const fake = new FakeBridge()
    const other = SessionId('sess-other')
    fake.muxFactory = async function* (signal) {
      yield {
        frame: { type: 'session/projection', sessionId, key: 'title', seq: 2, value: 'A' },
        requestId: 'projection',
      }
      yield {
        frame: { type: 'session/queue', sessionId: other, items: [] },
        requestId: 'queue',
      }
      await hang(signal)
    }
    const notes = new Notifications()
    const gateway = new TuiGateway(fake, notes)
    const { generation } = await hello(gateway)
    const baseline = await gateway.handleRequest('tui.subscribe', {
      generation, scope: 'all',
    }, '2') as { resumeClass: string }
    await wait(5)
    expect(baseline.resumeClass).toBe('baseline-required')
    expect(gateway.controlPlane.store.snapshot(String(sessionId))?.projections.title?.value).toBe('A')
    expect(gateway.controlPlane.store.snapshot(String(other))?.queue).toEqual([])
    expect(notes.items.some(item =>
      item.method === 'events.mux'
      && (item.params as { sessionId?: string }).sessionId === String(other),
    )).toBe(true)
    const before = notes.items.length
    const resume = await gateway.handleRequest('tui.subscribe', {
      generation, scope: 'session', sessionId, since: 1,
    }, '3') as { resumeClass: string }
    expect(resume.resumeClass).toBe('resume-accepted')
    expect(notes.items.slice(before).some(item => item.method === 'events.mux')).toBe(true)
    gateway.dispose()
  })

  it('reports unexpected stream close and failure as structured errors', async () => {
    const fake = new FakeBridge()
    fake.muxFactory = async function* () { throw new Error('mux died') }
    fake.hostFactory = async function* () { yield* [] }
    const notes = new Notifications()
    const gateway = new TuiGateway(fake, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.subscribe', { generation, scope: 'all' }, '2')
    await wait(5)
    const errors = notes.items
      .filter(item => item.method === 'events.mux' || item.method === 'events.host')
      .map(item => (item.params as { error?: { code?: string } }).error?.code)
    expect(errors).toContain('internal')
    expect(errors).toContain('closed')
    gateway.dispose()
  })
})

describe('serve / transport', () => {
  it('answers hello over newline JSON-RPC and disposes the bridge', async () => {
    const inbound = new PassThrough()
    const outbound = new PassThrough()
    const fake = new FakeBridge()
    const stop = serve(fake, inbound, outbound)
    inbound.write('not-json\n\n')
    inbound.write(`${JSON.stringify({
      jsonrpc: '2.0', id: 1, method: 'tui.hello',
      params: { protocolVersion: 1, clientType: 'test' },
    })}\n`)
    const parsed = await readResult(outbound)
    expect(parsed.ok && 'result' in parsed.message).toBe(true)
    stop()
    expect(fake.disposed).toBe(true)
  })

  it('writes -32601 without a handler and keeps start/close idempotent', async () => {
    const inbound = new PassThrough()
    const outbound = new PassThrough()
    const transport = new TuiLineTransport(inbound, outbound)
    transport.start()
    transport.start()
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 7, method: 'missing' })}\n`)
    const parsed = await readResult(outbound)
    expect(parsed.ok && 'error' in parsed.message && parsed.message.error.code).toBe(-32601)
    transport.close()
    transport.close()
  })

  it('maps handler failures and missing handlers onto JSON-RPC errors', async () => {
    const inbound = new PassThrough()
    const outbound = new PassThrough()
    const transport = new TuiLineTransport(inbound, outbound)
    transport.onRequest(async (method) => {
      if (method === 'boom') throw new Error('handler boom')
      throw new TuiMethodNotFoundError(method)
    })
    transport.start()
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'boom' })}\n`)
    const first = await readResult(outbound)
    expect(first.ok && 'error' in first.message && first.message.error.code).toBe(-32603)
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'missing' })}\n`)
    const second = await readResult(outbound)
    expect(second.ok && 'error' in second.message && second.message.error.code).toBe(-32601)
    transport.onRequest(async () => {
      throw new TuiRpcError('capability-denied', 'denied')
    })
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 3, method: 'denied' })}\n`)
    const third = await readResult(outbound)
    expect(third.ok && 'error' in third.message && third.message.error.data?.kind)
      .toBe('capability-denied')
    transport.onRequest(async () => {
      throw 'plain failure'
    })
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 4, method: 'plain' })}\n`)
    const fourth = await readResult(outbound)
    expect(fourth.ok && 'error' in fourth.message && fourth.message.error.code).toBe(-32603)
    transport.close()
  })
})

describe('dispatchUnary', () => {
  it('passes the caller operation identity and cancellation signal to the bridge', async () => {
    const fake = new FakeBridge()
    const controller = new AbortController()
    await expect(dispatchUnary(fake, 'session.list', { cursor: 1 }, 'operation-7', controller.signal))
      .resolves.toMatchObject({ ok: true })
    expect(fake.calls).toEqual([{
      method: 'session.list', params: { cursor: 1 }, operationId: 'operation-7',
    }])
  })

  it('returns a stable business error without dispatching a disabled capability', async () => {
    const fake = new FakeBridge()
    const gated = new Proxy(fake, {
      get(target, property) {
        if (property === 'info') {
          return {
            ...target.info,
            capabilities: { ...target.info.capabilities, fileReferences: false },
          }
        }
        const value = Reflect.get(target, property, target) as unknown
        return typeof value === 'function' ? value.bind(target) : value
      },
    }) as TuiBackend
    await expect(dispatchUnary(
      gated,
      'fileReferences.list',
      { sessionId, query: 'main' },
      'operation-disabled',
    )).resolves.toEqual({
      ok: false,
      error: {
        code: 'unsupported-capability',
        message: 'fileReferences.list requires the fileReferences capability',
        details: {
          method: 'fileReferences.list',
          capability: 'fileReferences',
          adapterFamily: 'controllers-v2',
          dshVersion: 'test',
        },
      },
    })
    expect(fake.calls).toEqual([])
  })
})
