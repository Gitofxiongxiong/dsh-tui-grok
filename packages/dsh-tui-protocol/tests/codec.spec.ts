import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import {
  API_PROXY_METHOD_SET,
  TUI_ERROR_CODES,
  TUI_PROTOCOL_VERSION,
  TUI_SERVER_INFO_NAME,
  TuiClientId,
  classifyMethod,
  decodeAttachParams,
  decodeAttachResult,
  decodeDetachParams,
  decodeHelloParams,
  decodeHelloResult,
  decodeJsonRpcMessage,
  decodeRespondParams,
  decodeSubscribeParams,
  isApiProxyMethod,
  isTuiNotificationMethod,
  isTuiRequestMethod,
  parseJsonRpcLine,
  serializeJsonRpcMessage,
  tuiError,
} from '../src/index.ts'
import { SessionId } from '../src/ids.ts'

const fixtures = join(dirname(fileURLToPath(import.meta.url)), 'fixtures')

function readFixture(name: string): unknown {
  return JSON.parse(readFileSync(join(fixtures, name), 'utf8')) as unknown
}

describe('parseJsonRpcLine', () => {
  it('parses a TUI hello request fixture', () => {
    const line = readFileSync(join(fixtures, 'hello-request.json'), 'utf8').trim()
    const parsed = parseJsonRpcLine(line)
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect(parsed.message).toMatchObject({ jsonrpc: '2.0', id: 1, method: 'tui.hello' })
    if (!('method' in parsed.message) || !('id' in parsed.message)) return
    const hello = decodeHelloParams(parsed.message.params)
    expect(hello).toEqual({
      ok: true,
      value: {
        protocolVersion: 1,
        clientType: 'tui',
        capabilities: { operator: true, observer: false, images: true },
        identity: {
          profile: 'tui-embedded',
          cwd: '/work',
          pluginDigest: 'abc',
          sandbox: 'off',
        },
      },
    })
  })

  it('parses a TUI hello result fixture', () => {
    const line = readFileSync(join(fixtures, 'hello-result.json'), 'utf8').trim()
    const parsed = parseJsonRpcLine(line)
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect('result' in parsed.message).toBe(true)
    if (!('result' in parsed.message)) return
    const result = decodeHelloResult(parsed.message.result)
    expect(result.ok).toBe(true)
    if (!result.ok) return
    expect(result.value.clientId).toBe('client-1')
    expect(result.value.resumeClass).toBe('baseline-required')
    expect(result.value.serverInfo.name).toBe(TUI_SERVER_INFO_NAME)
  })

  it('reports malformed JSON without throwing', () => {
    expect(parseJsonRpcLine('{')).toEqual({ ok: false, reason: 'malformed-json' })
  })
})

describe('decodeJsonRpcMessage', () => {
  it('rejects non-objects, arrays, and wrong jsonrpc versions', () => {
    expect(decodeJsonRpcMessage(null)).toEqual({ ok: false, reason: 'invalid-shape' })
    expect(decodeJsonRpcMessage([])).toEqual({ ok: false, reason: 'invalid-shape' })
    expect(decodeJsonRpcMessage({ jsonrpc: '1.0', method: 'x' })).toEqual({
      ok: false,
      reason: 'invalid-shape',
    })
  })

  it('decodes requests, notifications, and responses', () => {
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', method: 'tui.hello', id: 'a' })).toEqual({
      ok: true,
      message: { jsonrpc: '2.0', method: 'tui.hello', id: 'a' },
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', method: 'tui.hello', id: 'a', params: {} })).toEqual({
      ok: true,
      message: { jsonrpc: '2.0', method: 'tui.hello', id: 'a', params: {} },
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', method: 'tui.serverReady' })).toEqual({
      ok: true,
      message: { jsonrpc: '2.0', method: 'tui.serverReady' },
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', method: 'events.mux', params: { type: 'x' } })).toEqual({
      ok: true,
      message: { jsonrpc: '2.0', method: 'events.mux', params: { type: 'x' } },
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', id: 2, result: { ok: true } })).toEqual({
      ok: true,
      message: { jsonrpc: '2.0', id: 2, result: { ok: true } },
    })
  })

  it('rejects requests with a non-id, responses with both result and error, and incomplete responses', () => {
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', method: 'tui.hello', id: true })).toEqual({
      ok: false,
      reason: 'invalid-shape',
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', id: 1, result: 1, error: { code: 1, message: 'x' } })).toEqual({
      ok: false,
      reason: 'invalid-shape',
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', id: {}, result: 1 })).toEqual({
      ok: false,
      reason: 'invalid-shape',
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', id: 1 })).toEqual({
      ok: false,
      reason: 'invalid-shape',
    })
  })

  it('decodes error responses and rejects malformed error objects', () => {
    const error = tuiError('stale-generation', 'old', {
      generation: 3,
      sessionId: SessionId('s1'),
      requestId: 'r1',
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', id: 9, error })).toEqual({
      ok: true,
      message: { jsonrpc: '2.0', id: 9, error },
    })
    expect(decodeJsonRpcMessage({
      jsonrpc: '2.0',
      id: 8,
      error: { code: -32601, message: 'method not found' },
    })).toEqual({
      ok: true,
      message: {
        jsonrpc: '2.0',
        id: 8,
        error: { code: -32601, message: 'method not found' },
      },
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', id: 9, error: { code: 1 } })).toEqual({
      ok: false,
      reason: 'invalid-shape',
    })
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', id: 9, error: { code: 1, message: 'x', data: 1 } })).toEqual({
      ok: false,
      reason: 'invalid-shape',
    })
    expect(decodeJsonRpcMessage({
      jsonrpc: '2.0',
      id: 9,
      error: { code: 1, message: 'x', data: { kind: 'nope' } },
    })).toEqual({ ok: false, reason: 'invalid-shape' })
    expect(decodeJsonRpcMessage({
      jsonrpc: '2.0',
      id: 9,
      error: { code: 1, message: 'x', data: { kind: 'stale-generation', generation: -1 } },
    })).toEqual({ ok: false, reason: 'invalid-shape' })
    expect(decodeJsonRpcMessage({
      jsonrpc: '2.0',
      id: 9,
      error: { code: 1, message: 'x', data: { kind: 'unknown-session', sessionId: '' } },
    })).toEqual({ ok: false, reason: 'invalid-shape' })
    expect(decodeJsonRpcMessage({
      jsonrpc: '2.0',
      id: 9,
      error: { code: 1, message: 'x', data: { kind: 'already-resolved', requestId: '' } },
    })).toEqual({ ok: false, reason: 'invalid-shape' })
    const line = serializeJsonRpcMessage({ jsonrpc: '2.0', id: 9, error })
    expect(parseJsonRpcLine(line)).toEqual({
      ok: true,
      message: { jsonrpc: '2.0', id: 9, error },
    })
    const bare = tuiError('not-attached', 'missing')
    expect(decodeJsonRpcMessage({ jsonrpc: '2.0', id: 3, error: bare })).toEqual({
      ok: true,
      message: { jsonrpc: '2.0', id: 3, error: bare },
    })
  })
})

describe('serializeJsonRpcMessage', () => {
  it('round-trips a compact request', () => {
    const message = { jsonrpc: '2.0' as const, method: 'session.list', id: 4, params: { cursor: 'x' } }
    const line = serializeJsonRpcMessage(message)
    expect(line.includes('\n')).toBe(false)
    expect(parseJsonRpcLine(line)).toEqual({ ok: true, message })
  })
})

describe('classifyMethod', () => {
  it('splits TUI control, notifications, ApiProxy methods, and unknown names', () => {
    expect(classifyMethod('tui.hello')).toBe('tui-request')
    expect(classifyMethod('tui.attach')).toBe('tui-request')
    expect(classifyMethod('tui.serverReady')).toBe('tui-notification')
    expect(classifyMethod('events.host')).toBe('tui-notification')
    expect(classifyMethod('session.history')).toBe('api')
    expect(classifyMethod('session/prompt')).toBe('unknown')
    expect(isTuiRequestMethod('tui.respond')).toBe(true)
    expect(isTuiNotificationMethod('tui.serverDraining')).toBe(true)
    expect(isApiProxyMethod('llm.models')).toBe(true)
    expect(isApiProxyMethod('fileReferences.list')).toBe(true)
    expect(isApiProxyMethod('tui.hello')).toBe(false)
    expect(Object.keys(API_PROXY_METHOD_SET).length).toBeGreaterThan(20)
  })
})

describe('decodeHelloParams', () => {
  it('accepts a minimal hello and rejects bad versions, types, and nested fields', () => {
    expect(decodeHelloParams({ protocolVersion: TUI_PROTOCOL_VERSION, clientType: 'test' })).toEqual({
      ok: true,
      value: { protocolVersion: 1, clientType: 'test' },
    })
    expect(decodeHelloParams({
      protocolVersion: 1,
      clientType: 'tui',
      clientId: 'c1',
    })).toEqual({
      ok: true,
      value: { protocolVersion: 1, clientType: 'tui', clientId: TuiClientId('c1') },
    })
    expect(decodeHelloParams(null).ok).toBe(false)
    expect(decodeHelloParams({ protocolVersion: 2, clientType: 'tui' })).toEqual({
      ok: false,
      reason: 'protocol-version',
    })
    expect(decodeHelloParams({ protocolVersion: 1, clientType: 'web' })).toEqual({
      ok: false,
      reason: 'client-type',
    })
    expect(decodeHelloParams({ protocolVersion: 1, clientType: 'tui', clientId: '' })).toEqual({
      ok: false,
      reason: 'client-id',
    })
    expect(decodeHelloParams({ protocolVersion: 1, clientType: 'tui', capabilities: 1 })).toEqual({
      ok: false,
      reason: 'capabilities',
    })
    expect(decodeHelloParams({
      protocolVersion: 1,
      clientType: 'tui',
      capabilities: {},
    })).toEqual({
      ok: true,
      value: { protocolVersion: 1, clientType: 'tui', capabilities: {} },
    })
    expect(decodeHelloParams({
      protocolVersion: 1,
      clientType: 'tui',
      capabilities: { operator: true },
    })).toEqual({
      ok: true,
      value: { protocolVersion: 1, clientType: 'tui', capabilities: { operator: true } },
    })
    expect(decodeHelloParams({
      protocolVersion: 1,
      clientType: 'tui',
      capabilities: { operator: 'yes' },
    })).toEqual({ ok: false, reason: 'capabilities' })
    expect(decodeHelloParams({
      protocolVersion: 1,
      clientType: 'tui',
      identity: {},
    })).toEqual({
      ok: true,
      value: { protocolVersion: 1, clientType: 'tui', identity: {} },
    })
    expect(decodeHelloParams({ protocolVersion: 1, clientType: 'tui', identity: 1 })).toEqual({
      ok: false,
      reason: 'identity',
    })
    expect(decodeHelloParams({
      protocolVersion: 1,
      clientType: 'tui',
      identity: { cwd: 12 },
    })).toEqual({ ok: false, reason: 'identity' })
  })
})

describe('decodeHelloResult', () => {
  it('accepts the fixture result and rejects each required field', () => {
    const fixture = readFixture('hello-result.json') as { result: unknown }
    expect(decodeHelloResult(fixture.result).ok).toBe(true)
    expect(decodeHelloResult(null).ok).toBe(false)
    expect(decodeHelloResult({ protocolVersion: 2 }).ok).toBe(false)
    expect(decodeHelloResult({
      protocolVersion: 1,
      clientId: 'c',
      generation: 1,
      resumeClass: 'resume-accepted',
    }).ok).toBe(false)
    expect(decodeHelloResult({ protocolVersion: 1, clientId: '' }).ok).toBe(false)
    expect(decodeHelloResult({ protocolVersion: 1, clientId: 'c', generation: 1.5 }).ok).toBe(false)
    expect(decodeHelloResult({
      protocolVersion: 1,
      clientId: 'c',
      generation: 1,
      resumeClass: 'maybe',
    }).ok).toBe(false)
    expect(decodeHelloResult({
      protocolVersion: 1,
      clientId: 'c',
      generation: 1,
      resumeClass: 'baseline-required',
      serverInfo: { name: 'other', version: '1' },
    }).ok).toBe(false)
    expect(decodeHelloResult({
      protocolVersion: 1,
      clientId: 'c',
      generation: 1,
      resumeClass: 'resume-accepted',
      serverInfo: { name: TUI_SERVER_INFO_NAME, version: 1 },
    }).ok).toBe(false)
    expect(decodeHelloResult({
      protocolVersion: 1,
      clientId: 'c',
      generation: 1,
      resumeClass: 'resume-accepted',
      serverInfo: { name: TUI_SERVER_INFO_NAME, version: '1', identityDigest: 1 },
    }).ok).toBe(false)
    expect(decodeHelloResult({
      protocolVersion: 1,
      clientId: 'c',
      generation: 0,
      resumeClass: 'resume-accepted',
      serverInfo: { name: TUI_SERVER_INFO_NAME, version: '1' },
    }).ok).toBe(true)
  })
})

describe('session-scoped decoders', () => {
  const base = { sessionId: 'sess-1', generation: 2 }

  it('decodes attach, detach, and subscribe params', () => {
    expect(decodeAttachParams(base)).toEqual({
      ok: true,
      value: { sessionId: SessionId('sess-1'), generation: 2 },
    })
    expect(decodeDetachParams(base).ok).toBe(true)
    expect(decodeSubscribeParams(base).ok).toBe(true)
    expect(decodeAttachParams(null).ok).toBe(false)
    expect(decodeAttachParams({ sessionId: '', generation: 1 }).ok).toBe(false)
    expect(decodeAttachParams({ sessionId: 's', generation: -1 }).ok).toBe(false)
    expect(decodeSubscribeParams({ generation: 2, scope: 'all', since: -1 })).toEqual({
      ok: true,
      value: { generation: 2, scope: 'all', since: -1 },
    })
    expect(decodeSubscribeParams({ generation: 2, scope: 'session' }).ok).toBe(false)
    expect(decodeSubscribeParams({ generation: 2, scope: 'unknown' }).ok).toBe(false)
  })

  it('decodes attach results', () => {
    expect(decodeAttachResult({ attached: true, role: 'driver' })).toEqual({
      ok: true,
      value: { attached: true, role: 'driver' },
    })
    expect(decodeAttachResult({ attached: true, role: 'subscriber' }).ok).toBe(true)
    expect(decodeAttachResult({ attached: false, role: 'driver' }).ok).toBe(false)
    expect(decodeAttachResult({ attached: true, role: 'owner' }).ok).toBe(false)
  })

  it('decodes interaction responses', () => {
    expect(decodeRespondParams({
      ...base,
      requestId: 'rpc-1',
      interaction: { type: 'approval', approvalId: 'ap-1', outcome: { allow: true } },
    })).toEqual({
      ok: true,
      value: {
        sessionId: SessionId('sess-1'),
        generation: 2,
        requestId: 'rpc-1',
        interaction: { type: 'approval', approvalId: 'ap-1', outcome: { allow: true } },
      },
    })
    expect(decodeRespondParams({
      ...base,
      requestId: 'rpc-2',
      interaction: { type: 'question', answers: [1] },
    }).ok).toBe(true)
    expect(decodeRespondParams(null).ok).toBe(false)
    expect(decodeRespondParams({ sessionId: 'sess-1', generation: -1, requestId: 'r' }).ok).toBe(false)
    expect(decodeRespondParams({ ...base, requestId: '', interaction: { type: 'question', answers: [] } }).ok).toBe(false)
    expect(decodeRespondParams({ ...base, requestId: 'r', interaction: { type: 'approval' } }).ok).toBe(false)
    expect(decodeRespondParams({ ...base, requestId: 'r', interaction: { type: 'question' } }).ok).toBe(false)
    expect(decodeRespondParams({ ...base, requestId: 'r', interaction: { type: 'plan' } }).ok).toBe(false)
    expect(decodeRespondParams({ ...base, requestId: 'r' }).ok).toBe(false)
  })
})

describe('tuiError', () => {
  it('fills JSON-RPC codes and optional correlation fields', () => {
    expect(tuiError('protocol-version', 'bad version')).toEqual({
      code: TUI_ERROR_CODES['protocol-version'],
      message: 'bad version',
      data: { kind: 'protocol-version' },
    })
    expect(tuiError('capability-denied', 'no', { generation: 4 })).toMatchObject({
      code: TUI_ERROR_CODES['capability-denied'],
      data: { kind: 'capability-denied', generation: 4 },
    })
    expect(TuiClientId('x')).toBe('x')
  })
})
