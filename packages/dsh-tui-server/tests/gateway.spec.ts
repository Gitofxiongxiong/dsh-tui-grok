import { once } from 'node:events'
import { PassThrough } from 'node:stream'
import { describe, expect, it } from 'vitest'
import { Context, Service } from '@deepseek-ai/cordis'
import type { Agent } from '@deepseek-ai/dsh-agent'
import { CommandId } from '@deepseek-ai/dsh-commands/brand'
import type { FileReferenceService } from '@deepseek-ai/dsh-file-reference'
import type { ApiProxy, MuxFrame, RpcRequest } from '@deepseek-ai/dsh-host-apiproxy/api'
import { RpcId } from '@deepseek-ai/dsh-host-apiproxy/api'
import { SessionId } from '@deepseek-ai/dsh-session/types'
import {
  TUI_PROTOCOL_VERSION,
  TUI_SERVER_INFO_NAME,
  parseJsonRpcLine,
} from '@dsh-pager-grok/tui-protocol'
import * as TuiServer from '../src/index.ts'
import { TuiGateway, TUI_SERVER_VERSION } from '../src/gateway.ts'
import { TuiLineTransport } from '../src/transport.ts'
import { TuiMethodNotFoundError, TuiRpcError } from '../src/errors.ts'
import { serve } from '../src/serve.ts'
import { dispatchUnary } from '../src/dispatch.ts'

const sessionId = SessionId('sess-1')

async function readResult(stream: PassThrough) {
  for (;;) {
    const [chunk] = await once(stream, 'data') as [Buffer | string]
    const text = String(chunk)
    for (const line of text.split('\n')) {
      if (!line.trim()) continue
      const parsed = parseJsonRpcLine(line.trim())
      if (parsed.ok && 'id' in parsed.message) return parsed
    }
  }
}

function hang(signal: AbortSignal): Promise<never> {
  return new Promise((_, reject) => {
    const fail = (): void => {
      reject(signal.reason instanceof Error ? signal.reason : new Error(String(signal.reason ?? 'aborted')))
    }
    if (signal.aborted) fail()
    else signal.addEventListener('abort', fail, { once: true })
  })
}

function fakeApi(muxFrames: MuxFrame[] = []): ApiProxy {
  return {
    sessions: {
      list: async (request: RpcRequest<unknown>) => ({
        rpcId: request.rpcId,
        result: { ok: true, value: { items: [] } },
      }),
      history: async (request: RpcRequest<unknown>) => ({
        rpcId: request.rpcId,
        result: { ok: true, value: { events: [], hasMore: false } },
      }),
    },
    events: {
      async *mux(_request: RpcRequest<unknown>, signal: AbortSignal) {
        for (const payload of muxFrames) {
          yield { rpcId: RpcId('mux-1'), payload }
        }
        await hang(signal)
      },
      async *host(_request: RpcRequest<unknown>, signal: AbortSignal) {
        yield {
          rpcId: RpcId('host-1'),
          payload: { type: 'host/archived-sessions-changed', archivedSessionIds: [] },
        }
        await hang(signal)
      },
    },
    respond: async () => ({ accepted: true }),
  } as unknown as ApiProxy
}

class Notifications {
  readonly items: { method: string; params?: unknown }[] = []
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
  it('requires hello, then returns baseline-required and a serverReady notify', async () => {
    const notes = new Notifications()
    const gateway = new TuiGateway(fakeApi(), notes)
    await expect(gateway.handleRequest('session.list', {}, '1')).rejects.toBeInstanceOf(TuiRpcError)
    const result = await hello(gateway)
    await gateway.handleRequest('session.list', {}, '1b')
    expect(result.resumeClass).toBe('baseline-required')
    expect(result.serverInfo).toEqual({ name: TUI_SERVER_INFO_NAME, version: TUI_SERVER_VERSION })
    expect(notes.items.some(item => item.method === 'tui.serverReady')).toBe(true)
    gateway.dispose()
  })

  it('forwards file-reference discovery through the external provider seam', async () => {
    const fileReferences = {
      list: async () => [{ path: 'src/main.ts', kind: 'file' as const }],
    } as unknown as FileReferenceService
    const gateway = new TuiGateway(fakeApi(), new Notifications(), {
      fileReferences,
      resolveAgent: async () => ({ agent: {} as Agent }),
    })
    await hello(gateway)
    const result = await gateway.handleRequest('fileReferences.list', {
      sessionId,
      query: 'main',
    }, 'file-search') as { ok: boolean; value?: { items: Array<{ path: string; kind: string }> } }
    expect(result).toEqual({
      ok: true,
      value: { items: [{ path: 'src/main.ts', kind: 'file' }] },
    })
    gateway.dispose()
  })

  it('forwards the official per-agent command directory without inventing a TUI roster', async () => {
    const agent = {} as Agent
    const gateway = new TuiGateway(fakeApi(), new Notifications(), {
      resolveAgent: async id => id === sessionId
        ? { agent }
        : { error: { code: 'session-not-found', message: 'missing' } },
      commands: {
        list: received => {
          expect(received).toBe(agent)
          return [
            { name: 'permission', description: 'Switch permissions', input: { hint: '<preset>' } },
            { name: 'plan', description: 'Enter or leave plan mode', input: { hint: '[off|message]', images: true } },
          ]
        },
        execute: async (received, line, images, signal) => {
          expect(received).toBe(agent)
          expect(images).toEqual([])
          expect(signal.aborted).toBe(false)
          if (line === '/missing') return undefined
          return {
            commandId: CommandId('cmd-test-1'),
            result: { kind: 'success', text: `ran ${line}` },
          }
        },
      },
    })
    await hello(gateway)
    await expect(gateway.handleRequest('commands/list', { agentId: sessionId }, 'commands'))
      .resolves.toEqual({
        ok: true,
        value: [
          { name: 'permission', description: 'Switch permissions', input: { hint: '<preset>' } },
          { name: 'plan', description: 'Enter or leave plan mode', input: { hint: '[off|message]', images: true } },
        ],
      })
    await expect(gateway.handleRequest('commands/list', {}, 'commands-invalid'))
      .resolves.toMatchObject({ ok: false, error: { code: 'invalid-request' } })
    await expect(gateway.handleRequest('commands/execute', {
      agentId: sessionId,
      line: '/permission danger-full-access',
      images: [],
    }, 'command-execute')).resolves.toEqual({
      ok: true,
      value: {
        matched: true,
        execution: {
          commandId: 'cmd-test-1',
          result: { kind: 'success', text: 'ran /permission danger-full-access' },
        },
      },
    })
    await expect(gateway.handleRequest('commands/execute', {
      agentId: sessionId,
      line: '/missing',
    }, 'command-missing')).resolves.toEqual({
      ok: true,
      value: { matched: false, execution: null },
    })
    await expect(gateway.handleRequest('commands/execute', {
      agentId: sessionId,
      line: '/plan',
      images: [{}],
    }, 'command-images')).resolves.toMatchObject({
      ok: false,
      error: { code: 'invalid-request' },
    })
    gateway.dispose()
  })

  it('cancels an executing command when the TUI connection is disposed', async () => {
    let started!: () => void
    const running = new Promise<void>(resolve => { started = resolve })
    const gateway = new TuiGateway(fakeApi(), new Notifications(), {
      resolveAgent: async () => ({ agent: {} as Agent }),
      commands: {
        list: () => [],
        execute: async (_agent, _line, _images, signal) => {
          started()
          await hang(signal)
        },
      },
    })
    await hello(gateway)
    const pending = gateway.handleRequest('commands/execute', {
      agentId: sessionId,
      line: '/compact',
    }, 'command-cancel')
    await running
    gateway.dispose()
    await expect(pending).resolves.toMatchObject({
      ok: false,
      error: { code: 'cancelled' },
    })
  })

  it('rejects a mismatched protocol version', async () => {
    const gateway = new TuiGateway(fakeApi(), new Notifications())
    await expect(gateway.handleRequest('tui.hello', {
      protocolVersion: 2,
      clientType: 'tui',
    }, '1')).rejects.toMatchObject({ kind: 'protocol-version' })
    gateway.dispose()
  })

  it('rejects stale generation and unknown methods', async () => {
    const gateway = new TuiGateway(fakeApi(), new Notifications())
    const { generation } = await hello(gateway)
    await expect(gateway.handleRequest('tui.attach', {
      sessionId, generation: generation + 1,
    }, '2')).rejects.toMatchObject({ kind: 'stale-generation' })
    await expect(gateway.handleRequest('nope', {}, '3')).rejects.toBeInstanceOf(TuiMethodNotFoundError)
    await expect(gateway.handleRequest('events.mux', {}, '3b')).rejects.toBeInstanceOf(TuiMethodNotFoundError)
    await expect(gateway.handleRequest('tui.attach', { generation }, '4')).rejects.toBeInstanceOf(TuiRpcError)
    gateway.dispose()
  })

  it('attaches, buffers mux until history, then flushes', async () => {
    const notes = new Notifications()
    const frame: MuxFrame = { type: 'session/subscribed', sessionId, lastSeq: -1 }
    const other: MuxFrame = {
      type: 'session/subscribed',
      sessionId: SessionId('sess-other'),
      lastSeq: 0,
    }
    const live: MuxFrame = { type: 'session/subscribed', sessionId, lastSeq: 4 }
    const orphan: MuxFrame = {
      type: 'stream/error',
      error: { code: 'internal', message: 'mux', details: {} },
    }
    const api = fakeApi()
    api.events.mux = async function* (_request, signal) {
      yield { rpcId: RpcId('mux-1'), payload: orphan }
      yield { rpcId: RpcId('mux-2'), payload: other }
      yield { rpcId: RpcId('mux-3'), payload: frame }
      await new Promise(resolve => setTimeout(resolve, 40))
      yield { rpcId: RpcId('mux-live'), payload: live }
      await hang(signal)
    }
    const gateway = new TuiGateway(api, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.attach', { sessionId, generation }, '2')
    await gateway.handleRequest('tui.subscribe', { sessionId, generation }, '2b')
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(notes.items.some(item =>
      item.method === 'events.mux' && (item.params as MuxFrame).type === 'session/subscribed',
    )).toBe(false)
    expect(notes.items.some(item =>
      item.method === 'events.mux' && (item.params as MuxFrame).type === 'stream/error',
    )).toBe(true)
    await gateway.handleRequest('session.history', { sessionId }, '3')
    expect(notes.items.some(item => item.method === 'events.mux')).toBe(true)
    expect(notes.items.some(item => item.method === 'events.host')).toBe(true)
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(notes.items.some(item =>
      item.method === 'events.mux' && (item.params as MuxFrame).type === 'session/subscribed'
      && (item.params as { lastSeq?: number }).lastSeq === 4,
    )).toBe(true)
    await gateway.handleRequest('session.history', { sessionId }, '4')
    await gateway.handleRequest('tui.detach', { sessionId, generation }, '5')
    await expect(gateway.handleRequest('tui.detach', { sessionId, generation }, '6'))
      .rejects.toMatchObject({ kind: 'unknown-session' })
    gateway.dispose()
  })

  it('keeps the live backlog buffered when history returns an ApiProxy error', async () => {
    const notes = new Notifications()
    const api = fakeApi()
    api.sessions.history = async request => ({
      rpcId: request.rpcId,
      result: {
        ok: false,
        error: { code: 'session-not-found', message: 'missing', details: { sessionId } },
      },
    })
    api.events.mux = async function* (_request, signal) {
      yield {
        rpcId: RpcId('mux-live'),
        payload: { type: 'session/subscribed', sessionId, lastSeq: 3 },
      }
      await hang(signal)
    }
    const gateway = new TuiGateway(api, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.attach', { sessionId, generation }, '2')
    await new Promise(resolve => setTimeout(resolve, 10))
    await gateway.handleRequest('session.history', { sessionId }, '3')
    expect(notes.items.some(item =>
      item.method === 'events.mux' && (item.params as { type?: string }).type === 'session/subscribed',
    )).toBe(false)
    gateway.dispose()
  })

  it('restarts pumps and drops old attachments on a repeated hello', async () => {
    const notes = new Notifications()
    const gateway = new TuiGateway(fakeApi(), notes)
    const first = await hello(gateway)
    await gateway.handleRequest('tui.attach', { sessionId, generation: first.generation }, '2')
    const second = await gateway.handleRequest('tui.hello', {
      protocolVersion: TUI_PROTOCOL_VERSION,
      clientType: 'test',
      clientId: first.clientId,
    }, '3') as { generation: number; clientId: string }
    expect(second.generation).toBe(first.generation + 1)
    expect(second.clientId).toBe(first.clientId)
    await expect(gateway.handleRequest('tui.detach', {
      sessionId,
      generation: second.generation,
    }, '4')).rejects.toMatchObject({ kind: 'unknown-session' })
    gateway.dispose()
  })

  it('forwards tui.respond through the approval payload shape', async () => {
    const gateway = new TuiGateway(fakeApi(), new Notifications())
    const { generation } = await hello(gateway)
    const receipt = await gateway.handleRequest('tui.respond', {
      sessionId,
      generation,
      requestId: 'rpc-1',
      interaction: { type: 'approval', approvalId: 'ap-1', outcome: 'allowed-once' },
    }, '2')
    expect(receipt).toEqual({ accepted: true })
    const q = await gateway.handleRequest('tui.respond', {
      sessionId,
      generation,
      requestId: 'rpc-2',
      interaction: { type: 'question', answers: { answers: [] } },
    }, '3')
    expect(q).toEqual({ accepted: true })
    gateway.dispose()
  })

  it('rejects reusing an interaction request id with a different answer', async () => {
    const gateway = new TuiGateway(fakeApi(), new Notifications())
    const { generation } = await hello(gateway)
    const params = {
      sessionId,
      generation,
      requestId: 'rpc-reused',
      interaction: { type: 'approval', approvalId: 'ap-1', outcome: 'allowed-once' },
    }
    await expect(gateway.handleRequest('tui.respond', params, '2')).resolves.toEqual({ accepted: true })
    await expect(gateway.handleRequest('tui.respond', {
      ...params,
      interaction: { type: 'approval', approvalId: 'ap-1', outcome: 'rejected' },
    }, '3')).rejects.toMatchObject({ kind: 'already-resolved', extra: { requestId: 'rpc-reused' } })
    gateway.dispose()
  })

  it('allows a corrected interaction payload after the host rejects the first attempt', async () => {
    const api = fakeApi()
    let attempts = 0
    api.respond = async () => {
      attempts += 1
      return attempts === 1
        ? { accepted: false, reason: 'bad-response' }
        : { accepted: true }
    }
    const gateway = new TuiGateway(api, new Notifications())
    const { generation } = await hello(gateway)
    const base = { sessionId, generation, requestId: 'rpc-corrected' }

    await expect(gateway.handleRequest('tui.respond', {
      ...base,
      interaction: { type: 'question', answers: { answers: [{ id: 'q1', selected: ['one'] }] } },
    }, '2')).resolves.toEqual({ accepted: false, reason: 'bad-response' })
    await expect(gateway.handleRequest('tui.respond', {
      ...base,
      interaction: { type: 'question', answers: { answers: [{ id: 'q1', selected: ['two'] }] } },
    }, '3')).resolves.toEqual({ accepted: true })
    expect(attempts).toBe(2)
    gateway.dispose()
  })

  it('swallows mux/host pump failures that are not abort', async () => {
    const api = fakeApi()
    api.events.mux = async function* () {
      throw new Error('mux died')
    }
    api.events.host = async function* () {
      throw new Error('host died')
    }
    const notes = new Notifications()
    const gateway = new TuiGateway(api, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.attach', { sessionId, generation }, '2')
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(notes.items.some(item =>
      item.method === 'events.mux' && (item.params as { type?: string }).type === 'stream/error',
    )).toBe(true)
    expect(notes.items.some(item =>
      item.method === 'events.host' && (item.params as { type?: string }).type === 'stream/error',
    )).toBe(true)
    gateway.dispose()
  })

  it('reports a normal stream close as a structured stream error', async () => {
    const api = fakeApi()
    api.events.mux = async function* () { yield* [] }
    api.events.host = async function* () { yield* [] }
    const notes = new Notifications()
    const gateway = new TuiGateway(api, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.attach', { sessionId, generation }, '2')
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(notes.items.some(item =>
      item.method === 'events.mux'
      && (item.params as { type?: string }).type === 'stream/error'
      && (item.params as { error?: { code?: string } }).error?.code === 'closed',
    )).toBe(true)
    expect(notes.items.some(item =>
      item.method === 'events.host'
      && (item.params as { type?: string }).type === 'stream/error'
      && (item.params as { error?: { code?: string } }).error?.code === 'closed',
    )).toBe(true)
    gateway.dispose()
  })

  it('repeats a provided clientId on hello', async () => {
    const gateway = new TuiGateway(fakeApi(), new Notifications())
    const result = await gateway.handleRequest('tui.hello', {
      protocolVersion: 1,
      clientType: 'tui',
      clientId: 'client-fixed',
    }, '1') as { clientId: string }
    expect(result.clientId).toBe('client-fixed')
    gateway.dispose()
  })

  it('ignores history flush without a string sessionId and rejects bad control payloads', async () => {
    const gateway = new TuiGateway(fakeApi(), new Notifications())
    const { generation } = await hello(gateway)
    await gateway.handleRequest('session.history', { sessionId: 1 }, '2')
    await gateway.handleRequest('session.history', null, '3')
    await gateway.handleRequest('session.history', { sessionId: 'sess-1' }, '3b')
    await expect(gateway.handleRequest('tui.hello', {
      protocolVersion: 1, clientType: 'web',
    }, '4')).rejects.toBeInstanceOf(TuiRpcError)
    await expect(gateway.handleRequest('tui.subscribe', { generation }, '5')).rejects.toBeInstanceOf(TuiRpcError)
    await expect(gateway.handleRequest('tui.detach', { generation }, '6')).rejects.toBeInstanceOf(TuiRpcError)
    await expect(gateway.handleRequest('tui.respond', { generation, sessionId }, '7')).rejects.toBeInstanceOf(TuiRpcError)
    await gateway.handleRequest('tui.subscribe', { sessionId: SessionId('sess-fresh'), generation }, '8')
    await (gateway as unknown as {
      handleControl: (method: string, params: unknown) => Promise<unknown>
    }).handleControl('tui.missing', {})
      .then(
        () => { throw new Error('expected method-not-found') },
        (error: unknown) => { expect(error).toBeInstanceOf(TuiMethodNotFoundError) },
      )
    gateway.dispose()
  })

  it('fans out an explicit all-session subscription to unattached control frames', async () => {
    const notes = new Notifications()
    const other = SessionId('sess-other')
    const api = fakeApi()
    api.events.mux = async function* (_request, signal) {
      yield {
        rpcId: RpcId('mux-a'),
        payload: { type: 'session/projection', sessionId, key: 'title', seq: 3, value: 'A' },
      }
      yield {
        rpcId: RpcId('mux-b'),
        payload: { type: 'session/queue', sessionId: other, items: [] },
      }
      await hang(signal)
    }
    const gateway = new TuiGateway(api, notes)
    const { generation } = await hello(gateway)
    const receipt = await gateway.handleRequest('tui.subscribe', {
      generation,
      scope: 'all',
    }, '2') as { scope: string; resumeClass: string; watermarks: Record<string, number> }
    await new Promise(resolve => setTimeout(resolve, 10))
    expect(receipt.scope).toBe('all')
    expect(receipt.resumeClass).toBe('baseline-required')
    expect(gateway.controlPlane.store.snapshot(String(sessionId))?.projections.title?.value).toBe('A')
    expect(gateway.controlPlane.store.snapshot(String(other))?.queue).toEqual([])
    const muxNotes = notes.items.filter(item => item.method === 'events.mux')
    expect(muxNotes.some(item => (item.params as { sessionId?: string }).sessionId === String(sessionId))).toBe(true)
    expect(muxNotes.some(item => (item.params as { sessionId?: string }).sessionId === String(other))).toBe(true)
    expect(notes.items.some(item => item.method === 'tui.controlPlaneBaseline')).toBe(true)
    gateway.dispose()
  })

  it('keeps a session-scoped subscription from receiving another session host status', async () => {
    const notes = new Notifications()
    const other = SessionId('sess-other')
    const api = fakeApi()
    api.events.host = async function* (_request, signal) {
      yield { rpcId: RpcId('host-a'), payload: { type: 'host/session-status', sessionId, running: true } }
      yield { rpcId: RpcId('host-b'), payload: { type: 'host/session-status', sessionId: other, running: true } }
      await hang(signal)
    }
    const gateway = new TuiGateway(api, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.subscribe', { sessionId, generation }, '2')
    await new Promise(resolve => setTimeout(resolve, 10))
    const hostNotes = notes.items
      .filter(item => item.method === 'events.host')
      .map(item => (item.params as { sessionId?: string }).sessionId)
    expect(hostNotes).toContain(String(sessionId))
    expect(hostNotes).not.toContain(String(other))
    gateway.dispose()
  })

  it('returns a resume receipt and replays retained control records', async () => {
    const notes = new Notifications()
    const api = fakeApi()
    api.events.mux = async function* (_request, signal) {
      yield {
        rpcId: RpcId('mux-a'),
        payload: { type: 'session/projection', sessionId, key: 'title', seq: 2, value: 'A' },
      }
      await hang(signal)
    }
    const gateway = new TuiGateway(api, notes)
    const { generation } = await hello(gateway)
    await gateway.handleRequest('tui.subscribe', { generation, scope: 'all' }, '2')
    await new Promise(resolve => setTimeout(resolve, 10))
    const before = notes.items.length
    const receipt = await gateway.handleRequest('tui.subscribe', {
      generation, scope: 'session', sessionId, since: 1,
    }, '3') as { resumeClass: string; scope: string; sessionId?: string }
    expect(receipt).toMatchObject({ resumeClass: 'resume-accepted', scope: 'session', sessionId })
    expect(notes.items.length).toBeGreaterThan(before)
    expect(notes.items.slice(before).some(item =>
      item.method === 'events.mux' && (item.params as { type?: string }).type === 'session/projection',
    )).toBe(true)
    expect(notes.items.slice(before).some(item => item.method === 'tui.controlPlaneBaseline')).toBe(false)
    gateway.dispose()
  })
})

describe('serve / transport', () => {
  it('answers hello over newline JSON-RPC and ignores malformed lines', async () => {
    const inbound = new PassThrough()
    const outbound = new PassThrough()
    const stop = serve(fakeApi(), inbound, outbound)
    inbound.write('not-json\n')
    inbound.write('\n')
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tui.hello', params: {
      protocolVersion: 1, clientType: 'test',
    } })}\n`)
    const parsed = await readResult(outbound)
    expect(parsed.ok).toBe(true)
    if (parsed.ok) expect('result' in parsed.message).toBe(true)
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'tui.serverReady' })}\n`)
    inbound.end()
    await once(inbound, 'end')
    stop()
  })

  it('writes -32601 when no handler is installed', async () => {
    const inbound = new PassThrough({ encoding: 'utf8' })
    const outbound = new PassThrough()
    const transport = new TuiLineTransport(inbound, outbound)
    transport.start()
    transport.start()
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 7, method: 'tui.hello' })}\n`)
    const chunk = await once(outbound, 'data') as [Buffer]
    const parsed = parseJsonRpcLine(String(chunk[0]).trim())
    expect(parsed.ok).toBe(true)
    if (parsed.ok && 'error' in parsed.message) {
      expect(parsed.message.error.code).toBe(-32601)
    }
    transport.close()
    transport.close()
  })

  it('maps thrown errors onto JSON-RPC error objects', async () => {
    const inbound = new PassThrough()
    const outbound = new PassThrough()
    const transport = new TuiLineTransport(inbound, outbound)
    transport.onRequest(async (method) => {
      if (method === 'boom') throw new Error('handler boom')
      throw new TuiMethodNotFoundError(method)
    })
    transport.start()
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'boom' })}\n`)
    const first = await once(outbound, 'data') as [Buffer]
    const a = parseJsonRpcLine(String(first[0]).trim())
    expect(a.ok && 'error' in a.message && a.message.error.code).toBe(-32603)
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'missing' })}\n`)
    const second = await once(outbound, 'data') as [Buffer]
    const b = parseJsonRpcLine(String(second[0]).trim())
    expect(b.ok && 'error' in b.message && b.message.error.code).toBe(-32601)
    transport.onRequest(async () => {
      throw new TuiRpcError('capability-denied', 'no')
    })
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 4, method: 'rpc' })}\n`)
    const third = await once(outbound, 'data') as [Buffer]
    const c = parseJsonRpcLine(String(third[0]).trim())
    expect(c.ok && 'error' in c.message && c.message.error.data?.kind).toBe('capability-denied')
    transport.onRequest(async () => {
      throw 'plain'
    })
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 6, method: 'plain' })}\n`)
    const fourth = await once(outbound, 'data') as [Buffer]
    const d = parseJsonRpcLine(String(fourth[0]).trim())
    expect(d.ok && 'error' in d.message && d.message.error.code).toBe(-32603)
    inbound.write(Buffer.from(`${JSON.stringify({ jsonrpc: '2.0', id: 8, method: 'plain' })}\n`))
    inbound.emit('error', new Error('gone'))
    transport.notify('events.mux', { type: 'stream/error' })
    await once(outbound, 'data')
    transport.close()
  })
})

describe('dispatchUnary', () => {
  it('maps unknown ApiProxy methods to -32601 and impl crashes to TuiRpcError', async () => {
    await expect(dispatchUnary(fakeApi(), 'nope.nope', {}, '1')).rejects.toBeInstanceOf(TuiMethodNotFoundError)
    const crashing = fakeApi()
    crashing.sessions.list = async () => {
      throw new Error('boom')
    }
    await expect(dispatchUnary(crashing, 'session.list', {}, '2')).rejects.toBeInstanceOf(TuiRpcError)
  })
})

describe('plugin apply', () => {
  it('starts the TUI gateway through its declared Agent dependencies', async () => {
    const inbound = new PassThrough()
    const outbound = new PassThrough()
    const ctx = new Context()
    const session = {
      header: { id: sessionId },
      events: [],
    }
    const agent = { id: sessionId, session } as unknown as Agent
    class FakeAgents extends Service {
      constructor() {
        super(ctx, 'agents')
      }
      get(id: string): Agent | undefined {
        return id === sessionId ? agent : undefined
      }
      isOwnedBy(): boolean {
        return false
      }
    }
    class FakeSessions extends Service {
      constructor() {
        super(ctx, 'sessions')
      }
      get(id: string): typeof session | undefined {
        return id === sessionId ? session : undefined
      }
    }
    class FakeProxy extends Service {
      constructor() {
        super(ctx, 'apiProxy')
        Object.assign(this, fakeApi())
      }
    }
    class FakeCommands extends Service {
      constructor() {
        super(ctx, 'commands')
      }
      list(): readonly [] {
        return []
      }
      execute(): undefined {
        return undefined
      }
    }
    await ctx.plugin(FakeAgents)
    await ctx.plugin(FakeSessions)
    await ctx.plugin(FakeProxy)
    await ctx.plugin(FakeCommands)
    await ctx.plugin(TuiServer, { input: inbound, output: outbound })
    inbound.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tui.hello', params: {
      protocolVersion: 1, clientType: 'test',
    } })}\n`)
    const helloResult = await readResult(outbound)
    expect(helloResult.ok).toBe(true)
    if (!helloResult.ok || !('result' in helloResult.message)) throw new Error('tui.hello failed')
    expect(helloResult.message.result).toMatchObject({
      protocolVersion: TUI_PROTOCOL_VERSION,
      serverInfo: { name: TUI_SERVER_INFO_NAME },
    })
    await ctx.fiber.dispose()
  })
})
