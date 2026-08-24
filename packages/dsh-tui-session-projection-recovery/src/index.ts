/**
 * Recover persisted projection values for cold `session.list` rows.
 *
 * The host ApiProxy already owns the authoritative session roster. This
 * plugin decorates that public method only when the projection-cache service
 * is mounted and a row is detached and missing its optional projection block.
 * The cache service remains responsible for log identity checks, folding, and
 * durable write-back.
 *
 * @module @dsh-pager-grok/tui-session-projection-recovery
 */

import type { Context } from '@deepseek-ai/cordis'
import Schema from '@deepseek-ai/schemastery'
import type {
  RpcRequest,
  RpcResponse,
  SessionListMetadata,
  SessionProjectionsBlock,
  SessionSummary,
} from '@deepseek-ai/dsh-host-apiproxy/api'
// These type-only imports apply the Context declarations without adding a
// runtime dependency on the host implementation.
import type {} from '@deepseek-ai/dsh-host-apiproxy'
import type {} from '@deepseek-ai/dsh-session'
import type {} from '@deepseek-ai/dsh-session-projection-cache'
import type { SessionId } from '@deepseek-ai/dsh-session/types'

export const name = 'tui-session-projection-recovery'
export const inject = ['apiProxy', 'sessions']

/** This adapter has no deployment knobs; the empty schema keeps Loader wiring explicit. */
export interface Config {}
export const Config: Schema<Config> = Schema.object({})

type SessionListRequest = RpcRequest<{ cursor?: string }>
type SessionListValue = { items: SessionSummary[] }
type SessionListResponse = RpcResponse<SessionListValue>
type SessionListMethod = (request: SessionListRequest, signal?: AbortSignal) => Promise<SessionListResponse>

/** Bound the number of cold log folds started by one list request. */
const RECOVERY_BATCH_SIZE = 16

/** Projection values are structurally open at this seam. */
interface ProjectionValues {
  sessionListMetadata?: SessionListMetadata
  [key: string]: unknown
}

/** The public cache method used by this adapter. */
interface ColdProjectionCache {
  coldSnapshot(id: SessionId, signal?: AbortSignal): Promise<SessionProjectionsBlock>
}

/** Apply the list metadata projection without weakening the host's visibility safety. */
function applyRecoveredMetadata(
  row: SessionSummary,
  values: ProjectionValues,
): SessionSummary {
  const metadata = values.sessionListMetadata
  if (metadata === undefined) return row
  return {
    ...row,
    // A checkpoint saying `blank: true` is only a prefix fact. Keep the host's
    // conservative visible value; a recovered non-blank prefix is authoritative
    // for this row and can safely clear it.
    blank: row.blank && metadata.blank,
    // The host result already includes creation time (or an older hint), so a
    // recovered prompt can only move recency forward.
    updatedAt: Math.max(row.updatedAt, metadata.lastPromptAt ?? 0),
  }
}

/** Merge one non-empty recovered snapshot into a cold summary row. */
function recoveredRow(row: SessionSummary, block: SessionProjectionsBlock): SessionSummary {
  if (Object.keys(block.values).length === 0) return row
  const values = block.values as ProjectionValues
  return {
    ...applyRecoveredMetadata(row, values),
    projections: block,
  }
}

/**
 * Decorate one list result. Attached sessions are deliberately skipped: their
 * live registry snapshot is already the host's authoritative path.
 */
export async function recoverColdSessionList(
  ctx: Context,
  response: SessionListResponse,
  signal?: AbortSignal,
): Promise<SessionListResponse> {
  if (!response.result.ok) return response
  const cache = ctx.get('sessionProjectionCache') as ColdProjectionCache | undefined
  if (cache === undefined) return response
  const items = response.result.value.items
  const next = [...items]
  for (let offset = 0; offset < items.length; offset += RECOVERY_BATCH_SIZE) {
    signal?.throwIfAborted()
    const batch = items.slice(offset, offset + RECOVERY_BATCH_SIZE)
    const settled = await Promise.allSettled(batch.map(async (row, index) => {
      if (row.projections !== undefined || ctx.sessions.get(row.sessionId) !== undefined) {
        return { index, row }
      }
      signal?.throwIfAborted()
      try {
        const block = await cache.coldSnapshot(row.sessionId, signal)
        // The host may attach the session while the cold fold is in flight.
        // Leave that race to the next authoritative list instead of applying
        // a detached snapshot over a now-live row.
        if (ctx.sessions.get(row.sessionId) !== undefined) return { index, row }
        return { index, row: recoveredRow(row, block) }
      } catch (error) {
        signal?.throwIfAborted()
        ctx.logger.warn(
          `session.list: cold projection recovery for "${row.sessionId}" failed (serving the row without it): ${String(error)}`,
        )
        return { index, row }
      }
    }))
    for (const result of settled) {
      if (result.status === 'fulfilled') {
        next[offset + result.value.index] = result.value.row
      } else {
        // The only expected rejection is cancellation from the caller. Keep
        // that contract instead of turning it into a successful partial list.
        throw result.reason
      }
    }
  }
  return { ...response, result: { ok: true, value: { items: next } } }
}

/**
 * Install the public-method decorator. The disposer restores the exact
 * previous function, so unloading/reloading the plugin cannot leave a stale
 * wrapper behind.
 */
export function apply(ctx: Context, _config: Config): void {
  ctx.effect(() => {
    const sessions = ctx.apiProxy.sessions
    const previous = sessions.list as SessionListMethod
    const decorated: SessionListMethod = async (request, signal) => {
      const response = await previous(request)
      return recoverColdSessionList(ctx, response, signal)
    }
    sessions.list = decorated
    return () => {
      if (sessions.list === decorated) sessions.list = previous
    }
  }, 'tui-session-projection-recovery.list')
}
