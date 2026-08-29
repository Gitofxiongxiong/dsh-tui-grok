# 修复 Windows 测试沙箱 SYSTEMROOT

> 记录时间：2026-08-29 18:04:31 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

GitHub Actions 的 `windows-2022` job 在 `dsh-pager-bin/tests/hello.rs` 中所有通过
`TestSandbox::command()` 启动 Node backend 的测试上稳定失败。Node 尚未执行 mock
脚本就触发 `Assertion failed: ncrypto::CSPRNG(nullptr, 0)`；直接继承环境启动 Node
的同文件测试通过。最新 `main` run `33246404546` 与多问题 PR run `33246620823`
均复现，重跑仍失败。

根因是 `TestSandbox::command()` 使用 `Command::env_clear()` 后只恢复 PATH、HOME 和
临时目录变量，没有恢复 Windows 系统运行时需要的 `SYSTEMROOT`。Rust 官方 issue
rust-lang/rust#114737 记录了相同边界：Windows 系统函数可能通过包含
`%SYSTEMROOT%` 的注册表路径延迟加载 DLL；Go/libuv 也会在清空环境时保留该变量。

## 设计契约与范围

- 测试 sandbox 仍不继承凭据、DSH server、代理或用户配置；只在 Windows 保存
  进程启动所需的 `SYSTEMROOT`。
- 不改生产 transport/backend、Node 版本、CI matrix 或产品环境契约。
- 计划文件：
  - `crates/dsh-pager-test-support/src/sandbox.rs`
  - 本进度记录
- 新增 Windows-only 契约测试，验证 sandbox 环境包含与父进程一致的
  `SYSTEMROOT`，并能在清理后的环境中正常启动 Node。

## 风险、回滚与预期验证

- 风险：多继承一个系统路径变量会降低“空环境”的字面纯度，但不会泄露凭据；
  Windows 缺失该变量本身会破坏系统 DLL/加密服务，因此这是平台运行时基线。
- 回滚：revert 本记录对应 commit；测试会恢复当前 Windows CI 失败。
- 预期验证：
  - `cargo fmt --all -- --check`
  - `cargo test -p dsh-pager-test-support --locked`
  - `cargo test -p dsh-pager-bin --test hello --locked`
  - `cargo test --workspace --locked`
  - PR 的 ubuntu-24.04、macos-14、windows-2022 和 verify-ts jobs 全部通过

## 实际修改

- `TestSandbox::new()` 在 Windows 上读取父进程的 `SYSTEMROOT`，并将它加入
  `env_clear()` 后显式恢复的最小环境；若变量缺失则返回清晰的 `NotFound` 错误。
- 新增 Windows-only 契约测试，分别验证 `SYSTEMROOT` 值被原样保留，以及清理环境
  后 Node 可以加载 `node:crypto` 并生成随机字节。
- 未调整 CI matrix、Node 版本或生产代码。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test -p dsh-pager-test-support --locked`：通过，9 tests。
- `cargo test -p dsh-pager-bin --test hello --locked`：通过，14 tests。
- `cargo test --workspace --locked`：通过。
- `pnpm install --frozen-lockfile && pnpm run verify:ts`：通过。
- Windows-only 回归测试需由 PR 的 `windows-2022` job 提供平台实测证据；该结果记录
  在 PR checks 中，避免为外部 CI 结果追加无功能差异的提交。

## Git 提交

- commit message：`fix(test): preserve SYSTEMROOT in Windows sandbox`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-29_18-04-31_修复Windows测试沙箱SystemRoot.md`
- 暂存区审计：仅包含 `crates/dsh-pager-test-support/src/sandbox.rs` 与本记录；
  `git diff --check` 通过。

## 未解决问题和下一步

- 创建独立 PR，等待 ubuntu、macOS、Windows 与 TypeScript checks 全部通过后以
  rebase 方式合并；随后将多问题弹窗功能 PR 变基到修复后的 `main`。
