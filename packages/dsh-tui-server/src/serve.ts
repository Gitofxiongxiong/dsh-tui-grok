/**
 * Stream wiring extracted so tests can drive the gateway without Cordis inject.
 *
 * @module @dsh-pager-grok/tui-server/serve
 */

import type { Readable, Writable } from 'node:stream'
import type { ApiProxy } from '@deepseek-ai/dsh-host-apiproxy/api'
import type { FileReferenceService } from '@deepseek-ai/dsh-file-reference'
import { TuiGateway } from './gateway.js'
import { TuiLineTransport, type TuiLineTransportOptions } from './transport.js'
import type { TuiDispatchExtensions } from './dispatch.js'
import type { SessionModeServices } from './session-mode.js'

export interface TuiServeOptions extends TuiLineTransportOptions {
  /** Optional public Harness file-reference provider mounted by the profile. */
  fileReferences?: FileReferenceService
  /** Public Harness session-to-agent resolver used by file-reference lookup. */
  resolveAgent?: TuiDispatchExtensions['resolveAgent']
  /** In-process session-mode writers used by `tui.setSessionMode`. */
  sessionMode?: SessionModeServices
}

/**
 * Bind a TUI gateway to caller-owned streams.
 * @param api - host ApiProxy.
 * @param input - framed request stream.
 * @param output - framed response/notification stream.
 * @param options - bounded output/backpressure policy.
 * @returns a disposer that aborts pumps and detaches listeners.
 */
export function serve(
  api: ApiProxy,
  input: Readable,
  output: Writable,
  options: TuiServeOptions = {},
): () => void {
  const { fileReferences, resolveAgent, sessionMode, ...transportOptions } = options
  const transport = new TuiLineTransport(input, output, transportOptions)
  const gateway = new TuiGateway(api, transport, { fileReferences, resolveAgent, sessionMode })
  transport.onRequest(async (method, params, id) =>
    gateway.handleRequest(method, params, String(id)))
  transport.start()
  return () => {
    gateway.dispose()
    transport.close()
  }
}
