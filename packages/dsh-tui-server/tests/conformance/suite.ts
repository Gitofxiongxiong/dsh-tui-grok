import { describe, expect, it, vi } from 'vitest'
import { SessionId } from '@dsh-pager-grok/tui-protocol'
import {
  SESSION_CREATE_GOLDEN,
  SESSION_LIST_GOLDEN,
  SESSION_SEARCH_GOLDEN,
  WORKSPACE_BASELINE_GOLDEN,
} from './goldens.ts'
import type { AdapterConformanceFactory, RecordLike } from './types.ts'

function openingSnapshot(records: readonly RecordLike[], cursor = 4): RecordLike {
  return {
    type: 'snapshot',
    header: { id: SessionId('session-1') },
    cursor,
    records,
    hasMore: true,
    projections: { asOfSeq: cursor, values: { title: 'Opening' } },
  }
}

/** Register the stable TUI behavior required from every DSH adapter family. */
export function registerAdapterConformance(
  family: string,
  factory: AdapterConformanceFactory,
): void {
  describe(`${family} adapter conformance`, () => {
    it('1. maps session list/search/create and cold history DTOs', async () => {
      const f = factory()
      const signal = new AbortController().signal
      try {
        await expect(f.backend.call('session.list', {}, 'list', signal)).resolves.toEqual(SESSION_LIST_GOLDEN)
        await expect(f.backend.call('session.search', { query: 'needle' }, 'search', signal))
          .resolves.toEqual(SESSION_SEARCH_GOLDEN)
        await expect(f.backend.call('session.create', { cwd: '/work' }, 'create', signal))
          .resolves.toEqual(SESSION_CREATE_GOLDEN)
        await expect(f.backend.call('session.history', {
          sessionId: f.sessionId,
          maxMessages: 10,
        }, 'history', signal)).resolves.toEqual({
          ok: true,
          value: {
            events: [
              { event: { type: 'user/message', seq: 0, time: 1, data: { content: [] } } },
              { event: { type: 'assistant/message', seq: 1, time: 2, data: { content: [] } } },
            ],
            hasMore: false,
            projections: { asOfSeq: 1, values: {} },
          },
        })
        expect(f.calls.inspect).toEqual(['session-1'])
      } finally {
        f.backend.dispose()
      }
    })

    it('2. binds opening history and old pages to the opening cursor', async () => {
      const f = factory()
      f.setSessionFollow([openingSnapshot([{
        type: 'chunks',
        event: {
          type: 'chunkrow/text-chunks',
          seq: 2,
          time: 10,
          data: { turn: 1, step: 0, index: 0, dt: [1, 2], texts: ['a', 'b', 'c'] },
        },
      }], 4)])
      const abort = new AbortController()
      f.backend.attachSession(f.sessionId)
      const follower = f.backend.followSession(f.sessionId, abort.signal)[Symbol.asyncIterator]()
      try {
        await expect(follower.next()).resolves.toMatchObject({
          value: { frame: { type: 'session/subscribed', sessionId: f.sessionId, lastSeq: 4 } },
        })
        await expect(f.backend.call('session.history', { sessionId: f.sessionId }, 'opening', abort.signal))
          .resolves.toEqual({
            ok: true,
            value: {
              events: [
                { event: { type: 'assistant/chunk', seq: 2, time: 10, data: { turn: 1, step: 0, chunk: { type: 'text-delta', index: 0, text: 'a' } } } },
                { event: { type: 'assistant/chunk', seq: 3, time: 11, data: { turn: 1, step: 0, chunk: { type: 'text-delta', index: 0, text: 'b' } } } },
                { event: { type: 'assistant/chunk', seq: 4, time: 13, data: { turn: 1, step: 0, chunk: { type: 'text-delta', index: 0, text: 'c' } } } },
              ],
              hasMore: true,
              projections: { asOfSeq: 4, values: { title: 'Opening' } },
            },
          })
        await f.backend.call('session.history', {
          sessionId: f.sessionId,
          beforeSeq: 2,
          maxMessages: 1,
        }, 'old-page', abort.signal)
        expect(f.calls.page).toEqual([{
          address: { kind: 'session', sessionId: f.sessionId },
          throughSeq: 4,
          beforeSeq: 2,
          maxMessages: 1,
        }])
      } finally {
        abort.abort()
        f.backend.dispose()
      }
    })

    it('3. preserves the history/live barrier without loss or duplication', async () => {
      const f = factory()
      f.setSessionFollow([
        openingSnapshot([{
          type: 'event',
          event: { type: 'assistant/message', seq: 4, time: 10, data: { content: [] } },
        }], 4),
        { type: 'event', event: { type: 'turn/end', seq: 5, time: 11, data: { turn: 1 } } },
      ])
      const abort = new AbortController()
      f.backend.attachSession(f.sessionId)
      const follower = f.backend.followSession(f.sessionId, abort.signal)[Symbol.asyncIterator]()
      try {
        const history = f.backend.call('session.history', { sessionId: f.sessionId }, 'barrier', abort.signal)
        await expect(follower.next()).resolves.toMatchObject({ value: { frame: { lastSeq: 4 } } })
        await expect(history).resolves.toMatchObject({
          ok: true,
          value: { events: [{ event: { seq: 4 } }] },
        })
        await expect(follower.next()).resolves.toMatchObject({
          value: { frame: { type: 'session/event', event: { seq: 5 } } },
        })
      } finally {
        abort.abort()
        f.backend.dispose()
      }
    })

    it('4. preserves prompt request identity and cancellation', async () => {
      const f = factory()
      const signal = new AbortController().signal
      try {
        await expect(f.backend.call('session.prompt', {
          sessionId: f.sessionId,
          mode: 'queue',
          content: [],
        }, 'carrier', signal)).resolves.toMatchObject({ ok: false, error: { code: 'bad-request' } })
        const params = {
          sessionId: f.sessionId,
          requestId: 'operation-9',
          mode: 'queue',
          content: [{ type: 'text', text: 'hello' }],
        }
        await expect(f.backend.call('session.prompt', params, 'carrier', signal))
          .resolves.toEqual({ ok: true, value: { accepted: true } })
        expect(f.calls.prompt).toEqual([params])

        f.setPromptMode('hang')
        const cancelled = new AbortController()
        const pending = f.backend.call('session.prompt', params, 'cancelled', cancelled.signal)
        cancelled.abort(new Error('cancelled by test'))
        await expect(pending).resolves.toMatchObject({ ok: false, error: { code: 'cancelled' } })
      } finally {
        f.backend.dispose()
      }
    })

    it('5. maps queue, jobs, and projection control frames', async () => {
      const f = factory()
      f.setControl([{
        type: 'baseline',
        value: {
          queues: { 'session-1': [{ id: 'q1' }] },
          jobs: { 'session-1': [{ id: 'j1' }] },
          projections: { 'session-1': { asOfSeq: 7, values: { title: 'Live' } } },
        },
      }])
      const abort = new AbortController()
      const mux = f.backend.muxFrames(abort.signal)[Symbol.asyncIterator]()
      try {
        await expect(mux.next()).resolves.toMatchObject({
          value: { frame: { type: 'session/queue', sessionId: f.sessionId, items: [{ id: 'q1' }] } },
        })
        await expect(mux.next()).resolves.toMatchObject({
          value: { frame: { type: 'session/jobs', sessionId: f.sessionId, jobs: [{ id: 'j1' }] } },
        })
        await expect(mux.next()).resolves.toMatchObject({
          value: { frame: { type: 'session/projection', key: 'title', value: 'Live', seq: 7 } },
        })
      } finally {
        abort.abort()
        f.backend.dispose()
      }
    })

    it('6. claims, replays, resolves, and aborts waterfall interactions', async () => {
      const f = factory()
      const abort = new AbortController()
      const mux = f.backend.muxFrames(abort.signal)[Symbol.asyncIterator]()
      f.backend.attachSession(f.sessionId)
      try {
        const delegated = vi.fn(async () => 'delegated')
        const [approval] = f.emit('approval/request', {
          agent: f.agent,
          toolName: 'bash',
          callId: 'call-1',
          reason: 'confirm',
        }, delegated) as [Promise<unknown>]
        const requested = await mux.next()
        expect(requested.value).toMatchObject({
          frame: { type: 'approval/requested', sessionId: f.sessionId, toolName: 'bash' },
        })
        const requestId = (requested.value as { requestId: string }).requestId
        f.backend.resetConnection()
        f.backend.attachSession(f.sessionId)
        await expect(mux.next()).resolves.toMatchObject({ value: { requestId } })
        await expect(f.backend.respond(requestId, { outcome: 'allowed-once' }))
          .resolves.toEqual({ accepted: true })
        await expect(approval).resolves.toBe('allowed-once')
        await expect(mux.next()).resolves.toMatchObject({
          value: { frame: { type: 'approval/resolved', outcome: 'allowed-once' } },
        })
        await expect(f.backend.respond(requestId, { outcome: 'rejected' }))
          .resolves.toEqual({ accepted: false, reason: 'not-pending' })
        expect(delegated).not.toHaveBeenCalled()

        const lifetime = new AbortController()
        const [question] = f.emit('user-questions/request', {
          agent: f.agent,
          questions: [{ id: 'q1', prompt: 'Choose', options: [] }],
          signal: lifetime.signal,
        }, delegated) as [Promise<unknown>]
        await expect(mux.next()).resolves.toMatchObject({ value: { frame: { type: 'question/requested' } } })
        lifetime.abort()
        await expect(question).rejects.toThrow(/cancelled/)
        await expect(mux.next()).resolves.toMatchObject({
          value: { frame: { type: 'question/resolved', outcome: 'cancelled' } },
        })
      } finally {
        abort.abort()
        f.backend.dispose()
      }
    })

    it('7. maps workspace baseline, follow order/archive, and mutation DTOs', async () => {
      const f = factory()
      f.setWorkspace([
        { type: 'baseline', value: WORKSPACE_BASELINE_GOLDEN.value },
        {
          type: 'upsert',
          workspace: {
            workspaceId: 'workspace-1', path: '/work', title: 'Renamed',
            sessionIds: [f.sessionId], createdAt: '1', updatedAt: '2',
          },
        },
        { type: 'order', workspaceIds: ['workspace-1'] },
        { type: 'archived', archivedSessionIds: [f.sessionId] },
      ])
      const signal = new AbortController().signal
      await expect(f.backend.call('workspace.list', {}, 'workspaces', signal))
        .resolves.toEqual(WORKSPACE_BASELINE_GOLDEN)
      const abort = new AbortController()
      const host = f.backend.hostFrames(abort.signal)[Symbol.asyncIterator]()
      try {
        await expect(host.next()).resolves.toMatchObject({ value: { type: 'host/workspace-changed' } })
        await expect(host.next()).resolves.toMatchObject({ value: { type: 'host/workspace-order-changed' } })
        await expect(host.next()).resolves.toMatchObject({ value: { type: 'host/archived-sessions-changed' } })
        await expect(host.next()).resolves.toMatchObject({
          value: { type: 'host/workspace-changed', workspace: { title: 'Renamed' } },
        })
        await expect(host.next()).resolves.toMatchObject({
          value: { type: 'host/workspace-order-changed', workspaceIds: ['workspace-1'] },
        })
        await expect(host.next()).resolves.toMatchObject({
          value: { type: 'host/archived-sessions-changed', archivedSessionIds: [f.sessionId] },
        })
        await expect(f.backend.call('workspace.archiveSession', {
          sessionId: f.sessionId,
          archived: true,
        }, 'archive', signal)).resolves.toEqual({ ok: true, value: { archived: true } })
      } finally {
        abort.abort()
        f.backend.dispose()
      }
    })

    it('8. maps settings and credentials without exposing secret reads', async () => {
      const f = factory()
      const signal = new AbortController().signal
      try {
        await expect(f.backend.call('settings.describe', {}, 'settings', signal)).resolves.toEqual({
          ok: true, value: { namespaces: [{ ns: 'llm', revision: 1 }] },
        })
        await expect(f.backend.call('settings.update', {
          ns: 'llm', patch: { model: 'deepseek-chat' }, expectedRevision: 1,
        }, 'settings-update', signal)).resolves.toMatchObject({ ok: true, value: { revision: 2 } })
        await expect(f.backend.call('credentials.describe', {
          refs: ['DEEPSEEK_API_KEY'],
        }, 'credentials', signal)).resolves.toEqual({
          ok: true,
          value: { credentials: { DEEPSEEK_API_KEY: { configured: false, writable: true } } },
        })
        await f.backend.call('credentials.set', {
          ref: 'DEEPSEEK_API_KEY', value: 'redacted-test-value',
        }, 'credential-set', signal)
        await f.backend.call('credentials.unset', {
          ref: 'DEEPSEEK_API_KEY',
        }, 'credential-unset', signal)
        expect(f.calls.credentials.map(call => call.kind)).toEqual(['set', 'unset'])
      } finally {
        f.backend.dispose()
      }
    })

    it('9. maps model, preset, goal, and subagent results', async () => {
      const f = factory()
      const signal = new AbortController().signal
      try {
        await expect(f.backend.call('session.models', {
          sessionId: f.sessionId,
        }, 'models', signal)).resolves.toMatchObject({
          ok: true,
          value: { current: { provider: 'deepseek', model: 'deepseek-chat' }, routable: true },
        })
        await expect(f.backend.call('agentPreset.list', {}, 'presets', signal)).resolves.toEqual({
          ok: true,
          value: { presets: [{ id: 'standard' }], authorable: true, hasDocument: true },
        })
        await expect(f.backend.call('goal.create', {
          sessionId: f.sessionId, objective: 'Ship it',
        }, 'goal', signal)).resolves.toEqual({ ok: true, value: { id: 'goal-1', revision: 1 } })
        await expect(f.backend.call('subagent.list', {
          parentSessionId: f.sessionId,
        }, 'subagents', signal)).resolves.toEqual({
          ok: true,
          value: { entries: [{ childSessionId: SessionId('child-1') }], parentAvailable: true },
        })
      } finally {
        f.backend.dispose()
      }
    })

    it('10. maps file references, directory picker, skills, and commands', async () => {
      const f = factory()
      const signal = new AbortController().signal
      try {
        await expect(f.backend.call('fileReferences.list', {
          sessionId: f.sessionId, query: 'main',
        }, 'files', signal)).resolves.toEqual({
          ok: true, value: { items: [{ path: 'src/main.ts', kind: 'file' }] },
        })
        await expect(f.backend.call('host.pickDirectory', {}, 'pick', signal))
          .resolves.toEqual({ ok: true, value: { path: '/work' } })
        await expect(f.backend.call('skill.list', {
          sessionId: f.sessionId,
        }, 'skills', signal)).resolves.toEqual({
          ok: true, value: { entries: [{ name: 'review' }] },
        })
        await expect(f.backend.call('commands/list', {
          agentId: f.sessionId,
        }, 'commands', signal)).resolves.toEqual({
          ok: true, value: [{ name: 'compact', description: 'Compact history' }],
        })
        await expect(f.backend.call('commands/execute', {
          agentId: f.sessionId, line: '/compact', images: [],
        }, 'execute', signal)).resolves.toEqual({
          ok: true,
          value: {
            matched: true,
            execution: { commandId: 'command-1', result: { kind: 'success', text: 'ok' } },
          },
        })
      } finally {
        f.backend.dispose()
      }
    })

    it('11. normalizes upstream error codes into stable API errors', async () => {
      const f = factory()
      f.failSessionList({ code: 'GOAL_STALE', message: 'stale goal' })
      try {
        await expect(f.backend.call('session.list', {}, 'error', new AbortController().signal))
          .resolves.toEqual({
            ok: false,
            error: { code: 'internal', message: '[object Object]', details: { goalCode: 'GOAL_STALE' } },
          })
      } finally {
        f.backend.dispose()
      }
    })

    it.skip('12. capability-missing semantics belong to Phase 5 core enforcement', () => {
      // controllers-v2 advertises every current capability as true. A false
      // capability family registers this scenario when core enforcement exists.
    })
  })
}
