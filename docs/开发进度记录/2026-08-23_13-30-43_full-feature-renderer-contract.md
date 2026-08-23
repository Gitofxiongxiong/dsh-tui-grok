# Full-feature renderer contract

> 记录时间：2026-08-23 13:30:43 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

修订 Grok renderer parity 方案，明确 Markdown、Diff、File Search、Suggestion、
Image、Workspace、Agent/Task/Subagent 都是新项目必须保留的用户可见能力；同时
为 renderer 迁移建立显式的 DSH-neutral feature snapshot 分区，避免迁移过程
只完成主屏视觉而遗漏产品功能。

## 设计契约和复用依据

- 对应长期计划章节：2.1、2.3、2.4、3.2、M1-M7、M10-M11。
- Grok source path：`/home/leo/code/grok-build/crates/codegen/`。
- Grok mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`。
- Grok SOURCE_REV：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：A0/A1/B；排除等级 D 仅适用于 Grok agent loop、shell/tool
  orchestration、ACP、RPC、配置、持久化和 telemetry runtime，不适用于其
  对应的 view、renderer、状态机和测试。
- DSH 仍是数据、身份、generation、能力协商和副作用的唯一真源。

## 计划修改范围

- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`：补充完整能力闭包、能力矩阵、
  host seam 和分阶段出口。
- `crates/dsh-pager-grok-ui/src/host_adapter.rs`：增加 feature snapshot 分区，
  让 file search、suggestion、image、workspace、agent/task/subagent 在 DTO
  层有明确归属；未有 host authority 的数据显式为空或 unsupported，不伪造成功。
- 本记录文件。
- 不在本批修改 Grok vendor 文件、DSH protocol、loader 或 runtime 绘制路径；
  下一批迁移各 renderer 闭包时另建进度记录。

## 风险、回滚和依赖

- 风险：新增 DTO 字段会扩大 snapshot 合同；通过默认值和确定性构造保持兼容。
- 风险：file search 和真实 inline image 需要 Harness/terminal capability；本批
  只建立 typed seam，不把 session.search 误标为 filesystem search。
- 回滚：恢复本记录列出的方案和 host adapter 文件即可，不触碰 DSH 真源。
- 依赖：现有 `DshRenderContent`、`ControlPlaneStore`、`CapabilityMatrix`、
  `GrokHostSnapshot` 和已有 protocol/PTY/parity tests。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo test -p dsh-pager-grok-ui --locked`
- `cargo test --workspace --locked`
- `python3 scripts/check-source-manifest.py`
- `scripts/e2e.sh`
- `git diff --check`

## 实际修改

- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`
  - 将 Markdown、Diff、File Search、Suggestion、Image、Workspace、
    Agent/Task/Subagent 明确列为必须迁移的用户可见能力。
  - 澄清只排除 Grok 业务执行 runtime，不排除对应的 UI、renderer、交互状态机
    和测试。
  - 增加 P2A 能力闭包表、feature `available/pending/unsupported` 三态、
    host contract 规则和完整 snapshot 分区。
- `crates/dsh-pager-grok-ui/src/host_adapter.rs`
  - 扩展 capability matrix 的 `file_search`、`subagents` 字段。
  - 增加 `FeatureStatus`、`FileSearchSnapshot`、`SuggestionSnapshot`、
    `MediaSnapshot`、`WorkspaceSnapshot`、`AgentSnapshot` 及稳定 ID 行 DTO。
  - 在 session/control-plane projection 中生成这些分区；file search 在只有
    capability、尚无 query/result snapshot 时保持 `Pending`，不伪造结果。
  - 从结构化 transcript 提取 image metadata，从 control plane 投影 workspace
    rows/order 和 task rows。
  - 增加 capability/status contract assertions。
- `docs/开发进度记录/2026-08-23_13-30-43_full-feature-renderer-contract.md`
  - 本批审计记录。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo check -p dsh-pager-grok-ui`：通过。
- `cargo test -p dsh-pager-grok-ui --locked`：通过，185 tests + doctests。
- `cargo test --workspace --locked`：通过。
- `python3 scripts/check-source-manifest.py`：通过，8 rows，local/upstream drift 0。
- `scripts/e2e.sh`：通过，含 clippy、648 semantic cases、workspace tests、mock
  PTY 和 terminal restore；输出 `DSH/Grok M8-M10 end-to-end checks passed`。
- `git diff --check`：通过。

## Git 提交

- commit message：`feat: define full-feature renderer host contract`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_13-30-43_full-feature-renderer-contract.md`
- 暂存区审计：待提交前执行。

## 未解决问题和下一步

- Grok Markdown/diff/block renderer、PromptWidget、file search overlay、suggestion
  controller、image preview、workspace pane 和完整 AgentView 仍需按 renderer
  闭包分别迁移。
- 需要确认 DeepSeek Harness 的 filesystem search、image attachment/preview、
  workspace action 和 subagent event contract 后，才能把对应 snapshot 从
  `unsupported/pending` 推进到真实数据路径。
