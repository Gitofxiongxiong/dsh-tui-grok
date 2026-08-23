# File Reference TUI Gateway 与 Media Result Buffer

> 记录时间：2026-08-23 15:02:00 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

闭合 File Search 与 Image/media 的真实 host effect seam，使新 TUI 不再以固定
unsupported 或静态 placeholder 代替 DeepSeek Harness 的权威响应，同时保持
renderer local result buffer 与 `SessionState` 真源分离。

## 设计契约和复用依据

- 对应长期计划：`GROK_RENDERER_PIXEL_PARITY_PLAN.md` P2A、P6 和 host contract；
- Grok source：固定 mirror commit
  `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，SOURCE_REV
  `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`；本批不复制 Grok renderer；
- 复用等级：DSH protocol/loader/effect 为 C；Grok File Search/Image renderer
  仍为 planned A1/B；Grok runtime 为 D；
- 动作边界：`UiIntent -> UiEffect -> receipt -> authoritative response -> renderer DTO`。

## 计划与实际修改范围

- `crates/dsh-pager-protocol/src/lib.rs`：typed file-reference response DTO；
- `crates/dsh-pager/src/{lib.rs,loader.rs}`：`fileReferences.list` loader 和有界
  attachment response；
- `crates/dsh-pager-grok-ui/src/{effects.rs,runtime.rs}`：effect handoff、query
  revision、mention insertion 和 media preview result buffer；
- `crates/dsh-pager/tests/capability_effects.rs`：transport contract tests；
- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`：能力状态和 P6 执行顺序；
- 本记录：变更审计。

不在范围内：Grok PromptWidget/Scrollback renderer、Harness agent runtime、把临时
result buffer 写入 session log、删除 frozen fallback shell。

## 实际修改

- DeepSeek Harness 增加 `fileReferences.list` ApiProxy contract、schema、fetch carrier
  和 TUI method registration，调用现有 `ctx.fileReferences` provider；
- Rust protocol/loader 增加 typed file-reference DTO 和 `list_file_references`；
- `UiEffect::FileSearchQuery` 不再固定返回 unsupported，真实读取 path-only candidates；
- File Search 结果按 query revision 保存在 host adapter result buffer，选择后以 `@path`
  或 `@"path with spaces"` 插入 prompt；
- Media preview 将 `session.attachment` 的有界响应放入独立 preview buffer，并显示
  media type、原始 bytes、base64 payload 大小和尺寸；
- 增加 Rust transport contract、UI revision/formatting 和 Harness gateway tests。

## 边界

- `session.search` 仍只搜索会话内容，不参与 filesystem file search；
- File Reference provider 只返回 path/kind，不读取文件内容；
- attachment data 不写入 `SessionState`，不进入 renderer DTO 的长期 snapshot；
- 当前 File Search/Image 视觉仍是过渡实现，Grok renderer 闭包尚未完成。

## 风险、回滚和依赖

- attachment payload 限制为 1 MiB；超限、缺失和 host error 保持显式失败；
- query response 只在 revision/query 匹配时进入当前 overlay，避免 stale 覆盖；
- 回滚本提交即可恢复原 effect seam，不影响 DeepSeek Harness session truth。

## 验证结果

- `cargo test --workspace --locked`：通过，Grok UI 201 tests，workspace 全绿；
- `cargo fmt --all -- --check`：通过；
- `python3 scripts/check-protocol-fixtures.py`：通过；
- `python3 scripts/check-source-manifest.py`：通过；
- `python3 scripts/parity-matrix.py`：通过，972 cases / 8 fixtures；
- `git diff --check`：通过；
- DeepSeek Harness `pnpm run typecheck`：通过；
- DeepSeek Harness 定向 vitest：60 files / 887 tests 通过；
- DeepSeek Harness `pnpm run lint:contracts-ready`：通过。

## Git 提交

- commit message：`feat: wire file references and media result buffers`
- Progress-Record trailer：
  `docs/开发进度记录/2026-08-23_15-02-00_file-reference-media-result-buffer.md`
- 暂存区审计：只暂存本记录列出的 dsh-pager-grok 文件；DeepSeek Harness 的
  既有 control-plane 混合工作树和下一批 PromptWidget 记录不进入本提交。

## 未解决问题和下一步

按 parity 方案 P6 开始 vendor `PromptWidget`、`AgentViewLayout` 和上游测试闭包，
随后迁移 Scrollback/Markdown/Diff/Image renderer，避免继续扩展 frozen fallback shell。
