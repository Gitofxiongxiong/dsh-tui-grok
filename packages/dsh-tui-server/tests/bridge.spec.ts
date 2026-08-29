import { describe, expect, it, vi } from 'vitest'
import { SessionId, type MuxFrame } from '@dsh-pager-grok/tui-protocol'
import { TuiHarnessBridge, type TuiHarnessContext } from '../src/bridge.ts'

type RecordLike = Record<string, unknown>
type Listener = (...args: unknown[]) => unknown

function hang(signal: AbortSignal): Promise<never> {
  return new Promise((_, reject) => {
    const fail = (): void => reject(signal.reason instanceof Error
      ? signal.reason
      : new Error(String(signal.reason ?? 'aborted')))
    if (signal.aborted) fail()
    else signal.addEventListener('abort', fail, { once: true })
  })
}

async function* held<Value>(values: readonly Value[], signal: AbortSignal): AsyncIterable<Value> {
  for (const value of values) yield value
  await hang(signal)
}

interface HarnessFixture {
  context: TuiHarnessContext
  listeners: Map<string, Set<Listener>>
  agent: { id: ReturnType<typeof SessionId>; session: { events: RecordLike[] } }
  calls: {
    prompt: RecordLike[]
    page: RecordLike[]
    inspect: string[]
    resolveAgent: string[]
  }
  setFollow(values: readonly RecordLike[]): void
  setControl(values: readonly RecordLike[]): void
  setWorkspace(values: readonly RecordLike[]): void
  emit(event: string, ...args: unknown[]): unknown[]
}

function fixture(): HarnessFixture {
  const listeners = new Map<string, Set<Listener>>()
  const sid = SessionId('session-1')
  const agent = { id: sid, session: { events: [] as RecordLike[] } }
  const calls = { prompt: [] as RecordLike[], page: [] as RecordLike[], inspect: [] as string[], resolveAgent: [] as string[] }
  let followValues: readonly RecordLike[] = []
  let controlValues: readonly RecordLike[] = []
  let workspaceValues: readonly RecordLike[] = [{
    type: 'baseline',
    value: { items: [], archivedSessionIds: [] },
  }]

  const context = {
    sessionController: {
      list: async () => ({
        items: [{
          sessionId: sid,
          projections: { asOfSeq: 3, values: { agentPreset: 'ptc' } },
        }],
      }),
      search: async (request: RecordLike) => ({ items: [], query: request.query }),
      create: async (request: RecordLike) => ({ sessionId: request.sessionId ?? sid }),
      selectModel: async (request: RecordLike) => request,
      modelCatalog: async () => ({
        default: { provider: 'deepseek', model: 'deepseek-chat' },
        routableProviders: ['deepseek'],
        groups: [{ provider: 'deepseek', models: [] }],
        failures: [],
      }),
      canOpenWorkspacePath: () => true,
      openWorkspacePath: async () => ({ opened: true }),
      rename: async (request: RecordLike) => ({ title: request.title, seq: 4 }),
      fork: async () => ({ sessionId: SessionId('forked') }),
      prompt: async (request: RecordLike) => {
        calls.prompt.push(request)
        return { accepted: true }
      },
      attachment: async () => ({ attachment: {}, data: '' }),
      updateQueue: () => ({ accepted: true }),
      cancel: () => ({ accepted: true }),
      inspect: async (sessionId: ReturnType<typeof SessionId>) => {
        calls.inspect.push(String(sessionId))
        return {
          meta: { id: sessionId },
          events: [
            { type: 'user/message', seq: 0, time: 1, data: { content: [] } },
            { type: 'assistant/message', seq: 1, time: 2, data: { content: [] } },
          ],
        }
      },
      page: async (request: RecordLike) => {
        calls.page.push(request)
        return {
          records: [{
            type: 'event',
            event: { type: 'user/message', seq: 0, time: 1, data: { content: [] } },
          }],
          hasMore: false,
        }
      },
      follow: (request: RecordLike, signal: AbortSignal) => {
        void request
        return held(followValues, signal)
      },
      control: (signal: AbortSignal) => held(controlValues, signal),
      resolveAgent: async (sessionId: ReturnType<typeof SessionId>) => {
        calls.resolveAgent.push(String(sessionId))
        return { agent }
      },
    },
    workspaceController: {
      create: async (request: RecordLike) => request,
      rename: async (request: RecordLike) => request,
      delete: async (request: RecordLike) => request,
      insertBefore: async () => ({ workspaceIds: [] }),
      insertSessionBefore: async () => ({ sessionIds: [] }),
      archiveSession: async () => ({ archived: true }),
      follow: (signal: AbortSignal) => held(workspaceValues, signal),
    },
    directoryPickerController: {
      pick: async () => '/work',
      list: async (path: string | undefined) => ({ path, entries: [] }),
      createDirectory: async (path: string, name: string) => `${path}/${name}`,
    },
    settingsController: {
      describe: () => ({ namespaces: [] }),
      canOpenAgentPresetDirectory: () => true,
      update: async () => ({ revision: 2, value: {} }),
      replace: async () => ({ revision: 2, value: {} }),
      mutate: async () => ({ revision: 2, value: {} }),
      openSettingsDocument: async () => ({ opened: true }),
      openAgentPresetDirectory: async () => ({ opened: true }),
    },
    credentialsController: {
      describe: async (refs: string[]) => Object.fromEntries(refs.map(ref => [ref, {
        configured: false, writable: true,
      }])),
      set: async () => undefined,
      unset: async () => undefined,
    },
    agentPresets: {
      remoteExportList: async () => ({ presets: [], authorable: true }),
      readDocument: async () => ({ document: '' }),
      remoteExportCopy: async () => undefined,
      remoteExportDelete: async () => undefined,
      select: async (_agent: unknown, id: string) => id,
    },
    goals: {
      remoteExportCreate: () => ({ id: 'goal-1', revision: 1 }),
      edit: () => ({ id: 'goal-1', revision: 2 }),
      pause: () => ({ id: 'goal-1', revision: 2 }),
      resume: () => ({ id: 'goal-1', revision: 2 }),
      complete: () => ({ id: 'goal-1', revision: 2 }),
      clear: () => undefined,
    },
    llm: {
      listProviders: () => [{ id: 'deepseek', name: 'DeepSeek' }],
      listConfigurableProviders: () => [{
        provider: 'deepseek', displayName: 'DeepSeek', settingsNs: 'llm', settingsPath: ['deepseek'],
      }],
      remoteDiscoverModels: async () => [],
    },
    subagents: {
      remoteExportList: async () => ({ entries: [], parentAvailable: true }),
      prompt: async () => ({ messageId: 'message-1' }),
      interruptByParent: () => ({ accepted: true }),
    },
    commands: {
      list: () => [{ name: 'compact', description: 'Compact history' }],
      execute: async () => ({ commandId: 'command-1', result: { kind: 'success', text: 'ok' } }),
    },
    sessionFileReferences: {
      list: async () => [{ path: 'src/main.ts', kind: 'file' }],
    },
    sessionSkillCatalog: {
      list: async () => ({ entries: [] }),
    },
    agents: {
      get: (id: ReturnType<typeof SessionId>) => id === sid ? agent : undefined,
      list: () => [agent],
      roots: () => [agent],
    },
    tools: {
      get: () => undefined,
    },
    on(event: string, listener: Listener) {
      const bucket = listeners.get(event) ?? new Set<Listener>()
      bucket.add(listener)
      listeners.set(event, bucket)
      return () => bucket.delete(listener)
    },
  } as unknown as TuiHarnessContext

  return {
    context,
    listeners,
    agent,
    calls,
    setFollow(values) { followValues = values },
    setControl(values) { controlValues = values },
    setWorkspace(values) { workspaceValues = values },
    emit(event, ...args) {
      return [...(listeners.get(event) ?? [])].map(listener => listener(...args))
    },
  }
}

function snapshot(records: readonly RecordLike[], cursor = 4): RecordLike {
  return {
    type: 'snapshot',
    header: { id: SessionId('session-1') },
    cursor,
    records,
    hasMore: true,
    projections: { asOfSeq: cursor, values: { title: 'Opening' } },
  }
}

describe('TuiHarnessBridge unary compatibility', () => {
  it('flattens session projections and keeps cold history reads agentless', async () => {
    const f = fixture()
    const bridge = new TuiHarnessBridge(f.context)
    const signal = new AbortController().signal
    await expect(bridge.call('session.list', {}, 'list-1', signal)).resolves.toEqual({
      ok: true,
      value: {
        items: [{
          sessionId: SessionId('session-1'),
          agentPreset: 'ptc',
          projections: { asOfSeq: 3, values: { agentPreset: 'ptc' } },
        }],
      },
    })
    const history = await bridge.call('session.history', {
      sessionId: SessionId('session-1'), maxMessages: 10,
    }, 'history-1', signal)
    expect(history).toMatchObject({ ok: true, value: { hasMore: false } })
    expect(f.calls.inspect).toEqual(['session-1'])
    expect(f.calls.resolveAgent).toEqual([])
    bridge.dispose()
  })

  it('requires and forwards caller-owned prompt request ids', async () => {
    const f = fixture()
    const bridge = new TuiHarnessBridge(f.context)
    const signal = new AbortController().signal
    await expect(bridge.call('session.prompt', {
      sessionId: SessionId('session-1'), mode: 'queue', content: [],
    }, 'carrier-id', signal)).resolves.toMatchObject({
      ok: false, error: { code: 'bad-request' },
    })
    const params = {
      sessionId: SessionId('session-1'),
      requestId: 'operation-9',
      mode: 'queue',
      content: [{ type: 'text', text: 'hello' }],
    }
    await expect(bridge.call('session.prompt', params, 'carrier-id', signal))
      .resolves.toEqual({ ok: true, value: { accepted: true } })
    expect(f.calls.prompt).toEqual([params])
    bridge.dispose()
  })

  it('maps controller, command, settings, credential, and provider services without an ApiProxy', async () => {
    const f = fixture()
    const bridge = new TuiHarnessBridge(f.context)
    const signal = new AbortController().signal
    await expect(bridge.call('commands/list', { agentId: SessionId('session-1') }, '1', signal))
      .resolves.toMatchObject({ ok: true, value: [{ name: 'compact' }] })
    await expect(bridge.call('fileReferences.list', {
      sessionId: SessionId('session-1'), query: 'main',
    }, '2', signal)).resolves.toMatchObject({
      ok: true, value: { items: [{ path: 'src/main.ts' }] },
    })
    await expect(bridge.call('credentials.describe', { refs: ['DEEPSEEK_API_KEY'] }, '3', signal))
      .resolves.toMatchObject({
        ok: true,
        value: { credentials: { DEEPSEEK_API_KEY: { configured: false, writable: true } } },
      })
    await expect(bridge.call('llm.providers', {}, '4', signal)).resolves.toMatchObject({
      ok: true,
      value: { providers: [{ provider: 'deepseek', active: true }] },
    })
    bridge.dispose()
  })
})

describe('TuiHarnessBridge history stream', () => {
  it('uses the opening snapshot, expands chunk rows losslessly, and pins old pages to its cursor', async () => {
    const f = fixture()
    f.setFollow([
      snapshot([
        {
          type: 'chunks',
          event: {
            type: 'chunkrow/text-chunks',
            seq: 2,
            time: 10,
            data: { turn: 1, step: 0, index: 0, dt: [1, 2], texts: ['a', 'b', 'c'] },
          },
        },
      ], 4),
      { type: 'event', event: { type: 'turn/end', seq: 5, time: 20, data: { turn: 1 } } },
    ])
    const bridge = new TuiHarnessBridge(f.context)
    const sessionId = SessionId('session-1')
    bridge.attachSession(sessionId)
    const abort = new AbortController()
    const follower = bridge.followSession(sessionId, abort.signal)[Symbol.asyncIterator]()
    await expect(follower.next()).resolves.toMatchObject({
      value: { frame: { type: 'session/subscribed', lastSeq: 4 } },
    })
    const opening = await bridge.call('session.history', { sessionId }, 'opening', abort.signal)
    expect(opening).toMatchObject({ ok: true, value: { hasMore: true } })
    if (!opening.ok) throw new Error('history failed')
    const events = (opening.value as { events: Array<{ event: RecordLike }> }).events.map(row => row.event)
    expect(events.map(event => [event.type, event.seq, event.time])).toEqual([
      ['assistant/chunk', 2, 10],
      ['assistant/chunk', 3, 11],
      ['assistant/chunk', 4, 13],
    ])
    await expect(follower.next()).resolves.toMatchObject({
      value: { frame: { type: 'session/event', event: { seq: 5 } } },
    })
    await bridge.call('session.history', { sessionId, beforeSeq: 2, maxMessages: 1 }, 'older', abort.signal)
    expect(f.calls.page).toEqual([{
      address: { kind: 'session', sessionId },
      throughSeq: 4,
      beforeSeq: 2,
      maxMessages: 1,
    }])
    abort.abort()
    bridge.dispose()
  })

  it('soft-falls back when a tool presenter throws', async () => {
    const f = fixture()
    f.context.tools = {
      get: () => ({ presentCall: () => { throw new Error('old presenter') } }),
    }
    f.setFollow([
      snapshot([]),
      {
        type: 'event',
        event: {
          type: 'tool/call', seq: 5, time: 20,
          data: { callId: 'call-1', name: 'bash', arguments: '{}' },
        },
      },
    ])
    const bridge = new TuiHarnessBridge(f.context)
    bridge.attachSession(SessionId('session-1'))
    const abort = new AbortController()
    const follower = bridge.followSession(SessionId('session-1'), abort.signal)[Symbol.asyncIterator]()
    await follower.next()
    const live = await follower.next()
    expect(live.value).toMatchObject({ frame: { type: 'session/event' } })
    expect((live.value as { frame: RecordLike }).frame).not.toHaveProperty('view')
    abort.abort()
    bridge.dispose()
  })
})

describe('TuiHarnessBridge control and interaction streams', () => {
  it('maps control/workspace baselines and host lifecycle events', async () => {
    const f = fixture()
    f.setControl([{
      type: 'baseline',
      value: {
        queues: { 'session-1': [{ id: 'q1' }] },
        jobs: { 'session-1': [{ id: 'j1' }] },
        projections: { 'session-1': { asOfSeq: 7, values: { title: 'Live' } } },
      },
    }])
    f.setWorkspace([{
      type: 'baseline',
      value: {
        items: [{
          workspaceId: 'workspace-1', path: '/work', title: 'Work',
          sessionIds: [SessionId('session-1')], createdAt: '1', updatedAt: '1',
        }],
        archivedSessionIds: [],
      },
    }])
    const bridge = new TuiHarnessBridge(f.context)
    const abort = new AbortController()
    const mux = bridge.muxFrames(abort.signal)[Symbol.asyncIterator]()
    await expect(mux.next()).resolves.toMatchObject({ value: { frame: { type: 'session/queue' } } })
    await expect(mux.next()).resolves.toMatchObject({ value: { frame: { type: 'session/jobs' } } })
    await expect(mux.next()).resolves.toMatchObject({ value: { frame: { type: 'session/projection', seq: 7 } } })
    const host = bridge.hostFrames(abort.signal)[Symbol.asyncIterator]()
    await expect(host.next()).resolves.toMatchObject({ value: { type: 'host/workspace-changed' } })
    f.emit('api-session/status', SessionId('session-1'), true)
    let lifecycle
    do lifecycle = await host.next()
    while ((lifecycle.value as { type?: string }).type !== 'host/session-status')
    expect(lifecycle.value).toMatchObject({ type: 'host/session-status', running: true })
    abort.abort()
    bridge.dispose()
  })

  it('claims only an attached runtime root, replays a stable id, and resolves at most once', async () => {
    const f = fixture()
    const bridge = new TuiHarnessBridge(f.context)
    const abort = new AbortController()
    const mux = bridge.muxFrames(abort.signal)[Symbol.asyncIterator]()
    bridge.attachSession(f.agent.id)
    const delegated = vi.fn(async () => 'unavailable')
    const [pending] = f.emit('approval/request', {
      agent: f.agent,
      toolName: 'bash',
      callId: 'call-1',
      reason: 'destructive command',
    }, delegated) as [Promise<unknown>]
    const requested = await mux.next()
    expect(requested.value).toMatchObject({
      frame: { type: 'approval/requested', sessionId: f.agent.id, toolName: 'bash' },
    })
    const requestId = (requested.value as { requestId: string }).requestId
    expect(delegated).not.toHaveBeenCalled()

    bridge.resetConnection()
    bridge.attachSession(f.agent.id)
    const replayed = await mux.next()
    expect((replayed.value as { requestId: string }).requestId).toBe(requestId)
    await expect(bridge.respond(requestId, { outcome: 'allowed-once' })).resolves.toEqual({ accepted: true })
    await expect(pending).resolves.toBe('allowed-once')
    await expect(bridge.respond(requestId, { outcome: 'rejected' }))
      .resolves.toEqual({ accepted: false, reason: 'not-pending' })
    abort.abort()
    bridge.dispose()
  })

  it('delegates questions outside the attached root and cancels an owned question with its signal', async () => {
    const f = fixture()
    const bridge = new TuiHarnessBridge(f.context)
    const fallback = vi.fn(async () => ({ answers: [] }))
    const foreign = { id: SessionId('foreign') }
    const [delegated] = f.emit('user-questions/request', {
      agent: foreign, questions: [],
    }, fallback) as [Promise<unknown>]
    await expect(delegated).resolves.toEqual({ answers: [] })
    expect(fallback).toHaveBeenCalledOnce()

    bridge.attachSession(f.agent.id)
    const abortStream = new AbortController()
    const mux = bridge.muxFrames(abortStream.signal)[Symbol.asyncIterator]()
    const lifetime = new AbortController()
    const [pending] = f.emit('user-questions/request', {
      agent: f.agent,
      questions: [{ id: 'q1', prompt: 'Choose', options: [] }],
      signal: lifetime.signal,
    }, fallback) as [Promise<unknown>]
    await expect(mux.next()).resolves.toMatchObject({ value: { frame: { type: 'question/requested' } } })
    lifetime.abort()
    await expect(pending).rejects.toThrow(/cancelled/)
    await expect(mux.next()).resolves.toMatchObject({
      value: { frame: { type: 'question/resolved', outcome: 'cancelled' } },
    })
    abortStream.abort()
    bridge.dispose()
  })
})
