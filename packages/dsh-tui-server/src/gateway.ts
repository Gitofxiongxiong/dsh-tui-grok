/**
 * One TUI client on one framed connection: hello, attach, ApiProxy forward,
 * and mux/host fan-out with a per-session live buffer until history returns.
 *
 * @module @dsh-pager-grok/tui-server/gateway
 */

import { randomUUID } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import type { ApiProxy, HostFrame, MuxFrame } from '@deepseek-ai/dsh-host-apiproxy/api'
import { RpcId } from '@deepseek-ai/dsh-host-apiproxy/api'
import type { SessionId } from '@deepseek-ai/dsh-session/types'
import {
  TUI_PROTOCOL_VERSION,
  TUI_SERVER_INFO_NAME,
  classifyMethod,
  decodeAttachParams,
  decodeDetachParams,
  decodeHelloParams,
  decodeRespondParams,
  decodeSetSessionModeParams,
  decodeSubscribeParams,
  TuiClientId,
  type ConnectionGeneration,
  type TuiHelloResult,
  type TuiMuxFrame,
  type TuiSubscribeResult,
} from '@dsh-pager-grok/tui-protocol'
import { ControlPlaneRouter } from './control-plane.js'
import { dispatchRespond, dispatchUnary, type TuiDispatchExtensions } from './dispatch.js'
import { TuiMethodNotFoundError, TuiRpcError } from './errors.js'
import {
  applySessionMode,
  nextSessionMode,
  sessionModeById,
  type ModeEvent,
} from './session-mode.js'

/** Outbound notification sink. */
export interface TuiNotifyPeer {
  notify(method: string, params?: unknown): void
}

/** Package version advertised in hello `serverInfo.version`. */
export const TUI_SERVER_VERSION = readOwningPackageVersion()

function readOwningPackageVersion(): string {
  const here = dirname(fileURLToPath(import.meta.url))
  for (const candidate of [
    join(here, '../package.json'),
    join(here, '../../package.json'),
    join(here, '../../../package.json'),
  ]) {
    try {
      const pkg = JSON.parse(readFileSync(candidate, 'utf8')) as {
        name?: string
        version?: string
      }
      if (
        typeof pkg.version === 'string' &&
        typeof pkg.name === 'string' &&
        pkg.name.startsWith('@dsh-pager-grok/')
      ) {
        return pkg.version
      }
    } catch {
      // Walk toward the package root; missing parents are expected.
    }
  }
  throw new Error('unable to read @dsh-pager-grok package version for tui.hello serverInfo')
}

function sessionIdOfMux(frame: MuxFrame): SessionId | undefined {
  if ('sessionId' in frame && typeof frame.sessionId === 'string') {
    return frame.sessionId
  }
  return undefined
}

/**
 * Serves one TUI client. Mux starts on the first attach so the stream's
 * baseline pending interactions are available; later attaches on the same
 * connection see live frames only (history remains the content baseline).
 */
export class TuiGateway {
  private clientId: string | undefined
  private generation: ConnectionGeneration = 0
  private readonly attached = new Set<string>()
  private readonly subscriptions = new Set<string>()
  private controlPlaneSubscribed = false
  private readonly buffering = new Map<string, (TuiMuxFrame & { generation?: number })[]>()
  /** Public for the native client/tests that need a roster baseline. */
  readonly controlPlane = new ControlPlaneRouter()
  private muxAbort: AbortController | undefined
  private hostAbort: AbortController | undefined
  /**
   * Interaction responses are at-most-once by request id.  Keep the payload
   * fingerprint next to the in-flight/completed promise so reusing an id for
   * a different answer cannot silently replay the first answer.
   */
  private readonly respondedRequests = new Map<string, { fingerprint: string; promise: Promise<unknown> }>()

  constructor(
    private readonly api: ApiProxy,
    private readonly peer: TuiNotifyPeer,
    private readonly extensions: TuiDispatchExtensions = {},
  ) {}

  /** Abort mux/host pumps. */
  dispose(): void {
    this.muxAbort?.abort()
    this.hostAbort?.abort()
  }

  /**
   * Handle one inbound JSON-RPC request.
   * @param method - JSON-RPC method.
   * @param params - JSON-RPC params.
   * @param rpcId - JSON-RPC id, forwarded into ApiProxy envelopes.
   * @returns the JSON-RPC result value.
   */
  async handleRequest(method: string, params: unknown, rpcId: string): Promise<unknown> {
    const kind = classifyMethod(method)
    if (method !== 'tui.hello' && this.clientId === undefined) {
      throw new TuiRpcError('not-attached', 'tui.hello is required before other methods')
    }
    if (kind === 'tui-request') return await this.handleControl(method, params)
    if (kind === 'api') {
      const result = await dispatchUnary(this.api, method, params, rpcId, this.extensions)
      if (method === 'session.list' && apiResultSucceeded(result)) {
        this.controlPlane.store.seedSessionList((result as { value?: unknown }).value)
      }
      if (method === 'workspace.list' && apiResultSucceeded(result)) {
        this.controlPlane.store.seedWorkspaceList((result as { value?: unknown }).value)
      }
      // A failed ApiProxy result is still a successful carrier response. Keep
      // the live backlog buffered until the history value itself committed;
      // otherwise a client could observe live events without crossing the
      // replay/load barrier.
      if (method === 'session.history' && apiResultSucceeded(result)) this.flushHistoryBuffer(params)
      return result
    }
    throw new TuiMethodNotFoundError(method)
  }

  private async handleControl(method: string, params: unknown): Promise<unknown> {
    switch (method) {
      case 'tui.hello':
        return this.hello(params)
      case 'tui.attach':
        return this.attach(params)
      case 'tui.detach':
        return this.detach(params)
      case 'tui.subscribe':
        return this.subscribe(params)
      case 'tui.respond':
        return this.respond(params)
      case 'tui.setSessionMode':
        return await this.setSessionMode(params)
      default:
        throw new TuiMethodNotFoundError(method)
    }
  }

  private hello(params: unknown): TuiHelloResult {
    const decoded = decodeHelloParams(params)
    if (!decoded.ok) {
      throw new TuiRpcError(
        decoded.reason === 'protocol-version' ? 'protocol-version' : 'not-attached',
        `invalid tui.hello: ${decoded.reason}`,
      )
    }
    // A repeated hello starts a new connection generation. Any stream pump
    // and attach backlog from the prior generation must be discarded before a
    // new attach, otherwise old frames can leak into the fresh baseline.
    this.resetConnection()
    this.clientId = decoded.value.clientId ?? this.clientId ?? TuiClientId(`tui_${randomUUID()}`)
    this.generation += 1
    this.controlPlane.setGeneration(this.generation)
    this.peer.notify('tui.serverReady')
    return {
      protocolVersion: TUI_PROTOCOL_VERSION,
      clientId: TuiClientId(this.clientId),
      generation: this.generation,
      resumeClass: 'baseline-required',
      serverInfo: {
        name: TUI_SERVER_INFO_NAME,
        version: TUI_SERVER_VERSION,
      },
    }
  }

  private requireGeneration(generation: ConnectionGeneration): void {
    if (generation !== this.generation) {
      throw new TuiRpcError('stale-generation', 'stale connection generation', { generation: this.generation })
    }
  }

  private attach(params: unknown): { attached: true; role: 'driver' } {
    const decoded = decodeAttachParams(params)
    if (!decoded.ok) throw new TuiRpcError('not-attached', `invalid tui.attach: ${decoded.reason}`)
    this.requireGeneration(decoded.value.generation)
    const id = String(decoded.value.sessionId)
    this.attached.add(id)
    this.subscriptions.add(id)
    if (!this.buffering.has(id)) this.buffering.set(id, [])
    this.ensurePumps()
    return { attached: true, role: 'driver' }
  }

  private detach(params: unknown): Record<string, never> {
    const decoded = decodeDetachParams(params)
    if (!decoded.ok) throw new TuiRpcError('not-attached', `invalid tui.detach: ${decoded.reason}`)
    this.requireGeneration(decoded.value.generation)
    const id = String(decoded.value.sessionId)
    if (!this.attached.has(id)) {
      throw new TuiRpcError('unknown-session', 'session is not attached', { sessionId: decoded.value.sessionId })
    }
    this.attached.delete(id)
    this.subscriptions.delete(id)
    this.buffering.delete(id)
    this.stopPumpsIfUnused()
    return {}
  }

  private async subscribe(params: unknown): Promise<TuiSubscribeResult> {
    const decoded = decodeSubscribeParams(params)
    if (!decoded.ok) throw new TuiRpcError('not-attached', `invalid tui.subscribe: ${decoded.reason}`)
    this.requireGeneration(decoded.value.generation)
    const scope = decoded.value.scope ?? (decoded.value.sessionId === undefined ? 'all' : 'session')
    const id = decoded.value.sessionId === undefined ? undefined : String(decoded.value.sessionId)
    if (scope === 'session' && id === undefined) {
      throw new TuiRpcError('not-attached', 'session subscription requires sessionId')
    }
    if (scope === 'all' || scope === 'control-plane') this.controlPlaneSubscribed = true
    if (id !== undefined) {
      this.subscriptions.add(id)
      if (!this.buffering.has(id)) this.buffering.set(id, [])
    }
    this.ensurePumps()
    // Async generators publish their initial host/mux baseline on the next
    // turn. Yield once so the unary receipt and baseline notification include
    // those frames whenever the carrier can provide them immediately; a
    // silent/long-lived stream still returns promptly with an empty baseline.
    await Promise.resolve()
    const resumeClass = this.controlPlane.store.canResume(id, decoded.value.since)
      ? 'resume-accepted' as const
      : 'baseline-required' as const
    const baseline = scopedBaseline(this.controlPlane.store.baseline(resumeClass), id)
    if (resumeClass === 'baseline-required') {
      // The baseline is also sent as a notification so clients that use a
      // streaming-only reader do not need to depend on the unary result.
      this.peer.notify('tui.controlPlaneBaseline', baseline)
    } else {
      // A reconnecting client already owns the prior snapshot. Replay only
      // retained control records after its watermark; presentation history is
      // still crossed through session.history on attach.
      for (const record of this.controlPlane.store.replay(id, decoded.value.since)) {
        notifyControlRecord(this.peer, record)
      }
    }
    return {
      generation: this.generation,
      resumeClass,
      scope,
      ...id === undefined ? {} : { sessionId: decoded.value.sessionId },
      watermarks: Object.fromEntries(baseline.sessions
        .filter(session => session.lastSeenSeq !== undefined)
        .map(session => [session.sessionId, session.lastSeenSeq as number])),
    }
  }

  private async respond(params: unknown): Promise<unknown> {
    const decoded = decodeRespondParams(params)
    if (!decoded.ok) throw new TuiRpcError('not-attached', `invalid tui.respond: ${decoded.reason}`)
    this.requireGeneration(decoded.value.generation)
    const requestKey = decoded.value.requestId
    const interaction = decoded.value.interaction
    const value = interaction.type === 'approval'
      ? {
        sessionId: decoded.value.sessionId,
        approvalId: interaction.approvalId,
        outcome: interaction.outcome,
      }
      : {
        sessionId: decoded.value.sessionId,
        answer: interaction.answers,
      }
    const fingerprint = stableJson({
      sessionId: decoded.value.sessionId,
      requestId: decoded.value.requestId,
      interaction: decoded.value.interaction,
    })
    const previous = this.respondedRequests.get(requestKey)
    if (previous !== undefined) {
      if (previous.fingerprint !== fingerprint) {
        throw new TuiRpcError(
          'already-resolved',
          `request ${requestKey} was already answered with a different payload`,
          { requestId: requestKey, sessionId: decoded.value.sessionId, generation: decoded.value.generation },
        )
      }
      return await previous.promise
    }
    const pending = dispatchRespond(this.api, decoded.value.requestId, value)
    this.respondedRequests.set(requestKey, { fingerprint, promise: pending })
    let result: unknown
    try {
      result = await pending
    } catch (error) {
      this.respondedRequests.delete(requestKey)
      throw error
    }
    while (this.respondedRequests.size > 1024) {
      const first = this.respondedRequests.keys().next().value
      if (first === undefined) break
      this.respondedRequests.delete(first)
    }
    return result
  }

  private async setSessionMode(params: unknown): Promise<unknown> {
    const decoded = decodeSetSessionModeParams(params)
    if (!decoded.ok) throw new TuiRpcError('not-attached', `invalid tui.setSessionMode: ${decoded.reason}`)
    this.requireGeneration(decoded.value.generation)
    const services = this.extensions.sessionMode
    if (services === undefined) {
      throw new TuiRpcError('capability-denied', 'session mode switching is unavailable', {
        sessionId: decoded.value.sessionId,
        generation: decoded.value.generation,
      })
    }
    const resolved = await services.resolveAgent(decoded.value.sessionId)
    if ('error' in resolved) {
      const kind = resolved.error.code === 'session-not-found' ? 'unknown-session' : 'capability-denied'
      throw new TuiRpcError(kind, resolved.error.message, {
        sessionId: decoded.value.sessionId,
        generation: decoded.value.generation,
      })
    }
    const spec = decoded.value.modeId === undefined
      ? nextSessionMode(resolved.agent.session.events as readonly ModeEvent[])
      : sessionModeById(decoded.value.modeId)
    if (spec === undefined) {
      throw new TuiRpcError('not-attached', 'unknown session mode', {
        sessionId: decoded.value.sessionId,
        generation: decoded.value.generation,
      })
    }
    try {
      await applySessionMode(resolved.agent, spec, services)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      throw new TuiRpcError(
        message === 'plan-unavailable' ? 'capability-denied' : 'not-attached',
        message === 'plan-unavailable'
          ? 'plan mode is unavailable for this session'
          : `session mode switch failed: ${message}`,
        { sessionId: decoded.value.sessionId, generation: decoded.value.generation },
      )
    }
    return { accepted: true, mode: spec }
  }

  private flushHistoryBuffer(params: unknown): void {
    if (params === null || typeof params !== 'object' || Array.isArray(params)) return
    const sessionId = (params as { sessionId?: unknown }).sessionId
    if (typeof sessionId !== 'string') return
    const buffered = this.buffering.get(sessionId)
    this.buffering.delete(sessionId)
    if (buffered === undefined) return
    for (const frame of buffered) this.peer.notify('events.mux', frame)
  }

  private ensurePumps(): void {
    if (this.muxAbort !== undefined && this.hostAbort !== undefined) return
    // If one stream ended while the other remained alive, restart the pair
    // together on the next subscription so a partial carrier cannot leave a
    // client with a permanently one-sided control plane.
    this.muxAbort?.abort()
    this.hostAbort?.abort()
    this.muxAbort = new AbortController()
    this.hostAbort = new AbortController()
    const muxSignal = this.muxAbort.signal
    const hostSignal = this.hostAbort.signal
    const pumpGeneration = this.generation
    void Promise.all([
      this.pumpMux(muxSignal, pumpGeneration),
      this.pumpHost(hostSignal, pumpGeneration),
    ]).then(() => undefined, () => undefined)
  }

  private async pumpMux(signal: AbortSignal, generation: ConnectionGeneration): Promise<void> {
    try {
      for await (const envelope of this.api.events.mux(
        { rpcId: RpcId('tui-mux'), payload: {} },
        signal,
      )) {
        this.emitMux(envelope.payload, String(envelope.rpcId), generation)
      }
      if (!signal.aborted) this.notifyStreamClosed('mux', generation)
    } catch (error) {
      if (signal.aborted) return
      this.notifyStreamError('mux', error, generation)
    } finally {
      this.clearPump('mux', generation, signal)
    }
  }

  private async pumpHost(signal: AbortSignal, generation: ConnectionGeneration): Promise<void> {
    try {
      for await (const envelope of this.api.events.host(
        { rpcId: RpcId('tui-host'), payload: {} },
        signal,
      )) {
        this.emitHost(envelope.payload, generation)
      }
      if (!signal.aborted) this.notifyStreamClosed('host', generation)
    } catch (error) {
      if (signal.aborted) return
      this.notifyStreamError('host', error, generation)
    } finally {
      this.clearPump('host', generation, signal)
    }
  }

  private clearPump(stream: 'mux' | 'host', generation: number, signal: AbortSignal): void {
    if (generation !== this.generation) return
    if (stream === 'mux' && this.muxAbort?.signal === signal) this.muxAbort = undefined
    if (stream === 'host' && this.hostAbort?.signal === signal) this.hostAbort = undefined
  }

  private stopPumpsIfUnused(): void {
    if (this.attached.size > 0 || this.subscriptions.size > 0 || this.controlPlaneSubscribed) return
    this.muxAbort?.abort()
    this.hostAbort?.abort()
    this.muxAbort = undefined
    this.hostAbort = undefined
  }

  private notifyStreamClosed(stream: 'mux' | 'host', generation: number): void {
    this.peer.notify(`events.${stream}`, {
      type: 'stream/error',
      generation,
      error: {
        code: 'closed',
        message: `${stream} stream closed unexpectedly`,
        details: {},
      },
    })
  }

  private notifyStreamError(stream: 'mux' | 'host', error: unknown, generation: number): void {
    this.peer.notify(`events.${stream}`, {
      type: 'stream/error',
      generation,
      error: {
        code: 'internal',
        message: error instanceof Error ? error.message : String(error),
        details: {},
      },
    })
  }

  private emitMux(frame: MuxFrame, requestId: string, generation: ConnectionGeneration): void {
    if (generation !== this.generation) return
    const folded = this.controlPlane.routeMux(frame, generation, requestId)
    if (folded.stale || folded.duplicate) return
    const delivered = addRequestId(frame, requestId, generation)
    const sessionId = sessionIdOfMux(frame)
    if (sessionId === undefined) {
      if (this.controlPlaneSubscribed || this.attached.size > 0) this.peer.notify('events.mux', delivered)
      return
    }
    const key = String(sessionId)
    const isPresentation = frame.type === 'session/event'
    const shouldDeliver = this.attached.has(key)
      ? true
      : this.controlPlaneSubscribed || this.subscriptions.has(key)
    if (!shouldDeliver) return
    // A session-specific baseline/control frame is held behind the history
    // load barrier for an attached session. Unattached control subscribers get
    // it immediately, which is what keeps Dashboard roster rows live.
    const buffer = this.buffering.get(key)
    if (this.attached.has(key) && buffer !== undefined) {
      buffer.push(delivered)
      return
    }
    if (isPresentation && !this.attached.has(key)) return
    this.peer.notify('events.mux', delivered)
  }

  private emitHost(frame: HostFrame, generation: ConnectionGeneration): void {
    if (generation !== this.generation) return
    const folded = this.controlPlane.routeHost(frame, generation)
    if (folded.stale || folded.duplicate) return
    const target = 'sessionId' in frame && typeof frame.sessionId === 'string'
      ? String(frame.sessionId)
      : undefined
    const shouldDeliver = target === undefined
      ? this.controlPlaneSubscribed || this.attached.size > 0 || this.subscriptions.size > 0
      : this.controlPlaneSubscribed || this.attached.has(target) || this.subscriptions.has(target)
    if (shouldDeliver) {
      this.peer.notify('events.host', stampHost(frame, generation))
    }
  }

  private resetConnection(): void {
    this.muxAbort?.abort()
    this.hostAbort?.abort()
    this.muxAbort = undefined
    this.hostAbort = undefined
    this.attached.clear()
    this.subscriptions.clear()
    this.controlPlaneSubscribed = false
    this.buffering.clear()
    this.respondedRequests.clear()
  }
}

/** Stable enough JSON identity for decoded JSON-RPC interaction values. */
function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`
  if (value !== null && typeof value === 'object') {
    return `{${Object.entries(value as Record<string, unknown>)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`)
      .join(',')}}`
  }
  return JSON.stringify(value)
}

function apiResultSucceeded(value: unknown): boolean {
  return value !== null
    && typeof value === 'object'
    && !Array.isArray(value)
    && (value as { ok?: unknown }).ok === true
}

function addRequestId(frame: MuxFrame, requestId: string, generation?: number): TuiMuxFrame & { generation?: number } {
  const stamp = generation === undefined ? {} : { generation }
  if (frame.type === 'approval/requested' || frame.type === 'question/requested') {
    return { ...frame, requestId, ...stamp }
  }
  return { ...frame, ...stamp }
}

function stampHost(frame: HostFrame, generation: number): HostFrame & { generation?: number } {
  return { ...frame, generation }
}

function scopedBaseline(
  baseline: ReturnType<ControlPlaneRouter['store']['baseline']>,
  sessionId: string | undefined,
): ReturnType<ControlPlaneRouter['store']['baseline']> {
  if (sessionId === undefined) return baseline
  return {
    ...baseline,
    sessions: baseline.sessions.filter(session => session.sessionId === sessionId),
    records: baseline.records.filter(record => record.sessionId === sessionId),
  }
}

function notifyControlRecord(
  peer: TuiNotifyPeer,
  record: ReturnType<ControlPlaneRouter['store']['records']>[number],
): void {
  if (record.stream === 'mux') {
    const frame = record.frame as TuiMuxFrame
    peer.notify('events.mux', { ...frame, generation: record.generation })
  } else {
    peer.notify('events.host', { ...record.frame, generation: record.generation })
  }
}
