import type { SessionId } from '@dsh-pager-grok/tui-protocol'
import type {
  AgentLike,
  GoalServiceLike,
  RecordLike,
  TuiHarnessContext,
} from './context.js'
import {
  agentId,
  failure,
  flattenSessionList,
  optionalNumber,
  optionalString,
  requireArray,
  requireContinuable,
  requireRecord,
  requireString,
  requireStringArray,
} from './normalize.js'

export interface ControllersV2UnaryOperations {
  sessionHistory(params: RecordLike, signal: AbortSignal): Promise<RecordLike>
  sessionModels(sessionId: string, signal: AbortSignal): Promise<RecordLike>
  subagentHistory(params: RecordLike, signal: AbortSignal): Promise<RecordLike>
  hostDescription(): Promise<RecordLike>
  workspaceBaseline(signal: AbortSignal): Promise<unknown>
  agentPresetList(): Promise<RecordLike>
  mutateGoal(
    params: RecordLike,
    mutate: (goals: GoalServiceLike, agent: AgentLike) => unknown,
    alreadyRef?: boolean,
  ): Promise<unknown>
  providerDirectory(): RecordLike[]
  resolveAgent(sessionId: SessionId): Promise<AgentLike>
  requireService<Value>(value: Value | undefined, label: string): Value
}

/** Map one stable TUI unary method onto controllers-v2 services. */
export async function callControllersV2Unary(
  ctx: TuiHarnessContext,
  method: string,
  params: RecordLike,
  signal: AbortSignal,
  operations: ControllersV2UnaryOperations,
): Promise<unknown> {
  const session = ctx.sessionController
  switch (method) {
    case 'session.list':
      return flattenSessionList(await session.list(params, signal))
    case 'session.search':
      return await session.search(params, signal)
    case 'session.create':
      return await session.create(params)
    case 'session.history':
      return await operations.sessionHistory(params, signal)
    case 'session.models':
      return await operations.sessionModels(requireString(params, 'sessionId'), signal)
    case 'session.selectModel':
      return await session.selectModel(params)
    case 'session.rename':
      return await session.rename(params)
    case 'session.fork':
      return await session.fork(params)
    case 'session.prompt':
      requireString(params, 'requestId')
      return await session.prompt(params, signal)
    case 'session.attachment':
      return await session.attachment(params)
    case 'session.updateQueue':
      return await session.updateQueue(params)
    case 'session.cancel':
      return await session.cancel(params)
    case 'subagent.list':
      return await ctx.subagents.remoteExportList(
        requireString(params, 'parentSessionId') as SessionId,
        signal,
      )
    case 'subagent.history':
      return await operations.subagentHistory(params, signal)
    case 'subagent.prompt':
      requireString(params, 'requestId')
      return await ctx.subagents.prompt(params, signal)
    case 'subagent.interrupt':
      return ctx.subagents.interruptByParent(
        requireString(params, 'childSessionId') as SessionId,
        requireString(params, 'parentSessionId') as SessionId,
        requireContinuable(params),
      )
    case 'host.describe':
      return await operations.hostDescription()
    case 'host.pickDirectory':
      return { path: await ctx.directoryPickerController.pick(signal) }
    case 'host.listDirectory':
      return await ctx.directoryPickerController.list(optionalString(params, 'path'), signal)
    case 'host.createDirectory':
      return {
        path: await ctx.directoryPickerController.createDirectory(
          requireString(params, 'path'),
          requireString(params, 'name'),
        ),
      }
    case 'host.openPath':
      return await session.openWorkspacePath({ path: requireString(params, 'path') }, signal)
    case 'workspace.list':
      return await operations.workspaceBaseline(signal)
    case 'workspace.create':
      return await ctx.workspaceController.create(params)
    case 'workspace.rename':
      return await ctx.workspaceController.rename(params)
    case 'workspace.delete':
      return await ctx.workspaceController.delete(params)
    case 'workspace.insertBefore':
      return await ctx.workspaceController.insertBefore(params)
    case 'workspace.insertSessionBefore':
      return await ctx.workspaceController.insertSessionBefore(params)
    case 'workspace.archiveSession':
      return await ctx.workspaceController.archiveSession(params)
    case 'skill.list':
      return await operations.requireService(ctx.sessionSkillCatalog, 'session skill catalog')
        .list({ sessionId: requireString(params, 'sessionId') }, signal)
    case 'fileReferences.list': {
      const agent = await operations.resolveAgent(requireString(params, 'sessionId') as SessionId)
      const items = await operations.requireService(ctx.sessionFileReferences, 'file reference service')
        .list(agent, requireString(params, 'query'), signal)
      return { items }
    }
    case 'commands/list': {
      const agent = await operations.resolveAgent(agentId(params))
      return ctx.commands.list(agent)
    }
    case 'commands/execute': {
      const agent = await operations.resolveAgent(agentId(params))
      const line = requireString(params, 'line')
      const images = Array.isArray(params.images) ? params.images : []
      const execution = await ctx.commands.execute(agent, line, images, signal)
      return { matched: execution !== undefined, execution: execution ?? null }
    }
    case 'agentPreset.list':
      return await operations.agentPresetList()
    case 'agentPreset.select': {
      const presets = operations.requireService(ctx.agentPresets, 'agent preset service')
      const agent = await operations.resolveAgent(requireString(params, 'sessionId') as SessionId)
      return { agentPreset: await presets.select(agent, requireString(params, 'agentPreset')) }
    }
    case 'agentPreset.read':
      return await operations.requireService(ctx.agentPresets, 'agent preset service')
        .readDocument(requireString(params, 'agentPreset'))
    case 'agentPreset.copy': {
      const presets = operations.requireService(ctx.agentPresets, 'agent preset service')
      const id = requireString(params, 'agentPreset')
      await presets.remoteExportCopy(requireString(params, 'from'), id, optionalString(params, 'name'))
      return { agentPreset: id }
    }
    case 'agentPreset.openDocument':
      return await ctx.settingsController.openAgentPresetDirectory(
        requireString(params, 'agentPreset'), signal,
      )
    case 'agentPreset.remove':
      await operations.requireService(ctx.agentPresets, 'agent preset service')
        .remoteExportDelete(requireString(params, 'agentPreset'))
      return {}
    case 'goal.create':
      return await operations.mutateGoal(params, (goals, agent) => goals.remoteExportCreate(agent, {
        objective: requireString(params, 'objective'),
        ...params.maxGoalRounds === undefined ? {} : { maxGoalRounds: params.maxGoalRounds },
      }), true)
    case 'goal.edit':
      return await operations.mutateGoal(params, (goals, agent) => goals.edit(agent, params.ref, {
        ...params.objective === undefined ? {} : { objective: params.objective },
        ...params.maxGoalRounds === undefined ? {} : { maxGoalRounds: params.maxGoalRounds },
      }))
    case 'goal.pause':
      return await operations.mutateGoal(params, (goals, agent) => goals.pause(agent, params.ref))
    case 'goal.resume':
      return await operations.mutateGoal(params, (goals, agent) => goals.resume(agent, params.ref))
    case 'goal.complete':
      return await operations.mutateGoal(params, (goals, agent) => goals.complete(agent, params.ref))
    case 'goal.clear':
      await operations.mutateGoal(params, (goals, agent) => goals.clear(agent, params.ref), true)
      return { cleared: true }
    case 'settings.describe':
      return ctx.settingsController.describe()
    case 'settings.openDocument':
      return await ctx.settingsController.openSettingsDocument(signal)
    case 'settings.update':
      return await ctx.settingsController.update(
        requireString(params, 'ns'), requireRecord(params, 'patch'), optionalNumber(params, 'expectedRevision'),
      )
    case 'settings.replace':
      return await ctx.settingsController.replace(
        requireString(params, 'ns'), requireRecord(params, 'section'), optionalNumber(params, 'expectedRevision'),
      )
    case 'settings.mutate':
      return await ctx.settingsController.mutate(
        requireString(params, 'ns'), requireArray(params, 'ops'), optionalNumber(params, 'expectedRevision'),
      )
    case 'credentials.describe':
      return { credentials: await ctx.credentialsController.describe(requireStringArray(params, 'refs')) }
    case 'credentials.set':
      await ctx.credentialsController.set(requireString(params, 'ref'), requireString(params, 'value'))
      return {}
    case 'credentials.unset':
      await ctx.credentialsController.unset(requireString(params, 'ref'))
      return {}
    case 'llm.providers':
      return { providers: operations.providerDirectory() }
    case 'llm.models': {
      const catalog = await session.modelCatalog()
      return { groups: catalog.groups ?? [], failures: catalog.failures ?? [] }
    }
    case 'llm.discoverModels': {
      const settingsNs = requireString(params, 'settingsNs')
      const request = { ...params }
      delete request.settingsNs
      return { models: await ctx.llm.remoteDiscoverModels(settingsNs, request, signal) }
    }
    default:
      throw failure('bad-request', `unsupported TUI API method "${method}"`)
  }
}
