import type { TuiBackendInfo } from '../../core/backend.js'
import type { ApiProxyV1Extensions } from './context.js'

export function apiProxyV1Info(
  dshVersion: string,
  extensions: ApiProxyV1Extensions,
): TuiBackendInfo {
  if (dshVersion.length === 0) throw new Error('apiproxy-v1 requires an exact DSH version')
  const hasAgentExtensions = extensions.resolveAgent !== undefined
  return {
    adapterFamily: 'apiproxy-v1',
    dshVersion,
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
      fileReferences: hasAgentExtensions && extensions.fileReferences !== undefined,
      directoryPicker: true,
    },
  }
}
