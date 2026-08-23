/**
 * Host-derived multi-session control-plane state.
 *
 * The session log remains owned by the host.  This store keeps only bounded,
 * value-backed snapshots needed to render a roster and to cross reconnect/load
 * barriers.  Transcript `session/event` frames are classified as presentation
 * traffic and are never used as the control-plane baseline.
 *
 * @module @dsh-pager-grok/tui-server/control-plane
 */

import type {
  HostFrame,
  MuxFrame,
  ResumeClass,
  TuiMuxFrame,
} from '@dsh-pager-grok/tui-protocol'

/** A value-backed projection cell. */
export interface SessionProjectionSnapshot {
  seq: number
  value: unknown
}

/** A pending approval/question kept in the control-plane snapshot. */
export interface PendingInteractionSnapshot {
  requestId: string
  kind: 'approval' | 'question'
  approvalId?: string
  toolName?: string
  reason?: string
  questions?: unknown[]
}

/** Metadata supplied by a host/session-added frame. */
export interface SessionRosterMetadata {
  blank?: boolean
  parentSessionId?: string
  origin?: string
  cwd?: string
  agentPreset?: string
}

/** One session's bounded control-plane projection. */
export interface SessionControlSnapshot extends SessionRosterMetadata {
  sessionId: string
  workspaceId?: string
  generation: number
  /** Host session.list activity time, kept separately from cache TTL activity. */
  updatedAtMs?: number
  lastSeenSeq?: number
  subscribedLastSeq?: number
  projectionWatermark?: number
  projections: Record<string, SessionProjectionSnapshot>
  queue: unknown[]
  /** Internal baseline bit; an empty snapshot is still a first update. */
  queueInitialized?: boolean
  jobs: unknown[]
  /** Internal baseline bit; an empty snapshot is still a first update. */
  jobsInitialized?: boolean
  pendingInteractions: PendingInteractionSnapshot[]
  running?: boolean
  lastError?: { code?: string; message: string; details?: unknown }
  removed?: boolean
  archived?: boolean
  lastActivityAt: number
}

/** Workspace state is intentionally value-backed: the host owns its schema. */
export interface WorkspaceControlSnapshot {
  workspaceId: string
  value: unknown
  order: number
  lastActivityAt: number
}

/** Connection lifecycle exposed alongside roster snapshots. */
export interface ControlPlaneConnectionState {
  phase: 'connected' | 'reconnecting' | 'baseline-required' | 'draining' | 'disconnected'
  generation: number
  lastError?: string
}

/** A bounded record retained for a control-plane replay. */
export interface ControlPlaneRecord {
  stream: 'mux' | 'host'
  generation: number
  sessionId?: string
  sequence?: number
  frame: TuiMuxFrame | HostFrame
  at: number
}

/** Complete baseline returned by a control-plane subscription. */
export interface ControlPlaneBaseline {
  generation: number
  resumeClass: ResumeClass
  sessions: SessionControlSnapshot[]
  workspaces: WorkspaceControlSnapshot[]
  workspaceOrder: string[]
  archivedSessionIds: string[]
  records: ControlPlaneRecord[]
}

/** Result of folding one incoming frame. */
export interface ControlPlaneApplyResult {
  accepted: boolean
  duplicate: boolean
  stale: boolean
  control: boolean
  presentation: boolean
  sessionId?: string
  sequence?: number
}

/** Store options. All limits are finite and apply to the complete cache. */
export interface ControlPlaneStoreOptions {
  /** Maximum retained session snapshots. Defaults to 512. */
  maxSessions?: number
  /** Maximum replay records per session and for host records. Defaults to 256. */
  maxRecordsPerSession?: number
  /** Idle snapshot TTL in milliseconds. Defaults to one hour. */
  ttlMs?: number
  /** Injectable clock for deterministic tests. */
  now?: () => number
}

const DEFAULT_MAX_SESSIONS = 512
const DEFAULT_MAX_RECORDS = 256
const DEFAULT_TTL_MS = 60 * 60 * 1000

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function clone<T>(value: T): T {
  // structuredClone is available on every supported Node runtime.  Keeping a
  // JSON fallback makes the store usable in small test runners as well.
  if (typeof structuredClone === 'function') return structuredClone(value)
  return JSON.parse(JSON.stringify(value)) as T
}

function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`
  if (isObject(value)) {
    return `{${Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`)
      .join(',')}}`
  }
  try {
    const encoded: unknown = JSON.stringify(value)
    return typeof encoded === 'string' ? encoded : 'undefined'
  } catch {
    return String(value)
  }
}

function sessionIdOf(frame: MuxFrame | HostFrame): string | undefined {
  const candidate = isObject(frame) ? frame as Record<string, unknown> : undefined
  if (candidate !== undefined && typeof candidate.sessionId === 'string') return candidate.sessionId
  return undefined
}

function sequenceOf(frame: MuxFrame | HostFrame): number | undefined {
  if (!isObject(frame)) return undefined
  if (frame.type === 'session/event' && isObject(frame.event) && typeof frame.event.seq === 'number') {
    return frame.event.seq
  }
  if (frame.type === 'session/projection' && typeof frame.seq === 'number') return frame.seq
  if (frame.type === 'session/subscribed' && typeof frame.lastSeq === 'number') return frame.lastSeq
  return undefined
}

function frameGenerationOf(frame: MuxFrame | HostFrame): number | undefined {
  const value = isObject(frame)
    ? (frame as Record<string, unknown>).generation
    : undefined
  if (typeof value !== 'number') return undefined
  return Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined
}

function withRequestId(frame: MuxFrame, requestId?: string): TuiMuxFrame | MuxFrame {
  if (requestId === undefined) return clone(frame)
  if (frame.type === 'approval/requested' || frame.type === 'question/requested') {
    return { ...frame, requestId }
  }
  return clone(frame)
}

function requestIdOf(frame: MuxFrame, requestId?: string): string | undefined {
  if (requestId !== undefined) return requestId
  const candidate = frame as unknown as Record<string, unknown>
  return typeof candidate.requestId === 'string' ? candidate.requestId : undefined
}

function snapshotFromSession(sessionId: string, generation: number, now: number): SessionControlSnapshot {
  return {
    sessionId,
    generation,
    projections: {},
    queue: [],
    jobs: [],
    pendingInteractions: [],
    lastActivityAt: now,
  }
}

/**
 * Bounded, generation-aware control-plane cache.
 *
 * `applyMux` and `applyHost` are idempotent: replaying a frame with the same
 * sequence, request id, or complete snapshot never changes a visible value.
 */
export class ControlPlaneStore {
  private readonly options: Required<ControlPlaneStoreOptions>
  private generation = 0
  private readonly sessionsById = new Map<string, SessionControlSnapshot>()
  private readonly sessionRecords = new Map<string, ControlPlaneRecord[]>()
  private readonly seenSequences = new Map<string, Set<number>>()
  private readonly hostRecords: ControlPlaneRecord[] = []
  private readonly workspacesById = new Map<string, WorkspaceControlSnapshot>()
  private workspaceOrderValue: string[] = []
  private archived = new Set<string>()
  private readonly requestFingerprints = new Map<string, string>()
  private readonly hostFingerprints = new Set<string>()
  private readonly hostFingerprintOrder: string[] = []
  private revisionValue = 0
  private connectionState: ControlPlaneConnectionState = {
    phase: 'baseline-required',
    generation: 0,
  }

  constructor(options: ControlPlaneStoreOptions = {}) {
    this.options = {
      maxSessions: Math.max(1, Math.floor(options.maxSessions ?? DEFAULT_MAX_SESSIONS)),
      maxRecordsPerSession: Math.max(1, Math.floor(options.maxRecordsPerSession ?? DEFAULT_MAX_RECORDS)),
      ttlMs: Math.max(1, Math.floor(options.ttlMs ?? DEFAULT_TTL_MS)),
      now: options.now ?? (() => Date.now()),
    }
  }

  /** Current connection generation. */
  get currentGeneration(): number {
    return this.generation
  }

  /** Monotonic value for UI consumers that need cheap change detection. */
  get revision(): number {
    return this.revisionValue
  }

  /** Current control-plane connection lifecycle. */
  connection(): ControlPlaneConnectionState {
    return clone(this.connectionState)
  }

  markPhase(phase: ControlPlaneConnectionState['phase'], lastError?: string): void {
    this.connectionState = {
      phase,
      generation: this.generation,
      ...lastError === undefined ? {} : { lastError },
    }
    this.revisionValue += 1
  }

  /**
   * Move to a new transport generation.  Old snapshots and replay records are
   * discarded atomically; the next stream baseline repopulates them.
   */
  setGeneration(generation: number): boolean {
    if (!Number.isSafeInteger(generation) || generation < 0 || generation === this.generation) return false
    this.generation = generation
    this.sessionsById.clear()
    this.sessionRecords.clear()
    this.seenSequences.clear()
    this.hostRecords.length = 0
    this.hostFingerprints.clear()
    this.hostFingerprintOrder.length = 0
    this.workspacesById.clear()
    this.workspaceOrderValue = []
    this.archived.clear()
    this.requestFingerprints.clear()
    this.connectionState = { phase: 'baseline-required', generation }
    this.revisionValue += 1
    return true
  }

  /** Remove expired snapshots and enforce the session count cap. */
  prune(now = this.options.now()): void {
    const expiry = now - this.options.ttlMs
    while (this.hostRecords[0]?.at !== undefined && this.hostRecords[0].at < expiry) {
      this.hostRecords.shift()
    }
    for (const records of this.sessionRecords.values()) {
      while (records[0]?.at !== undefined && records[0].at < expiry) records.shift()
    }
    for (const [id, snapshot] of this.sessionsById) {
      if (snapshot.lastActivityAt < expiry) {
        this.sessionsById.delete(id)
        this.sessionRecords.delete(id)
        this.seenSequences.delete(id)
      }
    }
    if (this.sessionsById.size <= this.options.maxSessions) return
    const oldest = [...this.sessionsById.values()]
      .sort((left, right) => left.lastActivityAt - right.lastActivityAt)
    for (const snapshot of oldest.slice(0, this.sessionsById.size - this.options.maxSessions)) {
      this.sessionsById.delete(snapshot.sessionId)
      this.sessionRecords.delete(snapshot.sessionId)
      this.seenSequences.delete(snapshot.sessionId)
    }
  }

  /** Read one immutable session snapshot. */
  snapshot(sessionId: string): SessionControlSnapshot | undefined {
    const value = this.sessionsById.get(sessionId)
    return value === undefined ? undefined : clone(value)
  }

  /** Read all session snapshots in stable id order. */
  snapshots(): SessionControlSnapshot[] {
    return [...this.sessionsById.values()]
      .sort((left, right) => left.sessionId.localeCompare(right.sessionId))
      .map(clone)
  }

  /** Seed roster/projection metadata from the host `session.list` value. */
  seedSessionList(value: unknown): void {
    if (!isObject(value) || !Array.isArray(value.items)) return
    const now = this.options.now()
    const listed = new Set(value.items
      .filter((item): item is Record<string, unknown> => isObject(item) && typeof item.sessionId === 'string')
      .map(item => item.sessionId as string))
    // The host response is a complete roster baseline.  Preserve an absent
    // snapshot as an explicit removed row so a stale selection can explain
    // why it disappeared instead of silently targeting a recycled id.
    for (const [id, snapshot] of this.sessionsById) {
      if (!listed.has(id)) {
        snapshot.removed = true
        snapshot.lastActivityAt = now
      }
    }
    for (const item of value.items) {
      if (!isObject(item) || typeof item.sessionId !== 'string') continue
      const snapshot = this.ensureSession(item.sessionId, now)
      snapshot.removed = false
      if (typeof item.blank === 'boolean') snapshot.blank = item.blank
      if (typeof item.running === 'boolean') snapshot.running = item.running
      // `session.list` is a complete metadata baseline. Clear optional fields
      // omitted by the host instead of retaining stale lineage/cwd data from
      // an earlier host/session-added frame.
      if (typeof item.parentSessionId === 'string') snapshot.parentSessionId = item.parentSessionId
      else delete snapshot.parentSessionId
      if (typeof item.origin === 'string') snapshot.origin = item.origin
      else delete snapshot.origin
      if (typeof item.cwd === 'string') snapshot.cwd = item.cwd
      else delete snapshot.cwd
      if (typeof item.agentPreset === 'string') snapshot.agentPreset = item.agentPreset
      else delete snapshot.agentPreset
      if (typeof item.updatedAt === 'number' && Number.isFinite(item.updatedAt)) {
        snapshot.updatedAtMs = item.updatedAt
      }
      // `lastActivityAt` is the cache eviction clock, not the host's
      // historical activity timestamp. A quiet but valid old session must not
      // disappear immediately after a fresh session.list baseline.
      snapshot.lastActivityAt = now
      const projections = isObject(item.projections) ? item.projections : undefined
      const asOfSeq = projections !== undefined && typeof projections.asOfSeq === 'number'
        ? projections.asOfSeq
        : undefined
      const projectionValues = projections !== undefined && isObject(projections.values)
        ? projections.values
        : undefined
      if (asOfSeq !== undefined && projectionValues !== undefined) {
        for (const [key, value] of Object.entries(projectionValues)) {
          const previous = snapshot.projections[key]
          if (previous === undefined || previous.seq < asOfSeq) {
            snapshot.projections[key] = { seq: asOfSeq, value: clone(value) }
          }
        }
        snapshot.projectionWatermark = Math.max(snapshot.projectionWatermark ?? -1, asOfSeq)
        snapshot.lastSeenSeq = Math.max(snapshot.lastSeenSeq ?? -1, asOfSeq)
      }
    }
    this.revisionValue += 1
    this.prune(now)
  }

  /** Seed workspace order and archive metadata from `workspace.list`. */
  seedWorkspaceList(value: unknown): void {
    if (!isObject(value) || !Array.isArray(value.items)) return
    const now = this.options.now()
    this.workspacesById.clear()
    this.workspaceOrderValue = []
    // `workspace.list` is a complete baseline. Clear old memberships before
    // applying the new accounts so a removed workspace cannot leave a stale
    // workspaceId on a live session snapshot.
    for (const snapshot of this.sessionsById.values()) delete snapshot.workspaceId
    for (const [index, item] of value.items.entries()) {
      if (!isObject(item) || typeof item.workspaceId !== 'string') continue
      const id = item.workspaceId
      this.workspacesById.set(id, {
        workspaceId: id,
        value: clone(item),
        order: index,
        lastActivityAt: now,
      })
      this.workspaceOrderValue.push(id)
      const sessionIds = Array.isArray(item.sessionIds)
        ? item.sessionIds.filter((sessionId): sessionId is string => typeof sessionId === 'string')
        : []
      this.updateWorkspaceMembership(id, sessionIds)
    }
    if (Array.isArray(value.archivedSessionIds)) {
      this.archived = new Set(value.archivedSessionIds.filter((id): id is string => typeof id === 'string'))
      for (const [id, snapshot] of this.sessionsById) snapshot.archived = this.archived.has(id)
    }
    this.revisionValue += 1
    this.prune(now)
  }

  /** Read workspace snapshots in host order, with a stable id tie-breaker. */
  workspaces(): WorkspaceControlSnapshot[] {
    const rank = new Map(this.workspaceOrderValue.map((id, index) => [id, index]))
    return [...this.workspacesById.values()]
      .sort((left, right) => (rank.get(left.workspaceId) ?? Number.MAX_SAFE_INTEGER)
        - (rank.get(right.workspaceId) ?? Number.MAX_SAFE_INTEGER)
        || left.workspaceId.localeCompare(right.workspaceId))
      .map(clone)
  }

  /** Current workspace order. */
  workspaceOrder(): string[] {
    return [...this.workspaceOrderValue]
  }

  /** Current archive set. */
  archivedSessionIds(): string[] {
    return [...this.archived].sort()
  }

  /** Build a value-backed baseline for a new control-plane subscriber. */
  baseline(resumeClass: ResumeClass = 'baseline-required'): ControlPlaneBaseline {
    this.prune()
    return {
      generation: this.generation,
      resumeClass,
      sessions: this.snapshots(),
      workspaces: this.workspaces(),
      workspaceOrder: this.workspaceOrder(),
      archivedSessionIds: this.archivedSessionIds(),
      records: this.records(),
    }
  }

  /** Return bounded control records for one session since an optional seq. */
  replay(sessionId?: string, since?: number): ControlPlaneRecord[] {
    const source = sessionId === undefined
      ? [...this.hostRecords, ...[...this.sessionRecords.values()].flat()]
      : [...(this.hostRecords.filter(record => record.sessionId === sessionId)), ...(this.sessionRecords.get(sessionId) ?? [])]
    return source
      .filter(record => record.generation === this.generation && (since === undefined || (record.sequence ?? -1) > since))
      .sort((left, right) => left.at - right.at)
      .map(clone)
  }

  /** Return all currently retained records, useful for a diagnostic baseline. */
  records(): ControlPlaneRecord[] {
    const records = [...this.hostRecords]
    for (const session of this.sessionRecords.values()) records.push(...session)
    return records.sort((left, right) => left.at - right.at).map(clone)
  }

  /** Fold a mux payload. */
  applyMux(frame: MuxFrame, generation = this.generation, requestId?: string): ControlPlaneApplyResult {
    const stampedGeneration = frameGenerationOf(frame) ?? generation
    return this.apply('mux', frame, stampedGeneration, requestId)
  }

  /** Fold a host payload. */
  applyHost(frame: HostFrame, generation = this.generation): ControlPlaneApplyResult {
    const stampedGeneration = frameGenerationOf(frame) ?? generation
    return this.apply('host', frame, stampedGeneration)
  }

  /**
   * Whether the bounded replay can satisfy a subscriber's watermark without
   * requiring a fresh baseline. A missing watermark is never a resume.
   */
  canResume(sessionId: string | undefined, since: number | undefined): boolean {
    if (since === undefined || !Number.isSafeInteger(since) || since < -1) return false
    const selected = sessionId === undefined ? undefined : this.snapshot(sessionId)
    if (sessionId !== undefined && selected === undefined) return false
    const targets = sessionId === undefined
      ? this.snapshots()
      : [selected as SessionControlSnapshot]
    if (sessionId === undefined && targets.length === 0) return false
    if (sessionId === undefined && this.hostRecords.some(record => record.sequence === undefined)) return false
    for (const snapshot of targets) {
      // Queue/jobs/interaction/status records do not carry the event cursor.
      // If any such record is retained for the target, the gateway cannot
      // prove that it is older than `since`; conservatively require a fresh
      // baseline instead of claiming a lossless resume.
      const retained = this.replay(snapshot.sessionId)
      if (retained.some(record => record.sequence === undefined)) return false
      const latest = snapshot.lastSeenSeq ?? snapshot.subscribedLastSeq ?? snapshot.projectionWatermark
      if (latest === undefined || latest <= since) continue
      const records = retained
        .filter(record => record.sequence !== undefined && record.sequence > since)
        .sort((left, right) => (left.sequence ?? -1) - (right.sequence ?? -1))
      const sequenced = records
        .filter(record => record.sequence !== undefined)
      if (sequenced.length === 0) return false
      const sequenceValues = [...new Set(sequenced
        .map(record => record.sequence)
        .filter((sequence): sequence is number => sequence !== undefined))]
        .sort((left, right) => left - right)
      const first = sequenceValues[0]
      if (first === undefined || first > since + 1) return false
      if (sequenceValues.some((value, index) => index > 0 && value > (sequenceValues[index - 1] as number) + 1)) return false
      const last = sequenceValues[sequenceValues.length - 1]
      if (last === undefined || last < latest) return false
    }
    return true
  }

  /** Remember a client mutation request and return whether it is a replay. */
  rememberRequest(requestId: string, payload: unknown): boolean {
    if (!requestId) return false
    const fingerprint = stableJson(payload)
    const previous = this.requestFingerprints.get(requestId)
    if (previous !== undefined) return true
    this.requestFingerprints.set(requestId, fingerprint)
    // Request ids are connection-local. Keep the table bounded with FIFO-like
    // oldest insertion semantics supplied by Map iteration order.
    while (this.requestFingerprints.size > this.options.maxRecordsPerSession * 4) {
      const first = this.requestFingerprints.keys().next().value
      if (first === undefined) break
      this.requestFingerprints.delete(first)
    }
    return false
  }

  private apply(
    stream: 'mux' | 'host',
    frame: MuxFrame | HostFrame,
    generation: number,
    requestId?: string,
  ): ControlPlaneApplyResult {
    this.prune()
    const sessionId = sessionIdOf(frame)
    const sequence = sequenceOf(frame)
    const effectiveRequestId = stream === 'mux'
      ? requestIdOf(frame as MuxFrame, requestId)
      : undefined
    const presentation = frame.type === 'session/event'
    const control = !presentation
    if (generation < this.generation) {
      return {
        accepted: false,
        duplicate: false,
        stale: true,
        control,
        presentation,
        ...sessionId === undefined ? {} : { sessionId },
        ...sequence === undefined ? {} : { sequence },
      }
    }
    if (generation > this.generation) this.setGeneration(generation)
    const now = this.options.now()
    const snapshot = sessionId === undefined ? undefined : this.ensureSession(sessionId, now)
    let duplicate = false

    if (stream === 'mux' && snapshot !== undefined) {
      const mux = frame as MuxFrame
      switch (mux.type) {
        case 'session/event': {
          const seq = mux.event.seq
          const seen = this.seenSequences.get(snapshot.sessionId) ?? new Set<number>()
          if (seen.has(seq)) duplicate = true
          else {
            seen.add(seq)
            while (seen.size > this.options.maxRecordsPerSession * 4) {
              const oldest = seen.values().next().value
              if (oldest === undefined) break
              seen.delete(oldest)
            }
            this.seenSequences.set(snapshot.sessionId, seen)
            snapshot.lastSeenSeq = Math.max(snapshot.lastSeenSeq ?? seq, seq)
          }
          break
        }
        case 'session/subscribed':
          if (snapshot.subscribedLastSeq !== undefined && mux.lastSeq <= snapshot.subscribedLastSeq) duplicate = true
          else snapshot.subscribedLastSeq = mux.lastSeq
          if (snapshot.lastSeenSeq === undefined || mux.lastSeq > snapshot.lastSeenSeq) snapshot.lastSeenSeq = mux.lastSeq
          break
        case 'session/projection': {
          const previous = snapshot.projections[mux.key]
          if (previous !== undefined && previous.seq >= mux.seq) duplicate = true
          else {
            snapshot.projections[mux.key] = { seq: mux.seq, value: clone(mux.value) }
            snapshot.projectionWatermark = Math.max(snapshot.projectionWatermark ?? -1, mux.seq)
            snapshot.lastSeenSeq = Math.max(snapshot.lastSeenSeq ?? -1, mux.seq)
          }
          break
        }
        case 'session/queue': {
          const next = clone(mux.items)
          duplicate = snapshot.queueInitialized === true && stableJson(snapshot.queue) === stableJson(next)
          if (!duplicate) snapshot.queue = next
          snapshot.queueInitialized = true
          break
        }
        case 'session/jobs': {
          const next = clone(mux.jobs)
          duplicate = snapshot.jobsInitialized === true && stableJson(snapshot.jobs) === stableJson(next)
          if (!duplicate) snapshot.jobs = next
          snapshot.jobsInitialized = true
          break
        }
        case 'approval/requested': {
          // approvalId is the host-owned interaction identity.  The carrier
          // rpc/request id may change when a pending request is replayed, so
          // it must not create a second actionable row.
          const approvalId = String(mux.approvalId)
          const key = effectiveRequestId || approvalId
          const next: PendingInteractionSnapshot = {
            requestId: key,
            kind: 'approval',
            approvalId,
            toolName: mux.toolName,
            ...mux.reason === undefined ? {} : { reason: mux.reason },
          }
          const index = snapshot.pendingInteractions.findIndex(item => item.kind === 'approval' && item.approvalId === approvalId)
          if (index >= 0) duplicate = true
          else snapshot.pendingInteractions.push(next)
          break
        }
        case 'question/requested': {
          const key = effectiveRequestId || `question:${stableJson(mux.questions)}`
          const next: PendingInteractionSnapshot = { requestId: key, kind: 'question', questions: clone(mux.questions) }
          const index = snapshot.pendingInteractions.findIndex(item => item.requestId === key)
          if (index >= 0) duplicate = stableJson(snapshot.pendingInteractions[index]) === stableJson(next)
          else snapshot.pendingInteractions.push(next)
          break
        }
        case 'approval/resolved': {
          const before = snapshot.pendingInteractions.length
          snapshot.pendingInteractions = snapshot.pendingInteractions.filter(item => item.approvalId !== String(mux.approvalId))
          duplicate = before === snapshot.pendingInteractions.length
          break
        }
        case 'question/resolved': {
          const before = snapshot.pendingInteractions.length
          const request = String(mux.questionRpcId)
          snapshot.pendingInteractions = snapshot.pendingInteractions.filter(item => item.requestId !== request)
          duplicate = before === snapshot.pendingInteractions.length
          break
        }
        case 'stream/error': {
          duplicate = stableJson(snapshot.lastError) === stableJson(mux.error)
          snapshot.lastError = clone(mux.error)
          break
        }
        default:
          break
      }
      // A duplicate frame is a replay, not new activity.  Keeping the
      // timestamp untouched makes idempotency observable and lets TTL pruning
      // evict a genuinely idle snapshot even when a carrier retries bytes.
      if (!duplicate) {
        snapshot.lastActivityAt = now
        snapshot.updatedAtMs = now
      }
    } else if (stream === 'host') {
      const fingerprint = stableJson(frame)
      duplicate = this.hostFingerprints.has(fingerprint)
      if (!duplicate) {
        this.rememberHostFingerprint(fingerprint)
        this.applyHostFrame(frame as HostFrame, now)
      }
    }

    if (!duplicate && frame.type === 'stream/error') {
      const message = isObject(frame.error) && typeof frame.error.message === 'string'
        ? frame.error.message
        : 'stream error'
      this.markPhase('reconnecting', message)
    } else if (!duplicate && this.connectionState.phase === 'baseline-required') {
      this.markPhase('connected')
    }

    if (!duplicate) {
      if (control || stream === 'host') {
        const storedFrame = stream === 'mux'
          ? withRequestId(frame as MuxFrame, effectiveRequestId)
          : clone(frame as HostFrame)
        this.record({
          stream,
          generation: this.generation,
          ...sessionId === undefined ? {} : { sessionId },
          ...sequence === undefined ? {} : { sequence },
          frame: storedFrame as TuiMuxFrame | HostFrame,
          at: now,
        })
      }
      this.revisionValue += 1
    }
    this.prune(now)
    return {
      accepted: true,
      duplicate,
      stale: false,
      control,
      presentation,
      ...sessionId === undefined ? {} : { sessionId },
      ...sequence === undefined ? {} : { sequence },
    }
  }

  private ensureSession(sessionId: string, now: number): SessionControlSnapshot {
    let snapshot = this.sessionsById.get(sessionId)
    if (snapshot === undefined) {
      snapshot = snapshotFromSession(sessionId, this.generation, now)
      this.sessionsById.set(sessionId, snapshot)
    }
    snapshot.generation = this.generation
    if (this.archived.has(sessionId)) snapshot.archived = true
    return snapshot
  }

  private applyHostFrame(frame: HostFrame, now: number): void {
    const sessionId = sessionIdOf(frame)
    if (sessionId !== undefined) {
      const snapshot = this.ensureSession(sessionId, now)
      switch (frame.type) {
        case 'host/session-added':
          snapshot.blank = frame.blank
          if (frame.parentSessionId !== undefined) snapshot.parentSessionId = String(frame.parentSessionId)
          else delete snapshot.parentSessionId
          if (frame.origin !== undefined) snapshot.origin = frame.origin
          else delete snapshot.origin
          if (frame.cwd !== undefined) snapshot.cwd = frame.cwd
          else delete snapshot.cwd
          if (frame.agentPreset !== undefined) snapshot.agentPreset = frame.agentPreset
          else delete snapshot.agentPreset
          snapshot.removed = false
          break
        case 'host/session-removed':
          snapshot.removed = true
          break
        case 'host/session-status':
          snapshot.running = frame.running
          break
        case 'host/agent-error':
          snapshot.lastError = { message: frame.message }
          break
        default:
          break
      }
      snapshot.lastActivityAt = now
    }
    switch (frame.type) {
      case 'host/workspace-changed': {
        const workspace = frame.workspace as unknown as Record<string, unknown>
        const id = String(workspace.workspaceId)
        const existing = this.workspacesById.get(id)
        this.workspacesById.set(id, {
          workspaceId: id,
          value: clone(frame.workspace),
          order: existing?.order ?? this.workspaceOrderValue.length,
          lastActivityAt: now,
        })
        if (!this.workspaceOrderValue.includes(id)) this.workspaceOrderValue.push(id)
        const sessionIds = Array.isArray(workspace.sessionIds)
          ? workspace.sessionIds.filter((value): value is string => typeof value === 'string')
          : []
        this.updateWorkspaceMembership(id, sessionIds)
        break
      }
      case 'host/workspace-removed': {
        const id = String(frame.workspaceId)
        this.workspacesById.delete(id)
        this.workspaceOrderValue = this.workspaceOrderValue.filter(item => item !== id)
        for (const snapshot of this.sessionsById.values()) {
          if (snapshot.workspaceId === id) delete snapshot.workspaceId
        }
        break
      }
      case 'host/workspace-order-changed':
        this.workspaceOrderValue = frame.workspaceIds.map(String)
        break
      case 'host/archived-sessions-changed':
        this.archived = new Set(frame.archivedSessionIds.map(String))
        for (const [id, snapshot] of this.sessionsById) snapshot.archived = this.archived.has(id)
        break
      default:
        break
    }
  }

  private updateWorkspaceMembership(workspaceId: string, sessionIds: string[]): void {
    const members = new Set(sessionIds)
    const now = this.options.now()
    for (const sessionId of members) {
      const snapshot = this.ensureSession(sessionId, now)
      snapshot.workspaceId = workspaceId
    }
    for (const snapshot of this.sessionsById.values()) {
      if (members.has(snapshot.sessionId)) snapshot.workspaceId = workspaceId
      else if (snapshot.workspaceId === workspaceId) delete snapshot.workspaceId
    }
  }

  private rememberHostFingerprint(fingerprint: string): void {
    this.hostFingerprints.add(fingerprint)
    this.hostFingerprintOrder.push(fingerprint)
    const limit = this.options.maxRecordsPerSession * 4
    while (this.hostFingerprintOrder.length > limit) {
      const oldest = this.hostFingerprintOrder.shift()
      if (oldest !== undefined) this.hostFingerprints.delete(oldest)
    }
  }

  private record(record: ControlPlaneRecord): void {
    if (record.sessionId !== undefined) {
      const records = this.sessionRecords.get(record.sessionId) ?? []
      records.push(record)
      while (records.length > this.options.maxRecordsPerSession) records.shift()
      this.sessionRecords.set(record.sessionId, records)
    } else {
      this.hostRecords.push(record)
      while (this.hostRecords.length > this.options.maxRecordsPerSession) this.hostRecords.shift()
    }
  }
}

/**
 * Router facade used by a client connection.  It keeps the store independent
 * from a particular attached SessionState and exposes the fan-out decision.
 */
export class ControlPlaneRouter {
  constructor(readonly store = new ControlPlaneStore()) {}

  /** Route a mux frame into the store and return its delivery classification. */
  routeMux(frame: MuxFrame, generation = this.store.currentGeneration, requestId?: string): ControlPlaneApplyResult {
    return this.store.applyMux(frame, generation, requestId)
  }

  /** Route a host frame into the store and return its delivery classification. */
  routeHost(frame: HostFrame, generation = this.store.currentGeneration): ControlPlaneApplyResult {
    return this.store.applyHost(frame, generation)
  }

  /** Reset the router for a new transport generation. */
  setGeneration(generation: number): boolean {
    return this.store.setGeneration(generation)
  }
}
