# PromptWidget renderer contract tranche

> 记录时间：2026-08-23 15:14:16 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

按 `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md` 的 P6 顺序推进 TUI：先把 Grok
`PromptWidget` 与 `AgentViewLayout` 的可复用纯 renderer 契约从当前手写 shell 中
分离出来，同时保留 Markdown、Diff、File Search、Suggestion/history、Image、
Workspace、Agent/Task/Subagent 的既有 DSH host seams。当前不把 Grok agent loop、
shell、ACP、RPC、配置、持久化或 telemetry runtime 引入生产依赖。

本批先完成 PromptWidget 的 source/dependency 盘点、manifest/source-map 入口和
可编译的 DSH-neutral render contract；完整上游 PromptWidget 的交互闭包与
Scrollback/Markdown/Diff/Image 接线留在后续批次。

## 设计契约和复用依据

- 对应长期计划章节：`GROK_RENDERER_PIXEL_PARITY_PLAN.md` P0、P2、P6；
  `GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md` 2.1、2.2、2.3、2.4、2.5。
- Grok source path：`/home/leo/code/grok-build/crates/codegen/xai-grok-pager/src/views/prompt_widget/`
  与 `app/agent_view/{prompt.rs,render.rs,interactions.rs}`。
- Grok mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`。
- SOURCE_REV：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：PromptWidget 纯绘制/布局为 A1/B；DSH DTO 和 Intent/Effect 为 C；
  Grok agent/runtime 依赖为 D，不进入本批。
- DSH-neutral seam：`PromptRenderContract` 只接受稳定文本、焦点、尺寸、主题
  role、候选/媒体状态和 capability；不持有 `SessionState`、RPC 或 Grok session。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/views/prompt_contract.rs`：新增纯 PromptWidget
  render contract、几何预算和可测试的 chrome/content rect solver。
- `crates/dsh-pager-grok-ui/src/views/mod.rs`：导出 contract 模块。
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`：登记 PromptWidget source map、
  纯闭包边界、旧实现替换条件和本批 hash。
- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`：把 P6 第一步拆成可验收的 contract
  门禁，明确完整能力闭包仍未完成。
- 本进度记录：完成后回写实际结果、验证和下一步。

不在范围内：复制 Grok runtime 文件、修改 DSH protocol/Harness、删除
`runtime.rs` fallback、直接替换生产绘制路径、改变现有 capability effects。

## 风险、回滚和依赖

- 风险：上游 PromptWidget 依赖面大，过早整包 vendor 会引入禁止的 runtime；
  因此本批只落纯 renderer contract，避免产生第二套业务真源。
- 风险：contract 与上游字段漂移；通过 source map 和上游 hash 固定来源，后续
  接线必须以同一 revision 的 upstream tests 和 semantic buffer 对齐。
- 回滚：删除本批新增模块、manifest rows、计划增量和进度记录，不触碰已有 host
  seam 或用户未提交修改。
- 依赖：`ratatui::layout::Rect`、现有 `Theme`/`TerminalCapabilities` 和
  `PromptEditor` 的 DTO 投影。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo test -p dsh-pager-grok-ui --locked`
- `python3 scripts/check-source-manifest.py`
- `python3 scripts/parity-matrix.py`
- `git diff --check`

## 实际修改

- `src/views/prompt_contract.rs` 新增与固定上游逐字段对应的
  `PromptStyleContract`、`PromptInfoContract`、`PromptFlagContract` 和
  `PromptSurface`；
- 新增纯 `PromptGeometry::compute`，复现 upstream chrome/content/top/text/
  textarea/info/dim rect split，不绘制 cell；
- 新增 `desired_prompt_height`，保留 TextArea wrapped rows、max-height clamp 和
  history browse 冻结规则；
- `SOURCE_MANIFEST.md` 登记 PromptWidget tests、AgentView layout/render source hash，
  并将依赖分类为 A0/A1/B/C/D；
- parity 方案把 P6.1 拆成 contract、TextArea、draw core、完整 pane solver 和
  controller 五个硬门禁，当前只标记第一个门禁完成。

未修改 production draw；`views/agent.rs::render_prompt_buffer` 仍是明确的 fallback，
本批不声称 PromptWidget 像素 parity 已完成。

## 验证结果

- `cargo fmt --all -- --check`：首次发现本批标准排版差异；运行
  `cargo fmt --all` 后复查通过；
- `cargo test -p dsh-pager-grok-ui --locked`：通过，206 tests、1 doctest；
- `python3 scripts/check-source-manifest.py`：通过，8 copied rows，local/upstream
  drift 0，missing license 0；
- `python3 scripts/parity-matrix.py`：通过，972 cases / 8 fixtures；
- `git diff --check`：通过。

## Git 提交

- commit message：`refactor: define upstream prompt render contract`
- Progress-Record trailer：
  `docs/开发进度记录/2026-08-23_15-14-16_prompt-widget-render-contract.md`
- 暂存区审计：仅包含本记录列出的五个文件；不包含 DeepSeek Harness 工作树。

## 未解决问题和下一步

下一批将把 `PromptEditor` 收敛到 workspace 已有的固定 Grok `TextArea`，抽取
upstream `PromptWidget::draw` 的 chrome/TextArea/info/cursor core，替换当前
`AgentView::render_prompt_buffer`，并迁移相应 semantic cell tests。File Search、
Suggestion/history、Image controller 随后接入同一 widget state，不能留在 runtime
平行 banner/controller 中。
