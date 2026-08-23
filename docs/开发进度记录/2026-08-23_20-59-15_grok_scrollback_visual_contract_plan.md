# Grok scrollback 视觉契约与折叠分组方案补充

> 记录时间：2026-08-23 20:59:15 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

用户根据真实 Grok Build TUI 截图指出两类可见缺口：user 与 agent 历史消息使用不同的 block/background；thinking 与 tool 调用默认折叠，并以左侧连续 rail 和 group header 表示相邻消息。此前实现完成了 DSH entry projection、stream 收敛和 Hidden/Collapsed 的单条默认投影，但还没有把 Grok scrollback 的组件层级、dense group、折叠手势和视觉验收写成足够明确的方案契约。本批次只修正文档，使后续实现不会把 projection 完成误判为 Grok scrollback parity 完成。

## 设计契约和复用依据

- 对应长期计划章节：`docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md` 的 2.1、3.1、4.2、M2、M4、M8、10.2、10.4、10.5。
- 目标计划：`docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md`。
- Grok source path：`/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/scrollback/blocks/{user,agent,thinking}.rs`、`scrollback/blocks/tool/*`、`scrollback/state/{groups,verb_group}.rs`、`scrollback/wrappers/entry_renderer.rs`、`app/agent_view/selection.rs`、`scrollback/scrollback_pane.rs`。
- Grok mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`；`SOURCE_REV`：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`；许可证：上游 Apache-2.0，已核对 `/home/leo/code/grok-build/LICENSE` 与 `third_party/NOTICE`。
- 复用等级：B（结构性迁移 Grok block、scrollback state、group renderer 和 mouse state machine；本批次不复制或修改 vendor 代码）。
- 关键事实：`UserPromptBlock` 有 `bg_light`/vpad/三行折叠；`AgentMessageBlock` 无背景/vpad 且不可折叠；Thinking/Tool 可折叠并可加入 `VerbRun`；Grok 的实际手势为单击选择、双击折叠/展开（部分 block 双击进入 viewer），不是任意单击直接展开。

## 计划修改范围

- 文件：
  - `docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md`
  - `docs/开发进度记录/2026-08-23_20-59-15_grok_scrollback_visual_contract_plan.md`
- 预期行为：补充截图可观察的 user/agent block 视觉契约、Thinking/Tool 三态折叠、VerbRun/Truncation dense group 与左侧 rail、group header、真实鼠标手势矩阵；明确 Phase 2（单条 projection）与 Phase 3（完整 scrollback parity）的边界、测试和出口。
- 不在范围内：不修改 Rust 实现、vendor、SOURCE_MANIFEST、协议、fixture、Cargo.lock，不运行真实 Harness，不声称本批次完成 Grok parity。

## 风险、回滚和依赖

- 风险：文档若只写“折叠/分组”而不约束背景、padding、gap、header 命中和 click count，后续实现仍可能出现截图中的同底色或无法展开问题；将通过表格化视觉/交互契约和逐项验收降低风险。
- 风险：用户口语中的“点击展开”与上游实际 double-click 语义不同；文档将同时记录用户可感知行为和 Grok source 的精确手势，避免测试歧义。
- 回滚：本批次仅新增/修改上述两份文档；若方案评审不接受，可回退该计划文件和本记录，不影响现有代码与协议。
- 依赖：后续代码实现需以固定 Grok source snapshot、上游 tests、semantic buffer/geometry、PTY mouse 和真实 backend 证据为门禁。

## 预期验证

- `git diff --check`
- `rg` 检查计划中存在视觉组件、dense group/rail、鼠标矩阵、Phase 2/3 边界和测试断言关键词。
- 本批次不运行 cargo/PTY，因为没有代码或 fixture 变更。

## 实际修改

- 文件：`docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md`
- 文件：`docs/开发进度记录/2026-08-23_20-59-15_grok_scrollback_visual_contract_plan.md`
- 摘要：
  - 增加 UserPrompt/AgentMessage/Thinking/Tool/Subagent/GroupHeader 的视觉组件表，明确 background、vpad、默认 fold、groupability 和左侧连续 rail。
  - 明确 entry 级三态折叠与 group 级 synthetic header 的分层；记录 canonical entry/stable ID/anchor 不被投影删除。
  - 增加单击选择、双击 fold/group toggle、特殊 tool viewer、三击行为的真实 Grok 鼠标矩阵，并说明“点击展开”在验收中对应双击 affordance。
  - 记录当前 successor 的 `ScrollbackPane`/runtime 缺口，补充 UserPrompt/AgentMessage wrapper、ContextGroup、VerbRun/Truncation、mouse/hit-map 的实现契约。
  - 将 Phase 2 收窄为单条 Hidden/Collapsed projection，将 ContextGroup 和完整 scrollback visual/mouse parity 移入 Phase 3；拆分 P19-A 默认可见性与 P19-B 交互展开。
  - 保留“真实 P19/P20 未跑、完整 renderer parity 未完成”的状态说明。

## 验证结果

- 命令：`git diff --check`
- 结果：通过，无 whitespace error。
- 命令：`rg -n "UserPrompt|AgentMessage|background|vpad|VerbRun|Truncation|accent rail|click-count|P19-A|P19-B|Phase 2|Phase 3|ContextGroup" docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md`
- 结果：通过；视觉组件、折叠/分组、rail、鼠标矩阵、P19-A/P19-B 和 Phase 2/3 边界均可检索。
- 代码/PTY：未运行；本批次仅修改文档，没有 Rust、fixture 或协议变化。此前实现批次的 targeted tests、workspace build 和 mock PTY 结果不在本批次重新计入。

## Git 提交

- commit message：未提交（用户未要求提交；当前 successor 的 Git 提交门禁另行执行）。
- Progress-Record trailer：未提交。
- 暂存区审计：待完成。

## 未解决问题和下一步

实现阶段需要在 `dsh-pager-grok-ui` 引入 Grok-derived block wrappers、view-local display/group state、统一 layout/hit map 和 mouse click-count reducer，并以 P19-B/P20 真实 DeepSeek Harness 场景及 Grok reference golden 收口。当前工作树还保留此前代码批次的未提交修改；本批次未提交、未暂存，未改变这些代码范围。
