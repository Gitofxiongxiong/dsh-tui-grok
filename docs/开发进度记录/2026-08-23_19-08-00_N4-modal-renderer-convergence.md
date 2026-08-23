# N4 modal renderer convergence

> 记录时间：2026-08-23 19:08:00 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

N4 首批已经让 transcript、task/subagent、dashboard 使用独立 view adapter，但
runtime 仍内嵌 File Search/Image modal 的绘制循环。此批只移动视觉职责，不改动
N1 controller/effect contract：对应 view module 消费 typed DTO、stable selection
和 capability status，runtime 只负责 overlay composition。

## 设计契约和复用依据

- File Search renderer 继续使用 `FileSearchSnapshot` 的 path/kind/typed preview，
  path-only row 不生成虚假的 line/snippet；selected target 仍是 stable row id。
- Image renderer 继续使用 `MediaSnapshot`、attachment identity 和
  `MediaPreviewBuffer`，unsupported/pending/missing bytes 显式展示。
- 这是 frozen runtime fallback 的职责迁移，不复制 Grok agent/runtime；后续可
  以同一 module seam 替换完整上游 overlay renderer。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/views/file_search/line_viewer.rs`：File Search
  modal renderer。
- `crates/dsh-pager-grok-ui/src/media.rs`：Image modal renderer 与 preview DTO。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：删除内嵌 modal renderer，改 import。
- `crates/dsh-pager-grok-ui/src/views/mod.rs`：导出 media/view renderer seam（如需）。
- 本记录文件：收尾回写。

## 风险、回滚和依赖

- 风险：只移动视觉代码不应改变 effect admission；用现有 UI tests + 全量 E2E
  对比输出和 unsupported 文案。
- 回滚：回退本记录单一 commit，保留 N1-N4 前序 commits。

## 实际修改

- `views/file_search/line_viewer.rs` 现在承载 File Search modal 的 typed row、
  preview、selection 和 pending/unsupported/empty surface；runtime 删除对应
  `buffer.set_string` 循环，仅组合 modal 与 controller。
- `media.rs` 承载 `MediaPreviewBuffer` 和 Image preview modal renderer；preview
  bytes、attachment identity、metadata/fallback 文案保持原 contract，runtime
  不再复制图片列表绘制逻辑。
- runtime 仅 import view renderer，既有 tests 继续验证 path-only rows、不支持状态、
  matching attachment buffer 和 preview metadata。

## 验证结果

- `cargo fmt --all`
- `cargo check -p dsh-pager-grok-ui`
- `cargo test -p dsh-pager-grok-ui --lib --quiet`（232 passed）
- `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features -- -D warnings`
- `git diff --check`

## Git 提交

- commit message：`refactor: move capability modal renderers into views`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_19-08-00_N4-modal-renderer-convergence.md`

## 未解决问题和下一步

- Kitty/iTerm image escape 仍保持显式 unsupported/fallback，等待 backend capability
  设计；本批不改变该事实。
