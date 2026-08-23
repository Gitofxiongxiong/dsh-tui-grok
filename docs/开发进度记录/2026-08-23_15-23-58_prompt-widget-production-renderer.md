# PromptWidget production renderer

> 记录时间：2026-08-23 15:23:58 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

完成 `GROK_RENDERER_PIXEL_PARITY_PLAN.md` P6.1 的第二、三门禁：让 production
prompt 使用 workspace 已有的固定 Grok `TextArea` 负责 wrap、selection、cursor、
mouse-ready geometry 和 viewport，并用固定上游 `PromptWidget::draw` 的纯 chrome/
TextArea/info/cursor draw core 替换 `views/agent.rs` 手写 renderer。

## 设计契约和复用依据

- 上游：`crates/codegen/xai-grok-pager/src/views/prompt_widget/{mod.rs,tests.rs}`、
  `src/views/agent.rs`、`src/app/agent_view/render.rs`；
- mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`；
  SOURCE_REV：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`；
- 复用等级：TextArea A0；Prompt draw core A1/B；DSH PromptMode/DTO C；Grok
  agent/session/shell/ACP/runtime D；
- DSH-neutral seam：`PromptEditor` 暴露 TextArea 绘制所需引用和编辑结果，
  `GrokPromptRenderer` 消费 `PromptStyleContract`/`PromptInfoContract` 与 TextArea，
  不读取 `SessionState` 或调用 RPC；
- 稳定身份/generation：本批只改变 local draft renderer，不改变 effect identity、
  session generation 或 authoritative receipt semantics；
- 替换旧模块：删除 `AgentView::render_prompt_buffer` 的 cell 绘制职责和
  `PromptViewport` 手工 wrap production 路径。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/input/mod.rs`：`PromptEditor` 改用 Grok `TextArea`；
- `crates/dsh-pager-grok-ui/src/app.rs`：主 prompt mouse focus 路由到同一 TextArea；
- `crates/dsh-pager-grok-ui/src/views/prompt_contract.rs`：validation-only Clippy
  等价改写；
- `crates/dsh-pager-grok-ui/src/views/prompt_widget.rs`：新增 upstream-derived pure
  production draw core；
- `crates/dsh-pager-grok-ui/src/views/{mod.rs,agent.rs}`：导出并调用唯一 renderer；
- `crates/dsh-pager-grok-ui/src/{runtime.rs,parity.rs}`：改为传递 editor/contract，
  更新 semantic fixtures；
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`、
  `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`：更新真实状态和来源；
- 本记录：回写实际结果、验证和下一步。

不在范围内：Grok file-search/suggestion/history/image controller、Scrollback、完整
AgentView pane solver、DSH protocol/Harness、删除整个 frozen runtime。

## 风险、回滚和依赖

- 风险：EditBuffer 与 TextArea key semantics 不完全相同；保留现有输入测试并新增
  Unicode/wrap/cursor/selection semantic cell tests；
- 风险：改变 prompt 可用宽度会影响 height；height 和 draw 必须共用同一 contract；
- 回滚：回退本提交，恢复前一提交中的 contract-only 状态；
- 依赖：`dsh-grok-textarea`、`PromptStyleContract`、`PromptInfoContract`、现有 Theme。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo test -p dsh-pager-grok-ui --locked`
- `cargo test --workspace --locked`
- `python3 scripts/check-source-manifest.py`
- `python3 scripts/parity-matrix.py`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager`
- `git diff --check`

## 实际结果

- `PromptEditor` 的内部状态从 `EditBuffer + PromptViewport` 收敛为 workspace
  `TextArea`；保留 CRLF 规范化、控制字符过滤、64 KiB 粘贴上限、尾空格和换行提交
  语义，并使用 TextArea 的 key/undo/selection/cursor/wrap state；
- 新增 `views/prompt_widget.rs::GrokPromptRenderer`，按固定上游绘制顺序渲染 area
  fill、accent、top/title、prefix、stateful TextArea、placeholder、side/bottom border、
  info line、unfocused blend 和真实 cursor；
- production runtime 和 semantic runner 共用该 renderer；prompt 高度和 multiline
  label 由同一 `PromptGeometry + TextArea::desired_height` 产生；
- 删除 `PromptViewport`、手工 wrap 和 `AgentView::render_prompt_buffer`；AgentView
  只接收已经算出的 prompt height，不再绘制 prompt cell；
- 主 prompt 的 click/drag/scroll/selection-copy 事件转交同一 TextArea，点击 prompt
  会恢复 prompt focus；overlay 的 File Search、Suggestion、Image、Workspace、Agent
  路径未删除或降级；
- 新增 box/caption/text/cursor、wrap viewport、selection style 和 mouse geometry
  semantic tests。P6.1 第 1-3 门禁完成，第 4-5 门禁保持未完成。
- 严格 Clippy 首轮发现的 3 个既有/contract-test 机械告警已做等价改写，不改变
  file-search paste 或 selection movement 行为。

## 验证结果

- `cargo test -p dsh-pager-grok-ui --locked`：210 tests、1 doctest 通过；
- `cargo test --workspace --locked`：workspace tests/doctests 全通过；
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`：
  通过；
- `python3 scripts/check-source-manifest.py`：8 copied rows，drift 0；
- `python3 scripts/parity-matrix.py`：972 cases、8 fixtures 通过；
- `python3 scripts/check-protocol-fixtures.py`：2 fixtures 同步；
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`：session、
  picker、resize、queue、mouse、terminal restore 全通过；
- `cargo fmt --all -- --check`、`git diff --check`：通过。

## 下一步

1. P6.1 第 4 门禁：迁移完整 AgentView pane solver/render order，让 tasks、catalog、
   queue、banner、status、scrollbar/timeline 与 hit map 共用同一 snapshot；
2. P6.1 第 5 门禁：接 Grok File Search、Suggestion/history 和 prompt Image
   controller；
3. 随后迁移 ScrollbackPane 的 Markdown/Diff/Image block renderer，不删除这些能力
   的现有可用 fallback，直到对应 reference/PTY 门禁通过。
