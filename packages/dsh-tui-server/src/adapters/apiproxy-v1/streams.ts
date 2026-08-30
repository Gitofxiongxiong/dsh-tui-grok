import type { HostFrame, SessionId } from '@dsh-pager-grok/tui-protocol'
import type { TuiMuxEnvelope } from '../../core/backend.js'
import type { ApiProxyV1Like } from './context.js'
import { hostFrame, muxEnvelope } from './normalize.js'

export async function* apiProxyMuxFrames(
  api: ApiProxyV1Like,
  signal: AbortSignal,
): AsyncIterable<TuiMuxEnvelope> {
  for await (const envelope of api.events.mux({ rpcId: 'tui-mux', payload: {} }, signal)) {
    yield muxEnvelope(envelope)
  }
}

export async function* apiProxyHostFrames(
  api: ApiProxyV1Like,
  signal: AbortSignal,
): AsyncIterable<HostFrame> {
  for await (const envelope of api.events.host({ rpcId: 'tui-host', payload: {} }, signal)) {
    yield hostFrame(envelope)
  }
}

/** ApiProxy v1 carries session events on mux; keep the core follower idle. */
export async function* emptySessionFollower(
  _sessionId: SessionId,
  signal: AbortSignal,
): AsyncIterable<TuiMuxEnvelope> {
  if (!signal.aborted) {
    await new Promise<void>(resolve => signal.addEventListener('abort', () => resolve(), { once: true }))
  }
}
