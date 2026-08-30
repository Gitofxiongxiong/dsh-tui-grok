import type {
  ApiResult,
  HostFrame,
  SessionId,
  TuiUnaryMethod,
} from '@dsh-pager-grok/tui-protocol'
import type {
  TuiBackend,
  TuiBackendInfo,
  TuiMuxEnvelope,
} from '../../core/backend.js'
import { assertBackendSelection } from '../../core/backend-selection.js'
import type {
  ApiProxyV1Extensions,
  ApiProxyV1Like,
  ToFetchHandlerLike,
} from './context.js'
import { apiProxyV1Info } from './plugin.js'
import { apiErrorResult, respondReceipt } from './normalize.js'
import {
  apiProxyHostFrames,
  apiProxyMuxFrames,
  emptySessionFollower,
} from './streams.js'
import { callApiProxyV1Unary } from './unary.js'

export interface ApiProxyV1BackendOptions {
  api: ApiProxyV1Like
  dshVersion: string
  toFetchHandler: ToFetchHandlerLike
  extensions?: ApiProxyV1Extensions
}

/** TuiBackend adapter for DSH releases that expose Host ApiProxy v1. */
export class ApiProxyV1Backend implements TuiBackend {
  readonly info: TuiBackendInfo
  private readonly extensions: ApiProxyV1Extensions
  private readonly handler: ReturnType<ToFetchHandlerLike>

  constructor(private readonly options: ApiProxyV1BackendOptions) {
    if (typeof options.toFetchHandler !== 'function') {
      throw new Error('apiproxy-v1 requires a validated toFetchHandler runtime export')
    }
    this.extensions = options.extensions ?? {}
    this.info = apiProxyV1Info(options.dshVersion, this.extensions)
    assertBackendSelection(this.info)
    this.handler = options.toFetchHandler(options.api)
    if (typeof this.handler?.fetch !== 'function') {
      throw new Error('apiproxy-v1 toFetchHandler must return a fetch-capable handler')
    }
  }

  async call(
    method: TuiUnaryMethod,
    params: unknown,
    operationId: string,
    signal: AbortSignal,
  ): Promise<ApiResult> {
    try {
      return await callApiProxyV1Unary(
        this.options.api,
        this.handler,
        this.extensions,
        method,
        params,
        operationId,
        signal,
      )
    } catch (error: unknown) {
      return apiErrorResult(error, signal)
    }
  }

  attachSession(_sessionId: SessionId): void {}

  detachSession(_sessionId: SessionId): void {}

  followSession(sessionId: SessionId, signal: AbortSignal): AsyncIterable<TuiMuxEnvelope> {
    return emptySessionFollower(sessionId, signal)
  }

  muxFrames(signal: AbortSignal): AsyncIterable<TuiMuxEnvelope> {
    return apiProxyMuxFrames(this.options.api, signal)
  }

  hostFrames(signal: AbortSignal): AsyncIterable<HostFrame> {
    return apiProxyHostFrames(this.options.api, signal)
  }

  async respond(requestId: string, value: unknown): Promise<{ accepted: boolean; reason?: string }> {
    return respondReceipt(await this.options.api.respond({
      type: 'client-response',
      rpcId: requestId,
      result: { ok: true, value },
    }))
  }

  resetConnection(): void {}

  dispose(): void {}
}

export type {
  ApiProxyV1Extensions,
  ApiProxyV1Like,
  ProfileRequireLike,
  ToFetchHandlerLike,
} from './context.js'
export { resolveApiProxyV1Runtime } from './runtime.js'
