import type { SessionId } from '@dsh-pager-grok/tui-protocol'
import type { TuiBackend } from '../../src/core/backend.ts'

export type RecordLike = Record<string, unknown>

export interface ConformanceCalls {
  prompt: RecordLike[]
  page: RecordLike[]
  inspect: string[]
  resolveAgent: string[]
  settings: Array<{ kind: string; value: unknown }>
  credentials: Array<{ kind: string; value: unknown }>
}

/** Adapter-neutral controls exposed by one family-specific fake service set. */
export interface AdapterConformanceFixture {
  backend: TuiBackend
  sessionId: SessionId
  agent: { id: SessionId }
  calls: ConformanceCalls
  setSessionFollow(frames: readonly RecordLike[]): void
  setControl(frames: readonly RecordLike[]): void
  setWorkspace(frames: readonly RecordLike[]): void
  setPromptMode(mode: 'resolve' | 'hang'): void
  failSessionList(error: unknown): void
  emit(event: string, ...args: unknown[]): unknown[]
}

export type AdapterConformanceFactory = () => AdapterConformanceFixture
