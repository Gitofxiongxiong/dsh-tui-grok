# @dsh-pager-grok/tui-protocol

[English](README.md) | 中文

DeepSeek Harness 原生 TUI 客户端的共享协议格式。本包是外置纯库——无插件、无 Config、无注册——供协议两端共同使用：按行分帧的 JSON-RPC 2.0、TUI 控制方法、加载屏障类型，以及按 `RpcMethodMap` 原名转发的现有 ApiProxy 一元方法。运行时网关（`@dsh-pager-grok/tui-server`）服务此协议；原生 TUI 客户端使用相同结构。`fileReferences.list` 是外置 server 适配的可选 method，不要求修改 Harness 的 ApiProxy 源码。

## 分帧

`parseJsonRpcLine` / `serializeJsonRpcMessage` 将每个紧凑 JSON-RPC 2.0 对象编码为一行。带 `id` 与 `method` 的帧是请求，仅 `id` 是响应，仅 `method` 是通知。非法 JSON 与无效形状返回 `ParseResult` 失败；调用方忽略或记录它们。本包不持有字节流。

## 控制方法

| 方向 | 方法 | 类型 |
|---|---|---|
| client→server | `tui.hello` | `TuiHelloParams` → `TuiHelloResult` |
| client→server | `tui.attach` | `TuiAttachParams` → `TuiAttachResult` |
| client→server | `tui.detach` | `TuiDetachParams` → `{}` |
| client→server | `tui.subscribe` | `TuiSubscribeParams` → `TuiSubscribeResult` |
| client→server | `tui.respond` | `TuiRespondParams` → `{ accepted: boolean }` |
| server→client | `tui.serverReady` | `{}` |
| server→client | `tui.serverDraining` | `{}` |
| server→client | `tui.controlPlaneBaseline` | `TuiControlPlaneBaseline` |
| server→client | `events.mux` | `TuiMuxFrame` |
| server→client | `events.host` | `HostFrame` |

`tui.hello` 要求 `protocolVersion` 为 `1`，并返回 `serverInfo.name` `deepseek-harness-tui`。`tui.subscribe` 指定 `session`、`control-plane` 或 `all` 范围；只有有界 control-plane 记录覆盖请求水位时才返回 `resume-accepted`，否则返回并通知 `baseline-required`。session history 流仍没有游标回放，因此重连时仍需重新拉取 `session.history` 作为 presentation 内容基线。`generation` 在每次 hello 时递增；携带更旧 generation 的帧视为过期。

`classifyMethod` / `isTuiUnaryMethod` 识别同一连接上转发的一元业务方法（`session.history`、`session.prompt` 以及 `RpcMethodMap` 其余项）。这些参数在此保持不透明；由网关校验。

应用错误使用 JSON-RPC `error.data.kind`（`protocol-version`、`stale-generation`、`already-resolved`、`unknown-session`、`identity-mismatch`、`baseline-required`、`not-attached`、`capability-denied`）以及 `TUI_ERROR_CODES` 中的代码。

跨语言夹具位于 `tests/fixtures/`。

## 模型体验

无，因为此包定义面向客户端的协议格式；模型可见接口属于组合在对外服务的 TUI 网关后方的运行时插件。

#### KV Cache 影响

无；此包既不组装也不发送提供方请求。

## 已知限制与暂缓事项

- **无字节流传输**——编解码器解析并序列化行；把它们接到 stdio 或套接字属于 `tui-server` 与原生客户端。
- **无 transcript 游标回放**——ApiProxy 的 `events.mux` `since` 尚未实现；只有 `tui.subscribe` 背后的有界 control-plane 记录支持无损 resume 分类。
- **身份字段只解码、不强制**——`TuiClientIdentity` 被携带，以便日后的共享 server 拒绝不匹配的 profile、插件集合或 sandbox；本包不比较摘要。
