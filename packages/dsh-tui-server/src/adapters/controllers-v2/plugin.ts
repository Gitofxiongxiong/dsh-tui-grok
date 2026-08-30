import type { TuiBackendInfo } from '../../core/backend.js'

export const CONTROLLERS_V2_DSH_VERSION = '0.1.2-alpha.1'

export const CONTROLLERS_V2_INFO: TuiBackendInfo = {
  adapterFamily: 'controllers-v2',
  dshVersion: CONTROLLERS_V2_DSH_VERSION,
  profileSchema: 2,
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

/** Cordis services required by the controllers-v2 runtime plugin. */
export const CONTROLLERS_V2_INJECT = [
  'sessionController',
  'settingsController',
  'credentialsController',
  'workspaceController',
  'directoryPickerController',
  'agents',
  'commands',
  'llm',
  'subagents',
  'agentPresets',
  'goals',
  'sessionFileReferences',
  'sessionSkillCatalog',
  'tools',
] as const
