#!/usr/bin/env node
/**
 * Protocol-accurate stdio mock for hello, PR4 load barrier, and PR5 paging.
 */
import { createInterface } from 'node:readline'

const sessionId = 'session-mock'
let sessionTitle = 'Mock session'
const events = [
  { seq: 0, time: 1, type: 'turn/start', data: { turn: 1 } },
  {
    seq: 1,
    time: 2,
    type: 'user/message',
    surfaceOp: 'append',
    data: {
      id: 'user-1',
      role: 'user',
      source: { kind: 'user' },
      content: [{ type: 'text', text: 'hello from history' }],
    },
  },
  {
    seq: 2,
    time: 3,
    type: 'assistant/message',
    surfaceOp: 'append',
    data: {
      turn: 1,
      step: 0,
      message: {
        id: 'assistant-1',
        role: 'assistant',
        source: { kind: 'model', provider: 'mock', model: 'mock-1' },
        content: [{ type: 'text', text: 'history is loaded' }],
      },
    },
  },
  { seq: 3, time: 4, type: 'step/end', data: { turn: 1, step: 0 } },
  {
    seq: 4,
    time: 5,
    type: 'tool/call',
    data: {
      name: 'edit',
      callId: 'call-edit-1',
      arguments: JSON.stringify({
        path: 'src/mock.rs',
        old_string: 'old line',
        new_string: 'new line',
      }),
    },
    view: {
      for: 'call',
      view: {
        card: 'diff',
        title: 'Edit src/mock.rs',
        diffs: [{ path: 'src/mock.rs', oldText: 'old line', newText: 'new line' }],
        locations: [{ path: 'src/mock.rs' }],
      },
    },
  },
  {
    seq: 5,
    time: 6,
    type: 'tool/result',
    data: {
      message: {
        source: { callId: 'call-edit-1' },
        content: [{ type: 'text', text: 'edit applied' }],
      },
    },
    view: {
      for: 'result',
      view: {
        card: 'diff',
        diffs: [{ path: 'src/mock.rs', oldText: 'old line', newText: 'new line' }],
      },
    },
  },
]

let tailPushed = false
let approvalPending = false
let questionPending = false
let cancelPending = false
let archivedSessionIds = []
let queue = [
  {
    id: 'queue-1',
    placement: 'queued',
    message: {
      id: 'queue-1',
      role: 'user',
      content: [{ type: 'text', text: 'queued message\nsecond queue line' }],
      source: { kind: 'user' },
    },
  },
  {
    id: 'queue-steer',
    placement: 'steering',
    message: {
      id: 'queue-steer',
      role: 'user',
      content: [{ type: 'text', text: 'steering message' }],
      source: { kind: 'user' },
    },
  },
  {
    id: 'queue-context',
    placement: 'context',
    message: {
      id: 'queue-context',
      role: 'user',
      content: [
        { type: 'image', attachment: { attachmentId: 'mock-image' } },
        { type: 'text', text: 'context note' },
      ],
      source: { kind: 'plugin', plugin: 'mock' },
    },
  },
]

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`)
}

function success(id, value) {
  write({ jsonrpc: '2.0', id, result: { ok: true, value } })
}

function failure(id, code, message) {
  write({ jsonrpc: '2.0', id, result: { ok: false, error: { code, message, details: {} } } })
}

function entry(event) {
  const { view, ...wireEvent } = event
  return view === undefined ? { event: wireEvent } : { event: wireEvent, view }
}

function emitTailOnce() {
  if (tailPushed) return
  tailPushed = true
  write({
    jsonrpc: '2.0',
    method: 'events.mux',
    params: { type: 'session/subscribed', sessionId, lastSeq: 3 },
  })
  for (const event of events.slice(3, 4)) {
    write({
      jsonrpc: '2.0',
      method: 'events.mux',
      params: { type: 'session/event', sessionId, event },
    })
  }
  write({
    jsonrpc: '2.0',
    method: 'events.mux',
    params: { type: 'session/queue', sessionId, items: queue },
  })
}

const rl = createInterface({ input: process.stdin })
rl.on('line', (line) => {
  let message
  try {
    message = JSON.parse(line)
  } catch {
    return
  }
  if (message?.method === 'tui.hello') {
    write({ jsonrpc: '2.0', method: 'tui.serverReady' })
    write({ jsonrpc: '2.0', id: message.id, result: {
      protocolVersion: 1,
      clientId: 'client-mock',
      generation: 1,
      resumeClass: 'baseline-required',
      serverInfo: { name: 'deepseek-harness-tui', version: '0.1.0-test' },
    } })
    return
  }
  if (message?.method === 'session.list') {
    success(message.id, { items: [{
      sessionId,
      updatedAt: 4,
      running: false,
      blank: false,
      cwd: '/work',
      projections: { asOfSeq: 3, values: { title: sessionTitle } },
    }] })
    return
  }
  if (message?.method === 'workspace.list') {
    success(message.id, { items: [], archivedSessionIds })
    return
  }
  if (message?.method === 'session.search') {
    const query = String(message.params?.query ?? '')
    success(message.id, {
      items: query.toLowerCase().includes('history')
        ? [{ sessionId, snippet: 'history is loaded' }]
        : [],
      hasMore: false,
    })
    return
  }
  if (message?.method === 'session.create') {
    success(message.id, { sessionId: 'session-created' })
    return
  }
  if (message?.method === 'workspace.archiveSession') {
    const target = String(message.params?.sessionId ?? '')
    if (!archivedSessionIds.includes(target)) archivedSessionIds.push(target)
    success(message.id, { archivedSessionIds })
    return
  }
  if (message?.method === 'workspace.insertBefore') {
    success(message.id, { workspaceIds: [] })
    return
  }
  if (message?.method === 'workspace.insertSessionBefore') {
    success(message.id, {
      workspace: {
        workspaceId: String(message.params?.workspaceId ?? 'workspace-mock'),
        path: '/work',
        title: 'Mock workspace',
        sessionIds: [String(message.params?.sessionId ?? sessionId)],
        createdAt: new Date(0).toISOString(),
        updatedAt: new Date(0).toISOString(),
      },
    })
    return
  }
  if (message?.method === 'tui.attach' || message?.method === 'tui.subscribe') {
    write({ jsonrpc: '2.0', id: message.id, result: {
      attached: true,
      role: 'driver',
    } })
    return
  }
  if (message?.method === 'session.history') {
    const beforeSeq = message.params?.beforeSeq
    if (beforeSeq !== undefined) {
      success(message.id, { events: [], hasMore: false })
      return
    }
    emitTailOnce()
    success(message.id, {
      events: events.slice(0, 3).map(entry),
      hasMore: false,
      projections: { asOfSeq: 2, values: {} },
    })
    return
  }
  if (message?.method === 'subagent.list') {
    success(message.id, {
      entries: [{
        kind: 'child',
        id: 'child-mock',
        activity: 'inactive',
        mode: 'continuable',
        label: 'mock child',
        hasChildren: false,
      }],
      parentAvailable: true,
    })
    return
  }
  if (message?.method === 'subagent.history') {
    success(message.id, {
      events: events.slice(1, 3).map(entry),
      hasMore: false,
      projections: { asOfSeq: 2, values: {} },
    })
    return
  }
  if (message?.method === 'subagent.prompt') {
    success(message.id, { messageId: 'child-message-mock' })
    return
  }
  if (message?.method === 'subagent.interrupt') {
    success(message.id, { accepted: true })
    return
  }
  if (message?.method === 'session.rename') {
    sessionTitle = String(message.params?.title ?? '').trim()
    write({
      jsonrpc: '2.0',
      method: 'events.mux',
      params: { type: 'session/projection', sessionId, key: 'title', seq: 6, value: sessionTitle },
    })
    success(message.id, { title: sessionTitle, seq: 6 })
    return
  }
  if (message?.method === 'session.fork') {
    success(message.id, { sessionId: 'session-forked' })
    return
  }
  if (message?.method === 'session.prompt') {
    const promptText = message.params?.content?.[0]?.text ?? ''
    if (promptText === 'structured smoke') {
      for (const event of events.slice(4)) {
        write({
          jsonrpc: '2.0',
          method: 'events.mux',
          params: { type: 'session/event', sessionId, event },
        })
      }
      success(message.id, { accepted: true })
      return
    }
    if (promptText === 'cancel smoke') {
      cancelPending = true
      write({
        jsonrpc: '2.0',
        method: 'events.host',
        params: { type: 'host/session-status', sessionId, running: true },
      })
      success(message.id, { accepted: true })
      return
    }
    if (promptText === 'disconnect smoke') {
      console.error('MOCK_DISCONNECT')
      write({
        jsonrpc: '2.0',
        method: 'events.mux',
        params: {
          type: 'stream/error',
          error: { code: 'mock-disconnect', message: 'intentional smoke disconnect', details: {} },
        },
      })
      success(message.id, { accepted: true })
      return
    }
    approvalPending = true
    write({
      jsonrpc: '2.0',
      method: 'events.mux',
      params: {
        type: 'approval/requested',
        sessionId,
        requestId: 'approval-rpc',
        approvalId: 'approval-1',
        toolName: 'mock-tool',
        reason: 'interaction smoke',
      },
    })
    success(message.id, { accepted: true })
    return
  }
  if (message?.method === 'tui.respond') {
    const interaction = message.params?.interaction
    if (interaction?.type === 'approval' && approvalPending) {
      approvalPending = false
      write({
        jsonrpc: '2.0',
        method: 'events.mux',
        params: {
          type: 'approval/resolved',
          sessionId,
          approvalId: 'approval-1',
          outcome: interaction.outcome,
        },
      })
      questionPending = true
      write({
        jsonrpc: '2.0',
        method: 'events.mux',
        params: {
          type: 'question/requested',
          sessionId,
          requestId: 'question-rpc',
          questions: [{
            id: 'q1',
            question: 'Continue?',
            options: [{ label: 'yes' }, { label: 'no' }],
          }],
        },
      })
      write({ jsonrpc: '2.0', id: message.id, result: { accepted: true } })
      return
    }
    if (interaction?.type === 'question' && questionPending) {
      questionPending = false
      write({
        jsonrpc: '2.0',
        method: 'events.mux',
        params: {
          type: 'question/resolved',
          sessionId,
          questionRpcId: 'question-rpc',
          outcome: 'answered',
        },
      })
      write({ jsonrpc: '2.0', id: message.id, result: { accepted: true } })
      return
    }
    write({ jsonrpc: '2.0', id: message.id, result: { accepted: false } })
    return
  }
  if (message?.method === 'session.updateQueue') {
    const itemId = message.params?.itemId
    const action = message.params?.action
    if (!queue.some((item) => item.id === itemId)) {
      success(message.id, { accepted: false })
      return
    }
    if (action?.kind === 'remove' || action?.kind === 'steer') {
      queue = queue.filter((item) => item.id !== itemId)
    } else if (action?.kind === 'edit') {
      queue = queue.map((item) => ({
        ...item,
        message: { ...item.message, content: action.content },
      }))
    }
    write({
      jsonrpc: '2.0',
      method: 'events.mux',
      params: { type: 'session/queue', sessionId, items: queue },
    })
    success(message.id, { accepted: true })
    return
  }
  if (message?.method === 'session.cancel') {
    if (!cancelPending) {
      success(message.id, { accepted: true })
      return
    }
    cancelPending = false
    console.error('MOCK_CANCEL')
    write({
      jsonrpc: '2.0',
      method: 'events.host',
      params: { type: 'host/session-status', sessionId, running: false },
    })
    success(message.id, { accepted: true })
    return
  }
  if (message?.method === 'tui.detach') {
    write({ jsonrpc: '2.0', id: message.id, result: {} })
    return
  }
  failure(message?.id, 'internal', `mock does not implement ${message?.method}`)
})
