import type { TuiBackendInfo } from './backend.js'

/**
 * Re-check the CLI's registry selection inside the selected adapter process.
 * Direct fixture/script launches set none of these variables and retain their
 * existing behavior.
 */
export function assertBackendSelection(
  info: TuiBackendInfo,
  env: NodeJS.ProcessEnv = process.env,
): void {
  const expectedFamily = env.DSH_PAGER_EXPECTED_ADAPTER_FAMILY
  const expectedVersion = env.DSH_PAGER_EXPECTED_DSH_VERSION
  const expectedSchema = env.DSH_PAGER_EXPECTED_PROFILE_SCHEMA
  if (expectedFamily !== undefined && expectedFamily !== info.adapterFamily) {
    throw new Error(
      `adapter family mismatch: CLI selected ${expectedFamily}, runtime mounted ${info.adapterFamily}`,
    )
  }
  if (expectedVersion !== undefined && expectedVersion !== info.dshVersion) {
    throw new Error(
      `DSH version mismatch: CLI selected ${expectedVersion}, adapter mounted ${info.dshVersion}`,
    )
  }
  if (expectedSchema !== undefined && expectedSchema !== String(info.profileSchema)) {
    throw new Error(
      `profile schema mismatch: CLI selected ${expectedSchema}, adapter mounted ${String(info.profileSchema)}`,
    )
  }
}
