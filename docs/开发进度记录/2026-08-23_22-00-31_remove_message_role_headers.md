# 移除消息角色标题

## 状态

已完成

## 修改范围

- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- 本进度记录

## 目标

- 用户消息只显示首行提示符 `❯ `，不显示人为添加的 `You` 标题。
- 用户消息换行后使用等宽空白与正文对齐，不在每一行重复提示符。
- Agent 消息直接从 Markdown 正文开始，不显示人为添加的 `Assistant` 标题。
- 保留 thinking、tool、context 等非对话条目的语义标题。
- 用户消息的默认折叠阈值保持与 Grok TUI 一致：正文超过 3 行时预览前三行。

## 上游依据与复用级别

- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/blocks/user.rs`
  - `UserPromptBlock` 不绘制 `You`；首行使用 `prompt_arrow()`，续行使用等宽缩进。
  - `COLLAPSED_MAX_LINES` 为 3。
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/blocks/agent.rs`
  - `AgentMessageBlock` 直接绘制 Markdown 内容，不绘制 `Assistant`。
- `/home/leo/code/grok-build/crates/codegen/xai-grok-pager-render/src/glyphs.rs`
  - 正常终端提示符为 `❯ `。

采用 Level B 行为复用：沿用本项目已有 transcript 数据模型与 Markdown 渲染器，精确对齐上游消息外壳行为；不修改 host DTO 或事件协议。

## 计划验证

- transcript 定向单元测试
- `cargo test -p dsh-pager-grok-ui --lib --locked`
- `cargo clippy -p dsh-pager-grok-ui --lib --locked -- -D warnings`
- `cargo test --workspace --locked`
- 构建与 PTY smoke test
- `git diff --check`

## 实际修改

- `render_row` 对 `User` 与 `Assistant` 跳过角色标题行。
- `User` 使用 `❯ ` 首行提示符，后续行使用两个空格缩进；用户正文按纯文本绘制，避免把提示符重复到每一行。
- `Assistant` 继续走既有 Markdown AST 渲染路径，Markdown 标题、强调、行内代码和代码块均从第一行直接显示。
- 其他操作性条目仍保留语义标题，折叠/分组布局和 host DTO 未改变。
- 用户消息默认预览阈值从包含标题的 4 行调整为正文 3 行，与上游 `UserPromptBlock` 一致。
- 更新 transcript 测试，覆盖角色标题缺失、首行箭头、续行缩进、Markdown 首行和用户背景/折叠高度。

## 验证结果

- `cargo test -p dsh-pager-grok-ui --lib transcript::tests --locked`：12 passed
- `cargo test -p dsh-pager-grok-ui --lib --locked`：237 passed
- `cargo clippy -p dsh-pager-grok-ui --lib --locked -- -D warnings`：通过
- `cargo test --workspace --locked`：通过
- `cargo build --workspace --locked`：通过
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager`：通过
- `git diff --check`：通过

## 备注

本切片只修正用户/Agent 消息前的视觉角色标题；`DshRenderKind::label()` 仍保留给内部筛选、复制和语义识别使用，不代表会再次绘制标题。
