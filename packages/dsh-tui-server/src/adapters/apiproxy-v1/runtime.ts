import type {
  ProfileRequireLike,
  ToFetchHandlerLike,
} from './context.js'

export interface ApiProxyV1Runtime {
  toFetchHandler: ToFetchHandlerLike
}

/**
 * Resolve the ApiProxy carrier from the selected profile's module graph.
 * Callers must create the resolver beside that profile/runtime entry; using a
 * workspace-root resolver could silently select the wrong DSH architecture.
 */
export function resolveApiProxyV1Runtime(requireFromProfile: ProfileRequireLike): ApiProxyV1Runtime {
  let loaded: unknown
  try {
    loaded = requireFromProfile('@deepseek-ai/dsh-host-apiproxy')
  } catch (error: unknown) {
    throw new Error(
      `apiproxy-v1 could not resolve @deepseek-ai/dsh-host-apiproxy from the selected profile: ${error instanceof Error ? error.message : String(error)}`,
    )
  }
  if (typeof loaded !== 'object' || loaded === null) {
    throw new Error('apiproxy-v1 runtime module must export an object')
  }
  const toFetchHandler = (loaded as { toFetchHandler?: unknown }).toFetchHandler
  if (typeof toFetchHandler !== 'function') {
    throw new Error('apiproxy-v1 runtime is missing the required toFetchHandler export')
  }
  return { toFetchHandler: toFetchHandler as ToFetchHandlerLike }
}
