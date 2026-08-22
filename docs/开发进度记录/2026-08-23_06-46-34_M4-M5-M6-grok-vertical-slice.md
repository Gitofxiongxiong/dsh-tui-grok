# M4/M5/M6 Grok transcript、prompt 与 session picker 垂直切片

> 记录时间：2026-08-23 06:46:34 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

继续 `GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md` 的 M4、M5、M6：把已经存在的
DSH 结构化 presentation、Grok-derived textarea/picker 和 loader/control-plane
真正接入默认 UI 路径。当前代码虽有 DTO 和组件，但 transcript 仍只画纯文本，
prompt 会 trim 丢失内容且只支持单行，picker 只显示当前 session，attach effect
也明确未实现。

## 设计契约和复用依据

- 对应长期计划章节：M4.1-M4.6、M5.1-M5.5/M5.8、M6.1-M6.5/M6.8。
- Grok source path、commit、SOURCE_REV：`vendor/grok`，mirror
  `19d42e35c07a9c9244f03f6dfc0c4c353f970d4f9`，`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：A1/B；保留 Grok picker/modal/line editor，DSH 只提供 neutral DTO、
  loader、control-plane 和 effect。

## 计划修改范围

- `crates/dsh-pager/src/presentation.rs`
- `crates/dsh-pager-grok-ui/src/host_adapter.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- `crates/dsh-pager-grok-ui/src/app.rs`
- `crates/dsh-pager-grok-ui/src/effects.rs`
- `crates/dsh-pager-grok-ui/src/input/mod.rs`
- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- 本记录文件

预期行为：富内容保留 Diff/Unknown/Image 等 block 并按 role 绘制；prompt 支持
grapheme-safe 多行编辑、粘贴、空白保留和失败重试草稿；picker rows 携带 stable
session id，刷新/过滤不依赖数组位置，Selected 产生 attach intent 并由 runtime
穿过 load barrier 替换 session。超出范围：协议 wire breaking change、Grok agent
runtime、真实 workspace action 的产品语义。

## 风险、回滚和依赖

风险是 ratatui 版本差异、mock backend 对 session attach 的响应差异，以及现有
fallback tests 对单行 prompt 的假设。所有修改集中在 presentation/UI seam；可回滚
为恢复本记录列出的文件，DSH protocol/session 真源不变。验证包括 fmt/check/test、
相关 clippy、协议 fixture 和已有 PTY smoke。

## 实际修改

### M4

- `DshRenderBlock` 增加结构化 `Diff`，content 支持 object/string block，保留
  `display_text` 作为明确的非 rich fallback；scrollback cache 使用结构化 display
  projection，不再只读取 entry 的旧纯文本字段。
- 新增 `views/transcript.rs`，按 Markdown/reasoning/tool/result/diff/image/unknown
  role 绘制并提供不 trim 的 copy reconstruction。
- runtime 主 transcript 路径改用 `Scrollback::total_height`、`visible_lines`、
  overscan/materialization 和 stable virtual scroll top，避免每帧复制完整历史。

### M5

- 新增基于 Grok `EditBuffer` 的 `PromptEditor`：multiline、Shift+Enter 换行、
  grapheme-safe cursor normalization、soft-wrap viewport、CRLF normalization、
  bracketed paste 控制字符过滤和 64 KiB 上限。
- Submit 保留原始换行/尾随空格；空白 prompt 不发送；accepted/queued/pending
  才清 draft，rejected/failed 保留 draft 并显示重试诊断。
- 加入 prompt paste/viewport 单元覆盖。

### M6

- host adapter 新增 `from_session_with_control_plane`，从 DSH authoritative
  roster projection 生成 stable session rows、状态和 workspace detail；过滤和
  selection 通过 row ID 映射，不依赖数组索引。
- picker 打开时刷新 `session.list`/workspace baseline；Selected 编译为
  `UiIntent::AttachSession`/`UiEffect::AttachSession`，随后调用 `load_session_id`
  穿过 attach/history/live load barrier；成功替换当前 `SessionState`，失败保留
  picker 和原 transcript。
- A/B session 切换、gone/error 结果均显示显式诊断；旧 generation 仍由 loader/
  session guard 拒绝。

修改文件均为本记录计划范围；未修改 Grok vendor 内容和 DSH wire protocol。

## 验证结果

验证通过：

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --locked`（全 workspace、含 150 个 UI 测试）
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager`
- `git diff --check`

## Git 提交

本批提交使用本记录路径作为 `Progress-Record` trailer；暂存区审计和 commit
在收尾时执行。

## 未解决问题和下一步

仍未完成的长期出口：Grok 完整 markdown/code renderer golden、selection/mouse
跨 block copy、dashboard workspace hierarchy 的完整视觉 pane、50k entry 性能
矩阵和真实 backend parity。这些属于 M4.5-M4.8、M6.6-M6.8/M10 后续门禁，不在本
垂直切片中伪称完成。
