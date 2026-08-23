# AgentView 完整 pane solver 迁移

> 记录时间：2026-08-23 16:04:55 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

P6.1 的 PromptWidget production renderer 已收敛，但生产 AgentView 仍使用 DSH
简化 row stack，无法表达 tasks、catalog、todo、queue、btw、CTA、follow-ups、
voice、scrollbar/timeline 等上游 pane，也没有统一的 pane hit-test snapshot。本批迁移
固定 Grok revision 的完整纯布局求解器，使 runtime、parity、绘制和命中几何共用
同一份 `AgentViewLayout`。

完整 TUI 的 Markdown、Diff、File Search、Suggestion/history、Image、Workspace、
Agent/Task/Subagent 均继续保留在计划和能力边界中；本批只完成 P6.1 第 4 门禁，
不把 pane solver 完成误报为完整 TUI 或 pixel parity 完成。

## 设计契约和复用依据

- 对应长期计划章节：P3 AgentViewLayout、P6/P6.1 第 4 门禁。
- Grok source path：
  - `crates/codegen/xai-grok-pager/src/views/agent.rs`
  - `crates/codegen/xai-grok-pager-render/src/appearance/config.rs`
  - `crates/codegen/xai-grok-pager/src/app/agent_view/render.rs`
- mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`。
- SOURCE_REV：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：布局/config 为 A1；runtime DSH DTO 高度接线为 B/C；Grok agent loop、
  shell/tools、ACP、RPC/config/auth/persistence/telemetry 为 D，继续排除。
- DSH-neutral seam：`GrokRenderSnapshot`/`AgentViewSnapshot` 只提供 authoritative
  tasks、subagents、queue、interaction、turn status、suggestions 等状态；缺少 DTO 的
  todo、CTA、follow-ups、voice pane 显式传 0。
- 新增 DTO/Intent/Effect：无；本批不新增副作用。
- 稳定 ID 和 generation：沿用 host adapter snapshot 和现有 hit map identity。
- 将替换的旧模块：`views/agent.rs` 的简化 row stack，以及 runtime/parity 对其
  header/transcript/footer compatibility 几何的依赖。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/appearance.rs`
  - 增加 DSH-neutral `LayoutConfig`、`ScrollbarConfig` 和上游 validation/effective
    padding 规则。
- `crates/dsh-pager-grok-ui/src/views/agent.rs`
  - 迁移完整 `AgentViewLayoutParams`、`AgentViewLayout::compute`、
    `rows_available_for_prompt`、`inner_width`、`PaneAreas`/hit-test。
- `crates/dsh-pager-grok-ui/src/runtime.rs`
  - 从一个 production snapshot 构造全部 pane 高度，先按可用 row budget clamp
    prompt，再让绘制、cursor、mouse/hit map 共用最终 layout。
- `crates/dsh-pager-grok-ui/src/parity.rs`
  - semantic parity runner 使用同一完整布局求解器和字段名。
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`
  - 更新 AgentView/config 的集成状态和本地 seam 说明。
- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`
  - 记录第 4 门禁的真实完成范围及剩余第 5 门禁/scrollback renderer 工作。
- `docs/开发进度记录/2026-08-23_16-04-55_agent-view-pane-solver.md`
  - 回写实际文件、验证结果、阻塞和下一步。
- 预期行为：pane 顺序、gap 省略、短终端降级、status self-clamp、prompt budget、
  scrollbar/timeline geometry 与固定 Grok solver 一致；40x12、80x24、120x40 不
  重叠、不越界。
- 不在范围内：Markdown/Diff/Image block renderer、File Search/Suggestion/history
  controller、Workspace dashboard、Task/Subagent pane 专用 renderer，以及任何
  Grok/DSH agent runtime 或协议改动。

## 风险、回滚和依赖

- 风险：更多 pane 会压缩 scrollback；通过上游 `Min(5)` 和 prompt probe budget
  控制，不允许 prompt 抢占 scrollback floor。
- 风险：runtime 旧 hit map 与新 pane snapshot 漂移；所有主 pane 命中从最终 layout
  同帧注册，overlay 仍保持更高优先级。
- 风险：短终端可选 pane 过度承诺；沿用上游 CTA/follow-up suppression，并对无
  authoritative DTO 的 pane 传 0。
- 回滚：单一对应 commit 可整体 revert；不修改协议、host authority 或持久化数据。
- 依赖：固定 Grok mirror、现有 `GrokRenderSnapshot`、PromptWidget/TextArea renderer。

## 实际修改

- `crates/dsh-pager-grok-ui/src/appearance.rs`
  - 增加 Grok-derived `LayoutConfig`、`ScrollbarConfig`，保留默认值、compact
    effective padding、validation 和 scrollbar gutter 规则。
- `crates/dsh-pager-grok-ui/src/views/agent.rs`
  - 迁移完整 pane 参数与结果字段、pane 顺序、optional gap omission、short terminal
    CTA/follow-up suppression、scrollback `Min(5)`、status self-clamp、prompt probe
    budget、scrollbar/timeline carve-out 和 `PaneAreas` hit-test。
- `crates/dsh-pager-grok-ui/src/runtime.rs`
  - 用单帧 `AgentViewLayoutParams`/`AgentViewLayout` 驱动 prompt、scrollback content、
    timeline、主 pane hit map、cursor 和 inline tasks/catalog/queue projection；异步
    subagent 列表先合入同一 snapshot；保留 File Search/Suggestion/Image/Workspace/
    Agent overlay owner/effect reducer。
- `crates/dsh-pager-grok-ui/src/parity.rs`
  - semantic runner 使用完整 solver、pane rows 和同一命中几何。
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`
  - 更新 Appearance/AgentView solver 的来源和集成状态。
- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`
  - 将 P6.1 第 4 门禁标为完成，明确第 5 门禁和完整 TUI 剩余范围。
- `docs/开发进度记录/2026-08-23_16-04-55_agent-view-pane-solver.md`
  - 本批审计记录。

## 验证结果

- `cargo test -p dsh-pager-grok-ui --locked`
  - 216 tests + 1 doctest passed。
- `cargo test --workspace --locked`
  - 全工作区通过。
- `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features --locked -- -D warnings`
  - passed。
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
  - passed。
- `cargo fmt --all -- --check`
  - passed。
- `python3 scripts/check-source-manifest.py`
  - 8 rows，local/upstream drift 0，missing license 0。
- `python3 scripts/parity-matrix.py`
  - 972 cases，8 fixtures。
- `python3 scripts/check-protocol-fixtures.py`
  - 2 fixtures in sync。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`
  - exit 0；session/picker/resize/queue/mouse/terminal restore passed。
- `git diff --check`
  - passed。

## Git 提交

- commit message：`refactor: use production agent pane solver`。
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_16-04-55_agent-view-pane-solver.md`
- 暂存区审计：提交前仅包含本记录列出的目标文件和本记录；无第二份主记录、无
  target/生成物。

## 未解决问题和下一步

- P6.1 第 5 门禁仍需迁移 File Search、Suggestion/history、Image controller。
- Scrollback Markdown/Diff/Image production block renderer、Workspace dashboard 和
  Agent/Task/Subagent 专用 pane renderer 仍需后续独立记录推进。
