/**
 * Stream wiring extracted so tests can drive the gateway without Cordis inject.
 *
 * @module @dsh-pager-grok/tui-server/serve
 */

import type { Readable, Writable } from 'node:stream'
import type { TuiBackend } from './backend.js'
import { TuiGateway } from './gateway.js'
import { TuiLineTransport, type TuiLineTransportOptions } from './transport.js'

export interface TuiServeOptions extends TuiLineTransportOptions {
}

/**
 * Bind a TUI gateway to caller-owned streams.
 * @param bridge - in-process Harness controller adapter.
 * @param input - framed request stream.
 * @param output - framed response/notification stream.
 * @param options - bounded output/backpressure policy.
 * @returns a disposer that aborts pumps and detaches listeners.
 */
export function serve(
  bridge: TuiBackend,
  input: Readable,
  output: Writable,
  options: TuiServeOptions = {},
): () => void {
  const transport = new TuiLineTransport(input, output, options)
  const gateway = new TuiGateway(bridge, transport)
  transport.onRequest(async (method, params, id) =>
    gateway.handleRequest(method, params, String(id)))
  transport.start()
  return () => {
    gateway.dispose()
    bridge.dispose()
    transport.close()
  }
}
