import type {
  ApiResult,
  HostFrame,
  MuxFrame,
} from '@dsh-pager-grok/tui-protocol'
import type { ApiProxyEnvelopeLike, RecordLike } from './context.js'

export function asRecord(value: unknown, label = 'value'): RecordLike {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} must be an object`)
  }
  return value as RecordLike
}

export function normalizeApiResult(value: unknown): ApiResult {
  const result = asRecord(value, 'ApiProxy result')
  if (result.ok === true && Object.hasOwn(result, 'value')) return result as unknown as ApiResult
  if (result.ok === false) {
    const error = asRecord(result.error, 'ApiProxy error')
    if (typeof error.code === 'string' && typeof error.message === 'string') {
      return {
        ok: false,
        error: {
          code: error.code,
          message: error.message,
          details: Object.hasOwn(error, 'details') ? error.details : {},
        },
      }
    }
  }
  throw new Error('ApiProxy result must be an ok/value or ok/error carrier')
}

export function apiErrorResult(error: unknown, signal: AbortSignal): ApiResult {
  if (signal.aborted) {
    return { ok: false, error: { code: 'cancelled', message: 'operation was cancelled', details: {} } }
  }
  if (typeof error === 'object' && error !== null) {
    const code = (error as { code?: unknown }).code
    if (typeof code === 'string') {
      return {
        ok: false,
        error: {
          code: code.startsWith('GOAL_') ? 'internal' : code.toLowerCase().replaceAll('_', '-'),
          message: error instanceof Error ? error.message : String(error),
          details: code.startsWith('GOAL_') ? { goalCode: code } : {},
        },
      }
    }
  }
  return {
    ok: false,
    error: {
      code: 'internal',
      message: error instanceof Error ? error.message : String(error),
      details: {},
    },
  }
}

export function muxEnvelope(envelope: ApiProxyEnvelopeLike): { frame: MuxFrame; requestId: string } {
  const frame = asRecord(envelope.payload, 'ApiProxy mux payload')
  if (typeof frame.type !== 'string') throw new Error('ApiProxy mux payload requires a frame type')
  if (envelope.rpcId === undefined || envelope.rpcId === null) {
    throw new Error('ApiProxy mux envelope requires rpcId')
  }
  return { frame: frame as MuxFrame, requestId: String(envelope.rpcId) }
}

export function hostFrame(envelope: ApiProxyEnvelopeLike): HostFrame {
  const frame = asRecord(envelope.payload, 'ApiProxy host payload')
  if (typeof frame.type !== 'string') throw new Error('ApiProxy host payload requires a frame type')
  return frame as HostFrame
}

export function respondReceipt(value: unknown): { accepted: boolean; reason?: string } {
  const receipt = asRecord(value, 'ApiProxy respond receipt')
  if (receipt.accepted === true) return { accepted: true }
  if (receipt.accepted === false) {
    return {
      accepted: false,
      ...typeof receipt.reason === 'string' ? { reason: receipt.reason } : {},
    }
  }
  throw new Error('ApiProxy respond receipt requires accepted boolean')
}
