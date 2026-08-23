# UserPrompt vpad contract

> 记录时间：2026-08-23 21:35:00 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标

补齐真实 Grok `UserPromptBlock` 与 `AgentMessageBlock` 的最后一个可见 wrapper 差异：默认 UserPrompt band 上下各保留一行 vpad，AgentMessage 不增加 vpad；vpad 是视觉行但不可选择、不可复制。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- 本进度记录

不修改 canonical DTO、协议或 host scrollback height seam；UserPrompt vpad 计入同一个 projection height/index，正文命中仍共享 render-time geometry。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo test -p dsh-pager-grok-ui --lib --locked`
- `cargo build -p dsh-pager-bin --locked`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`
- `git diff --check`

## 实际修改

- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
  - UserPrompt band 增加顶部/底部各一行背景 vpad；AgentMessage 路径不增加 vpad。
  - vpad 行计入投影高度和 anchor，但标记为 `selectable=false`、`copy_text=""`，不会进入 runtime hit map、选择或复制。
- `crates/dsh-pager-grok-ui/src/runtime.rs`
  - paint vpad 行但跳过其 geometry/hit map，正文行仍使用相同 entry/line identity。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test -p dsh-pager-grok-ui --lib --locked`：通过，236 tests passed。
- `cargo test --workspace --locked`：通过，workspace 单元测试与 doctest 全部通过。
- `cargo clippy -p dsh-pager-grok-ui --lib --locked -- -D warnings`：通过。
- `cargo build -p dsh-pager-bin --locked`：通过。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`：通过。
- `git diff --check`：通过。

## 未解决问题和下一步

完整 Grok wrapper/selection parity、特殊 tool viewer、ContextGroup P19-B 和真实 Harness geometry 仍需后续批次。
