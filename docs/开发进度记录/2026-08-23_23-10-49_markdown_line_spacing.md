# Markdown 行间距调整

> 记录时间：2026-08-23 23:10:49 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

将生产 TUI 中 Markdown 正文的视觉行高从当前每个终端行 1 行，调整为平均约
1.25 行；工具调用、思考摘要、用户消息和其它非 Markdown block 保持现有密度。

## 设计契约和复用依据

- 对应长期计划章节：Grok Build 适配计划 2.1 视觉 spacing、3.3 文档/虚拟化、
  P02/P03 semantic render。
- Grok source path、commit、SOURCE_REV：`/home/leo/code/grok-build`，
  mirror `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，SOURCE_REV
  `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`；参考
  `crates/codegen/xai-grok-pager/src/scrollback/blocks/markdown_content.rs`
  与 scrollback layout 的显式 blank-row spacing。
- 复用等级：B（保留现有 DSH-neutral Markdown AST、RichPaintLine、Scrollback
  height/cache 和 hit-map，仅在 projection 层加入可测的 spacer row）。

## 计划修改范围

- 文件：`crates/dsh-pager-grok-ui/src/views/transcript.rs`。
- 文件：本进度记录。
- 预期行为：Markdown 渲染行每累计 4 个可见 Markdown 行插入 1 个不可选择的
  spacer row，形成平均 5/4 的终端行高；spacer 参与 height、scroll、anchor、
  hit-map 的统一几何，但不进入 copy payload。
- 不在范围内：用户消息、工具调用/结果、思考摘要、代码/差异 block 的单独
  行距；Markdown AST、主题和 host presentation contract。

## 风险、回滚和依赖

- 终端使用整数行高，1.25 只能通过稳定的 4 行 + 1 spacer 分布近似；短于 4
  行的 Markdown 片段不会出现额外空行。
- spacer 改变 transcript height、scroll anchor 和 viewport 呈现，必须补单测。
- 回滚方式：删除本记录范围内的 spacer 标记、投影插入逻辑和对应测试，恢复
  原始每个 `RichPaintLine` 一行布局。

## 实际修改

- `crates/dsh-pager-grok-ui/src/views/transcript.rs`：为 `SemanticLine` 增加
  Markdown 标记；只对 Markdown projection 统计实际换行后的可见行，每 4 行插入
  1 个带相同 rail/background 的不可选择 spacer；spacer 参与 RichPaintLine 的
  `line_index` 和 Scrollback 高度，从而与滚动、anchor、hit-map 共用同一几何。
- `crates/dsh-pager-grok-ui/src/views/transcript.rs`：新增
  `markdown_lines_use_fractional_terminal_spacing` 测试，验证 5 行 Markdown
  产生 5 个可选择正文行 + 1 个不可复制 spacer，且 line index 连续。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test -p dsh-pager-grok-ui --locked`：通过，240 tests + doctests。
- `cargo test --workspace --locked`：通过，workspace tests/doctests 全部通过。
- `cargo clippy -p dsh-pager-grok-ui --all-targets --locked -- -D warnings`：通过。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager`：通过。

## Git 提交

- commit message：`feat: tune Markdown transcript line spacing`。
- Progress-Record trailer：`Progress-Record: docs/开发进度记录/2026-08-23_23-10-49_markdown_line_spacing.md`。
- 暂存区审计：计划只包含本记录和 `crates/dsh-pager-grok-ui/src/views/transcript.rs`；提交前复核。

## 未解决问题和下一步

- 终端高度是整数行，当前通过每 4 个 Markdown 行插入 1 个 spacer 近似 1.25；
  短于 4 行的片段不会单独增加空行。若真实截图仍显得过密，下一批可以把频率
  调整为更密的 Grok-specific block/paragraph spacing，但不应改动工具/思考密度。
