# Media preview effect

时间：2026-08-23 14:34:00 +0800

## 目标

让 Image/Media overlay 使用 Harness 已存在的 attachment authority，而不是只
展示 transcript metadata。

## 本批范围

- 增加 `dsh_pager::fetch_attachment`，调用 `session.attachment`，保留 Host 对
  session 引用、附件权限和存储的授权边界。
- 增加 `UiIntent::PreviewMedia` / `UiEffect::PreviewMedia`，以稳定
  `session + attachmentId` identity 提交预览读取。
- Image Preview overlay 支持 Up/Down 选中和 Enter 读取；receipt 只表示读取
  是否被 host 接纳，数据不会伪造写入 session transcript。
- 对 base64 preview 设置 1 MiB 上限，超限明确失败，避免 UI effect 搬运无界媒体。
- 缺少 attachment id、terminal capability 或 host attachment service 时保留
  unsupported/fallback 文案。

## 下一批

把 `AttachmentPreview` 的 metadata/data 接入独立 media preview snapshot 与
terminal-specific renderer；继续接入 workspace archive/reorder effect 的
receipt/convergence。
