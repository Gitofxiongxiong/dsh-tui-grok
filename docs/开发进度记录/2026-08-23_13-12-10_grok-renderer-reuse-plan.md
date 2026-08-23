# Grok renderer 复用与像素级前端对齐方案

> 记录时间：2026-08-23 13:12:10 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

本批只新增一份可执行方案文档，说明后续如何直接复用 Grok Build 的视觉实现，
把 DSH 的 session/host 数据接入 Grok renderer，并以逐 cell parity 作为完成门禁。
不在本批修改生产代码。

## 设计契约和复用依据

- 对应长期计划章节：1.1、2.1、2.3、2.5、2.7、2.8、M10。
- Grok source path：`/home/leo/code/grok-build/crates/codegen/`。
- Grok mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`。
- Grok SOURCE_REV：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：A0/A1。优先复制原始 renderer 和低耦合 view；只在 DSH/Grok 接缝
  添加 adapter，不复制 Grok agent、shell、ACP、RPC、persistence 或 telemetry。
- 许可证：沿用 vendor LICENSE/NOTICE，并为新增文件更新
  `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`。

## 计划修改范围

- `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`：本批新增的实施方案。
- `docs/开发进度记录/2026-08-23_13-12-10_grok-renderer-reuse-plan.md`：本批记录。
- 本批不修改 `src/`、vendor、Cargo 配置或测试 fixture；后续实施必须创建新的
  进度记录并在其中扩大文件范围。

## 风险、回滚和依赖

- 风险：Grok renderer 的依赖闭包较大，直接复制会暴露现有 ratatui、Theme 和
  scrollback DTO 的差异；必须按阶段引入，不能通过引入完整 Grok runtime 绕过。
- 风险：上游模块复制后若发生本地改动，必须逐文件记录 source hash 和适配原因。
- 回滚：本批只删除新增方案文档和本记录即可，不触碰已有生产代码。
- 依赖：固定 Grok source snapshot、现有 `dsh-grok-textarea`、vendor manifest、
  DSH `GrokHostSnapshot`/`UiEffect` 边界和现有 parity/PTY 测试基础设施。

## 实际修改

- 新增 `docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`，明确以 Grok renderer 为视觉真源、
  DSH 为数据/副作用真源、adapter 为唯一接缝。
- 记录当前已经复用的 Grok 模块，以及尚未进入生产路径的 PromptWidget、完整
  Theme/Appearance、AgentViewLayout 和 ScrollbackPane。
- 给出 P0-P5 实施顺序：源码闭包/provenance、Theme、PromptWidget、布局、
  Scrollback、runtime 收敛与旧 renderer 删除。
- 定义 Buffer 逐 cell parity 的字段、尺寸/状态矩阵、差异分类和验收门禁。

## 验证结果

- `git diff --check`：通过。
- 文档中的上游路径、mirror commit、SOURCE_REV 与当前 `SOURCE_MANIFEST.md`
  基线一致。
- 本批只新增文档，没有修改生产代码、vendor、Cargo 配置或协议 fixture。

## Git 提交

待提交。commit message 必须包含：

```text
Progress-Record: docs/开发进度记录/2026-08-23_13-12-10_grok-renderer-reuse-plan.md
```

## 未解决问题和下一步

下一批应先完成 Grok PromptWidget、完整 Theme/Appearance 和 AgentViewLayout 的
源码闭包盘点，然后创建独立进度记录开始 vendor 与 adapter 接线。
