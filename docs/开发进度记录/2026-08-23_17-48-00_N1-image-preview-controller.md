# N1 Image/media preview controller

> 记录时间：2026-08-23 17:48:00 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

File Search 与 Suggestion controller 已进入 production；本批收敛 Image preview
的 attachment identity、capability gate、pending/ready/unsupported 状态。Host
仍是 bytes/metadata 真源，UI 不把 metadata 列表冒充图片已经显示，也不在不支持
终端时发送无效 attachment RPC。

## 设计契约和复用依据

- 对应 GROK_RENDERER_PIXEL_PARITY_PLAN N1 Image：attachment chip/preview、尺寸
  限制和显式 fallback；固定 Grok prompt-image source 只作为 provenance 参考，
  不复制 Grok image/agent runtime。
- `MediaPreviewController` 只保存当前 attachment stable id 和 generation；旧
  completion 不能覆盖新选择。
- terminal/host capability 缺失返回 `Unsupported` 诊断；可执行请求才经过
  `AsyncEffectExecutor`，completion 只表示 bytes admission，renderer 仍显示
  explicit loaded/fallback surface。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/media.rs`：新增 preview controller/decision 和
  capability/identity tests。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：接 controller，移除 preview completion
  的散落 attachment 比较和 capability 绕过。
- 本记录文件：收尾回写。

## 风险、回滚和依赖

- 风险：把 host capability 与 terminal capability 混淆会导致假成功；两者都必须
  gate。
- 风险：用户快速上下移动 selection 时旧 bytes 到达；generation/id fence 必须
  保留。
- 回滚：回退本记录单一 commit，保留 N1 controller/effect commits。

## 实际修改

- 新增 `MediaPreviewController`，以 attachment stable id 和 generation 作为
  completion admission fence；新选择、清空预览都会使旧 completion 失效。
- 预览请求同时检查 terminal alternate-screen/cell-diff 与 host image
  capability；不满足时返回显式 `CapabilityUnavailable`，不会发出无效 RPC。
- runtime 只把 controller 接受且 attachment id 完全匹配的 host preview bytes
  写入 bounded preview buffer，保留 metadata/fallback surface 的真实状态。
- 加入旧 attachment、capability fallback 和 clear generation 的单元测试。

## 验证结果

- `cargo fmt --all -- --check`
- `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features -- -D warnings`
- `cargo test -p dsh-pager-grok-ui --lib --quiet`（227 passed）
- `git diff --check`

## Git 提交

- commit message：`feat: add capability-gated image preview controller`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_17-48-00_N1-image-preview-controller.md`

## 未解决问题和下一步

- 当前 terminal renderer 仍显示 bounded loaded metadata/fallback；实际 Kitty/
  iTerm image escape 需要独立 terminal backend capability/flush 设计，不在本批
  假造为已支持。
- N2 继续迁移 ScrollbackPane/block renderer。
