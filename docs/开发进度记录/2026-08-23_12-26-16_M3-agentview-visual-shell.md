# M3 AgentView visual shell

> 记录时间：2026-08-23 12:26:16 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

当前 TUI 仍由 runtime 直接拼接固定 header、transcript、prompt 和 footer，
与 Grok Build 的 AgentView 主屏在外边距、scrollback 最小高度、prompt 高度预算、
快捷键栏和状态行上存在明显差异。本批实现可回滚的 Grok-derived 主屏视觉外壳，
作为后续完整 block renderer 和交互迁移的几何基线。

## 设计契约和复用依据

- 对应长期计划章节：M3.1-M3.5、M4.3、M5.1、M10.1。
- Grok source path、commit、SOURCE_REV：
  `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/views/agent.rs`、
  `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`、
  `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`；布局规则结构性适配，Prompt chrome
  参考 `views/prompt_widget` 的 accent/border/info 语义。
- 复用等级 A0/A1/B/C/D：B（Grok 布局和 chrome 规则）；C（DSH snapshot 到
  status/prompt 文案的投影）。不引入 Grok agent/runtime/shell/tools。
- DSH-neutral seam：`AgentViewLayout` 和 `AgentViewModel` 只消费尺寸、模式、
  running、queue revision、能力和本地 prompt viewport；runtime 继续通过
  `GrokHostSnapshot` 和 UiIntent/UiEffect 边界提供数据。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/views/agent.rs`：布局 solver、主屏模型、prompt
  chrome/快捷键/状态行渲染 helper 和几何测试。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：改为使用 AgentView 统一布局与主屏
  chrome，保留现有 transcript、overlay 和 effect 路径。
- `docs/开发进度记录/2026-08-23_12-26-16_M3-agentview-visual-shell.md`：回写实际结果。
- 不在范围内：DSH protocol/host source、vendor 上游文件、完整 Grok
  scrollback/block renderer、queue/picker/interaction 业务状态机。

## 风险、回滚和依赖

- 风险：短终端可用行数不足时 prompt/transcript 几何退化；通过 80x24、120x40
  和 40x12 几何测试限制预算。
- 风险：runtime 仍保留旧 overlay 绘制路径；本批只替换默认主屏 chrome，不宣称
  完整 parity。
- 回滚：恢复本记录范围内两个源码文件即可；DSH 真源和 protocol 不变。
- 依赖：现有 `StatusBar`、`ShortcutsBar`、`RichTranscript`、`PromptEditor`、
  `Theme` 和 `AppShell` event state。

## 实际修改

- `crates/dsh-pager-grok-ui/src/views/agent.rs`
  - 以 Grok `AgentView` 的布局语义重建主屏 solver：外边距、compact/short
    terminal 断点、scrollback 最小 5 行、prompt 多行高度上限、turn status、
    status line、shortcuts row 和 timeline rail 几何来自同一布局快照。
  - 增加 DSH-neutral `PromptRenderState` 与 Grok-derived prompt chrome：圆角
    box、模式 title、左 accent line、queue/steer prefix、textarea viewport、
    model/session info 和 running/idle 状态。
  - 复用 vendor `ShortcutsBar`，补充 80x24、120x40、40x12 几何/预算测试。
- `crates/dsh-pager-grok-ui/src/runtime.rs`
  - snapshot 先于 layout 生成，runtime 只选择 prompt/状态行请求高度并调用
    `AgentView` 的统一布局与 chrome renderer；移除旧的 `Paragraph + TOP block`
    prompt 绘制。
  - 保留 RichTranscript、timeline、overlay 和 UiIntent/UiEffect 路径不变，
    并对窄 header 使用无重叠的 compact 文案。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo check --workspace`：通过。
- `cargo test --workspace --locked`：通过。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过。
- `python3 scripts/check-protocol-fixtures.py`：通过（2 files）。
- `python3 scripts/check-source-manifest.py`：通过（8 rows，local/upstream drift 0）。
- `python3 scripts/parity-matrix.py`：通过（648 cases，8 fixtures）。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`：通过，
  覆盖 mock session load、picker、resize、queue、mouse、Esc ladder 和 terminal
  restore。
- `scripts/e2e.sh`：通过，输出 `DSH/Grok M8-M10 end-to-end checks passed`。
- 手工 PTY semantic inspection：40x12 使用 compact chrome，120x40 保留外边距、
  full status/header 和 Prompt info row；两种尺寸均无越界且正常退出。

## Git 提交

- commit message：待提交。
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_12-26-16_M3-agentview-visual-shell.md`
- 暂存区审计：待提交前执行。

## 未解决问题和下一步

完整 Grok PromptWidget（textarea overlays、image chips、completion/history）和
AgentView block renderer 仍需后续批次迁移；本批只完成主屏布局与 Prompt chrome，
不宣称完整 M3/M2-M10 parity。下一步应接入 Grok block renderer/scrollback paint
window，并让 semantic runner 直接复用同一渲染树。
