import { SessionId } from '@dsh-pager-grok/tui-protocol'
import {
  ApiProxyV1Backend,
  type ApiProxyV1Like,
} from '../../src/adapters/apiproxy-v1/backend.ts'
import { CONFORMANCE_SESSION_ID, WORKSPACE_BASELINE_GOLDEN } from './goldens.ts'
import type {
  AdapterConformanceFixture,
  ConformanceCalls,
  RecordLike,
} from './types.ts'

interface PendingInteraction {
  kind: 'approval' | 'question'
  resolve(value: unknown): void
  reject(error: unknown): void
  sessionId: ReturnType<typeof SessionId>
  approvalId?: string
}

class PushQueue<Value> {
  private readonly values: Value[] = []
  private wake: (() => void) | undefined

  push(value: Value): void {
    this.values.push(value)
    this.wake?.()
  }

  async *read(signal: AbortSignal): AsyncIterable<Value> {
    const abort = (): void => this.wake?.()
    signal.addEventListener('abort', abort, { once: true })
    try {
      while (!signal.aborted) {
        while (this.values.length > 0) yield this.values.shift() as Value
        await new Promise<void>(resolve => { this.wake = resolve })
        this.wake = undefined
      }
    } finally {
      signal.removeEventListener('abort', abort)
    }
  }
}

function deferred(): {
  promise: Promise<unknown>
  resolve(value: unknown): void
  reject(error: unknown): void
} {
  let resolve!: (value: unknown) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<unknown>((ok, fail) => {
    resolve = ok
    reject = fail
  })
  return { promise, resolve, reject }
}

function ok(value: unknown): RecordLike {
  return { ok: true, value }
}

export function createApiProxyV1Fixture(): AdapterConformanceFixture {
  const mux = new PushQueue<{ rpcId: string; payload: unknown }>()
  const host = new PushQueue<{ rpcId: string; payload: unknown }>()
  const pending = new Map<string, PendingInteraction>()
  const agent = { id: CONFORMANCE_SESSION_ID }
  const calls: ConformanceCalls = {
    prompt: [],
    page: [],
    inspect: [],
    resolveAgent: [],
    settings: [],
    credentials: [],
  }
  let opening: RecordLike | undefined
  let workspace: readonly RecordLike[] = [{ type: 'baseline', value: WORKSPACE_BASELINE_GOLDEN.value }]
  let promptMode: 'resolve' | 'hang' = 'resolve'
  let sessionListError: unknown
  let nextRpcId = 0

  const api: ApiProxyV1Like = {
    events: {
      mux: (_request, signal) => mux.read(signal),
      host: (_request, signal) => host.read(signal),
    },
    async respond(message) {
      const interaction = pending.get(message.rpcId)
      if (interaction === undefined) return { accepted: false, reason: 'not-pending' }
      pending.delete(message.rpcId)
      const value = message.result.value as RecordLike
      if (interaction.kind === 'approval') {
        const outcome = value.outcome
        if (outcome !== 'allowed-once' && outcome !== 'rejected') {
          return { accepted: false, reason: 'bad-response' }
        }
        interaction.resolve(outcome)
        mux.push({
          rpcId: message.rpcId,
          payload: {
            type: 'approval/resolved',
            sessionId: interaction.sessionId,
            approvalId: interaction.approvalId,
            outcome,
          },
        })
      } else {
        interaction.resolve(value.answer)
        mux.push({
          rpcId: message.rpcId,
          payload: {
            type: 'question/resolved',
            sessionId: interaction.sessionId,
            questionRpcId: message.rpcId,
            outcome: 'answered',
          },
        })
      }
      return { accepted: true }
    },
  }

  const toFetchHandler = () => ({
    async fetch(request: Request): Promise<Response> {
      const envelope = JSON.parse(await request.text()) as {
        method: string
        payload: RecordLike
      }
      const { method, payload } = envelope
      let result: RecordLike
      if (method === 'session.list') {
        if (sessionListError !== undefined) throw sessionListError
        result = ok({
          items: [{
            sessionId: CONFORMANCE_SESSION_ID,
            cwd: '/work',
            running: false,
            blank: false,
            agentPreset: 'standard',
            projections: { asOfSeq: 3, values: { agentPreset: 'standard' } },
          }],
        })
      } else if (method === 'session.search') {
        result = ok({ items: [], query: payload.query })
      } else if (method === 'session.create') {
        result = ok({ sessionId: CONFORMANCE_SESSION_ID })
      } else if (method === 'session.history') {
        result = ok(historyValue(payload, opening, calls))
      } else if (method === 'session.models') {
        result = ok({
          current: { provider: 'deepseek', model: 'deepseek-chat' },
          routable: true,
          groups: [{ provider: 'deepseek', models: [{ id: 'deepseek-chat' }] }],
          failures: [],
        })
      } else if (method === 'session.prompt') {
        calls.prompt.push(payload)
        if (promptMode === 'hang') {
          await new Promise<void>((_resolve, reject) => {
            if (request.signal.aborted) {
              reject(request.signal.reason)
              return
            }
            request.signal.addEventListener('abort', () => reject(request.signal.reason), { once: true })
          })
        }
        result = ok({ accepted: true })
      } else if (method === 'workspace.list') {
        const baseline = workspace.find(frame => frame.type === 'baseline')
        result = ok((baseline?.value as RecordLike | undefined) ?? { items: [], archivedSessionIds: [] })
      } else if (method === 'workspace.archiveSession') {
        result = ok({ archived: true })
      } else if (method === 'settings.describe') {
        result = ok({ namespaces: [{ ns: 'llm', revision: 1 }] })
      } else if (method === 'settings.update') {
        calls.settings.push({ kind: 'update', value: payload })
        result = ok({ revision: 2, value: payload.patch })
      } else if (method === 'credentials.describe') {
        const refs = Array.isArray(payload.refs) ? payload.refs : []
        result = ok({
          credentials: Object.fromEntries(refs.map(ref => [String(ref), { configured: false, writable: true }])),
        })
      } else if (method === 'credentials.set') {
        calls.credentials.push({ kind: 'set', value: payload })
        result = ok({})
      } else if (method === 'credentials.unset') {
        calls.credentials.push({ kind: 'unset', value: payload })
        result = ok({})
      } else if (method === 'agentPreset.list') {
        result = ok({ presets: [{ id: 'standard' }], authorable: true, hasDocument: true })
      } else if (method === 'goal.create') {
        result = ok({ ref: { id: 'goal-1', revision: 1 } })
      } else if (method === 'subagent.list') {
        result = ok({ entries: [{ childSessionId: SessionId('child-1') }], parentAvailable: true })
      } else if (method === 'skill.list') {
        result = ok({ entries: [{ name: 'review' }] })
      } else if (method === 'host.pickDirectory') {
        result = ok({ path: '/work' })
      } else {
        result = ok(payload)
      }
      return new Response(JSON.stringify({ type: 'server-response', result }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      })
    },
  })

  const backend = new ApiProxyV1Backend({
    api,
    dshVersion: '0.1.1-rc.2',
    toFetchHandler,
    extensions: {
      resolveAgent: async (sessionId) => {
        calls.resolveAgent.push(String(sessionId))
        return { agent }
      },
      fileReferences: {
        list: async () => [{ path: 'src/main.ts', kind: 'file' }],
      },
      commands: {
        list: () => [{ name: 'compact', description: 'Compact history' }],
        execute: async () => ({ commandId: 'command-1', result: { kind: 'success', text: 'ok' } }),
      },
    },
  })

  return {
    backend,
    sessionId: CONFORMANCE_SESSION_ID,
    agent,
    calls,
    setSessionFollow(frames) {
      opening = frames.find(frame => frame.type === 'snapshot')
      for (const frame of frames) {
        if (frame.type === 'snapshot') {
          mux.push({
            rpcId: `session-open-${String(++nextRpcId)}`,
            payload: {
              type: 'session/subscribed',
              sessionId: CONFORMANCE_SESSION_ID,
              lastSeq: frame.cursor,
            },
          })
        } else if (frame.type === 'event') {
          mux.push({
            rpcId: `session-event-${String(++nextRpcId)}`,
            payload: {
              type: 'session/event',
              sessionId: CONFORMANCE_SESSION_ID,
              event: frame.event,
            },
          })
        }
      }
    },
    sessionFrames(signal) {
      return backend.muxFrames(signal)[Symbol.asyncIterator]()
    },
    setControl(frames) {
      for (const frame of frames) pushControlBaseline(mux, frame, () => String(++nextRpcId))
    },
    setWorkspace(frames) {
      workspace = frames
      for (const frame of frames) pushWorkspace(host, frame, () => String(++nextRpcId))
    },
    setPromptMode(mode) { promptMode = mode },
    failSessionList(error) { sessionListError = error },
    emit(event, ...args) {
      if (event !== 'approval/request' && event !== 'user-questions/request') return []
      const request = args[0] as RecordLike
      const settled = deferred()
      const rpcId = `${event === 'approval/request' ? 'approval' : 'question'}-${String(++nextRpcId)}`
      const interaction: PendingInteraction = {
        kind: event === 'approval/request' ? 'approval' : 'question',
        resolve: settled.resolve,
        reject: settled.reject,
        sessionId: CONFORMANCE_SESSION_ID,
        ...event === 'approval/request' ? { approvalId: rpcId } : {},
      }
      pending.set(rpcId, interaction)
      const payload = event === 'approval/request'
        ? {
          type: 'approval/requested',
          sessionId: CONFORMANCE_SESSION_ID,
          approvalId: rpcId,
          toolName: request.toolName,
          ...typeof request.callId === 'string' ? { callId: request.callId } : {},
          ...typeof request.reason === 'string' ? { reason: request.reason } : {},
        }
        : {
          type: 'question/requested',
          sessionId: CONFORMANCE_SESSION_ID,
          questions: request.questions,
        }
      mux.push({ rpcId, payload })
      if (event === 'approval/request') mux.push({ rpcId, payload })
      const signal = request.signal instanceof AbortSignal ? request.signal : undefined
      signal?.addEventListener('abort', () => {
        if (!pending.delete(rpcId)) return
        interaction.reject(new Error('user question was cancelled'))
        mux.push({
          rpcId,
          payload: {
            type: 'question/resolved',
            sessionId: CONFORMANCE_SESSION_ID,
            questionRpcId: rpcId,
            outcome: 'cancelled',
          },
        })
      }, { once: true })
      return [settled.promise]
    },
  }
}

function historyValue(
  params: RecordLike,
  opening: RecordLike | undefined,
  calls: ConformanceCalls,
): RecordLike {
  if (opening === undefined) {
    calls.inspect.push(String(params.sessionId))
    return {
      events: [
        { event: { type: 'user/message', seq: 0, time: 1, data: { content: [] } } },
        { event: { type: 'assistant/message', seq: 1, time: 2, data: { content: [] } } },
      ],
      hasMore: false,
      projections: { asOfSeq: 1, values: {} },
    }
  }
  const cursor = typeof opening.cursor === 'number' ? opening.cursor : 0
  if (params.beforeSeq !== undefined) {
    calls.page.push({
      address: { kind: 'session', sessionId: params.sessionId },
      throughSeq: cursor,
      beforeSeq: params.beforeSeq,
      ...params.maxMessages === undefined ? {} : { maxMessages: params.maxMessages },
    })
    return {
      events: [{ event: { type: 'user/message', seq: 0, time: 1, data: { content: [] } } }],
      hasMore: false,
    }
  }
  const records = Array.isArray(opening.records) ? opening.records as RecordLike[] : []
  return {
    events: records.flatMap(historyEntries),
    hasMore: opening.hasMore === true,
    projections: opening.projections,
  }
}

function historyEntries(record: RecordLike): Array<{ event: RecordLike }> {
  if (record.type === 'event') return [{ event: record.event as RecordLike }]
  const event = record.event as RecordLike
  const data = event.data as RecordLike
  const texts = Array.isArray(data.texts) ? data.texts : []
  const deltas = Array.isArray(data.dt) ? data.dt : []
  let time = typeof event.time === 'number' ? event.time : 0
  return texts.map((text, index) => {
    if (index > 0) time += typeof deltas[index - 1] === 'number' ? deltas[index - 1] as number : 0
    return {
      event: {
        type: 'assistant/chunk',
        seq: (typeof event.seq === 'number' ? event.seq : 0) + index,
        time,
        data: {
          turn: data.turn,
          step: data.step,
          chunk: { type: 'text-delta', index: 0, text },
        },
      },
    }
  })
}

function pushControlBaseline(
  queue: PushQueue<{ rpcId: string; payload: unknown }>,
  frame: RecordLike,
  nextId: () => string,
): void {
  const value = frame.value as RecordLike
  for (const [sessionId, items] of Object.entries((value.queues as RecordLike | undefined) ?? {})) {
    queue.push({ rpcId: `queue-${nextId()}`, payload: { type: 'session/queue', sessionId, items } })
  }
  for (const [sessionId, jobs] of Object.entries((value.jobs as RecordLike | undefined) ?? {})) {
    queue.push({ rpcId: `jobs-${nextId()}`, payload: { type: 'session/jobs', sessionId, jobs } })
  }
  for (const [sessionId, block] of Object.entries((value.projections as RecordLike | undefined) ?? {})) {
    const baseline = block as RecordLike
    for (const [key, projection] of Object.entries((baseline.values as RecordLike | undefined) ?? {})) {
      queue.push({
        rpcId: `projection-${nextId()}`,
        payload: { type: 'session/projection', sessionId, key, value: projection, seq: baseline.asOfSeq },
      })
    }
  }
}

function pushWorkspace(
  queue: PushQueue<{ rpcId: string; payload: unknown }>,
  frame: RecordLike,
  nextId: () => string,
): void {
  if (frame.type === 'baseline') {
    const value = frame.value as { items: RecordLike[]; archivedSessionIds: unknown[] }
    for (const workspace of value.items) {
      queue.push({ rpcId: `workspace-${nextId()}`, payload: { type: 'host/workspace-changed', workspace } })
    }
    queue.push({
      rpcId: `workspace-order-${nextId()}`,
      payload: { type: 'host/workspace-order-changed', workspaceIds: value.items.map(row => row.workspaceId) },
    })
    queue.push({
      rpcId: `workspace-archived-${nextId()}`,
      payload: { type: 'host/archived-sessions-changed', archivedSessionIds: value.archivedSessionIds },
    })
  } else if (frame.type === 'upsert') {
    queue.push({ rpcId: `workspace-${nextId()}`, payload: { type: 'host/workspace-changed', workspace: frame.workspace } })
  } else if (frame.type === 'order') {
    queue.push({ rpcId: `workspace-order-${nextId()}`, payload: { type: 'host/workspace-order-changed', workspaceIds: frame.workspaceIds } })
  } else if (frame.type === 'archived') {
    queue.push({ rpcId: `workspace-archived-${nextId()}`, payload: { type: 'host/archived-sessions-changed', archivedSessionIds: frame.archivedSessionIds } })
  }
}
