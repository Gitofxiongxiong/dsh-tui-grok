# N1 effect ledger 与 File Search contract 收敛

> 记录时间：2026-08-23 17:09:56 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

作为 `GROK_RENDERER_PIXEL_PARITY_PLAN.md` N1 的前置批次，先收敛两个
会让后续 controller 迁移产生伪完成的 contract 问题：

1. `DshEffectSink` 的 operation ledger 目前每次用户操作都重建，
   request sequence 和 completed identity 无法跨操作持久；
2. Harness `fileReferences.list` 只返回 `path/kind`，runtime 却把
   `kind` 写入 `snippet` 并伪造 `line = 0`，使路径候选被误报为
   line preview 能力。

本批不迁移新的视觉 renderer，不把现有 file-search overlay 标记为
Grok parity 完成。

## 设计契约和复用依据

- 对应长期计划章节：2.3 数据契约、2.4 Intent/Effect/Receipt、
  P2A File Search、P6.1 第 5 门禁、N1。
- Grok source path：
  `crates/codegen/xai-grok-pager/src/views/file_search/{mod.rs,state.rs,dropdown.rs,line_viewer.rs}`；
  固定 mirror commit `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，
  SOURCE_REV `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：本批 effect/file wire 为 C；File Search renderer/controller 仍为
  后续 B 级迁移，本批不自建替代 controller。
- DSH-neutral seam：`FileSearchSnapshot` 显式区分 path candidate 和
  optional line preview；`UiIntent -> UiEffect -> EffectLedger -> receipt`。
- 稳定 identity：operation 继续携带 session/request/generation/action/
  dedupe key；file query 继续携带 revision。
- 将替换的旧行为：runtime 构造 `line = 0`/`snippet = kind` 的伪预览。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/effects.rs`：抽出可持久的
  `EffectLedger`，使 request sequence 和 completed operation identity 不随 sink 重建。
- `crates/dsh-pager-grok-ui/src/host_adapter.rs`：将 File Search row 收敛为
  path/kind + optional typed preview，snapshot 显式携带 preview status。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：所有 effect 调用共享同一
  ledger；file reference response 不再伪造 line/snippet；过渡 renderer 显示
  path candidate 或真实 optional preview。
- 本记录文件：收尾时回写实际结果。
- 预期行为：重试 identity 可跨 sink 实例审计；path-only API 只表达
  candidate capability，line preview 缺失时显式 unsupported。
- 不在范围内：非阻塞 transport worker、Grok File Search/Suggestion/Image
  controller vendor、Scrollback/Dashboard/Agent renderer、全屏 reference runner。

## 风险、回滚和依赖

- 风险：调整 serde DTO 可能破坏现有 fixture；通过 host adapter 和 runtime
  unit tests 同步锁定。
- 风险：持久 ledger 若把普通同文本 prompt 当成永久重复，会误阻
  用户后续的有意提交；本批仅按明确 operation/request identity 去重，
  不对 prompt 内容做永久全局去重。
- 回滚：回退本记录对应的单一 commit，恢复 sink-local ledger 和旧
  FileSearchRow；不涉及协议 wire 变更。
- 依赖：现有 `fileReferences.list` path-only contract 和 N1 后续 Grok
  controller 批次。

## 实际修改

- `effects.rs` 新增由 `UiState` 持有的有界 `EffectLedger`，request identity
  与 completed-operation 去重不再随短生命周期 `DshEffectSink` 丢失；显式
  `UiContext::for_operation` 仍是重试复用 identity 的唯一入口。
- `host_adapter.rs` 将 File Search 行改为 `path/kind + optional preview`，
  解析 `line/snippet` 时保留真实可选字段，并为 snapshot 增加独立的
  `preview_status`。
- `runtime.rs` 的所有 effect sink 构造共享 `UiState.effect_ledger`；
  `fileReferences.list` 的 path-only 结果不再写入伪造的 `line = 0` 或
  `snippet = kind`，过渡 surface 按真实 preview 或 path candidate 渲染。
- 增加跨 sink identity、完成操作去重、真实 preview 与 path-only rows 的
  单元测试。

## 验证结果

- `cargo fmt --all`
- `cargo test -p dsh-pager-grok-ui --lib`
- 结果：219 tests passed。
- `git diff --check`
- 结果：通过；仅修改本记录声明的三个源文件和本记录文件。

## Git 提交

- commit message：`refactor: persist effect identity and file reference contract`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_17-09-56_N1-effect-ledger-file-contract.md`
- 暂存区审计：提交前确认仅包含 `effects.rs`、`host_adapter.rs`、
  `runtime.rs` 与本记录文件；提交后核对 trailer 和 `git show --check`。

## 未解决问题和下一步

- 非阻塞 effect executor 和 attach/load session swap 仍需独立记录。
- 下一批 vendor/结构性迁移 Grok File Search controller 和相邻测试。
