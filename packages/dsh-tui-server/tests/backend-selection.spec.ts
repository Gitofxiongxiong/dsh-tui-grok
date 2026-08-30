import { describe, expect, it } from 'vitest'
import { assertBackendSelection } from '../src/core/backend-selection.ts'
import type { TuiBackendInfo } from '../src/core/backend.ts'

const info: TuiBackendInfo = {
  adapterFamily: 'apiproxy-v1',
  dshVersion: '0.1.1-rc.2',
  profileSchema: 1,
  capabilities: {
    sessions: true,
    workspaces: true,
    settings: true,
    credentials: true,
    agentPresets: true,
    goals: true,
    subagents: true,
    approvals: true,
    questions: true,
    queue: true,
    jobs: true,
    skills: true,
    fileReferences: true,
    directoryPicker: true,
  },
}

describe('adapter startup selection assertion', () => {
  it('accepts direct launches and an exact CLI selection', () => {
    expect(() => assertBackendSelection(info, {})).not.toThrow()
    expect(() => assertBackendSelection(info, {
      DSH_PAGER_EXPECTED_ADAPTER_FAMILY: 'apiproxy-v1',
      DSH_PAGER_EXPECTED_DSH_VERSION: '0.1.1-rc.2',
      DSH_PAGER_EXPECTED_PROFILE_SCHEMA: '1',
    })).not.toThrow()
  })

  it.each([
    [{ DSH_PAGER_EXPECTED_ADAPTER_FAMILY: 'controllers-v2' }, /family mismatch/],
    [{ DSH_PAGER_EXPECTED_DSH_VERSION: '0.1.0-rc.8' }, /version mismatch/],
    [{ DSH_PAGER_EXPECTED_PROFILE_SCHEMA: '2' }, /schema mismatch/],
  ])('fails closed for a mismatched %j', (env, expected) => {
    expect(() => assertBackendSelection(info, env)).toThrow(expected)
  })
})
