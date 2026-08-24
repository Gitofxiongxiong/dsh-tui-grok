/**
 * Wire protocol for the DeepSeek Harness native TUI client: JSON-RPC line
 * framing, TUI control methods, load-barrier types, and ApiProxy method
 * forwarding. The runtime gateway plugin (`tui-server`) speaks this protocol;
 * the Rust pager client shares the same shapes.
 *
 * @module @dsh-pager-grok/tui-protocol
 */

export {
  TUI_ERROR_CODES,
  TUI_PROTOCOL_VERSION,
  TUI_SERVER_INFO_NAME,
} from './constants.js'
export {
  classifyMethod,
  decodeAttachParams,
  decodeAttachResult,
  decodeDetachParams,
  decodeHelloParams,
  decodeHelloResult,
  decodeJsonRpcMessage,
  decodeRespondParams,
  decodeSetSessionModeParams,
  decodeSubscribeParams,
  parseJsonRpcLine,
  serializeJsonRpcMessage,
  tuiError,
} from './codec.js'
export type { DecodeFailure, DecodeResult, ParseFailure, ParseResult, ParseSuccess } from './codec.js'
export { SessionId, TuiClientId } from './ids.js'
export {
  API_PROXY_METHOD_SET,
  isApiProxyMethod,
  isTuiNotificationMethod,
  isTuiRequestMethod,
} from './methods.js'
export type { ApiProxyMethod } from './methods.js'
export type {
  ConnectionGeneration,
  HostFrame,
  JsonRpcErrorObject,
  JsonRpcFailure,
  JsonRpcId,
  JsonRpcMessage,
  JsonRpcNotification,
  JsonRpcRequest,
  JsonRpcResponse,
  JsonRpcSuccess,
  LoadBacklogKind,
  MuxFrame,
  ResumeClass,
  RpcMethodMap,
  TuiAttachParams,
  TuiAttachResult,
  TuiAttachRole,
  TuiClientCapabilities,
  TuiClientIdentity,
  TuiClientType,
  TuiDetachParams,
  TuiErrorData,
  TuiErrorKind,
  TuiHelloParams,
  TuiHelloResult,
  TuiInteractionResponse,
  TuiMuxFrame,
  TuiNotificationMap,
  TuiNotificationMethod,
  TuiRequestMap,
  TuiRequestMethod,
  TuiRespondParams,
  TuiRespondResult,
  TuiSessionMode,
  TuiSessionModeId,
  TuiSetSessionModeParams,
  TuiSetSessionModeResult,
  TuiControlPlaneBaseline,
  TuiControlPlaneRecord,
  TuiSessionControlSnapshot,
  TuiSessionProjection,
  TuiStampedMuxFrame,
  TuiStampedHostFrame,
  TuiSubscribeScope,
  TuiSubscribeParams,
  TuiSubscribeResult,
} from './types.js'
