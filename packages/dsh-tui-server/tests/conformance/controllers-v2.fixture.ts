import { SessionId } from '@dsh-pager-grok/tui-protocol'
import {
  ControllersV2Backend,
  type TuiHarnessContext,
} from '../../src/adapters/controllers-v2/backend.ts'
import { CONFORMANCE_SESSION_ID } from './goldens.ts'
import type {
  AdapterConformanceFixture,
  ConformanceCalls,
  RecordLike,
} from './types.ts'

type Listener = (...args: unknown[]) => unknown

function hang(signal: AbortSignal): Promise<never> {
  return new Promise((_, reject) => {
    const fail = (): void => reject(signal.reason instanceof Error
      ? signal.reason
      : new Error(String(signal.reason ?? 'aborted')))
    if (signal.aborted) fail()
    else signal.addEventListener('abort', fail, { once: true })
  })
}

async function* held<Value>(values: readonly Value[], signal: AbortSignal): AsyncIterable<Value> {
  for (const value of values) yield value
  await hang(signal)
}

export function createControllersV2Fixture(): AdapterConformanceFixture {
  const listeners = new Map<string, Set<Listener>>()
  const agent = { id: CONFORMANCE_SESSION_ID, session: { events: [] as RecordLike[] } }
  const calls: ConformanceCalls = {
    prompt: [],
    page: [],
    inspect: [],
    resolveAgent: [],
    settings: [],
    credentials: [],
  }
  let sessionFollow: readonly RecordLike[] = []
  let control: readonly RecordLike[] = []
  let workspace: readonly RecordLike[] = [{
    type: 'baseline',
    value: {
      items: [{
        workspaceId: 'workspace-1',
        path: '/work',
        title: 'Work',
        sessionIds: [CONFORMANCE_SESSION_ID],
        createdAt: '1',
        updatedAt: '1',
      }],
      archivedSessionIds: [],
    },
  }]
  let promptMode: 'resolve' | 'hang' = 'resolve'
  let sessionListError: unknown

  const context = {
    sessionController: {
      list: async () => {
        if (sessionListError !== undefined) throw sessionListError
        return {
          items: [{
            sessionId: CONFORMANCE_SESSION_ID,
            cwd: '/work',
            running: false,
            blank: false,
            projections: { asOfSeq: 3, values: { agentPreset: 'standard' } },
          }],
        }
      },
      search: async (request: RecordLike) => ({ items: [], query: request.query }),
      create: async () => ({ sessionId: CONFORMANCE_SESSION_ID }),
      selectModel: async (request: RecordLike) => request,
      modelCatalog: async () => ({
        default: { provider: 'deepseek', model: 'deepseek-chat' },
        routableProviders: ['deepseek'],
        groups: [{ provider: 'deepseek', models: [{ id: 'deepseek-chat' }] }],
        failures: [],
      }),
      canOpenWorkspacePath: () => true,
      openWorkspacePath: async (request: RecordLike) => ({ opened: request.path }),
      rename: async (request: RecordLike) => ({ title: request.title, seq: 4 }),
      fork: async () => ({ sessionId: SessionId('forked') }),
      prompt: async (request: RecordLike, signal: AbortSignal) => {
        calls.prompt.push(request)
        if (promptMode === 'hang') return await hang(signal)
        return { accepted: true }
      },
      attachment: async () => ({ attachment: {}, data: '' }),
      updateQueue: () => ({ accepted: true }),
      cancel: () => ({ accepted: true }),
      inspect: async (sessionId: ReturnType<typeof SessionId>) => {
        calls.inspect.push(String(sessionId))
        return {
          meta: { id: sessionId },
          events: [
            { type: 'user/message', seq: 0, time: 1, data: { content: [] } },
            { type: 'assistant/message', seq: 1, time: 2, data: { content: [] } },
          ],
        }
      },
      page: async (request: RecordLike) => {
        calls.page.push(request)
        return {
          records: [{
            type: 'event',
            event: { type: 'user/message', seq: 0, time: 1, data: { content: [] } },
          }],
          hasMore: false,
        }
      },
      follow: (_request: RecordLike, signal: AbortSignal) => held(sessionFollow, signal),
      control: (signal: AbortSignal) => held(control, signal),
      resolveAgent: async (sessionId: ReturnType<typeof SessionId>) => {
        calls.resolveAgent.push(String(sessionId))
        return { agent }
      },
    },
    workspaceController: {
      create: async (request: RecordLike) => request,
      rename: async (request: RecordLike) => request,
      delete: async (request: RecordLike) => request,
      insertBefore: async () => ({ workspaceIds: ['workspace-1'] }),
      insertSessionBefore: async () => ({ sessionIds: [CONFORMANCE_SESSION_ID] }),
      archiveSession: async () => ({ archived: true }),
      follow: (signal: AbortSignal) => held(workspace, signal),
    },
    directoryPickerController: {
      pick: async () => '/work',
      list: async (path: string | undefined) => ({ path, entries: [{ name: 'src', kind: 'directory' }] }),
      createDirectory: async (path: string, name: string) => `${path}/${name}`,
    },
    settingsController: {
      describe: () => ({ namespaces: [{ ns: 'llm', revision: 1 }] }),
      canOpenAgentPresetDirectory: () => true,
      update: async (ns: string, value: RecordLike) => {
        calls.settings.push({ kind: 'update', value: { ns, value } })
        return { revision: 2, value }
      },
      replace: async (ns: string, value: RecordLike) => {
        calls.settings.push({ kind: 'replace', value: { ns, value } })
        return { revision: 2, value }
      },
      mutate: async (ns: string, value: unknown[]) => {
        calls.settings.push({ kind: 'mutate', value: { ns, value } })
        return { revision: 2, value: {} }
      },
      openSettingsDocument: async () => ({ opened: true }),
      openAgentPresetDirectory: async () => ({ opened: true }),
    },
    credentialsController: {
      describe: async (refs: string[]) => Object.fromEntries(refs.map(ref => [ref, {
        configured: false,
        writable: true,
      }])),
      set: async (ref: string, value: string) => {
        calls.credentials.push({ kind: 'set', value: { ref, value } })
      },
      unset: async (ref: string) => {
        calls.credentials.push({ kind: 'unset', value: { ref } })
      },
    },
    agentPresets: {
      remoteExportList: async () => ({ presets: [{ id: 'standard' }], authorable: true }),
      readDocument: async (agentPreset: string) => ({ agentPreset, document: '# preset' }),
      remoteExportCopy: async () => undefined,
      remoteExportDelete: async () => undefined,
      select: async (_agent: unknown, id: string) => id,
    },
    goals: {
      remoteExportCreate: () => ({ id: 'goal-1', revision: 1 }),
      edit: () => ({ id: 'goal-1', revision: 2 }),
      pause: () => ({ id: 'goal-1', revision: 2 }),
      resume: () => ({ id: 'goal-1', revision: 2 }),
      complete: () => ({ id: 'goal-1', revision: 2 }),
      clear: () => undefined,
    },
    llm: {
      listProviders: () => [{ id: 'deepseek', name: 'DeepSeek' }],
      listConfigurableProviders: () => [{
        provider: 'deepseek',
        displayName: 'DeepSeek',
        settingsNs: 'llm',
        settingsPath: ['deepseek'],
      }],
      remoteDiscoverModels: async () => [{ id: 'deepseek-chat' }],
    },
    subagents: {
      remoteExportList: async () => ({ entries: [{ childSessionId: SessionId('child-1') }], parentAvailable: true }),
      prompt: async () => ({ messageId: 'message-1' }),
      interruptByParent: () => ({ accepted: true }),
    },
    commands: {
      list: () => [{ name: 'compact', description: 'Compact history' }],
      execute: async () => ({ commandId: 'command-1', result: { kind: 'success', text: 'ok' } }),
    },
    sessionFileReferences: {
      list: async () => [{ path: 'src/main.ts', kind: 'file' }],
    },
    sessionSkillCatalog: {
      list: async () => ({ entries: [{ name: 'review' }] }),
    },
    agents: {
      get: (id: ReturnType<typeof SessionId>) => id === CONFORMANCE_SESSION_ID ? agent : undefined,
      list: () => [agent],
      roots: () => [agent],
    },
    tools: { get: () => undefined },
    on(event: string, listener: Listener) {
      const bucket = listeners.get(event) ?? new Set<Listener>()
      bucket.add(listener)
      listeners.set(event, bucket)
      return () => bucket.delete(listener)
    },
  } as unknown as TuiHarnessContext

  return {
    backend: new ControllersV2Backend(context),
    sessionId: CONFORMANCE_SESSION_ID,
    agent,
    calls,
    setSessionFollow(frames) { sessionFollow = frames },
    setControl(frames) { control = frames },
    setWorkspace(frames) { workspace = frames },
    setPromptMode(mode) { promptMode = mode },
    failSessionList(error) { sessionListError = error },
    emit(event, ...args) {
      return [...(listeners.get(event) ?? [])].map(listener => listener(...args))
    },
  }
}
