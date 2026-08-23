# 流式思考与工具块组件化投影

## 状态

已完成

## 修改范围

- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- 本进度记录

## 问题

DSH presentation 层已经保留 `DshRenderBlock::Reasoning`、`ToolCall`、`ToolResult` 和 `Markdown`，但同一个 `(turn, step)` streaming surface 的 blocks 会在 transcript 中被一次性 `render_block`。因此 thinking/tool block 虽然类型正确，生产 TUI 仍然显示成一整块连续正文。

## 上游调研

- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/blocks/thinking.rs`
  - running 默认 `Truncated`，完成后默认 `Collapsed`；折叠标题为 `Thinking…` / `Thought for Xs`。
  - thinking 可通过展开手势切换到完整 Markdown 内容。
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/blocks/agent.rs`
  - AgentMessage 保持 Markdown 正文展开，不参与 verb group。
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/state/verb_group.rs`
  - 连续 collapsed tool members 聚合为一个 verb header；finished collapsed thinking 可以作为 ThoughtMember 加入连续 rail，但不计入工具数量。
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/wrappers/entry_renderer.rs`
  - 折叠 header、accent/rail 和 block body 是分层 wrapper，不是把所有 block 展平为普通文本。

## 适配策略

采用 Level B 行为复用：不把依赖 Grok runtime 的 `RenderBlock`/`BlockContent` 原样引入 DSH host；在现有 `DshRenderContent` 上增加等价的 block-level projection、折叠状态、block hit target 和 rail 绘制。保留 host DTO 与 stable surface identity，避免破坏已有流式 finalize contract。

## 预期行为

- 同一 assistant streaming surface 内的 reasoning 默认只显示 `Thinking…`/`Thought` 摘要，双击对应 block 后显示正文。
- tool call 默认只显示 `▸ tool-name` 摘要及运行/结果状态，双击对应 block 后显示参数、diff 和结果。
- final Markdown block 继续直接展开显示，不被 thinking/tool block 的折叠状态遮挡。
- 相邻 reasoning/tool block 使用连续左侧 `│` rail；工具 group header 仍使用现有 entry-level group 语义。
- 点击/选择仍复用现有 `TranscriptBlock` hit target，不修改协议副作用边界。

## 计划验证

- transcript block projection 单元测试（thinking/tool collapsed 与 expanded、Markdown 保持展开）
- block hit target 与双击展开测试
- `cargo fmt --all`
- `cargo test -p dsh-pager-grok-ui --lib --locked`
- `cargo clippy -p dsh-pager-grok-ui --lib --locked -- -D warnings`
- `cargo test --workspace --locked`
- `cargo build --workspace --locked`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`
- `git diff --check`

## 实际修改

- 在 `transcript.rs` 增加 assistant surface 内的 block-level projection：
  - `Reasoning` 默认显示 `Thinking…`/`Thought` 摘要。
  - `ToolCall` 默认显示 `▸ tool-name`，参数和 diff 按 block 独立展开。
  - `ToolResult` 默认显示 `✓ result`/`✗ result`，详情按 block 独立展开。
  - `Markdown`/最终回复保持直接展开，不被操作 block 的折叠状态影响。
- 为每个结构化 block 添加 `block_index`，生产绘制路径改用既有 `HitTarget::TranscriptBlock`。
- `ScrollbackPane` 增加 `(entry_id, block_index)` 本地展开状态；双击只切换对应 thinking/tool block，不改变 canonical host DTO。
- 操作 block 绘制 `│ ` rail；最终 Markdown 作为 run breaker，不被 rail 包住。
- 历史回放中即使没有 `group_key`，只要 Assistant entry 含多个 typed blocks 也会进入同一投影。
- runtime 鼠标双击、选择高亮和 block hit target 均已适配；entry-level tool/context group 行为保持不变。

## 验证结果

- transcript 定向测试：13 passed
- `cargo clippy -p dsh-pager-grok-ui --lib --locked -- -D warnings`：通过
- `cargo test -p dsh-pager-grok-ui --lib --locked`：238 passed
- `cargo test --workspace --locked`：通过
- `cargo build --workspace --locked`：通过
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`：通过
- `git diff --check`：通过

## 备注

没有原样引入 Grok 的 `RenderBlock`/`BlockContent` 类型，因为它们依赖 Grok 自己的 scrollback/runtime 类型；本切片复用了其 ThinkingBlock、ToolCallBlock、AgentMessageBlock、VerbRun 的行为契约，并接到 DSH 已有 typed block 与稳定 surface 边界。
