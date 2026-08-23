# @dsh-pager-grok/tui-server

[English](README.md) | 中文

在调用方持有的流（生产环境为 stdin/stdout）上服务原生 TUI JSON-RPC 协议，并把业务调用转发到 `ctx.apiProxy` 的 Cordis 插件。仅具名导出（`name`、`inject`、`Config`、`apply`）；stdout 专用于协议帧。

## 行为

- `tui.hello` 分配 client id 与 generation，并返回初始 `resumeClass: baseline-required`；随后 `tui.subscribe` 明确返回 `session`、`control-plane` 或 `all` 范围，以及 `resume-accepted`/`baseline-required` 分类和水位。
- `tui.attach` / `tui.subscribe` 标记会话并启动 mux/host 泵。attached session 的 live mux 帧会缓冲到 `session.history` 返回，再按序冲洗；未 attached session 的控制帧进入有界 control-plane store，并可向 observer fan-out。
- baseline-required 订阅会发送 `tui.controlPlaneBaseline`，携带各 session 的 projection/queue/jobs/interaction 快照、workspace/archive 状态、generation 和保留的控制记录。回放受 TTL/数量上限约束；慢客户端丢通知后可从下一次 baseline 恢复。
- `session.history` 返回失败结果时仍保留该会话的 live 缓冲；重复 `tui.hello` 会开始新 generation，并丢弃上一代的泵与订阅。
- 任一事件流意外正常结束时也会报告 `stream/error`，客户端可以进入重连，而不会静默使用已失效的连接。
- ApiProxy 一元方法（`session.list`、`session.prompt` 等）经 `toFetchHandler` 转发，使请求 schema 仍留在网关包中。
- `tui.respond` 变为 `POST /api/respond`。同一 request id 的完全相同重放会返回原回执，而用不同答案复用该 id 会抛出 `already-resolved`。

`inject`：`apiProxy`。仅运行时的 `input` / `output` 覆盖供测试使用；它们不是 Config 字段。

## 模型体验

无，因为此包是面向客户端的呈现适配器；模型可见接口属于 ApiProxy 后方的运行时插件。

#### KV Cache 影响

无；此包既不组装也不发送提供方请求。

## 已知限制与暂缓事项

- **每个连接一个 mux/host 泵**——presentation history 仍按 session 经过加载屏障；control-plane store 会观察所有 session，SharedAuto 传输留在后续切片。
- **无 SharedAuto 监听**——本插件绑定一对 stdio（或注入的）流；Unix socket 的 connect-or-spawn 属于后续传输提供方。
- **不强制身份**——hello 可以携带 `TuiClientIdentity`，但本进程不把 profile、插件摘要或 sandbox 与共享 server 的密钥比较。
