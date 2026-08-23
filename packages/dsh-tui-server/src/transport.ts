/**
 * JSON-RPC 2.0 line transport over caller-owned streams. Uses the TUI codec
 * so malformed lines are ignored the same way both ends parse them.
 *
 * @module @dsh-pager-grok/tui-server/transport
 */

import type { Readable, Writable } from 'node:stream'
import { StringDecoder } from 'node:string_decoder'
import {
  parseJsonRpcLine,
  serializeJsonRpcMessage,
  tuiError,
  type JsonRpcErrorObject,
  type JsonRpcId,
} from '@dsh-pager-grok/tui-protocol'
import { TuiMethodNotFoundError, TuiRpcError } from './errors.js'

/** Handler for one inbound JSON-RPC request. */
export type TuiRequestHandler = (method: string, params: unknown, id: JsonRpcId) => Promise<unknown>

/** Bounded output policy for one slow TUI client. */
export interface TuiLineTransportOptions {
  /** Maximum frames waiting behind the active write. Defaults to 1024. */
  maxQueuedFrames?: number
  /** Called once when a notification is dropped for backpressure. */
  onBackpressure?: (queuedFrames: number) => void
}

/**
 * Line-delimited JSON-RPC endpoint. {@link start} attaches listeners;
 * {@link close} detaches them without destroying the streams.
 */
export class TuiLineTransport {
  private buffer = ''
  private readonly decoder = new StringDecoder('utf8')
  private started = false
  private closed = false
  private requestHandler: TuiRequestHandler | undefined
  private writeChain: Promise<void> = Promise.resolve()
  private readonly maxQueuedFrames: number
  private readonly onBackpressure: ((queuedFrames: number) => void) | undefined
  private queuedFrames = 0
  private backpressureReported = false

  constructor(
    private readonly input: Readable,
    private readonly output: Writable,
    options: TuiLineTransportOptions = {},
  ) {
    this.maxQueuedFrames = Math.max(1, Math.floor(options.maxQueuedFrames ?? 1024))
    this.onBackpressure = options.onBackpressure
  }

  /** Attach input listeners. Idempotent. */
  start(): void {
    if (this.started || this.closed) return
    this.started = true
    this.input.on('data', this.onData)
    this.input.on('error', this.onInputError)
    this.input.on('end', this.onInputEnd)
  }

  /** Detach listeners. Safe before {@link start}. */
  close(): void {
    if (this.closed) return
    this.closed = true
    this.input.off('data', this.onData)
    this.input.off('error', this.onInputError)
    this.input.off('end', this.onInputEnd)
  }

  /**
   * Install the inbound request handler.
   * @param handler - returns the JSON-RPC `result`; {@link TuiRpcError} becomes
   * an application error; {@link TuiMethodNotFoundError} becomes `-32601`.
   */
  onRequest(handler: TuiRequestHandler): void {
    this.requestHandler = handler
  }

  /**
   * Send a JSON-RPC notification.
   * @param method - notification method.
   * @param params - optional params object.
   */
  notify(method: string, params?: unknown): void {
    const message = params === undefined
      ? { jsonrpc: '2.0' as const, method }
      : { jsonrpc: '2.0' as const, method, params }
    this.enqueue(message)
  }

  private readonly onData = (chunk: Buffer | string): void => {
    this.buffer += typeof chunk === 'string' ? chunk : this.decoder.write(chunk)
    this.drainLines()
  }

  private drainLines(): void {
    for (;;) {
      const newline = this.buffer.indexOf('\n')
      if (newline < 0) break
      const line = this.buffer.slice(0, newline).trim()
      this.buffer = this.buffer.slice(newline + 1)
      if (!line) continue
      void this.handleLine(line)
    }
  }

  private readonly onInputError = (): void => {
    this.close()
  }

  private readonly onInputEnd = (): void => {
    this.buffer += this.decoder.end()
    this.drainLines()
    this.close()
  }

  private async handleLine(line: string): Promise<void> {
    const parsed = parseJsonRpcLine(line)
    if (!parsed.ok) return
    const message = parsed.message
    if (!('method' in message) || !('id' in message)) return
    const handler = this.requestHandler
    if (handler === undefined) {
      this.writeError(message.id, { code: -32601, message: `method not found: ${message.method}` })
      return
    }
    try {
      const result = await handler(message.method, message.params, message.id)
      this.enqueue({ jsonrpc: '2.0', id: message.id, result })
    } catch (error) {
      this.writeCaught(message.id, error)
    }
  }

  private writeCaught(id: JsonRpcId, error: unknown): void {
    if (error instanceof TuiRpcError) {
      this.writeError(id, tuiError(error.kind, error.message, error.extra))
      return
    }
    if (error instanceof TuiMethodNotFoundError) {
      this.writeError(id, { code: -32601, message: error.message })
      return
    }
    this.writeError(id, {
      code: -32603,
      message: error instanceof Error ? error.message : String(error),
    })
  }

  private writeError(id: JsonRpcId, error: JsonRpcErrorObject): void {
    this.enqueue({ jsonrpc: '2.0', id, error })
  }

  private enqueue(message: Parameters<typeof serializeJsonRpcMessage>[0]): void {
    const isNotification = !('id' in message)
    if (this.queuedFrames >= this.maxQueuedFrames && isNotification) {
      if (!this.backpressureReported) {
        this.backpressureReported = true
        this.onBackpressure?.(this.queuedFrames)
      }
      // Control-plane state is replayable from the gateway cache. Dropping a
      // notification for a slow client is therefore safer than blocking the
      // host event pump or other clients.
      return
    }
    const line = `${serializeJsonRpcMessage(message)}\n`
    this.queuedFrames += 1
    this.writeChain = this.writeChain
      .then(() => new Promise<void>((resolve) => {
        this.output.write(line, () => { resolve() })
      }))
      .finally(() => {
        this.queuedFrames = Math.max(0, this.queuedFrames - 1)
        if (this.queuedFrames < this.maxQueuedFrames) this.backpressureReported = false
      })
  }
}
