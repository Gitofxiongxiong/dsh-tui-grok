/**
 * Runtime method tables. The native TUI owns this compatibility catalog;
 * Harness controllers are an implementation detail of the server bridge.
 *
 * @module @dsh-pager-grok/tui-protocol/methods
 */

import type { TuiNotificationMethod, TuiRequestMethod } from './types.js'

/**
 * Every legacy unary method the TUI connection accepts. This list is frozen
 * independently of Harness' generated Typert catalog.
 */
export const API_PROXY_METHOD_SET = {
  'session.list': true,
  'session.search': true,
  'session.create': true,
  'session.history': true,
  'session.models': true,
  'session.selectModel': true,
  'session.rename': true,
  'session.fork': true,
  'session.prompt': true,
  'session.attachment': true,
  'session.updateQueue': true,
  'session.cancel': true,
  'subagent.list': true,
  'subagent.history': true,
  'subagent.prompt': true,
  'subagent.interrupt': true,
  'host.describe': true,
  'host.pickDirectory': true,
  'host.listDirectory': true,
  'host.createDirectory': true,
  'host.openPath': true,
  'fileReferences.list': true,
  'commands/list': true,
  'commands/execute': true,
  'workspace.list': true,
  'workspace.create': true,
  'workspace.rename': true,
  'workspace.delete': true,
  'workspace.insertBefore': true,
  'workspace.insertSessionBefore': true,
  'workspace.archiveSession': true,
  'skill.list': true,
  'agentPreset.list': true,
  'agentPreset.select': true,
  'agentPreset.read': true,
  'agentPreset.copy': true,
  'agentPreset.openDocument': true,
  'agentPreset.remove': true,
  'goal.create': true,
  'goal.edit': true,
  'goal.pause': true,
  'goal.resume': true,
  'goal.complete': true,
  'goal.clear': true,
  'settings.describe': true,
  'settings.openDocument': true,
  'settings.update': true,
  'settings.replace': true,
  'settings.mutate': true,
  'credentials.describe': true,
  'credentials.set': true,
  'credentials.unset': true,
  'llm.providers': true,
  'llm.models': true,
  'llm.discoverModels': true,
} as const

export type ApiProxyMethod = keyof typeof API_PROXY_METHOD_SET

const TUI_REQUEST_METHODS: Record<TuiRequestMethod, true> = {
  'tui.hello': true,
  'tui.attach': true,
  'tui.detach': true,
  'tui.subscribe': true,
  'tui.respond': true,
}

const TUI_NOTIFICATION_METHODS: Record<TuiNotificationMethod, true> = {
  'tui.serverReady': true,
  'tui.serverDraining': true,
  'tui.controlPlaneBaseline': true,
  'events.mux': true,
  'events.host': true,
}

/**
 * @param method - a JSON-RPC method name.
 * @returns whether the method is an ApiProxy unary forwarded on this connection.
 */
export function isApiProxyMethod(method: string): method is ApiProxyMethod {
  return Object.hasOwn(API_PROXY_METHOD_SET, method)
}

/**
 * @param method - a JSON-RPC method name.
 * @returns whether the method is a TUI control request.
 */
export function isTuiRequestMethod(method: string): method is TuiRequestMethod {
  return Object.hasOwn(TUI_REQUEST_METHODS, method)
}

/**
 * @param method - a JSON-RPC method name.
 * @returns whether the method is a TUI control or stream notification.
 */
export function isTuiNotificationMethod(method: string): method is TuiNotificationMethod {
  return Object.hasOwn(TUI_NOTIFICATION_METHODS, method)
}
