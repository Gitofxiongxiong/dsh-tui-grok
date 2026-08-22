# M6 picker 非 session row attach guard

> 记录时间：2026-08-23 07:03:40 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

复核 M6 attach vertical slice 后，picker 仍可携带 queue/diagnostic 辅助行；这些
stable ID 不是 session ID，不得进入 `load_session_id`。

## 设计契约和复用依据

- 对应长期计划章节：M6.3-M6.5。
- 复用等级：B/C；保持 Grok picker 状态机，guard 位于 DSH adapter/runtime seam。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/runtime.rs`
- 本记录文件

Selected 的非 session row 显示明确诊断并留在 picker，不产生 attach effect 或
RPC。回滚为恢复 runtime 单处 guard。验证 fmt/check/clippy/test/diff check。

## 实际修改

runtime 在编译 Attach intent 前拒绝包含辅助 row identity 后缀的 queue/diagnostic
行；picker 保持打开且显示 `Selected row is not attachable`，不会触发 RPC。

## 验证结果

验证通过：`cargo fmt --all`、`cargo check --workspace`、`cargo clippy
--workspace --all-targets -- -D warnings`、`cargo test --workspace --locked`、
`python3 scripts/pty-smoke.py --binary target/debug/dsh-pager`、`git diff --check`。

## Git 提交

提交包含本记录和 runtime guard，使用本记录路径作为 `Progress-Record` trailer。

## 未解决问题和下一步

无；后续仍按 M6.6-M6.8 补完整 dashboard/workspace parity。
