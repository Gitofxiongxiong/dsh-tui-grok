# M7 Queue、Approval/Question、Task 与 Status vertical slice

> 记录时间：2026-08-23 07:46:37 +0800
> 操作者/agent：Codex
> 状态：completed（vertical slice 基线）

## 目标与背景

继续 `GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md` 的 M7，接通已有 DSH queue/
interaction/jobs authority 与 Grok-derived modal/focus/effect seam。当前默认 runtime
没有 queue 或 interaction overlay，queue revision/pending receipt 也没有可见反馈。

## 设计契约和复用依据

- 对应长期计划章节：M7.1-M7.7，优先完成一条真实 queue mutation 与 approval/question
  response 路径。
- Grok source path、commit、SOURCE_REV：`vendor/grok`，mirror
  `19d42e35c07a9c9244f03f6dfc0c4c353f970d4f9`，`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：B/C；继续复用 Grok modal/status/timeline/line editor，DSH 只拥有
  queue revision、interaction identity、jobs projection 和 RPC effects。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/app.rs`
- `crates/dsh-pager-grok-ui/src/effects.rs`
- `crates/dsh-pager-grok-ui/src/host_adapter.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- `crates/dsh-pager-grok-ui/src/views/mod.rs`
- `crates/dsh-pager-grok-ui/src/views/queue.rs`
- `crates/dsh-pager-grok-ui/src/views/interaction.rs`
- `crates/dsh-pager-grok-ui/src/lib.rs`
- 本记录文件

预期行为：queue 通过 stable item ID、revision、pending receipt 编译并提交
QueueMutation；approval/question modal 绑定 request/generation，响应成功后等待
authoritative notification；jobs/status 在 header/footer 展示显式 running/error/
completed 状态；Esc/focus 仍由 AppShell 统一路由。完整 reorder drag、复杂 question
多选表单和 reference golden 不在本批范围。

## 风险、回滚和依赖

风险是 mock backend 对 queue/respond method 的覆盖程度，以及 modal 小尺寸布局。
修改集中在 UI seam，不改变 protocol wire schema 或 host authority；回滚为恢复本
记录列出的 UI 文件。验证包括 fmt/check/clippy/test、协议/source manifest 和 PTY。

## 实际修改

- `app.rs` 扩展 `Queue`/`Interaction` overlay、KeyOwner 和 key/mouse/paste owner
  路由；Esc 关闭、modal chrome click 和 overlay priority 保持由 AppShell 管理。
- 新增 `views/queue.rs`：按 authoritative queue revision 绘制 queued/steering/context
  分组、多行内容、stable item ID 选中、编辑/删除/steer shortcut 和 pending receipt。
- 新增 `views/interaction.rs`：approval/question modal 投影，approval allow/deny，
  question 选项/自由文本回答，以及保留 request/question ID 的协议 response builder。
- `runtime.rs` 接入 queue overlay 的选择、编辑、删除、steer、鼠标滚轮和 QueueMutation
  effect；local pending 只表达已接收 operation，下一次 authoritative queue revision
  变化后清理 pending 并显示收敛文案。
- `runtime.rs` 接入 interaction overlay 的 key/mouse/paste、approval/question response；
  operation 绑定当前 session generation 和 host request ID，accepted 后等待通知移除
  modal，不以 accepted 代替 authority。
- `host_adapter.rs` 从 control-plane jobs 投影 `TaskRow`；header/footer 展示 connection、
  model、queue revision、pending receipt、running/idle 和 running/error/completed task
  汇总。
- `effects.rs` 修正未预置 operation context 时 interaction request ID 的绑定，并加入
  generation/request identity 单元覆盖。

## 验证结果

- `cargo fmt --all`：通过。
- `cargo check --workspace`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test --workspace --locked`：通过（workspace 单测、协议、binary smoke、PTY
  相关测试；Grok UI 当前 153 tests）。
- `python3 scripts/check-protocol-fixtures.py`：通过。
- `python3 scripts/check-source-manifest.py`：通过（8 rows，local/upstream drift 0，
  missing license 0）。
- `cargo build -p dsh-pager-bin --locked && python3 scripts/pty-smoke.py --binary
  target/debug/dsh-pager`：通过，alternate screen/raw mode/cursor restore 正常。

## Git 提交

本记录对应当前 M7 提交（提交必须包含 trailer）：
`Progress-Record: docs/开发进度记录/2026-08-23_07-46-37_M7-queue-interaction-status-vertical-slice.md`

## 未解决问题和下一步

- 完整 reorder/drag、复杂 question 多选/逐题表单、interaction expiry/timeout/error
  modal、queue conflict/retry golden 和 M10 reference golden 留待后续门禁。
- 当前 mock backend 已覆盖协议层 queue/interaction smoke；尚未把真实 backend 的长时
  reconnect/stream jobs timeline 作为 M7 release 门禁。
