import type { ApiError, HostFrame, SessionId } from '@dsh-pager-grok/tui-protocol'
import type { RecordLike } from './context.js'

export function flattenSessionList(value: { items: RecordLike[] }): RecordLike {
  return {
    items: value.items.map((row) => {
      const projections = asOptionalRecord(row.projections)
      const values = asOptionalRecord(projections?.values)
      const projected = values?.agentPreset
      return {
        ...row,
        ...typeof projected === 'string'
          ? { agentPreset: projected }
          : typeof row.agentPreset === 'string' ? { agentPreset: row.agentPreset } : {},
      }
    }),
  }
}

export function sessionAddedFrame(summary: RecordLike): HostFrame {
  const projections = asOptionalRecord(summary.projections)
  const values = asOptionalRecord(projections?.values)
  const projected = values?.agentPreset
  return {
    type: 'host/session-added',
    sessionId: requireString(summary, 'sessionId') as SessionId,
    blank: summary.blank === true,
    ...typeof summary.parentSessionId === 'string' ? { parentSessionId: summary.parentSessionId as SessionId } : {},
    ...summary.origin === 'subagent' ? { origin: 'subagent' as const } : {},
    ...typeof summary.cwd === 'string' ? { cwd: summary.cwd } : {},
    ...typeof projected === 'string' ? { agentPreset: projected } : {},
  }
}

export function apiError(error: unknown, signal: AbortSignal): ApiError {
  if (signal.aborted) {
    return { code: 'cancelled', message: 'operation was cancelled', details: {} }
  }
  if (typeof error === 'object' && error !== null) {
    const failureValue = (error as { failure?: unknown }).failure
    if (isFailure(failureValue)) return failureValue
    if (isFailure(error)) return error
    const code = (error as { code?: unknown }).code
    if (typeof code === 'string') {
      return {
        code: code.startsWith('GOAL_') ? 'internal' : code.toLowerCase().replaceAll('_', '-'),
        message: error instanceof Error ? error.message : String(error),
        details: code.startsWith('GOAL_') ? { goalCode: code } : {},
      }
    }
  }
  return {
    code: 'internal',
    message: error instanceof Error ? error.message : String(error),
    details: {},
  }
}

function isFailure(value: unknown): value is ApiError {
  return typeof value === 'object'
    && value !== null
    && typeof (value as RecordLike).code === 'string'
    && typeof (value as RecordLike).message === 'string'
    && Object.hasOwn(value, 'details')
}

export function failure(code: string, message: string, details: unknown = {}): { failure: ApiError } {
  return { failure: { code, message, details } }
}

export function asRecord(value: unknown): RecordLike {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw failure('bad-request', 'request payload must be an object')
  }
  return value as RecordLike
}

export function asOptionalRecord(value: unknown): RecordLike | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as RecordLike
    : undefined
}

export function requireRecord(value: RecordLike, key: string): RecordLike {
  try {
    return asRecord(value[key])
  } catch {
    throw failure('bad-request', `${key} must be an object`)
  }
}

export function requireString(value: RecordLike, key: string): string {
  const item = value[key]
  if (typeof item !== 'string' || item.length === 0) {
    throw failure('bad-request', `${key} must be a non-empty string`)
  }
  return item
}

export function optionalString(value: RecordLike, key: string): string | undefined {
  const item = value[key]
  if (item === undefined) return undefined
  if (typeof item !== 'string') throw failure('bad-request', `${key} must be a string`)
  return item
}

export function optionalNumber(value: RecordLike, key: string): number | undefined {
  const item = value[key]
  if (item === undefined) return undefined
  if (typeof item !== 'number' || !Number.isSafeInteger(item)) {
    throw failure('bad-request', `${key} must be a safe integer`)
  }
  return item
}

export function requireArray(value: RecordLike, key: string): unknown[] {
  const item = value[key]
  if (!Array.isArray(item)) throw failure('bad-request', `${key} must be an array`)
  return item
}

export function requireStringArray(value: RecordLike, key: string): string[] {
  const items = requireArray(value, key)
  if (items.some(item => typeof item !== 'string')) {
    throw failure('bad-request', `${key} must contain only strings`)
  }
  return items as string[]
}

export function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
}

export function agentId(params: RecordLike): SessionId {
  const value = typeof params.agentId === 'string' ? params.agentId : params.sessionId
  if (typeof value !== 'string' || value.length === 0) {
    throw failure('bad-request', 'agentId is required')
  }
  return value as SessionId
}

export function requireMode(params: RecordLike): 'one-shot' | 'continuable' {
  if (params.mode === 'one-shot' || params.mode === 'continuable') return params.mode
  throw failure('bad-request', 'mode must be one-shot or continuable')
}

export function requireContinuable(params: RecordLike): 'continuable' {
  if (params.mode === 'continuable') return params.mode
  throw failure('bad-request', 'subagent interrupt requires continuable mode')
}
