# 更新 renderer parity 当前进度与下一步计划

> 记录时间：2026-08-23 16:47:57 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

用户要求在 `GROK_RENDERER_PIXEL_PARITY_PLAN.md` 中明确当前实现进度和后续推进
顺序。上一批 `94fe1ef` 已完成 P6.1 第 4 门禁，但长期计划仍有旧的“DSH 手写
row stack / runtime 手工串联”表述，容易误判实际状态。本批只校正文档事实，补充
完整 TUI 的剩余门禁和验收出口，不修改代码和协议。

## 设计契约和复用依据

- 对应长期计划章节：第 2 节当前基线、第 2.1 节执行状态、第 3.3 节 P6.1 第 4
  门禁、第 4 节分阶段实施、第 5 节像素 parity。
- Grok source/reuse 依据：固定 mirror `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，
  `SOURCE_REV=7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- DSH 仍是数据、身份、能力和副作用真源；Grok-derived UI 负责视觉、布局、焦点、
  selection、mouse、shortcut 和 fallback。
- 当前完成事实以提交 `94fe1ef refactor: use production agent pane solver` 及其
  `Progress-Record` 为准。

## 计划修改范围

- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`
  - 修正已完成的 Prompt/Appearance/AgentView/layout snapshot 状态。
  - 新增当前进度、未完成边界、下一步门禁顺序和完成定义。
- `docs/开发进度记录/2026-08-23_16-47-57_update-parity-progress-plan.md`
  - 本批审计记录。

不在范围内：任何 Rust 代码、vendor、fixture、协议、Cargo 文件和 runtime 行为。

## 风险、回滚和依赖

- 风险：文档状态与代码提交再次漂移；通过引用具体提交、门禁编号和验证命令保持
  可核对性。
- 风险：把 host contract 已存在误写成 renderer 已完成；每项能力继续分别列出
  Host/Effect、Renderer 和下一出口。
- 回滚：回退本批文档提交即可，不影响代码和协议。

## 实际修改

- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`
  - 修正 Prompt、Appearance、AgentView layout/chrome 和 orchestration 的实际状态，
    明确完整 pane solver 已进入 runtime/parity，但专用 pane renderer 和完整上游绘制
    顺序仍未闭合。
  - 更新 2.1 执行状态表，分别列出 Prompt core、AgentView layout/chrome、Scrollback、
    Markdown/Diff、File Search、Suggestion/history、Image、Workspace、Agent/Task/
    Subagent 的 Host/Effect 状态、Renderer 状态和下一出口。
  - 新增 3.4 当前进度总览，记录 Prompt core、AgentView geometry、Scrollback、
    capability surfaces 和验证基础设施的真实完成边界。
  - 新增 3.5 下一步计划和验收顺序：N1 先完成 File Search/Suggestion/history/Image
    controller，N2 迁移 Scrollback 及 Markdown/Diff/Image/Reasoning/Tool blocks，
    N3 迁移 Workspace/Dashboard 与 Agent/Task/Subagent pane，N4 再收敛生产路径并
    运行完整 parity/backend 门禁。
  - 更新 P6 当前执行顺序，明确 Prompt draw core 和 AgentView pane solver 已完成，
    当前下一步是 P6.1 第 5 门禁；同时保留“完整 TUI/pixel parity 尚未完成”的声明。
- 本批没有 Rust、vendor、协议、Cargo 或 runtime 行为改动。

## 验证结果

- `git diff --check`：通过。
- 文档范围审计：仅长期计划和本进度记录发生修改；新增章节、P6.1 第 4/5 门禁
  状态及 Markdown、Diff、File Search、Suggestion/history、Image、Workspace、
  Agent/Task/Subagent 能力边界均可由 `rg`/diff 核对。
- 未运行 Rust/PTY/parity 全量测试：本批仅修改文档，代码状态沿用提交 `94fe1ef`
  的验证结果。

## Git 提交

- 计划 commit message：`docs: record current renderer parity progress`。
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_16-47-57_update-parity-progress-plan.md`
- 暂存区审计：提交前只暂存本长期计划和本进度记录。

## 未解决问题和下一步

- 文档更新不会关闭任何 renderer 门禁；下一批代码工作仍从 P6.1 第 5 门禁开始。
