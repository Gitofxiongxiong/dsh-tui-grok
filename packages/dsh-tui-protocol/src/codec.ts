/**
 * JSON-RPC 2.0 line codec plus TUI control-payload decoders. ApiProxy method
 * params are left opaque: the gateway validates them.
 *
 * @module @dsh-pager-grok/tui-protocol/codec
 */

import { SessionId } from '@deepseek-ai/dsh-session/types'
import { TUI_ERROR_CODES, TUI_PROTOCOL_VERSION, TUI_SERVER_INFO_NAME } from './constants.js'
import { TuiClientId } from './ids.js'
import { isApiProxyMethod, isTuiNotificationMethod, isTuiRequestMethod } from './methods.js'
import type {
  ConnectionGeneration,
  JsonRpcErrorObject,
  JsonRpcFailure,
  JsonRpcId,
  JsonRpcMessage,
  JsonRpcNotification,
  JsonRpcRequest,
  JsonRpcSuccess,
  ResumeClass,
  TuiAttachParams,
  TuiAttachResult,
  TuiClientCapabilities,
  TuiClientIdentity,
  TuiClientType,
  TuiDetachParams,
  TuiErrorData,
  TuiErrorKind,
  TuiHelloParams,
  TuiHelloResult,
  TuiInteractionResponse,
  TuiRespondParams,
  TuiSessionModeId,
  TuiSetSessionModeParams,
  TuiSubscribeScope,
  TuiSubscribeParams,
} from './types.js'

/** Line-parse failure. Malformed JSON is not exceptional on a byte stream. */
export type ParseFailure = { ok: false; reason: 'malformed-json' | 'invalid-shape' }

export type ParseSuccess = { ok: true; message: JsonRpcMessage }

export type ParseResult = ParseSuccess | ParseFailure

export type DecodeFailure = { ok: false; reason: string }

export type DecodeResult<T> = { ok: true; value: T } | DecodeFailure

const CLIENT_TYPES: ReadonlySet<string> = new Set(['tui', 'test'])
const RESUME_CLASSES: ReadonlySet<string> = new Set(['resume-accepted', 'baseline-required'])
const SESSION_MODE_IDS: ReadonlySet<string> = new Set(['normal', 'plan', 'danger-full-access'])
const ATTACH_ROLES: ReadonlySet<string> = new Set(['driver', 'subscriber'])
const SUBSCRIBE_SCOPES: ReadonlySet<string> = new Set(['session', 'control-plane', 'all'])

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function isJsonRpcId(value: unknown): value is JsonRpcId {
  return typeof value === 'string' || typeof value === 'number'
}

function isNonNegativeInt(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 && Number.isFinite(value)
}

function isSequenceWatermark(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= -1 && Number.isFinite(value)
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0
}

/**
 * Parse one newline-stripped JSON-RPC line.
 * @param line - a single frame without its terminating newline.
 * @returns the message, or a failure that callers must ignore or log.
 */
export function parseJsonRpcLine(line: string): ParseResult {
  let parsed: unknown
  try {
    parsed = JSON.parse(line) as unknown
  } catch {
    return { ok: false, reason: 'malformed-json' }
  }
  return decodeJsonRpcMessage(parsed)
}

/**
 * Decode an already-parsed JSON value as a JSON-RPC 2.0 message.
 * @param value - the parsed JSON value.
 * @returns the message or an invalid-shape failure.
 */
export function decodeJsonRpcMessage(value: unknown): ParseResult {
  if (!isRecord(value) || value.jsonrpc !== '2.0') {
    return { ok: false, reason: 'invalid-shape' }
  }
  const hasId = Object.hasOwn(value, 'id')
  const method = typeof value.method === 'string' ? value.method : undefined
  const hasResult = Object.hasOwn(value, 'result')
  const hasError = Object.hasOwn(value, 'error')

  if (method !== undefined && hasId) {
    if (!isJsonRpcId(value.id)) return { ok: false, reason: 'invalid-shape' }
    const request: JsonRpcRequest = {
      jsonrpc: '2.0',
      method,
      id: value.id,
    }
    if (Object.hasOwn(value, 'params')) request.params = value.params
    return { ok: true, message: request }
  }

  if (method !== undefined && !hasId) {
    const notification: JsonRpcNotification = {
      jsonrpc: '2.0',
      method,
    }
    if (Object.hasOwn(value, 'params')) notification.params = value.params
    return { ok: true, message: notification }
  }

  if (hasId && (hasResult || hasError) && method === undefined) {
    if (!isJsonRpcId(value.id)) return { ok: false, reason: 'invalid-shape' }
    if (hasResult && hasError) return { ok: false, reason: 'invalid-shape' }
    if (hasResult) {
      const success: JsonRpcSuccess = { jsonrpc: '2.0', id: value.id, result: value.result }
      return { ok: true, message: success }
    }
    const error = decodeErrorObject(value.error)
    if (!error.ok) return { ok: false, reason: 'invalid-shape' }
    const failure: JsonRpcFailure = { jsonrpc: '2.0', id: value.id, error: error.value }
    return { ok: true, message: failure }
  }

  return { ok: false, reason: 'invalid-shape' }
}

function decodeErrorObject(value: unknown): DecodeResult<JsonRpcErrorObject> {
  if (!isRecord(value) || typeof value.code !== 'number' || typeof value.message !== 'string') {
    return { ok: false, reason: 'invalid-error' }
  }
  const error: JsonRpcErrorObject = { code: value.code, message: value.message }
  if (Object.hasOwn(value, 'data')) {
    const data = decodeErrorData(value.data)
    if (data.ok) {
      error.data = data.value
    } else {
      return data
    }
  }
  return { ok: true, value: error }
}

function decodeErrorData(value: unknown): DecodeResult<TuiErrorData> {
  if (!isRecord(value) || !isTuiErrorKind(value.kind)) {
    return { ok: false, reason: 'invalid-error-data' }
  }
  const data: TuiErrorData = { kind: value.kind }
  if (Object.hasOwn(value, 'generation')) {
    if (!isNonNegativeInt(value.generation)) return { ok: false, reason: 'invalid-error-data' }
    data.generation = value.generation
  }
  if (Object.hasOwn(value, 'sessionId')) {
    if (!isNonEmptyString(value.sessionId)) return { ok: false, reason: 'invalid-error-data' }
    data.sessionId = SessionId(value.sessionId)
  }
  if (Object.hasOwn(value, 'requestId')) {
    if (!isNonEmptyString(value.requestId)) return { ok: false, reason: 'invalid-error-data' }
    data.requestId = value.requestId
  }
  return { ok: true, value: data }
}

function isTuiErrorKind(value: unknown): value is TuiErrorKind {
  return typeof value === 'string' && Object.hasOwn(TUI_ERROR_CODES, value)
}

/**
 * Serialize a JSON-RPC message to a compact line, without a trailing newline.
 * @param message - the message to encode.
 * @returns the compact JSON text.
 */
export function serializeJsonRpcMessage(message: JsonRpcMessage): string {
  return JSON.stringify(message)
}

/**
 * Build a JSON-RPC application error for a TUI control failure.
 * @param kind - the TUI error kind.
 * @param message - human-readable message.
 * @param extra - optional generation/session/request correlation.
 * @returns a JSON-RPC error object.
 */
export function tuiError(
  kind: TuiErrorKind,
  message: string,
  extra: Omit<TuiErrorData, 'kind'> = {},
): JsonRpcErrorObject {
  const data: TuiErrorData = { kind }
  if (extra.generation !== undefined) data.generation = extra.generation
  if (extra.sessionId !== undefined) data.sessionId = extra.sessionId
  if (extra.requestId !== undefined) data.requestId = extra.requestId
  return { code: TUI_ERROR_CODES[kind], message, data }
}

/**
 * Classify a JSON-RPC request or notification method.
 * @param method - the method name.
 * @returns which table owns the method, or `unknown`.
 */
export function classifyMethod(method: string): 'tui-request' | 'tui-notification' | 'api' | 'unknown' {
  if (isTuiRequestMethod(method)) return 'tui-request'
  if (isTuiNotificationMethod(method)) return 'tui-notification'
  if (isApiProxyMethod(method)) return 'api'
  return 'unknown'
}

/**
 * Decode `tui.hello` params.
 * @param params - the JSON-RPC params value.
 * @returns the typed params or a decode failure.
 */
export function decodeHelloParams(params: unknown): DecodeResult<TuiHelloParams> {
  if (!isRecord(params)) return { ok: false, reason: 'hello-params' }
  if (params.protocolVersion !== TUI_PROTOCOL_VERSION) {
    return { ok: false, reason: 'protocol-version' }
  }
  if (typeof params.clientType !== 'string' || !CLIENT_TYPES.has(params.clientType)) {
    return { ok: false, reason: 'client-type' }
  }
  const decoded: TuiHelloParams = {
    protocolVersion: TUI_PROTOCOL_VERSION,
    clientType: params.clientType as TuiClientType,
  }
  if (Object.hasOwn(params, 'clientId')) {
    if (!isNonEmptyString(params.clientId)) return { ok: false, reason: 'client-id' }
    decoded.clientId = TuiClientId(params.clientId)
  }
  if (Object.hasOwn(params, 'capabilities')) {
    const capabilities = decodeCapabilities(params.capabilities)
    if (!capabilities.ok) return capabilities
    decoded.capabilities = capabilities.value
  }
  if (Object.hasOwn(params, 'identity')) {
    const identity = decodeIdentity(params.identity)
    if (!identity.ok) return identity
    decoded.identity = identity.value
  }
  return { ok: true, value: decoded }
}

function decodeCapabilities(value: unknown): DecodeResult<TuiClientCapabilities> {
  if (!isRecord(value)) return { ok: false, reason: 'capabilities' }
  const capabilities: TuiClientCapabilities = {}
  for (const key of ['operator', 'observer', 'images'] as const) {
    if (!Object.hasOwn(value, key)) continue
    if (typeof value[key] !== 'boolean') return { ok: false, reason: 'capabilities' }
    capabilities[key] = value[key]
  }
  return { ok: true, value: capabilities }
}

function decodeIdentity(value: unknown): DecodeResult<TuiClientIdentity> {
  if (!isRecord(value)) return { ok: false, reason: 'identity' }
  const identity: TuiClientIdentity = {}
  for (const key of ['profile', 'cwd', 'pluginDigest', 'sandbox'] as const) {
    if (!Object.hasOwn(value, key)) continue
    if (typeof value[key] !== 'string') return { ok: false, reason: 'identity' }
    identity[key] = value[key]
  }
  return { ok: true, value: identity }
}

/**
 * Decode `tui.hello` result.
 * @param result - the JSON-RPC result value.
 * @returns the typed result or a decode failure.
 */
export function decodeHelloResult(result: unknown): DecodeResult<TuiHelloResult> {
  if (!isRecord(result)) return { ok: false, reason: 'hello-result' }
  if (result.protocolVersion !== TUI_PROTOCOL_VERSION) {
    return { ok: false, reason: 'protocol-version' }
  }
  if (!isNonEmptyString(result.clientId)) return { ok: false, reason: 'client-id' }
  if (!isNonNegativeInt(result.generation)) return { ok: false, reason: 'generation' }
  if (typeof result.resumeClass !== 'string' || !RESUME_CLASSES.has(result.resumeClass)) {
    return { ok: false, reason: 'resume-class' }
  }
  if (!isRecord(result.serverInfo)) return { ok: false, reason: 'server-info' }
  if (result.serverInfo.name !== TUI_SERVER_INFO_NAME) return { ok: false, reason: 'server-info' }
  if (typeof result.serverInfo.version !== 'string') return { ok: false, reason: 'server-info' }
  const serverInfo: TuiHelloResult['serverInfo'] = {
    name: TUI_SERVER_INFO_NAME,
    version: result.serverInfo.version,
  }
  if (Object.hasOwn(result.serverInfo, 'identityDigest')) {
    if (typeof result.serverInfo.identityDigest !== 'string') {
      return { ok: false, reason: 'server-info' }
    }
    serverInfo.identityDigest = result.serverInfo.identityDigest
  }
  return {
    ok: true,
    value: {
      protocolVersion: TUI_PROTOCOL_VERSION,
      clientId: TuiClientId(result.clientId),
      generation: result.generation,
      resumeClass: result.resumeClass as ResumeClass,
      serverInfo,
    },
  }
}

function decodeSessionGeneration(
  params: unknown,
  reason: string,
): DecodeResult<{ sessionId: SessionId; generation: ConnectionGeneration }> {
  if (!isRecord(params)) return { ok: false, reason }
  if (!isNonEmptyString(params.sessionId)) return { ok: false, reason: 'session-id' }
  if (!isNonNegativeInt(params.generation)) return { ok: false, reason: 'generation' }
  return {
    ok: true,
    value: { sessionId: SessionId(params.sessionId), generation: params.generation },
  }
}

/**
 * Decode `tui.attach` params.
 * @param params - the JSON-RPC params value.
 * @returns the typed params or a decode failure.
 */
export function decodeAttachParams(params: unknown): DecodeResult<TuiAttachParams> {
  return decodeSessionGeneration(params, 'attach-params')
}

/**
 * Decode `tui.attach` result.
 * @param result - the JSON-RPC result value.
 * @returns the typed result or a decode failure.
 */
export function decodeAttachResult(result: unknown): DecodeResult<TuiAttachResult> {
  if (!isRecord(result) || result.attached !== true) return { ok: false, reason: 'attach-result' }
  if (typeof result.role !== 'string' || !ATTACH_ROLES.has(result.role)) {
    return { ok: false, reason: 'attach-role' }
  }
  return { ok: true, value: { attached: true, role: result.role as TuiAttachResult['role'] } }
}

/**
 * Decode `tui.detach` params.
 * @param params - the JSON-RPC params value.
 * @returns the typed params or a decode failure.
 */
export function decodeDetachParams(params: unknown): DecodeResult<TuiDetachParams> {
  return decodeSessionGeneration(params, 'detach-params')
}

/**
 * Decode `tui.subscribe` params.
 * @param params - the JSON-RPC params value.
 * @returns the typed params or a decode failure.
 */
export function decodeSubscribeParams(params: unknown): DecodeResult<TuiSubscribeParams> {
  if (!isRecord(params)) return { ok: false, reason: 'subscribe-params' }
  if (!isNonNegativeInt(params.generation)) return { ok: false, reason: 'generation' }
  const hasSession = Object.hasOwn(params, 'sessionId')
  if (hasSession && !isNonEmptyString(params.sessionId)) return { ok: false, reason: 'session-id' }
  let scope: TuiSubscribeScope
  if (Object.hasOwn(params, 'scope')) {
    if (typeof params.scope !== 'string' || !SUBSCRIBE_SCOPES.has(params.scope)) {
      return { ok: false, reason: 'scope' }
    }
    scope = params.scope as TuiSubscribeScope
  } else {
    // Keep the original session-scoped wire form strict. An all-session
    // subscription is an explicit opt-in so an omitted sessionId cannot turn
    // an old client typo into a global fan-out.
    if (!hasSession) return { ok: false, reason: 'session-id' }
    scope = 'session'
  }
  if (scope === 'session' && !hasSession) return { ok: false, reason: 'session-id' }
  if (Object.hasOwn(params, 'since') && !isSequenceWatermark(params.since)) {
    return { ok: false, reason: 'since' }
  }
  return {
    ok: true,
    value: {
      generation: params.generation,
      ...hasSession ? { sessionId: SessionId(params.sessionId as string) } : {},
      scope,
      ...Object.hasOwn(params, 'since') ? { since: params.since as number } : {},
    },
  }
}

/**
 * Decode `tui.respond` params.
 * @param params - the JSON-RPC params value.
 * @returns the typed params or a decode failure.
 */
export function decodeRespondParams(params: unknown): DecodeResult<TuiRespondParams> {
  if (!isRecord(params)) return { ok: false, reason: 'respond-params' }
  const base = decodeSessionGeneration(params, 'respond-params')
  if (!base.ok) return base
  if (!isNonEmptyString(params.requestId)) {
    return { ok: false, reason: 'request-id' }
  }
  const interaction = decodeInteraction(params.interaction)
  if (!interaction.ok) return interaction
  return {
    ok: true,
    value: {
      sessionId: base.value.sessionId,
      generation: base.value.generation,
      requestId: params.requestId,
      interaction: interaction.value,
    },
  }
}

/**
 * Decode `tui.setSessionMode` params.
 * @param params - the JSON-RPC params value.
 * @returns the typed params or a decode failure.
 */
export function decodeSetSessionModeParams(params: unknown): DecodeResult<TuiSetSessionModeParams> {
  const base = decodeSessionGeneration(params, 'session-mode-params')
  if (!base.ok) return base
  if (!isRecord(params)) return { ok: false, reason: 'session-mode-params' }
  if (!Object.hasOwn(params, 'modeId')) {
    return { ok: true, value: { sessionId: base.value.sessionId, generation: base.value.generation } }
  }
  if (typeof params.modeId !== 'string' || !SESSION_MODE_IDS.has(params.modeId)) {
    return { ok: false, reason: 'mode-id' }
  }
  return {
    ok: true,
    value: {
      sessionId: base.value.sessionId,
      generation: base.value.generation,
      modeId: params.modeId as TuiSessionModeId,
    },
  }
}

function decodeInteraction(value: unknown): DecodeResult<TuiInteractionResponse> {
  if (!isRecord(value) || typeof value.type !== 'string') {
    return { ok: false, reason: 'interaction' }
  }
  if (value.type === 'approval') {
    if (!isNonEmptyString(value.approvalId) || !Object.hasOwn(value, 'outcome')) {
      return { ok: false, reason: 'interaction' }
    }
    return {
      ok: true,
      value: { type: 'approval', approvalId: value.approvalId, outcome: value.outcome },
    }
  }
  if (value.type === 'question') {
    if (!Object.hasOwn(value, 'answers')) return { ok: false, reason: 'interaction' }
    return { ok: true, value: { type: 'question', answers: value.answers } }
  }
  return { ok: false, reason: 'interaction' }
}
