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
  decodeSubscribeParams,
  parseJsonRpcLine,
  serializeJsonRpcMessage,
  tuiError,
} from './codec.js'
export type { DecodeFailure, DecodeResult, ParseFailure, ParseResult, ParseSuccess } from './codec.js'
export type { Branded } from './brand.js'
export { SessionId, TuiClientId } from './ids.js'
export {
  TUI_CAPABILITY_SET,
  TUI_METHOD_CAPABILITY_MAP,
  TUI_NOTIFICATION_METHOD_SET,
  TUI_REQUEST_METHOD_SET,
  TUI_UNARY_METHOD_SET,
  capabilityForTuiUnaryMethod,
  isTuiNotificationMethod,
  isTuiRequestMethod,
  isTuiUnaryMethod,
} from './methods.js'
export type { TuiCapability, TuiUnaryMethod } from './methods.js'
export type {
  ConnectionGeneration,
  ApiError,
  ApiResult,
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
  JobView,
  QueuedInboxItem,
  ToolEventView,
  WorkspaceView,
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
  TuiControlPlaneBaseline,
  TuiControlPlaneRecord,
  TuiSessionControlSnapshot,
  TuiSessionProjection,
  TuiSessionEvent,
  TuiStampedMuxFrame,
  TuiStampedHostFrame,
  TuiSubscribeScope,
  TuiSubscribeParams,
  TuiSubscribeResult,
} from './types.js'
