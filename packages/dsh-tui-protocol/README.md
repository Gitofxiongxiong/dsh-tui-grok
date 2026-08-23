# @dsh-pager-grok/tui-protocol

English | [中文](README.zh.md)

Shared wire protocol for the DeepSeek Harness native TUI client. The package is a pure library — no plugin, no Config, no registration — that both wire ends speak: JSON-RPC 2.0 line framing, TUI control methods, load-barrier types, and forwarding of existing ApiProxy unary methods under their `RpcMethodMap` names. The runtime gateway (`tui-server`) will serve this protocol; the native TUI client shares the same shapes. Source modules are not exported as deep imports.

## Framing

`parseJsonRpcLine` / `serializeJsonRpcMessage` encode one compact JSON-RPC 2.0 object per line. Frames with `id` and `method` are requests, `id` alone is a response, and `method` alone is a notification. Malformed JSON and invalid shapes return a `ParseResult` failure; callers ignore or log them. This package does not own byte streams.

## Control methods

| Direction | Method | Types |
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

`tui.hello` requires `protocolVersion` `1` and returns `serverInfo.name` `deepseek-harness-tui`. `tui.subscribe` names a `session`, `control-plane`, or `all` scope and returns `resume-accepted` only when the bounded control-plane records cover the requested watermark; otherwise it returns and notifies `baseline-required`. The session history stream still has no cursor replay, so a reconnecting client must refetch `session.history` for presentation content. `generation` increments per hello; frames that carry an older generation are stale.

`classifyMethod` / `isApiProxyMethod` identify unary ApiProxy methods forwarded on the same connection (`session.history`, `session.prompt`, and the rest of `RpcMethodMap`). Those params stay opaque here; the gateway validates them.

Application errors use JSON-RPC `error.data.kind` (`protocol-version`, `stale-generation`, `already-resolved`, `unknown-session`, `identity-mismatch`, `baseline-required`, `not-attached`, `capability-denied`) and the codes in `TUI_ERROR_CODES`.

Cross-language fixtures live in `tests/fixtures/`.

## Model Experience

None, as this package defines the client-facing wire protocol; the model-visible surfaces belong to the runtime plugins composed behind the serving TUI gateway.

#### KV Cache effect

None; this package neither assembles nor sends a provider request.

## Known Limitations and Deferred Work

- **No byte-stream transport** — the codec parses and serializes lines; attaching them to stdio or a socket belongs to `tui-server` and the native client.
- **No transcript cursor replay** — `events.mux` `since` is unimplemented in ApiProxy; only the bounded control-plane records behind `tui.subscribe` support a lossless resume classification.
- **Identity fields are decoded, not enforced** — `TuiClientIdentity` is carried so a later shared server can refuse a mismatched profile, plugin set, or sandbox; this package does not compare digests.
