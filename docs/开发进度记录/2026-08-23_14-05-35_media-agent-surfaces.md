# Media 与 Agent/Task/Subagent surfaces

时间：2026-08-23 14:05:35 +0800

## 目标

把 image/media 和 Agent/Task/Subagent 从 transcript/status DTO 提升为可聚焦的
生产 TUI overlay，并保持 host authority 与 capability fallback 的语义。

## 本批范围

- 增加 Image Preview overlay：supported、pending、unsupported、missing attachment
  四种显示路径，禁止用占位文本冒充真实媒体数据。
- 增加 Agent Tasks overlay：task/subagent 稳定 ID、running/complete/error 状态和
  generation-safe snapshot 展示。
- 主 AgentView 继续负责 streaming/turn status，Workspace 继续由 Dashboard 负责。

## 下一批

接入真实 image attachment preview effect、task/subagent interrupt/inspect effect，
再把 Grok Scrollback block renderer 和 PromptWidget 完整源码闭包 vendor 进生产路径。
