# N3 Workspace and Agent pane controllers

> 记录时间：2026-08-23 18:20:00 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

Workspace/Dashboard 与 Agent/Task/Subagent 仍有 runtime 手写 overlay 文案，且
Agent overlay 的选择使用数组索引。Grok pane 的视觉输入应是 DSH snapshot，焦点
和动作目标必须按 stable ID 维持，刷新、重排和 late snapshot 不能把 selection
移到另一项。

## 设计契约和复用依据

- DSH `DashboardModel` 继续拥有 session/workspace rows、过滤、折叠、排序和 action
  lifecycle；UI `WorkspaceTree` 只绘制这些 DTO 并显示 pending/unsupported 状态。
- `AgentPaneController` 保存 `AgentItemId::Task/ Subagent`，以 id 重建顺序和
  selection；`InterruptSubagent` 只从当前 stable subagent id 生成 effect。
- pane renderer 保持上游 Grok 的 section/header/status/selection 视觉层次，
  不复制 Grok agent runtime 或发起 RPC。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/views/agent_panes.rs`：stable-ID controller 与
  task/subagent pane renderer。
- `crates/dsh-pager-grok-ui/src/views/workspace.rs`：workspace tree renderer，
  显式 capability/status fallback。
- `crates/dsh-pager-grok-ui/src/views/mod.rs`：注册 pane modules。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：接入 pane controller，删除 runtime
  index-based task renderer/selection，并让 dashboard consume workspace renderer。
- 本记录文件：收尾回写。

## 风险、回滚和依赖

- 风险：host 刷新后旧 selected id 不存在；controller 必须 deterministic fallback
  到当前第一项，并清除 stale id。
- 风险：interrupt completion 迟到；effect ledger/session generation 仍是最终
  admission fence，本批 controller 不宣称 host 已中断。
- 回滚：回退本记录单一 commit，保留 N1/N2 commits。

## 实际修改

- 新增 `AgentPaneController` 与 `AgentItemId`，task/subagent selection 以 stable
  id 保存；catalog refresh、reorder、removal 会保留同一 id 或 deterministic
  fallback，不再把数组 index 当作 interrupt 目标。
- task/subagent overlay 和 AgentView inline tasks/catalog 都经由
  `views/agent_panes.rs` 绘制，保留 Tasks/Subagents section、running/status/mode
  诊断和 selected/highlight；runtime 不再持有 index-based task renderer。
- 新增 `WorkspaceTreeController`，由 DashboardModel 的 stable workspace/session
  DTO 驱动 focus；dashboard header 显示当前 workspace focus 和 read-only/pending/
  unavailable 状态，workspace mutation 在 capability 不可用时明确拒绝并不发 RPC。
- `DashboardRenderState` 将 model、workspace capability、focus 和 theme 作为单一
  render 输入，减少 runtime 拼接 pane 文案；新增 stable refresh/fallback tests。

## 验证结果

- `cargo fmt --all`
- `cargo test -p dsh-pager-grok-ui --lib --quiet`（231 passed）
- `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features -- -D warnings`
- `git diff --check`

## Git 提交

- commit message：`feat: migrate workspace and agent pane controllers`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_18-20-00_N3-workspace-agent-panes.md`

## 未解决问题和下一步

- N4 继续收敛完整 AgentView 绘制顺序、parity cell capture、fallback 删除条件，
  并执行 workspace/PTY/backend 全量 E2E。
