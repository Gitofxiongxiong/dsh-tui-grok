# Roadmap milestone status audit

> 记录时间：2026-08-23 11:30:59 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

根据当前真实代码和已执行测试，修正
`docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md` 中容易把“迁移基线/
vertical slice”误读成“完整 Grok UI parity”的里程碑标注。明确区分已完成、
部分完成和未开始，尤其标出当前仍在生产入口中的 fallback UI shell。

## 设计契约和复用依据

- 对应长期计划章节：0.2、5.1、6.1、M3、M4、M10、M11。
- Grok source snapshot：mirror `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，
  `SOURCE_REV=7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 标注规则：`已完成` 仅用于满足该里程碑完整出口；`部分完成` 用于
  baseline/vertical slice/基础设施证据；`未开始` 用于尚未进入实现的出口。

## 计划修改范围

- `docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md`：增加状态标记说明，修正
  M0-M11 当前状态表，补充“当前可运行 UI 仍为过渡 shell”的总览。
- 本记录。
- 不修改 Rust、测试、协议、vendor 或真实 Harness checkout。

## 风险、回滚和依赖

- 这是文档状态校正，不改变运行行为；回滚为恢复本记录涉及的计划文件版本。
- 不把既有测试通过结果删除，只调整其语义范围，避免将 semantic/reference
  scaffold 宣称为原始 Grok 视觉 golden。

## 预期验证

- `git diff --check`。
- 检查路线图中的每个里程碑都有明确状态和剩余出口。
- `git status --short` 确认仅有本记录和计划文件变更。

## 实际修改

- `docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md`：新增 `✅/◐/❌` 状态定义；
  将 M0-M1 标为完整完成、M2-M10 标为部分完成、M11 标为未开始；明确当前
  默认入口仍是 `runtime.rs` fallback shell，并修正 M3/M10 的出口与证据措辞。
- 未修改 Rust、测试、协议、vendor 或 `/home/leo/code/deepseek-harness`。

## 验证结果

- `git diff --check`：通过。
- 只读审计确认：当前唯一完整完成的是 M0-M1；M2-M10 保留已有切片证据但
  未达到完整 Grok parity 出口；M11 尚未开始。

## Git 提交

- commit message：待提交。
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_11-30-59-roadmap-status-audit.md`。

## 未解决问题和下一步

- 完整 Grok `AppView/AgentView`、scrollback/block renderer 和 fallback removal
  仍需后续代码迁移批次；本批次只负责把路线图标注准确。
