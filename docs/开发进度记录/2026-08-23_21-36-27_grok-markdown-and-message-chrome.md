# Grok markdown and message chrome adaptation

> 记录时间：2026-08-23 21:36:27 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

当前 transcript 虽然保留了 `DshRenderContent::Markdown`，但 `render_markdown` 仍是
按字符串前缀猜测 heading/code/link 的 fallback；用户实际看到的 agent 回复仍可能
是原始 Markdown。UserPrompt 也缺少 Grok 的 `❯ ` prompt affordance。此次按真实
`/home/leo/code/grok-build` 的 `AgentMessageBlock`/`MarkdownContent`/`UserPromptBlock`
契约，把 markdown 解析和 message chrome 接入现有 DSH-neutral entry projection。

## 设计契约和复用依据

- 上游 source：
  - `crates/codegen/xai-grok-pager/src/scrollback/blocks/agent.rs`
  - `crates/codegen/xai-grok-pager/src/scrollback/blocks/markdown_content.rs`
  - `crates/codegen/xai-grok-pager/src/scrollback/blocks/user.rs`
  - `crates/codegen/xai-grok-pager-render/src/glyphs.rs`
- mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`；`SOURCE_REV`：
  `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：B；保留 Grok 的 block 责任、Markdown pretty/raw 方向、prompt arrow
  和 message role，DSH 继续拥有 canonical entry/content DTO。
- 不把 Grok agent/runtime 或外部 session 类型带入；当前 markdown adapter 使用
  与 Grok 相同的 `pulldown-cmark` 基础语法边界，后续可继续替换为完整
  `xai-grok-markdown` streaming crate。

## 计划修改范围

- `Cargo.toml`
- `crates/dsh-pager-grok-ui/Cargo.toml`
- `Cargo.lock`
- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- 本进度记录

预期行为：AgentMessage 的 headings、emphasis/strong、inline code、fenced code、
links、lists、blockquotes 和 paragraphs 按 Markdown AST 渲染；UserPrompt 正文行
带 Grok `❯ `（legacy fallback 为 `> `）前缀；`You`/`Assistant` role header 保持
稳定并参与 wrap/copy 的语义分层。

## 风险、回滚和依赖

- 风险：Markdown AST 行数与既有 wrapper height/copy geometry 不一致；统一从
  `semantic_lines` 生成 paint/copy/height，增加结构化测试。
- 风险：增加 `pulldown-cmark` 会更新 lockfile；只使用其已锁定兼容版本，失败时
  可回滚新增依赖和 transcript fallback 改动。
- 不在范围：完整 Grok syntect/LaTeX/Mermaid/streaming cache、special-tool viewer。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo test -p dsh-pager-grok-ui --lib --locked`
- `cargo test --workspace --locked`
- `cargo clippy -p dsh-pager-grok-ui --lib --locked -- -D warnings`
- `cargo build -p dsh-pager-bin --locked`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`
- `git diff --check`

## 实际修改

- `Cargo.toml` / `Cargo.lock`：加入与 Grok markdown parser 同源的
  `pulldown-cmark 0.13.4` workspace 依赖。
- `crates/dsh-pager-grok-ui/Cargo.toml`：接入 workspace parser。
- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
  - 用 pulldown-cmark AST 替换按字符串前缀猜测的 Markdown fallback；现在会处理
    heading、paragraph/soft-break、emphasis、strong、strike、inline/fenced code、
    links、plain URL、lists、task marker、blockquote、rule 和 Markdown role styles。
  - User row 保持 `You` header，并在正文行加 Grok `❯ ` prompt arrow；Agent row
    保持 `Assistant` header。
  - 新增 AST/chrome 回归测试，确认 Markdown syntax 不再作为原始 `#`/``` 文本出现。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test -p dsh-pager-grok-ui views::transcript::tests --lib --locked`：通过，12 tests passed；最后复跑覆盖了列表 marker、User arrow 和 Markdown role chrome。
- `cargo test --workspace --locked`：通过；workspace 单元测试和 doctest 全部通过，UI 237 tests passed。
- `cargo clippy -p dsh-pager-grok-ui --lib --locked -- -D warnings`：通过。
- `cargo build -p dsh-pager-bin --locked`：通过。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`：通过。
- `git diff --check`：通过。

## Git 提交

- 未提交（用户未要求提交）；保留工作树既有修改。

## 未解决问题和下一步

- 这次接入了 Grok `AgentMessageBlock` 的 Markdown AST 行为边界，但没有把完整的
  `xai-grok-markdown` 20k 行 streaming/syntect/LaTeX/Mermaid crate 整体复制进来；
  后续若需要逐像素 parity，应按独立批次 vendor 该 crate 及其依赖，并替换当前
  AST adapter。
- special-tool viewer、完整 selection/link hit map 和 P19-B/P20 真实 Harness 仍需后续批次。
