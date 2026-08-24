# Grok 后台任务与 Subagent TUI 功能点梳理

> 记录时间：2026-08-24 22:11:39 +0800
> 操作者/agent：Codex
> 状态：completed（文档与浏览器证据已完成；未改生产代码）

## 目标与背景

梳理 Grok Build TUI 对长时间后台任务（background command、monitor、`/loop`、workflow）
和 subagent 的显示、状态、展开及交互逻辑，重点回答“提示词上方如何提示仍在运行、
任务面板如何分组、双击/点击如何打开详情、取消和 Esc 如何回退”。文档供
`dsh-pager-grok` 后续对齐实现使用，并保留 Grok 源码路径和浏览器终端证据。

## 设计契约和复用依据

- 对应长期计划章节：Grok 前端复用、M4 scrollback/blocks、M6 dashboard/queue、M10 视觉 parity。
- Grok source revision：`/home/leo/code/grok-build`，mirror commit `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，`SOURCE_REV=7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 重点源码：`src/views/tasks_pane.rs`、`src/views/turn_status.rs`、`src/scrollback/blocks/{bg_task,subagent}.rs`、`src/app/agent.rs`、`src/app/subagent.rs`、`src/app/agent_view/selection.rs`、`src/app/acp_handler/{background,session_notification,subagent_activity}.rs`、`src/views/dashboard/{row,state}.rs`。
- 重点行为文档：`docs/user-guide/{16-subagents,20-background-tasks,23-dashboard}.md`。
- 复用等级：文档/调研，不改生产 UI 代码；后续实现应按 Grok 组件结构性复用（B），DSH 只提供 neutral DTO 与 Intent/Effect。

## 计划修改范围

- `docs/GROK_TUI_TASKS_AND_SUBAGENTS.md`：综合功能点、状态机、交互矩阵、对齐契约和源码索引。
- `docs/assets/grok-tui-tasks/`：隔离 Grok 会话经 `ttyd + xterm.js + Playwright` 截取的 `.xterm` 浏览器截图；仅保存不含凭据/私有会话的数据。
- 本记录：完成后补写实际文件、命令、截图状态、差异与阻塞。

不修改 Rust、vendor、协议、测试和既有未提交文件。

## 实际完成内容

- 完成 `docs/GROK_TUI_TASKS_AND_SUBAGENTS.md`：覆盖顶部 Tasks pane、prompt 上方 watcher cue、scrollback lifecycle block、任务/subagent 状态机、分组/排序/自动开关/尺寸、双击/Enter/鼠标/键盘交互、viewer 回退、DSH DTO 与 Intent/Effect 契约、边界条件、对齐验收清单和源码索引。
- 保存真实 `.xterm` 浏览器截图至 `docs/assets/grok-tui-tasks/`：后台任务列表、后台任务 viewer、subagent 列表、subagent child fullscreen 各一张。
- 按源码函数和用户指南章节核对行为：`TasksPane::sync`、`desired_height`、`turn_status::still_running_label`、scrollback 双击路由、ACP background/subagent 生命周期路由。

## 风险、回滚和依赖

- Grok 后台任务和 subagent 需要真实会话事件；模型/网络不可用时，源码和用户指南仍作为主证据，截图明确标记未捕获状态。
- 浏览器截图受字体、viewport、光标闪烁和 spinner 动效影响；记录固定环境与预期动态差异。
- 回滚：删除本记录、功能点文档和本记录列出的截图目录即可，不触及既有用户修改。

## 计划验证

- `rg`/`sed` 读取 Grok 源码及用户指南，核对状态和按键/鼠标路由。
- 通过 `~/.local/bin/ttyd` 启动隔离 Grok，使用 Playwright 聚焦 `.xterm-helper-textarea`，驱动真实按键/鼠标并截取 `.xterm`。
- `git diff --check`；如截图或浏览器不可用，记录为 partial/blocked，不声称像素 parity。

## 实际验证

- Grok 通过 `~/.local/bin/ttyd`（ttyd 1.7.7-40e79c7）启动在隔离 `/tmp/grok-tui-taskshots.dhhr5B` 工作目录；浏览器端使用 Playwright 1.62.1、Chromium、固定 1200×800/DPR1，等待 `.xterm` 并聚焦 `.xterm-helper-textarea`。
- 真实会话产生了 `sleep 90` 的后台任务和 subagent running 状态，并成功捕获双击后任务 stdout viewer、subagent child fullscreen。
- 截图文件已检查存在且可读；文档内链接均使用仓库相对路径。
- 已执行 `git diff --check`、新增 Markdown 的 no-index whitespace 检查和资产路径检查；没有运行 Rust 测试，因为本次仅增加文档/PNG，不涉及生产逻辑。

## Git 提交

- commit message：未创建 commit；保留工作区现状，等待用户决定提交边界。
- Progress-Record trailer：`Progress-Record: docs/开发进度记录/2026-08-24_22-11-39_Grok后台任务与SubagentTUI功能点.md`

## 未解决问题和下一步

- 本次目标没有阻塞项。截图中的 spinner、elapsed、光标和模型响应属于动态状态，适合行为/布局证据，不应当被当作像素级 golden。
- 后续实现阶段按文档第 8 节 DTO/Intent 契约接入 DSH，再用同样的 ttyd + xterm.js + Playwright 场景做 parity 验收。
