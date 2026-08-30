export type DecodeStorageRecord = (value: unknown) => unknown[]

export interface ControllersV2Runtime {
  decodeStorageRecord: DecodeStorageRecord
}

export interface ProfileRequireLike {
  (id: string): unknown
}

/** Resolve alpha-only session codecs from the selected family runtime graph. */
export function resolveControllersV2Runtime(
  requireFromProfile: ProfileRequireLike,
): ControllersV2Runtime {
  let loaded: unknown
  try {
    loaded = requireFromProfile('@deepseek-ai/dsh-session/chunk-rows')
  } catch (error: unknown) {
    throw new Error(
      `controllers-v2 could not resolve @deepseek-ai/dsh-session/chunk-rows from the selected profile: ${error instanceof Error ? error.message : String(error)}`,
    )
  }
  if (typeof loaded !== 'object' || loaded === null) {
    throw new Error('controllers-v2 chunk-row runtime module must export an object')
  }
  const decodeStorageRecord = (loaded as { decodeStorageRecord?: unknown }).decodeStorageRecord
  if (typeof decodeStorageRecord !== 'function') {
    throw new Error('controllers-v2 runtime is missing the required decodeStorageRecord export')
  }
  return { decodeStorageRecord: decodeStorageRecord as DecodeStorageRecord }
}
