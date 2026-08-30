import type { SessionId } from '@dsh-pager-grok/tui-protocol'

export type RecordLike = Record<string, unknown>

export interface ApiProxyEnvelopeLike {
  rpcId: unknown
  payload: unknown
}

export interface ApiProxyV1Like {
  events: {
    mux(request: { rpcId: string; payload: RecordLike }, signal: AbortSignal): AsyncIterable<ApiProxyEnvelopeLike>
    host(request: { rpcId: string; payload: RecordLike }, signal: AbortSignal): AsyncIterable<ApiProxyEnvelopeLike>
  }
  respond(message: {
    type: 'client-response'
    rpcId: string
    result: { ok: true; value: unknown }
  }): Promise<unknown>
}

export interface FetchHandlerLike {
  fetch(request: Request): Promise<Response>
}

export type ToFetchHandlerLike = (api: ApiProxyV1Like) => FetchHandlerLike

export interface AgentLike {
  id: SessionId
}

export interface FileReferencesLike {
  list(agent: AgentLike, query: string, signal: AbortSignal): Promise<unknown[]>
}

export interface CommandsLike {
  list(agent: AgentLike): readonly unknown[]
  execute(agent: AgentLike, line: string, images: readonly unknown[], signal: AbortSignal): Promise<unknown>
}

export type ResolveAgentLike = (
  sessionId: SessionId,
) => Promise<{ agent: AgentLike } | { error: { code: string; message: string; details?: unknown } }>

export interface ApiProxyV1Extensions {
  fileReferences?: FileReferencesLike
  resolveAgent?: ResolveAgentLike
  commands?: CommandsLike
}

/** CommonJS resolver created from the selected profile runtime entry. */
export type ProfileRequireLike = (id: string) => unknown
