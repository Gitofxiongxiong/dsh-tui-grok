# Workspace mutation effects

时间：2026-08-23 14:38:00 +0800

## 目标

把 Dashboard 的 workspace/session 操作接入统一 effect seam，并让 UI 在 receipt
之后重新从 control-plane 收敛，而不是本地假改列表。

## 本批范围

- 新增 `ArchiveSessionTarget` effect，Dashboard `x` 对选中 session 发送归档请求。
- 新增 `ReorderSession` effect，Dashboard `Shift+Up/Down` 使用 host workspace
  order 计算 `beforeSessionId`，调用 `workspace.insertSessionBefore`。
- 成功 admission 后刷新 `session.list` + `workspace.list`，重新投影 Dashboard；
  conflict/stale/unsupported 保留 receipt 诊断。
- 为 workspace mutation 增加稳定 target identity 测试和 Dashboard footer affordance。

## 下一批

接入 workspace 本身的 reorder/create/archive 操作和 Grok dashboard tree renderer；
媒体 preview bytes 继续接入独立 preview snapshot，避免把读取 receipt 当成已经绘制
bitmap。
