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
export const TUI_UNARY_METHOD_SET = {
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

export type TuiUnaryMethod = keyof typeof TUI_UNARY_METHOD_SET

/** Every client-to-server TUI control request method. */
export const TUI_REQUEST_METHOD_SET = {
  'tui.hello': true,
  'tui.attach': true,
  'tui.detach': true,
  'tui.subscribe': true,
  'tui.respond': true,
} as const satisfies Record<TuiRequestMethod, true>

/** Every server-to-client TUI control or stream notification method. */
export const TUI_NOTIFICATION_METHOD_SET = {
  'tui.serverReady': true,
  'tui.serverDraining': true,
  'tui.controlPlaneBaseline': true,
  'events.mux': true,
  'events.host': true,
} as const satisfies Record<TuiNotificationMethod, true>

/**
 * @param method - a JSON-RPC method name.
 * @returns whether the method is a TUI unary forwarded on this connection.
 */
export function isTuiUnaryMethod(method: string): method is TuiUnaryMethod {
  return Object.hasOwn(TUI_UNARY_METHOD_SET, method)
}

/**
 * @param method - a JSON-RPC method name.
 * @returns whether the method is a TUI control request.
 */
export function isTuiRequestMethod(method: string): method is TuiRequestMethod {
  return Object.hasOwn(TUI_REQUEST_METHOD_SET, method)
}

/**
 * @param method - a JSON-RPC method name.
 * @returns whether the method is a TUI control or stream notification.
 */
export function isTuiNotificationMethod(method: string): method is TuiNotificationMethod {
  return Object.hasOwn(TUI_NOTIFICATION_METHOD_SET, method)
}

/** @deprecated Use {@link TUI_UNARY_METHOD_SET} instead. */
export const API_PROXY_METHOD_SET = TUI_UNARY_METHOD_SET

/** @deprecated Use {@link TuiUnaryMethod} instead. */
export type ApiProxyMethod = TuiUnaryMethod

/** @deprecated Use {@link isTuiUnaryMethod} instead. */
export const isApiProxyMethod = isTuiUnaryMethod
