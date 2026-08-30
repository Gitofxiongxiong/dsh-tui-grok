import { decodeStorageRecord } from '@deepseek-ai/dsh-session/chunk-rows'
import type { HistoryRecord, RecordLike } from './context.js'
import { asRecord, requireString } from './normalize.js'

export const DEFAULT_MAX_MESSAGES = 50
const MESSAGE_TYPES = new Set(['user/message', 'assistant/message'])

export interface OpeningSnapshot {
  header: RecordLike
  cursor: number
  records: readonly HistoryRecord[]
  hasMore: boolean
  projections: RecordLike
}

export function recordsToEvents(records: readonly HistoryRecord[]): RecordLike[] {
  const events: RecordLike[] = []
  for (const record of records) {
    if (record.type === 'event') {
      events.push(record.event)
      continue
    }
    const event = record.event
    const type = typeof event.type === 'string' ? event.type.replace(/^chunkrow\//, '') : ''
    const expanded = decodeStorageRecord({
      type,
      seq0: event.seq,
      time0: event.time,
      data: event.data,
    })
    events.push(...expanded as unknown as RecordLike[])
  }
  return events
}

export function paginate(
  events: readonly RecordLike[],
  beforeSeq: number | undefined,
  maxMessages: number,
): { events: RecordLike[]; hasMore: boolean } {
  const window = beforeSeq === undefined
    ? [...events]
    : events.filter(event => typeof event.seq !== 'number' || event.seq < beforeSeq)
  let count = 0
  let cut = 0
  for (let index = window.length - 1; index >= 0; index -= 1) {
    const event = window[index] as RecordLike
    if (typeof event.type !== 'string' || !MESSAGE_TYPES.has(event.type)) continue
    count += 1
    let groupStart = typeof event.seq === 'number' ? event.seq : 0
    if (Array.isArray(event.sourceEventSeqs)) {
      for (const source of event.sourceEventSeqs) {
        if (typeof source === 'number' && source < groupStart) groupStart = source
      }
    }
    if (count >= maxMessages) {
      cut = groupStart
      break
    }
  }
  return {
    events: window.filter(event => typeof event.seq !== 'number' || event.seq >= cut),
    hasMore: cut > 0,
  }
}

export function rememberToolCall(
  calls: Map<string, { name: string; args: unknown }>,
  event: RecordLike,
): void {
  if (event.type === 'turn/end') {
    calls.clear()
    return
  }
  if (event.type !== 'tool/call') return
  try {
    const data = asRecord(event.data)
    calls.set(requireString(data, 'callId'), {
      name: requireString(data, 'name'),
      args: JSON.parse(requireString(data, 'arguments')),
    })
  } catch {
    // Generic tool cards cover malformed stored arguments.
  }
}

export function backscanArgs(
  events: readonly RecordLike[],
  callId: string,
): { name: string; args: unknown } | undefined {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index] as RecordLike
    if (event.type !== 'tool/call') continue
    try {
      const data = asRecord(event.data)
      if (data.callId !== callId) continue
      return { name: requireString(data, 'name'), args: JSON.parse(requireString(data, 'arguments')) }
    } catch {
      return undefined
    }
  }
  return undefined
}

export function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`
  if (value !== null && typeof value === 'object') {
    return `{${Object.entries(value as RecordLike)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`)
      .join(',')}}`
  }
  return JSON.stringify(value)
}
