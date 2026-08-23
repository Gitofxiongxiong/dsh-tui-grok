# `@dsh-pager-grok/tui-embedded`

[English](README.md) | 中文

外置原生 TUI 后端组合包。[`cordis.patch.yml`](cordis.patch.yml) 叠加在已安装的 DSH `dsh-base` 之上：提供编码 persona 和工具模式、禁用 HMR 以使 stdout 保持为 JSON-RPC 管道、将 Code Mode 的 worker 作为核心执行能力挂载，并插入进程外 pager 所需的 host 行——workspace 存储、browse 目录选择器、local file-reference、传输无关的 `api-gateway`，以及 stdin/stdout 上的外置 `tui-server`。它不挂载 HTTP 服务器、Web 运行时或浏览器插件，也不修改 Harness 源码。Agent 工具留在 host 平面；会话 preset 是后续叠加层。

将此 bundle 安装到 `grok-tui` 等自定义 profile 后运行 `dsh --profile grok-tui`。原生 pager（`dsh-pager`）在管道上发送 `tui.hello`。stdout 专用于协议帧；诊断信息应写到 stderr。

该包没有运行时 API；profile 组合器通过 manifest 的 `dsh.bundle.patch` 字段解析 patch，绝不通过代码。

## 模型体验

无，因为此组合包只挂载 stdio TUI 网关与 host 行；提示词与工具属于组合后的 base 与 host 插件。

#### KV Cache 影响

无；该组合包不向提供方请求添加任何 token。

## 已知限制与暂缓事项

- **stdout 是协议管道**：任何向 stdout 写入的插件都会破坏 TUI 帧；因此禁用了 HMR，且本组合包不挂载 logger。
- **仅 browse 目录选择器**：`host.pickDirectory` 没有原生对话框；pager 通过 `host.listDirectory` 列举目录。
- **无 SharedAuto 监听**：本进程服务一个 stdio 客户端；Unix socket 的 connect-or-spawn 属于后续传输。
- **无 agent-preset 名册**：此组合中 `agentPreset.*` RPC 没有名册；会话使用 host 平面的工具集。
