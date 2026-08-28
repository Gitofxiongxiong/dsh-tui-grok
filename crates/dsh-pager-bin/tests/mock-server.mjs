#!/usr/bin/env node
/**
 * Protocol-accurate stdio mock for hello, PR4 load barrier, and PR5 paging.
 */
import { createInterface } from 'node:readline'

const sessionId = 'session-mock'
let sessionTitle = 'Mock session'
const PERMISSION_PRESETS = ['workspace-write', 'danger-full-access']
let planActive = false
let currentPermission = 'workspace-write'
let controlsSeq = 20
let scrollStreamTimer = null
let scrollStreamSeq = 3

function controlProjections() {
  return {
    plan: { active: planActive, pending: false },
    permissions: {
      currentValue: currentPermission,
      options: PERMISSION_PRESETS.map((value) => ({ value, name: value })),
    },
  }
}

function emitControlProjections() {
  const values = controlProjections()
  controlsSeq += 1
  write({
    jsonrpc: '2.0',
    method: 'events.mux',
    params: { type: 'session/projection', sessionId, key: 'plan', seq: controlsSeq, value: values.plan },
  })
  controlsSeq += 1
  write({
    jsonrpc: '2.0',
    method: 'events.mux',
    params: {
      type: 'session/projection',
      sessionId,
      key: 'permissions',
      seq: controlsSeq,
      value: values.permissions,
    },
  })
}

function emitSessionEvent(event) {
  write({
    jsonrpc: '2.0',
    method: 'events.mux',
    params: { type: 'session/event', sessionId, event },
  })
}

function startScrollStream() {
  if (scrollStreamTimer !== null) return
  const markerText = Array.from(
    { length: 240 },
    (_, index) =>
      `SCROLL-MARKER-${String(index).padStart(4, '0')} ${'x'.repeat(72)}  `,
  ).join('\n')

  scrollStreamSeq += 1
  emitSessionEvent({ seq: scrollStreamSeq, time: Date.now(), type: 'turn/start', data: { turn: 2 } })
  scrollStreamSeq += 1
  emitSessionEvent({
    seq: scrollStreamSeq,
    time: Date.now(),
    type: 'user/message',
    surfaceOp: 'append',
    data: {
      id: 'user-scroll-smoke',
      role: 'user',
      source: { kind: 'user' },
      content: [{ type: 'text', text: 'stream scroll smoke' }],
    },
  })
  scrollStreamSeq += 1
  emitSessionEvent({
    seq: scrollStreamSeq,
    time: Date.now(),
    type: 'assistant/chunk',
    data: {
      turn: 2,
      step: 0,
      chunk: { type: 'text-delta', index: 0, text: `${markerText}\nTAIL-LIVE-0000\n` },
    },
  })
  write({
    jsonrpc: '2.0',
    method: 'events.host',
    params: { type: 'host/session-status', sessionId, running: true },
  })

  let tick = 0
  scrollStreamTimer = setInterval(() => {
    tick += 1
    scrollStreamSeq += 1
    emitSessionEvent({
      seq: scrollStreamSeq,
      time: Date.now(),
      type: 'assistant/chunk',
      data: {
        turn: 2,
        step: 0,
        chunk: {
          type: 'text-delta',
          index: 0,
          text: `TAIL-LIVE-${String(tick).padStart(4, '0')} streamed payload row\n`,
        },
      },
    })
    if (tick < 2000) return
    clearInterval(scrollStreamTimer)
    scrollStreamTimer = null
    scrollStreamSeq += 1
    emitSessionEvent({
      seq: scrollStreamSeq,
      time: Date.now(),
      type: 'step/end',
      data: { turn: 2, step: 0 },
    })
    write({
      jsonrpc: '2.0',
      method: 'events.host',
      params: { type: 'host/session-status', sessionId, running: false },
    })
  }, 30)
}
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
  const now = Date.now()
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
  write({
    jsonrpc: '2.0',
    method: 'events.mux',
    params: {
      type: 'session/projection',
      sessionId,
      key: 'capabilities',
      seq: 4,
      value: { subagents: true },
    },
  })
  write({
    jsonrpc: '2.0',
    method: 'events.host',
    params: {
      generation: 1,
      type: 'host/session-added',
      sessionId: 'child-mock',
      parentSessionId: sessionId,
      origin: 'subagent',
      blank: false,
    },
  })
  write({
    jsonrpc: '2.0',
    method: 'events.host',
    params: {
      generation: 1,
      type: 'host/session-status',
      sessionId: 'child-mock',
      running: true,
    },
  })
  write({
    jsonrpc: '2.0',
    method: 'events.mux',
    params: {
      type: 'session/jobs',
      sessionId,
      jobs: [
        {
          id: 'job-long',
          kind: 'command',
          label: 'Long background demo',
          status: 'running',
          detail: 'sleep 90',
          startedAt: now - 15_000,
        },
        {
          id: 'job-stopping',
          kind: 'bash',
          label: 'Stopping background demo',
          status: 'stopping',
          detail: 'signal: SIGTERM',
          startedAt: now - 6_000,
        },
        {
          id: 'job-killed',
          kind: 'bash',
          label: 'Killed background demo',
          status: 'killed',
          detail: 'signal: SIGTERM',
          startedAt: now - 3_000,
          finishedAt: now - 1_000,
        },
        {
          id: 'job-completed',
          kind: 'bash',
          label: 'Completed background demo',
          status: 'completed',
          detail: 'exit code: 0',
          startedAt: now - 20_000,
          finishedAt: now - 5_000,
        },
      ],
    },
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
      projections: { asOfSeq: 2, values: controlProjections() },
    })
    return
  }
  if (message?.method === 'subagent.list') {
    success(message.id, {
      entries: [
        {
          kind: 'child',
          id: 'child-mock',
          activity: 'running',
          mode: 'continuable',
          label: 'mock child',
          hasChildren: false,
        },
        {
          kind: 'child',
          id: 'child-settled',
          activity: 'inactive',
          mode: 'one-shot',
          label: 'settled one-shot',
          hasChildren: false,
        },
      ],
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
  if (message?.method === 'commands/list') {
    success(message.id, [
      { name: 'compact', description: 'Compact older conversation history' },
      {
        name: 'goal',
        description: 'set or view the goal for a long-running task',
        input: { hint: '[<objective>|clear|edit <objective>|pause|resume]', images: true },
      },
      {
        name: 'permission',
        description: 'Switch the permission preset (sandbox mode + approval policy)',
        input: { hint: '<preset>' },
      },
      {
        name: 'plan',
        description: 'Enter or leave plan mode',
        input: { hint: '[off|message]', images: true },
      },
    ])
    return
  }
  if (message?.method === 'session.prompt') {
    const promptText = message.params?.content?.[0]?.text ?? ''
    if (promptText === '/permission') {
      success(message.id, {
        accepted: true,
        command: {
          kind: 'success',
          text: `current preset ${currentPermission} (available: ${PERMISSION_PRESETS.join(', ')})`,
        },
      })
      return
    }
    if (promptText.startsWith('/permission ')) {
      const preset = promptText.slice('/permission '.length).trim()
      if (!PERMISSION_PRESETS.includes(preset)) {
        failure(message.id, -32602, `unknown permission preset: ${preset}`)
        return
      }
      currentPermission = preset
      emitControlProjections()
      success(message.id, {
        accepted: true,
        command: { kind: 'success', text: `preset ${currentPermission}` },
      })
      return
    }
    if (promptText === '/plan' || promptText.startsWith('/plan ')) {
      planActive = promptText === '/plan off' ? false : true
      emitControlProjections()
      success(message.id, {
        accepted: true,
        command: {
          kind: 'success',
          text: planActive ? 'Plan mode on. Use /plan off to leave.' : 'Plan mode off.',
        },
      })
      return
    }
    if (promptText === 'stream scroll smoke') {
      startScrollStream()
      success(message.id, { accepted: true })
      return
    }
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
