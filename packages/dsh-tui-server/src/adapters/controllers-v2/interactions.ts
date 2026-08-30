import type { SessionId } from '@dsh-pager-grok/tui-protocol'
import type { RecordLike } from './context.js'

export type PendingInteraction =
  | {
    kind: 'approval'
    id: string
    sessionId: SessionId
    approvalId: string
    request: RecordLike
    resolve: (value: unknown) => void
    reject: (error: unknown) => void
    next: () => unknown
    disposeAbort?: () => void
  }
  | {
    kind: 'question'
    id: string
    sessionId: SessionId
    request: RecordLike
    resolve: (value: unknown) => void
    reject: (error: unknown) => void
    next: () => unknown
    disposeAbort?: () => void
  }
