/**
 * Forward an ApiProxy unary method through the existing fetch carrier so
 * request schemas stay in one place.
 *
 * @module @dsh-pager-grok/tui-server/dispatch
 */

import { toFetchHandler } from '@deepseek-ai/dsh-host-apiproxy'
import type { ApiProxy } from '@deepseek-ai/dsh-host-apiproxy/api'
import type { ServerResponse } from '@deepseek-ai/dsh-host-apiproxy/api'
import { RpcId } from '@deepseek-ai/dsh-host-apiproxy/api'
import type { FileReferenceCandidate, FileReferenceService } from '@deepseek-ai/dsh-file-reference'
import type { Agent } from '@deepseek-ai/dsh-agent'
import type { CommandRuntime } from '@deepseek-ai/dsh-commands'
import type { SessionId } from '@deepseek-ai/dsh-session/types'
import { SessionId as brandSessionId } from '@deepseek-ai/dsh-session/types'
import { TuiMethodNotFoundError, TuiRpcError } from './errors.js'

export interface TuiDispatchExtensions {
  fileReferences?: FileReferenceService
  resolveAgent?: (sessionId: SessionId) => Promise<{ agent: Agent } | { error: { code: string; message: string; details?: unknown } }>
  /** Official DSH per-agent command runtime; the TUI never owns command semantics. */
  commands?: Pick<CommandRuntime, 'list' | 'execute'>
}

/**
 * Dispatch one ApiProxy unary call.
 * @param api - host ApiProxy.
 * @param method - RpcMethodMap key.
 * @param params - JSON-RPC params (the business payload).
 * @param rpcId - correlation id echoed into the ApiProxy envelope.
 * @returns the ApiProxy `RpcResult` (ok/value or ok/error).
 */
export async function dispatchUnary(
  api: ApiProxy,
  method: string,
  params: unknown,
  rpcId: string,
  extensions: TuiDispatchExtensions = {},
  signal: AbortSignal = new AbortController().signal,
): Promise<unknown> {
  if (method === 'fileReferences.list') {
    return await dispatchFileReferences(params, rpcId, extensions)
  }
  if (method === 'commands/list') {
    return await dispatchCommandsList(params, extensions)
  }
  if (method === 'commands/execute') {
    return await dispatchCommandsExecute(params, extensions, signal)
  }
  const handler = toFetchHandler(api)
  const response = await handler.fetch(new Request(`https://tui.local/api/${method}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      type: 'client-request',
      rpcId: RpcId(rpcId),
      method,
      payload: params ?? {},
    }),
  }))
  if (response.status === 404) throw new TuiMethodNotFoundError(method)
  if (!response.ok) {
    throw new TuiRpcError('not-attached', `api method ${method} failed with HTTP ${String(response.status)}`)
  }
  const json = await response.json() as ServerResponse
  return json.result
}

async function dispatchCommandsExecute(
  params: unknown,
  extensions: TuiDispatchExtensions,
  signal: AbortSignal,
): Promise<unknown> {
  if (extensions.commands === undefined || extensions.resolveAgent === undefined) {
    return {
      ok: false,
      error: {
        code: 'internal',
        message: 'command execution is unsupported by this external TUI composition',
        details: {},
      },
    }
  }
  if (params === null || typeof params !== 'object' || Array.isArray(params)) {
    return { ok: false, error: { code: 'invalid-request', message: 'invalid command execute params', details: {} } }
  }
  const value = params as { agentId?: unknown; line?: unknown; images?: unknown }
  if (typeof value.agentId !== 'string' || value.agentId.length === 0) {
    return { ok: false, error: { code: 'invalid-request', message: 'agentId is required', details: {} } }
  }
  if (typeof value.line !== 'string' || value.line.length === 0) {
    return { ok: false, error: { code: 'invalid-request', message: 'command line is required', details: {} } }
  }
  if (value.images !== undefined && (!Array.isArray(value.images) || value.images.length > 0)) {
    return {
      ok: false,
      error: {
        code: 'invalid-request',
        message: 'native TUI command image attachments are not supported',
        details: {},
      },
    }
  }
  const resolved = await extensions.resolveAgent(brandSessionId(value.agentId))
  if ('error' in resolved) return { ok: false, error: resolved.error }
  try {
    const execution = await extensions.commands.execute(resolved.agent, value.line, [], signal)
    return {
      ok: true,
      value: {
        matched: execution !== undefined,
        execution: execution ?? null,
      },
    }
  } catch (error: unknown) {
    return {
      ok: false,
      error: {
        code: signal.aborted ? 'cancelled' : 'internal',
        message: `command execution failed: ${error instanceof Error ? error.message : String(error)}`,
        details: {},
      },
    }
  }
}

async function dispatchCommandsList(
  params: unknown,
  extensions: TuiDispatchExtensions,
): Promise<unknown> {
  if (extensions.commands === undefined || extensions.resolveAgent === undefined) {
    return {
      ok: false,
      error: {
        code: 'internal',
        message: 'command discovery is unsupported by this external TUI composition',
        details: {},
      },
    }
  }
  if (params === null || typeof params !== 'object' || Array.isArray(params)) {
    return { ok: false, error: { code: 'invalid-request', message: 'invalid command list params', details: {} } }
  }
  const value = params as { agentId?: unknown }
  if (typeof value.agentId !== 'string' || value.agentId.length === 0) {
    return { ok: false, error: { code: 'invalid-request', message: 'agentId is required', details: {} } }
  }
  const resolved = await extensions.resolveAgent(brandSessionId(value.agentId))
  if ('error' in resolved) return { ok: false, error: resolved.error }
  try {
    return { ok: true, value: extensions.commands.list(resolved.agent) }
  } catch (error: unknown) {
    return {
      ok: false,
      error: {
        code: 'internal',
        message: `command discovery failed: ${error instanceof Error ? error.message : String(error)}`,
        details: {},
      },
    }
  }
}

async function dispatchFileReferences(
  params: unknown,
  rpcId: string,
  extensions: TuiDispatchExtensions,
): Promise<unknown> {
  if (extensions.fileReferences === undefined || extensions.resolveAgent === undefined) {
    return {
      ok: false,
      error: {
        code: 'internal',
        message: 'file reference search is unsupported by this external TUI composition',
        details: {},
      },
    }
  }
  if (params === null || typeof params !== 'object' || Array.isArray(params)) {
    return { ok: false, error: { code: 'invalid-request', message: 'invalid file reference params', details: {} } }
  }
  const value = params as { sessionId?: unknown; query?: unknown }
  if (typeof value.sessionId !== 'string' || typeof value.query !== 'string') {
    return { ok: false, error: { code: 'invalid-request', message: 'sessionId and query are required', details: {} } }
  }
  const resolved = await extensions.resolveAgent(brandSessionId(value.sessionId))
  if ('error' in resolved) return { ok: false, error: resolved.error }
  try {
    const items: FileReferenceCandidate[] = await extensions.fileReferences.list(
      resolved.agent,
      value.query,
      new AbortController().signal,
    )
    return { ok: true, value: { items } }
  } catch (error: unknown) {
    return {
      ok: false,
      error: { code: 'internal', message: `file reference search failed: ${error instanceof Error ? error.message : String(error)}`, details: {} },
    }
  }
}

/**
 * Forward a TUI interaction answer through ApiProxy.respond.
 * @param api - host ApiProxy.
 * @param rpcId - the pending interaction's rpcId.
 * @param value - approval or question payload.
 * @returns the carrier receipt.
 */
export async function dispatchRespond(
  api: ApiProxy,
  rpcId: string,
  value: unknown,
): Promise<unknown> {
  return await api.respond({
    type: 'client-response',
    rpcId: RpcId(rpcId),
    result: { ok: true, value },
  })
}
