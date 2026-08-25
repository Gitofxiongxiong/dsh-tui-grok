# 接入 Grok question_view

> 记录时间：2026-08-25 22:01:43 +0800
> 操作者/agent：Grok
> 状态：completed

## 目标与背景

DSH `ask_user_question` / `exit_plan_mode` 被接到自研 Interaction 弹窗，
按 1 回车会被答案框里的鼠标残片覆盖，host 拒绝。Grok 原生 ask 是
`question_view.rs`：占住 prompt 区，和 permission card 同一家族。
本批次 B 级移植该卡，替换弹窗。

## 设计契约和复用依据

- Grok：`crates/codegen/xai-grok-pager/src/views/question_view.rs`
- 复用等级 B：composer 放置、33% 高度帽、accent rail、选项 radio、
  1–N/Enter/Esc park；排除 ACP oneshot、LocalQuestionKind、plan 底栏、
  syntect preview。
- DSH 真源：`DshInteraction::Question`；`selected` 必须是 option label。
- permission_view 已证明同一接线：prompt_height + input 区直绘。

## 计划修改范围

- 文件：
  - `crates/dsh-pager-grok-ui/vendor/grok/xai-grok-pager/src/views/question_view.rs`
  - `crates/dsh-pager-grok-ui/src/views/mod.rs`
  - `crates/dsh-pager-grok-ui/src/views/interaction.rs`
  - `crates/dsh-pager-grok-ui/src/runtime.rs`
  - `crates/dsh-pager-grok-ui/src/app.rs`
  - `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`
  - 本进度记录
- 预期行为：问答卡替换 composer；1–N 立即按 label 提交；Esc 只把焦点
  交给 scrollback，不关 overlay、不 Quit。
- 不在范围内：`plan_approval_view` 全文、Grok ACP、freeform 多问题 tab。

## 风险、回滚和依赖

- 计划正文 `detail` 先截断展示，Ctrl-F 展开，避免小终端撑爆。
- 回滚：还原本记录文件。

## 预期验证

- `cargo test -p dsh-pager-grok-ui --lib question_ -- --nocapture`
- `cargo test -p dsh-pager-grok-ui --lib plan_review -- --nocapture`
- `cargo test -p dsh-pager-grok-ui --lib -- views::interaction views::question_view`
- `python3 scripts/check-source-manifest.py`

## 实际修改

- 文件：
  - `crates/dsh-pager-grok-ui/vendor/grok/xai-grok-pager/src/views/question_view.rs`
  - `crates/dsh-pager-grok-ui/src/views/mod.rs`
  - `crates/dsh-pager-grok-ui/src/views/interaction.rs`
  - `crates/dsh-pager-grok-ui/src/runtime.rs`
  - `crates/dsh-pager-grok-ui/src/app.rs`
  - `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`
  - 本进度记录
- 摘要：
  - B 级移植 Grok `question_view`：composer 接管、33% 高度帽、accent rail、
    1–N radio。
  - `DshInteraction::Question` 投影到该卡；`plan-review` 补 header
    `Plan review`；`selected` 只发 option label。
  - 有选项时 1–N 立即提交对应 label，Enter 提交当前项；键入/粘贴/鼠标
    CSI 不再进答案框。越界数字键不回落到第一项。
  - Esc 把焦点交给 scrollback，卡片保持；Enter/i 回到卡片。不画
    `Interaction · host request` 弹窗，不 dim。

## 验证结果

- 命令：
  - `cargo fmt -p dsh-pager-grok-ui` PASS
  - `python3 scripts/check-source-manifest.py` PASS（74 rows）
  - `cargo test -p dsh-pager-grok-ui --lib -- question_ plan_review views::interaction views::question_view question_card_parks question_replaces_composer` PASS（10 tests）
  - `cargo test -p dsh-pager-grok-ui --lib -- approval_replaces_composer permission_card_parks semantic_snapshot_captures all_overlay_events question_` PASS（11 tests）
  - `cargo test -p dsh-pager-grok-ui --lib` PASS（489 passed）

## Git 提交

- commit message：
  `feat(tui): render DSH questions with the Grok ask card`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-25_22-01-43_接入Grok-question-view.md`
- 暂存区审计：只包含本记录列出的文件；未纳入
  `2026-08-25_21-35-00_架构分析HTML报告.md` 与
  `2026-08-25_21-53-51_计划审批问答提交被拒.md`

## 未解决问题和下一步

- 重启 TUI 后，计划审批应出现在 prompt 区：按 `1` 直接提交 `Approve`。
- `plan_approval_view` 全文、freeform 多问题 tab、Grok ACP 仍排除。
- 无选项的自由输入仍走卡片上的草稿键，没有独立 freeform 行。
