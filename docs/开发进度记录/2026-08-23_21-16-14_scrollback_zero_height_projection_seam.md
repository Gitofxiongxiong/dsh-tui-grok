# Scrollback projection zero-height seam

> 记录时间：2026-08-23 21:16:14 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

本批次 fold/group vertical slice 已在 `dsh-pager-grok-ui` 生成 group header 和
`group_hidden` projection，但现有 `dsh-pager::Scrollback::set_rendered_height`
将所有非 `DshRenderVisibility::Hidden` entry 的高度强制为至少 1。这样 canonical
entry 虽未删除，折叠 group 成员却仍留下空行，无法实现 Grok 的 dense rail/gap 语义。

## 设计契约和复用依据

- 对应计划：`docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md` 的 group fold
  契约和 D.6；长期计划 M4.3。
- DSH seam：只放宽 `Scrollback::set_rendered_height` 对 view-time projection 的
  height contract；canonical entry、visibility DTO、height index identity 和
  anchor API 不变。
- 复用等级：C（DSH-owned height index seam），不复制 Grok runtime。

## 计划修改范围

- 文件：
  - `crates/dsh-pager/src/scrollback.rs`
  - `docs/开发进度记录/2026-08-23_21-16-14_scrollback_zero_height_projection_seam.md`
- 预期行为：仅允许 renderer 传入的 `height == 0` 成为 view projection 的零高度；
  Hidden 仍为零高度，普通可见 entry 仍至少一行；layout/anchor/paint window 正确
  跳过零高度成员。
- 不在范围内：不修改协议、presentation DTO、vendor、runtime mouse、canonical
  history 或 height index 算法。

## 风险、回滚和依赖

- 风险：错误调用若传入零高度可能隐藏普通 entry；调用方必须只在
  `ScrollbackPane::group_hidden` projection 中传零，新增单测保护可见 entry 最小高度。
- 回滚：回退本记录列出的 `scrollback.rs` 和本记录即可；UI vertical slice 可保留
  但 group 成员会退回一行高度。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo test -p dsh-pager --lib --locked`
- `cargo test -p dsh-pager-grok-ui views::transcript::tests --lib --locked`
- `git diff --check`

## 实际修改

- `crates/dsh-pager/src/scrollback.rs`
  - 将原 `set_rendered_height` 保持为普通 rich-renderer 边界：非 Hidden entry 高度仍至少为 1。
  - 新增 `set_projected_height`，仅供 view-time projection 使用，允许明确传入零高度；Hidden 仍强制为零。
  - 共用私有 `set_height`，不改变 canonical entries、stable ID、Fenwick height index 或 anchor API。
  - 新增回归测试，验证折叠成员仍保留在 canonical entries 中、layout height 为零，并可由普通 renderer 高度恢复。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test -p dsh-pager --lib --locked`：通过；scrollback 相关测试 13 passed。
- `cargo test -p dsh-pager-grok-ui views::transcript::tests --lib --locked`：通过；transcript tests 11 passed。
- `cargo test --workspace --locked`：通过。
- `git diff --check`：通过。

## Git 提交

- commit message：未提交（用户未要求提交）。
- Progress-Record trailer：待提交/未提交。
- 暂存区审计：当前工作树包含此前批次和本轮 UI vertical slice 修改，不暂存、不提交。

## 未解决问题和下一步

完整 Grok layout/group renderer、真实 Harness P19-B/P20 和 reference geometry 仍需后续批次；若未来需要让 `materialize_viewport` 参与同一投影，还应把 projection height provider 显式接入该 API，避免其兼容 plain-text 测量覆盖 view-local 零高度。
