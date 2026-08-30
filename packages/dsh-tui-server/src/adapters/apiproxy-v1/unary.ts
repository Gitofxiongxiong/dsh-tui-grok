import type {
  ApiResult,
  SessionId,
  TuiUnaryMethod,
} from '@dsh-pager-grok/tui-protocol'
import type {
  AgentLike,
  ApiProxyV1Extensions,
  ApiProxyV1Like,
  FetchHandlerLike,
  RecordLike,
} from './context.js'
import { asRecord, normalizeApiResult } from './normalize.js'

export async function callApiProxyV1Unary(
  api: ApiProxyV1Like,
  handler: FetchHandlerLike,
  extensions: ApiProxyV1Extensions,
  method: TuiUnaryMethod,
  params: unknown,
  operationId: string,
  signal: AbortSignal,
): Promise<ApiResult> {
  if (method === 'fileReferences.list') return await fileReferences(params, extensions, signal)
  if (method === 'commands/list') return await commandsList(params, extensions)
  if (method === 'commands/execute') return await commandsExecute(params, extensions, signal)
  if (method === 'session.prompt') {
    let value: RecordLike
    try {
      value = asRecord(params, 'session prompt params')
    } catch {
      return failure('bad-request', 'session prompt params must be an object')
    }
    if (typeof value.requestId !== 'string' || value.requestId.length === 0) {
      return failure('bad-request', 'requestId must be a non-empty string')
    }
  }
  void api
  const response = await handler.fetch(new Request(`https://tui.local/api/${method}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      type: 'client-request',
      rpcId: operationId,
      method,
      payload: params ?? {},
    }),
    signal,
  }))
  if (response.status === 404) throw new Error(`ApiProxy method not found: ${method}`)
  if (!response.ok) throw new Error(`ApiProxy method ${method} failed with HTTP ${String(response.status)}`)
  const payload = asRecord(await response.json(), 'ApiProxy server response')
  return normalizeApiResult(payload.result)
}

async function resolveAgent(
  params: RecordLike,
  extensions: ApiProxyV1Extensions,
): Promise<{ agent: AgentLike } | { error: ApiResult }> {
  if (extensions.resolveAgent === undefined) {
    return { error: unsupported('agent resolver is not mounted in this apiproxy-v1 profile') }
  }
  const raw = typeof params.agentId === 'string' ? params.agentId : params.sessionId
  if (typeof raw !== 'string' || raw.length === 0) {
    return { error: failure('bad-request', 'agentId or sessionId is required') }
  }
  const resolved = await extensions.resolveAgent(raw as SessionId)
  if ('error' in resolved) {
    return { error: { ok: false, error: { ...resolved.error, details: resolved.error.details ?? {} } } }
  }
  return { agent: resolved.agent }
}

async function fileReferences(
  params: unknown,
  extensions: ApiProxyV1Extensions,
  signal: AbortSignal,
): Promise<ApiResult> {
  if (extensions.fileReferences === undefined) return unsupported('file reference service is not mounted')
  let value: RecordLike
  try {
    value = asRecord(params, 'file reference params')
  } catch {
    return failure('bad-request', 'file reference params must be an object')
  }
  if (typeof value.query !== 'string') return failure('bad-request', 'file reference query is required')
  const resolved = await resolveAgent(value, extensions)
  if ('error' in resolved) return resolved.error
  const items = await extensions.fileReferences.list(resolved.agent, value.query, signal)
  return { ok: true, value: { items } }
}

async function commandsList(
  params: unknown,
  extensions: ApiProxyV1Extensions,
): Promise<ApiResult> {
  if (extensions.commands === undefined) return unsupported('command service is not mounted')
  let value: RecordLike
  try {
    value = asRecord(params, 'command list params')
  } catch {
    return failure('bad-request', 'command list params must be an object')
  }
  const resolved = await resolveAgent(value, extensions)
  if ('error' in resolved) return resolved.error
  return { ok: true, value: extensions.commands.list(resolved.agent) }
}

async function commandsExecute(
  params: unknown,
  extensions: ApiProxyV1Extensions,
  signal: AbortSignal,
): Promise<ApiResult> {
  if (extensions.commands === undefined) return unsupported('command service is not mounted')
  let value: RecordLike
  try {
    value = asRecord(params, 'command execute params')
  } catch {
    return failure('bad-request', 'command execute params must be an object')
  }
  if (typeof value.line !== 'string' || value.line.length === 0) {
    return failure('bad-request', 'command line is required')
  }
  if (value.images !== undefined && (!Array.isArray(value.images) || value.images.length > 0)) {
    return failure('bad-request', 'native TUI command image attachments are not supported')
  }
  const resolved = await resolveAgent(value, extensions)
  if ('error' in resolved) return resolved.error
  const execution = await extensions.commands.execute(resolved.agent, value.line, [], signal)
  return { ok: true, value: { matched: execution !== undefined, execution: execution ?? null } }
}

function failure(code: string, message: string): ApiResult {
  return { ok: false, error: { code, message, details: {} } }
}

function unsupported(message: string): ApiResult {
  return failure('internal', message)
}
