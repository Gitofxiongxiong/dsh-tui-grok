# P1/P2 Theme、Appearance 与 Prompt chrome 接线

时间：2026-08-23 13:43:24 +0800

## 目标

按照 `GROK_RENDERER_PIXEL_PARITY_PLAN.md`，先完成 renderer 基础层：登记 Grok
完整 UI 闭包，扩展 DSH renderer 的语义主题 token，提供不持有 runtime 引用的
Appearance 快照，并让生产 Prompt chrome 使用同一组 token。

## 本批范围

- 盘点并登记 Markdown、Diff、Scrollback、Prompt、File Search、Suggestion、Image、
  Workspace、Agent/Task/Subagent 的上游入口、DSH DTO 和 effect 接缝。
- 将 Prompt、Markdown、Diff、Image、Selection、Scrollbar 所需的 Grok 语义颜色
  纳入 `dsh-pager-render::Theme`。
- 添加 `GrokAppearanceSnapshot`，集中管理 compact、padding、scrollback floor、
  prompt chrome 和 scrollbar 参数。
- 保持 `SessionState`、RPC、agent loop 和文件系统副作用不进入 renderer 层。

## 验证

本批完成后运行 `cargo fmt --all -- --check`、`cargo test -p dsh-pager-grok-ui
--locked`、`cargo test --workspace --locked` 和 `scripts/check-source-manifest.py`。

## 下一批

接入独立的 Grok PromptWidget/Scrollback block renderer；在完成 DSH DTO 与 effect
后，再接 File Search、Suggestion、Image preview、Workspace 和 Agent/Task/Subagent
的真实状态闭包。
