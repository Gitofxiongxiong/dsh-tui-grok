# M10 cell parity prompt/layout

> 记录时间：2026-08-23 12:47:10 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

当前主屏已经有 AgentView 外壳，但与 Grok Build 的真实输出仍存在可见差异：
Prompt chrome 的 border/accent/padding/info line 不是上游规则，header 使用了
自定义 StatusBar 拼接，semantic runner 只比较文本和 rect，无法证明逐 cell 对齐。
本批以固定 Grok source snapshot 为 reference，收敛布局和 Prompt cell contract，
并增加逐 cell 的 glyph/颜色角色语义证据。

## 设计契约和复用依据

- 对应长期计划章节：M3.2、M4.3、M5.1、M10.1-M10.3。
- Grok source path、commit、SOURCE_REV：
  `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/views/agent.rs`、
  `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/views/prompt_widget/mod.rs`、
  `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`、
  `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：B。结构性适配 Grok `LayoutConfig`/`AgentViewLayout` 和
  `PromptWidget::draw` 的几何、glyph、padding、focus 色语义；不引入 Grok
  agent/runtime/shell/tools，也不修改 vendor 上游文件。
- DSH-neutral seam：cell parity 只消费 `AgentViewLayout`、PromptRenderState、
  `GrokHostSnapshot` 和 terminal area；不会从 UI 反向访问 RPC 或 SessionState。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/views/agent.rs`：按 Grok 规则修正布局与 prompt
  chrome，增加 cell-oriented render helpers/tests。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：移除剩余自定义 header 文案拼接，按
  AgentView chrome contract 传递 prompt/status 数据。
- `crates/dsh-pager-grok-ui/src/parity.rs`：扩展 semantic frame 的 cell-role
  采样和关键尺寸断言，用于 reference comparison。
- `docs/开发进度记录/2026-08-23_12-47-10_M10-cell-parity-prompt-layout.md`：回写实际结果。
- 不在范围内：DSH protocol/host source、vendor 上游文件、完整 block renderer、
  Prompt completion/history/image overlays、真实 Grok runtime。

## 风险、回滚和依赖

- 风险：严格 cell contract 可能暴露现有 fallback fixture 与新布局的差异；旧
  fallback 明确不是 Grok golden，本批只更新 runner 语义，不伪造 reference。
- 风险：短终端高度预算不足；保持 Grok `SHORT_TERMINAL_ROWS=16`、scrollback
  floor 和 prompt min height，并覆盖 40x12/80x24/120x40。
- 回滚：恢复本记录范围内三个源码文件即可，DSH protocol/session/effect 不变。
- 依赖：现有 ratatui Buffer、Theme、RichTranscript、StatusBar、ShortcutsBar。

## 实际修改

- `AgentViewLayout::compute` 改为 Grok 同构的 `Constraint::Length/Min` 行堆叠：
  outer padding、header gap、scrollback floor、turn status/banner gap、prompt
  gap、status line 与 shortcuts 按顺序分配；短终端和 compact 模式不再通过反向
  回推产生重叠矩形。
- Prompt 高度预算改为 `vpad_top + textarea rows + info/divider row`，空输入最小
  3 行，多行输入按实际 wrap 增长并保留上限。
- `render_prompt` 拆出 Buffer-level renderer，按 Grok cell 顺序绘制完整 chrome：
  `╭─╮` 顶部 divider、title inset、`│` 两侧、`❯ ` prefix、textarea 内容、
  `╰─╯` 底部 info divider；focus 会控制 cursor、border 和内容 dimming。
- runtime 使用真实 prompt focus，并将 viewport 高度计算改为扣除 top-vpad/info
  两行；宽屏 header 避免中心文案与右侧状态重叠。
- parity `SemanticFrame` 增加 prompt region 的 `SemanticCell` 签名（glyph、
  theme color role、modifier bits），并提供 `semantic_cells` 与 `cell_diff`。
  关键 prompt border、prefix、尺寸矩阵均有单测覆盖。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo check --workspace`：通过。
- `cargo test --workspace --locked`：通过（各 workspace tests/doc-tests）。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过。
- `python3 scripts/check-protocol-fixtures.py`：通过。
- `python3 scripts/check-source-manifest.py`：通过。
- `python3 scripts/parity-matrix.py`：通过，648 cases / 8 fixtures。
- `scripts/e2e.sh`：通过，包含 PTY resize、picker、queue、mouse 与 terminal restore。
- 80x24 PTY smoke：确认 Grok Prompt chrome、title、arrow prefix 与底部 info line
  已按新 cell contract 输出。

## Git 提交

已提交，commit message 包含以下 trailer：

```text
Progress-Record: docs/开发进度记录/2026-08-23_12-47-10_M10-cell-parity-prompt-layout.md
```

## 未解决问题和下一步

完整 Grok `PromptWidget` overlays、scrollback block renderer 和真实 reference
runner 仍需独立批次；本批完成后仍不宣称最终像素级 parity。
