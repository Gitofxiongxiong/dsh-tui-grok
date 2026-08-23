# 开发进度记录：transcript block chrome、选择框与间距

- 日期：2026-08-23
- 状态：已完成
- 需求来源：真实 Grok TUI 截图的像素级对齐反馈。

## 本次范围

- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- `crates/dsh-pager-grok-ui/src/selection.rs`（如选择状态需要扩展）
- 对应单元测试与回归验证。

## 目标行为

1. 思考与工具 block 双击独立折叠/展开，且相邻 operational block 的左侧 rail 连续。
2. 最终 Markdown 文本从 operational block 的 diamond 对齐列开始渲染。
3. 点击/拖拽最终回复后，在 transcript 内容范围外绘制 Grok 风格选择框（左侧竖线及上下角，视口裁剪时使用虚线边界）。
4. block 之间使用稍大的垂直留白，避免思考、工具和最终回复粘成一整块。

## 上游调研依据

- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/wrappers/entry_renderer.rs`
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/wrappers/accented.rs`
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/layout.rs`
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/selection.rs`
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/scrollback_pane.rs`

上游通过 `accent + block_pad_left/right` 对齐内容，通过 `SelectionBox` 在 entry 外层绘制 `┌│└` 边框，并将 vpad 纳入 block 高度计算。本适配继续保留 DSH 的稳定 entry/block identity，仅移植这些可观察行为。

## 实现结果

- `RichPaintLine.content_offset` 固定表达 upstream 的三列 entry chrome；普通 Markdown、用户 prompt 与工具/思考内容共享同一文本起始列。
- 工具调用与思考折叠标题使用 diamond；相邻 collapsed operational block 保持连续 rail，进入 Markdown 时插入一行 separator。
- runtime 保存点击的 `HitTarget`，绘制可覆盖 entry/block 文本范围的 `┌│└` selection box；视口边缘使用 `┆`。
- 双击仍通过 `(entry_id, block_index)` 路由到独立 block fold，非 foldable Markdown 不会误折叠。

## 验证结果

- `cargo fmt --all`
- `cargo clippy -p dsh-pager-grok-ui --lib --locked -- -D warnings`
- `cargo test -p dsh-pager-grok-ui --lib --locked`（239 passed）
- `cargo test --workspace --locked`
- `cargo build --workspace --locked`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`
- `git diff --check`
