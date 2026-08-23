import { Writable } from 'node:stream'
import { describe, expect, it } from 'vitest'
import { parseJsonRpcLine } from '@dsh-pager-grok/tui-protocol'
import { TuiLineTransport } from '../src/transport.ts'

describe('TuiLineTransport backpressure', () => {
  it('bounds queued notifications and reports one drop per pressure episode', async () => {
    const chunks: string[] = []
    const callbacks: (() => void)[] = []
    const output = new Writable({
      write(chunk, _encoding, callback) {
        chunks.push(String(chunk))
        callbacks.push(callback)
      },
    })
    const pressure: number[] = []
    const transport = new TuiLineTransport(undefined as never, output, {
      maxQueuedFrames: 1,
      onBackpressure: queued => pressure.push(queued),
    })

    transport.notify('one', { value: 1 })
    transport.notify('two', { value: 2 })
    transport.notify('three', { value: 3 })
    expect(pressure).toEqual([1])
    await new Promise(resolve => setImmediate(resolve))
    expect(chunks).toHaveLength(1)
    expect(parseJsonRpcLine(chunks[0]?.trim() ?? '').ok).toBe(true)

    callbacks.shift()?.()
    await new Promise(resolve => setImmediate(resolve))
    transport.notify('four', { value: 4 })
    transport.notify('five', { value: 5 })
    expect(pressure).toEqual([1, 1])
    callbacks.shift()?.()
    output.destroy()
  })
})
