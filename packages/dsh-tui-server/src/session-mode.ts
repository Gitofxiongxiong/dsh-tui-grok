/**
 * DSH session-mode cycle used by Shift+Tab: three named bundles of plan,
 * sandbox, and approval knobs. The switch is applied in-process through the
 * same write paths dsh-TUI uses; the session log remains the store.
 *
 * @module @dsh-pager-grok/tui-server/session-mode
 */

import type { Agent } from '@deepseek-ai/dsh-agent'
import type { SessionId } from '@deepseek-ai/dsh-session/types'
import type { TuiSessionMode, TuiSessionModeId } from '@dsh-pager-grok/tui-protocol'

/** One session-event shape used by the mode fold. */
export interface ModeEvent {
  type: string
  data?: unknown
}

/** In-process helpers the gateway uses to apply one mode. */
export interface SessionModeServices {
  resolveAgent: (sessionId: SessionId) => Promise<
    { agent: Agent } | { error: { code: string; message: string; details?: unknown } }
  >
  setApprovalPolicy?: (agent: Agent, policy: 'ask' | 'never') => void
  executePlan?: (
    agent: Agent,
    on: boolean,
    signal: AbortSignal,
  ) => Promise<'ok' | 'unavailable' | 'failed'>
}

/** The shipped Shift+Tab cycle. Array order IS the cycle order. */
export const SESSION_MODES: readonly TuiSessionMode[] = [
  {
    id: 'normal',
    label: 'normal',
    plan: false,
    sandbox: 'workspace-write',
    approval: 'ask',
  },
  {
    id: 'plan',
    label: 'plan',
    plan: true,
    sandbox: 'read-only',
    approval: 'ask',
  },
  {
    id: 'danger-full-access',
    label: 'danger-full-access',
    plan: false,
    sandbox: 'danger-full-access',
    approval: 'never',
  },
]

/**
 * Look up a shipped mode by id.
 * @param id - a session-mode identifier.
 * @returns the spec, or undefined when the id is not in the cycle.
 */
export function sessionModeById(id: TuiSessionModeId): TuiSessionMode | undefined {
  return SESSION_MODES.find(mode => mode.id === id)
}

function foldPlanActive(events: readonly ModeEvent[]): boolean {
  let active = false
  for (const event of events) {
    if (event.type === 'plan/mode') {
      active = (event.data as { active?: boolean } | undefined)?.active === true
    }
  }
  return active
}

function foldSandboxMode(events: readonly ModeEvent[]): string | undefined {
  let mode: string | undefined
  for (const event of events) {
    if (event.type === 'sandbox/mode') {
      const value = (event.data as { mode?: string } | undefined)?.mode
      if (typeof value === 'string') mode = value
    }
  }
  return mode
}

function foldApprovalPolicy(events: readonly ModeEvent[]): string | undefined {
  let policy: string | undefined
  for (const event of events) {
    if (event.type === 'approval/policy') {
      const value = (event.data as { policy?: string } | undefined)?.policy
      if (typeof value === 'string') policy = value
    }
  }
  return policy
}

function appendEvent(agent: Agent, type: string, data: Record<string, unknown>): void {
  ;(agent.session as unknown as { append(type: string, data: Record<string, unknown>): unknown })
    .append(type, data)
}

/**
 * First configured mode whose declared atoms all match the event folds.
 * No match → index 0 (the unmarked `normal` base). Matching is exact: a
 * fresh session with no knob events never falsely matches `approval: ask`.
 * @param events - session events in log order.
 * @returns the matching cycle index.
 */
export function deriveModeIndex(events: readonly ModeEvent[]): number {
  const index = SESSION_MODES.findIndex(
    spec =>
      foldPlanActive(events) === spec.plan
      && foldSandboxMode(events) === spec.sandbox
      && foldApprovalPolicy(events) === spec.approval,
  )
  return index >= 0 ? index : 0
}

/**
 * The next mode in the cycle, derived from the session log.
 * @param events - session events in log order.
 * @returns the spec Shift+Tab should apply.
 */
export function nextSessionMode(events: readonly ModeEvent[]): TuiSessionMode {
  const index = deriveModeIndex(events)
  return SESSION_MODES[(index + 1) % SESSION_MODES.length]!
}

/**
 * Apply one session mode. Plan is switched first; a failing plan toggle
 * aborts so sandbox/approval never land in a half-applied bundle.
 * @param agent - the live session agent.
 * @param spec - the target mode.
 * @param services - optional command/approval writers.
 */
export async function applySessionMode(
  agent: Agent,
  spec: TuiSessionMode,
  services: Pick<SessionModeServices, 'setApprovalPolicy' | 'executePlan'> = {},
): Promise<void> {
  const events = agent.session.events as readonly ModeEvent[]
  if (foldPlanActive(events) !== spec.plan) {
    if (services.executePlan !== undefined) {
      const result = await services.executePlan(agent, spec.plan, new AbortController().signal)
      if (result !== 'ok') {
        throw new Error(result === 'unavailable' ? 'plan-unavailable' : 'plan-failed')
      }
    } else {
      appendEvent(agent, 'plan/mode', { active: spec.plan })
    }
  }
  if (foldSandboxMode(agent.session.events as readonly ModeEvent[]) !== spec.sandbox) {
    appendEvent(agent, 'sandbox/mode', { mode: spec.sandbox })
  }
  if (foldApprovalPolicy(agent.session.events as readonly ModeEvent[]) !== spec.approval) {
    if (services.setApprovalPolicy !== undefined) {
      services.setApprovalPolicy(agent, spec.approval)
    } else {
      appendEvent(agent, 'approval/policy', { policy: spec.approval })
    }
  }
}
