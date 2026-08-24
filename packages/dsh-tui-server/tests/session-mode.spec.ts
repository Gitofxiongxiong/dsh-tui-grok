import type { Agent } from '@deepseek-ai/dsh-agent'
import { describe, expect, it } from 'vitest'
import {
  SESSION_MODES,
  applySessionMode,
  deriveModeIndex,
  nextSessionMode,
  sessionModeById,
  type ModeEvent,
} from '../src/session-mode.ts'

function fakeAgent(events: ModeEvent[] = []): Agent {
  return {
    session: {
      events,
      append(type: string, data: Record<string, unknown>) {
        events.push({ type, data })
      },
    },
  } as unknown as Agent
}

describe('session-mode cycle', () => {
  it('treats an empty log as normal and cycles normal → plan → danger-full-access', () => {
    expect(deriveModeIndex([])).toBe(0)
    expect(nextSessionMode([]).id).toBe('plan')
    const afterPlan: ModeEvent[] = [
      { type: 'plan/mode', data: { active: true } },
      { type: 'sandbox/mode', data: { mode: 'read-only' } },
      { type: 'approval/policy', data: { policy: 'ask' } },
    ]
    expect(deriveModeIndex(afterPlan)).toBe(1)
    expect(nextSessionMode(afterPlan).id).toBe('danger-full-access')
    const afterFull: ModeEvent[] = [
      ...afterPlan,
      { type: 'plan/mode', data: { active: false } },
      { type: 'sandbox/mode', data: { mode: 'danger-full-access' } },
      { type: 'approval/policy', data: { policy: 'never' } },
    ]
    expect(deriveModeIndex(afterFull)).toBe(2)
    expect(nextSessionMode(afterFull).id).toBe('normal')
  })

  it('applies every declared atom and skips no-op writes', async () => {
    const events: ModeEvent[] = []
    const agent = fakeAgent(events)
    await applySessionMode(agent, sessionModeById('plan')!)
    expect(events).toEqual([
      { type: 'plan/mode', data: { active: true } },
      { type: 'sandbox/mode', data: { mode: 'read-only' } },
      { type: 'approval/policy', data: { policy: 'ask' } },
    ])
    await applySessionMode(agent, sessionModeById('plan')!)
    expect(events).toHaveLength(3)
    await applySessionMode(agent, SESSION_MODES[2]!)
    expect(events.at(-1)).toEqual({ type: 'approval/policy', data: { policy: 'never' } })
  })

  it('aborts sandbox and approval writes when the plan command fails', async () => {
    const events: ModeEvent[] = []
    const agent = fakeAgent(events)
    await expect(applySessionMode(agent, sessionModeById('plan')!, {
      executePlan: async () => 'unavailable',
    })).rejects.toThrow('plan-unavailable')
    expect(events).toEqual([])
  })

  it('uses the approval service when it is mounted', async () => {
    const events: ModeEvent[] = []
    const agent = fakeAgent(events)
    const policies: string[] = []
    await applySessionMode(agent, sessionModeById('danger-full-access')!, {
      setApprovalPolicy: (_agent, policy) => {
        policies.push(policy)
        events.push({ type: 'approval/policy', data: { policy } })
      },
    })
    expect(policies).toEqual(['never'])
    expect(events.some(event => event.type === 'sandbox/mode')).toBe(true)
  })
})
