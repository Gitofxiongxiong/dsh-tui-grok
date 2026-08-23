# Grok scrollback fold/group vertical slice

> 记录时间：2026-08-23 21:10:00 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

按 `GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md` 的 Phase 3 契约，继续修复截图中可见的 transcript 差异：UserPrompt 与 AgentMessage 使用不同的视觉 wrapper/background；Thinking/Tool 默认可折叠；相邻 tool/thinking 形成 dense group，显示 synthetic header 和连续左侧 rail；双击 entry/group header 改变本地 view projection。canonical DSH entries、stable ID、copy payload 和 host 真源不改变。

本批次选择一个可回滚的 UI vertical slice，不声称完成完整 Grok renderer parity：先把现有 `ScrollbackPane`/runtime 的 projection、paint 和 mouse 路径接通，保留当前 DSH `Scrollback` 作为高度/anchor owner。

## 设计契约和复用依据

- 对应计划：`docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md` 的“截图对应的视觉和交互契约”、D.1-D.8、Phase 3、P19-B；长期计划 M4/M8。
- Grok source path：`/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/blocks/{user,agent,thinking}.rs`、`scrollback/state/{groups,verb_group}.rs`、`scrollback/wrappers/entry_renderer.rs`、`app/agent_view/selection.rs`。
- Grok mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`；`SOURCE_REV`：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`；许可证/NOTICE 已在前一份方案记录核对。
- 复用等级：B；在现有 DSH-neutral DTO 上保留 Grok 的 entry display mode、group scan、synthetic header 和 click-count 语义，不引入 Grok runtime。
- DSH seam：`ScrollbackPane` 持有 view-local expanded entry/group 集合；`Scrollback` 继续拥有 canonical entries、height index 和 `ScrollAnchor`；runtime 只把鼠标意图交给 pane，不读协议 JSON。

## 计划修改范围

- 文件：
  - `crates/dsh-pager-grok-ui/src/views/transcript.rs`
  - `crates/dsh-pager-grok-ui/src/runtime.rs`
  - `docs/开发进度记录/2026-08-23_21-10-00_grok_scrollback_fold_group_vertical_slice.md`
- 预期行为：
  - UserPrompt 长文本默认最多三行并使用 `theme.bg_light` prompt band；AgentMessage 无 prompt band/vpad。
  - Thinking/Tool/ToolResult 默认摘要折叠，双击后展开；短 user prompt 不被误折叠。
  - 连续 tool/thinking/context entry 形成 VerbRun/ContextGroup projection；折叠成员零高度，header 绑定首 entry ID，group 内无 gap，rail 连续。
  - runtime 记录同一 entry 的双击，单击仍用于选择/拖选；resize/更新后保留或清理本地状态和 anchor。
- 不在范围内：不修改 `crates/dsh-pager` host DTO/协议/SessionState，不复制 vendor 文件，不实现完整 Grok markdown AST、viewer、selection state machine 或真实 P19/P20 Harness 门禁。

## 风险、回滚和依赖

- 风险：当前 `Scrollback` 高度 index 只知道 canonical entry，synthetic group header 必须映射到首 entry 并复用其高度；任何 projection/height 不一致都可能导致 scroll anchor 漂移。通过 pane 内单一 projection map、`set_rendered_height` 和 anchor restore 降低风险。
- 风险：ratatui Paragraph 外层默认填充 `bg_base`；UserPrompt band 需要在 line spans 中填充背景和 trailing spaces，避免只给文字着色。
- 风险：crossterm MouseEvent 不携带 click count；runtime 需要按 entry ID 和短时间窗口判定双击，拖动不能触发展开。
- 回滚：回退本记录列出的两个 Rust 文件和本记录；host/协议和此前代码批次不受影响。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo test -p dsh-pager-grok-ui --lib --locked`
- `cargo test -p dsh-pager --lib --locked`
- `cargo build -p dsh-pager-bin --locked`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`
- `git diff --check`

## 实际修改

- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
  - 增加 view-local `DisplayMode`、`ProjectionInfo` 和连续 group 扫描。
  - UserPrompt 长文本默认三行预览，使用 `theme.bg_light` 填充整行；AgentMessage 保持无 prompt band 的 markdown/body 路径。
  - Thinking/Tool/ToolResult/Error 默认摘要或截断；连续 verb/context entry 生成 synthetic group header、连续 `│` rail，并让折叠成员进入零高度布局。
  - pane 通过稳定 entry ID 保存展开状态，重建宽度投影时不修改 canonical history。
  - `RichPaintLine.copy_text` 与视觉 rail/header 分离，保留复制和命中测试的纯文本坐标。
- `crates/dsh-pager-grok-ui/src/runtime.rs`
  - transcript 左键单击仍进入选择；同一 entry 在 450ms 内第二次点击切换 group/fold。
  - resize、滚动重建和拖选清理 click tracker；展开后通过宽度重建和 anchor 恢复避免跳尾。
- `docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md`
  - 将当前状态同步为“Phase 3 第一条 transcript vertical slice 已进入默认路径”，并明确 special-tool viewer、完整 ContextGroup/三击、geometry golden 与 P19-A/P19-B/P20 仍未完成。
- 保留限制：special-tool viewer、完整 Grok selection state machine、真实 Harness P19/P20 与完整 markdown AST parity 不在本批次范围内。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test --workspace --locked`：通过；workspace 单元测试和 doctest 全部通过，`dsh-pager-grok-ui` 为 236 tests passed。
- `cargo build -p dsh-pager-bin --locked`：通过。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`：通过。
- `cargo clippy -p dsh-pager-grok-ui -p dsh-pager --lib --locked -- -D warnings`：通过。
- `git diff --check`：通过。
- 新增单测覆盖：User/Agent 背景契约、长 User 三行预览与展开、dense tool group 零高度成员/连续 rail/从 header 展开。

## Git 提交

- commit message：未提交（用户未要求提交）。
- Progress-Record trailer：待提交/未提交。
- 暂存区审计：当前工作树包含此前批次修改，本批次不暂存、不提交。

## 未解决问题和下一步

完整 Grok EntryRenderer/ScrollbackPane、真实 ContextGroup 展开策略、special-tool viewer、selection/copy parity、geometry golden 和 P19-B/P20 仍需后续批次。
