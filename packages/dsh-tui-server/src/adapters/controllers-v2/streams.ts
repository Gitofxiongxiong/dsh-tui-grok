export class AsyncQueue<Value> {
  private readonly values: Value[] = []
  private waiter: (() => void) | undefined
  private closed = false
  private error: unknown

  push(value: Value): void {
    if (this.closed) return
    this.values.push(value)
    this.waiter?.()
  }

  close(): void {
    if (this.closed) return
    this.closed = true
    this.waiter?.()
  }

  fail(error: unknown): void {
    this.error = error
    this.close()
  }

  async *read(signal: AbortSignal): AsyncIterable<Value> {
    const wake = (): void => this.waiter?.()
    signal.addEventListener('abort', wake, { once: true })
    try {
      while (!signal.aborted) {
        while (this.values.length > 0) yield this.values.shift() as Value
        if (this.closed) {
          if (this.error !== undefined) throw this.error
          return
        }
        await new Promise<void>(resolve => { this.waiter = resolve })
        this.waiter = undefined
      }
    } finally {
      signal.removeEventListener('abort', wake)
    }
  }
}
