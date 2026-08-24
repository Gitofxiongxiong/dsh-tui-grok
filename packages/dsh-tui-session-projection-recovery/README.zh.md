# @dsh-pager-grok/tui-session-projection-recovery

[English](README.md) | 中文

为 `session.list` 补上冷行投影恢复路径的 Cordis 插件。它只装饰公开的
`ctx.apiProxy.sessions.list` seam，不复制或修改 DeepSeek Harness 宿主源码。

## 行为

- 先调用已经组合好的宿主 `session.list`。
- 跳过已 attach 的会话，以及已经带有 `projections` 的行。
- 对挂载了 `ctx.sessionProjectionCache` 且仍为 detached 的行，以每批 16 行的有界
  并发调用 `coldSnapshot(sessionId, signal)`。
- 非空快照合并回行。`sessionListMetadata` 可以推进 `updatedAt` 并清除 `blank`；
  仅表示检查点前缀为空的快照不能隐藏原本可见的行。
- 缓存或日志失败时 fail-soft，保留原行；调用方提供 `AbortSignal` 时传播取消。

`coldSnapshot()` 仍负责身份校验、投影折叠和持久化检查点写回。插件不维护第二份
缓存，也不直接读取原始日志。

## 组合方式

在 profile patch 中将本插件放在 `@deepseek-ai/dsh-host-apiproxy` 和
`@deepseek-ai/dsh-session-projection-cache` 之后。本仓库的
`dsh-tui-embedded` bundle 已自动挂载它。

## 模型体验

无。这是宿主侧读模型适配器，不增加 provider prompt 内容或模型可见事件。

#### KV Cache 影响

无。持久化检查点仍由官方 projection-cache 服务负责。

## 已知限制

- 已发布的 `SessionsApi.list` 契约没有取消参数；直接 in-process 调用若额外提供
  `AbortSignal` 会被尊重，普通 JSON-RPC carrier 仍使用宿主默认请求生命周期。
- 只有挂载了 projection-cache 和投影 registry，插件才能恢复其暴露的值；缺少其中
  任一服务时返回原始的仅元数据行。
