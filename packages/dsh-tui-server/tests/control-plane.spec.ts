import { describe, expect, it } from 'vitest'
import { SessionId, type HostFrame, type MuxFrame } from '@dsh-pager-grok/tui-protocol'
import { ControlPlaneRouter, ControlPlaneStore } from '../src/core/control-plane.ts'

type WorkspaceId = string

const a = SessionId('session-a')
const b = SessionId('session-b')

const eventFrame = (seq: number): MuxFrame => ({
  type: 'session/event', sessionId: a,
  event: { type: 'assistant/message', seq, time: seq, data: {} },
} as MuxFrame)

describe('ControlPlaneStore', () => {
  it('keeps interleaved sessions isolated and deduplicates sequence/snapshot replay', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(3)
    const projectionA: MuxFrame = {
      type: 'session/projection', sessionId: a, key: 'title', seq: 4, value: 'A',
    }
    const projectionB: MuxFrame = {
      type: 'session/projection', sessionId: b, key: 'title', seq: 2, value: 'B',
    }
    expect(store.applyMux(projectionA, 3).accepted).toBe(true)
    expect(store.applyMux(projectionB, 3).accepted).toBe(true)
    expect(store.applyMux(projectionA, 3).duplicate).toBe(true)
    expect(store.snapshot(a)?.projections.title?.value).toBe('A')
    expect(store.snapshot(b)?.projections.title?.value).toBe('B')

    const queue: MuxFrame = { type: 'session/queue', sessionId: a, items: [] }
    expect(store.applyMux(queue, 3).duplicate).toBe(false)
    expect(store.applyMux(queue, 3).duplicate).toBe(true)
    expect(store.snapshot(a)?.queue).toEqual([])
  })

  it('rejects old generations and clears the cache at a new baseline', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(2)
    store.applyMux({ type: 'session/subscribed', sessionId: a, lastSeq: 9 }, 2)
    expect(store.applyMux({ type: 'session/subscribed', sessionId: a, lastSeq: 10 }, 1).stale).toBe(true)
    expect(store.snapshot(a)?.subscribedLastSeq).toBe(9)
    store.setGeneration(3)
    expect(store.snapshot(a)).toBeUndefined()
    expect(store.baseline().resumeClass).toBe('baseline-required')
  })

  it('folds host roster, status, workspace, archive, and error frames', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(1)
    const added: HostFrame = {
      type: 'host/session-added', sessionId: a, blank: true, cwd: '/work',
    }
    store.applyHost(added, 1)
    store.applyHost({ type: 'host/session-status', sessionId: a, running: true }, 1)
    store.applyHost({ type: 'host/agent-error', sessionId: a, message: 'failed' }, 1)
    store.applyHost({
      type: 'host/workspace-changed',
      workspace: {
        workspaceId: 'ws-1' as WorkspaceId, path: '/work', title: 'Work', sessionIds: [a],
        createdAt: '2026-01-01', updatedAt: '2026-01-01',
      },
    }, 1)
    store.applyHost({ type: 'host/workspace-order-changed', workspaceIds: ['ws-1' as WorkspaceId] }, 1)
    store.applyHost({ type: 'host/archived-sessions-changed', archivedSessionIds: [a] }, 1)
    const snapshot = store.snapshot(String(a))
    expect(snapshot).toMatchObject({ blank: true, cwd: '/work', running: true, archived: true })
    expect(snapshot?.lastError?.message).toBe('failed')
    expect(store.workspaces()[0]?.workspaceId).toBe('ws-1')
    expect(store.archivedSessionIds()).toEqual([String(a)])
  })

  it('enforces ttl and count bounds', () => {
    let now = 0
    const store = new ControlPlaneStore({ maxSessions: 1, ttlMs: 10, now: () => now })
    store.setGeneration(1)
    store.applyMux({ type: 'session/queue', sessionId: a, items: [] }, 1)
    now = 5
    store.applyMux({ type: 'session/queue', sessionId: b, items: [] }, 1)
    expect(store.snapshots()).toHaveLength(1)
    now = 20
    store.prune()
    expect(store.snapshots()).toHaveLength(0)
  })

  it('deduplicates client request ids and exposes a stable replay record', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(1)
    expect(store.rememberRequest('r-1', { x: 1 })).toBe(false)
    expect(store.rememberRequest('r-1', { x: 1 })).toBe(true)
    store.applyMux({ type: 'session/subscribed', sessionId: a, lastSeq: 4 }, 1)
    const replay = store.replay(String(a))
    expect(replay).toHaveLength(1)
    expect(replay[0]?.frame).toMatchObject({ type: 'session/subscribed', lastSeq: 4 })
  })

  it('rejects stamped stale frames and accepts a covered replay watermark', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(4)
    expect(store.applyMux({
      type: 'session/projection', sessionId: a, key: 'title', seq: 3, value: 'old', generation: 3,
    } as MuxFrame, 4).stale).toBe(true)
    store.applyMux({ type: 'session/projection', sessionId: a, key: 'title', seq: 4, value: 'new' }, 4)
    expect(store.canResume(String(a), 3)).toBe(true)
    expect(store.canResume(String(a), undefined)).toBe(false)
  })

  it('requires a baseline when a retained cursor has an internal sequence gap', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(1)
    store.applyMux(eventFrame(0), 1)
    store.applyMux(eventFrame(2), 1)
    expect(store.canResume(String(a), -1)).toBe(false)
  })

  it('requires a baseline when an unsequenced control snapshot changed', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(1)
    store.applyMux({ type: 'session/queue', sessionId: a, items: [] }, 1)
    expect(store.canResume(String(a), -1)).toBe(false)
    expect(store.canResume(String(a), 0)).toBe(false)
  })

  it('maps workspace membership into the session snapshot and deduplicates host frames', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(1)
    const workspace: HostFrame = {
      type: 'host/workspace-changed',
      workspace: {
        workspaceId: 'ws-1' as WorkspaceId, path: '/work', title: 'Work', sessionIds: [a],
        createdAt: '2026-01-01', updatedAt: '2026-01-01',
      },
    }
    store.applyHost(workspace, 1)
    store.applyHost({ type: 'host/session-status', sessionId: a, running: true }, 1)
    expect(store.snapshot(String(a))?.workspaceId).toBe('ws-1')
    expect(store.applyHost(workspace, 1).duplicate).toBe(true)
    expect(store.replay(undefined)).toHaveLength(2)
  })

  it('keeps an unseen out-of-order event while dropping an exact sequence replay', () => {
    const store = new ControlPlaneStore()
    store.setGeneration(1)
    expect(store.applyMux(eventFrame(5), 1).duplicate).toBe(false)
    expect(store.applyMux(eventFrame(4), 1).duplicate).toBe(false)
    expect(store.applyMux(eventFrame(4), 1).duplicate).toBe(true)
    expect(store.snapshot(String(a))?.lastSeenSeq).toBe(5)
  })

  it('treats session.list as an authoritative roster and preserves missing rows as gone', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(1)
    store.applyHost({ type: 'host/session-status', sessionId: a, running: true }, 1)
    store.applyHost({ type: 'host/session-status', sessionId: b, running: false }, 1)
    store.seedSessionList({ items: [{
      sessionId: a,
      updatedAt: 10,
      running: true,
      blank: false,
    }] })
    expect(store.snapshot(String(a))?.removed).toBe(false)
    expect(store.snapshot(String(b))?.removed).toBe(true)
  })

  it('clears stale optional roster metadata on a complete session.list baseline', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(1)
    store.applyHost({
      type: 'host/session-added', sessionId: a, blank: true,
      parentSessionId: b, origin: 'subagent', cwd: '/stale', agentPreset: 'old',
    }, 1)
    store.seedSessionList({ items: [{ sessionId: String(a), updatedAt: 7, running: false, blank: false }] })
    expect(store.snapshot(String(a))).toMatchObject({
      blank: false, running: false, updatedAtMs: 7,
    })
    expect(store.snapshot(String(a))?.parentSessionId).toBeUndefined()
    expect(store.snapshot(String(a))?.cwd).toBeUndefined()
  })

  it('deduplicates approval replay by host approval id even when request ids differ', () => {
    const store = new ControlPlaneStore({ now: () => 100 })
    store.setGeneration(1)
    const frame = {
      type: 'approval/requested', sessionId: a, approvalId: 'approval-1', toolName: 'rm',
    } as MuxFrame
    expect(store.applyMux(frame, 1, 'rpc-a').duplicate).toBe(false)
    expect(store.applyMux(frame, 1, 'rpc-b').duplicate).toBe(true)
    expect(store.snapshot(String(a))?.pendingInteractions).toHaveLength(1)
  })
})

describe('ControlPlaneRouter', () => {
  it('routes both streams through one generation-aware store', () => {
    const router = new ControlPlaneRouter(new ControlPlaneStore())
    router.setGeneration(7)
    router.routeMux({ type: 'session/queue', sessionId: a, items: [] }, 7)
    router.routeHost({ type: 'host/session-status', sessionId: a, running: false }, 7)
    expect(router.store.snapshot(String(a))).toMatchObject({ generation: 7, running: false })
  })
})
