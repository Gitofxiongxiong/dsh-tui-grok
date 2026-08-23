# Suggestion controller TUI surface

时间：2026-08-23 14:00:04 +0800

## 目标

把 Grok suggestion/history/slash 的候选 viewport 接入生产 Prompt 交互，同时保留
DSH host 的 capability 和 authoritative suggestion snapshot。

## 本批范围

- Prompt 输入区上方渲染 suggestion 候选、选中态和 pending/unsupported 状态。
- Up/Down 选择、Tab 接受、Esc 清除候选的状态机由 UI owner 处理。
- 候选只来自 `GrokHostSnapshot::suggestions.items`，无 capability 时不伪造。
- 保持已有 prompt history (`Ctrl-P/Ctrl-N`) 和 slash 文本编辑行为。

## 下一批

接入真实 Grok SuggestionController 的排序/viewport fixture，并把 suggestion 接受
动作发成 DSH `UiIntent`/receipt；随后继续 Image preview 和 Workspace/Agent pane。
