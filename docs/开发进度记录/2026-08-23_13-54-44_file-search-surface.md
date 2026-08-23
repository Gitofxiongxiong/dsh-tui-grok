# File Search TUI surface

时间：2026-08-23 13:54:44 +0800

## 目标

把 File Search 从 host DTO seam 接入生产 TUI，保留 Grok 的 overlay/focus 形态，
并严格区分 authoritative result、pending 和 unsupported。

## 本批范围

- 增加 `FileSearch` overlay、独立 `KeyOwner` 和 Esc/paste/mouse 路由。
- 增加查询编辑、稳定结果 ID、选中行、line/snippet preview。
- 没有 DSH filesystem search effect 时渲染 pending/unsupported 诊断，不把
  `session.search` 或空列表冒充文件搜索完成。
- 在 Dashboard/主界面保留 workspace、agent/task、suggestion、image 等已有
  surface，不改变其 host authority。

## 下一批

增加 filesystem search `UiIntent`/`UiEffect` 和真实 backend snapshot；随后接入
Grok SuggestionController、Image preview lifecycle、Workspace mutation 和
Agent/Subagent pane。
