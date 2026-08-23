# 外置 DSH TUI 后端与 Harness 原仓库恢复

## 目标

把 `/home/leo/code/deepseek-harness` 中为本项目新增的 TUI protocol、control-plane、stdio server、embedded bundle 与 file-reference wire seam 迁入本仓库，形成可由 DSH profile 安装的外部插件套件；迁移完成并验证后，将 Harness 工作树恢复到 `origin/master`，不在其原仓库保留本地修改。

## 约束与范围

本记录对应一个迁移提交，允许修改的目标文件/目录如下：

- `packages/dsh-tui-protocol/**`：TUI JSON-RPC、通知和 control-plane wire library。
- `packages/dsh-tui-control-plane/**`：连接级 control-plane 状态、去重、resume 与 backpressure library。
- `packages/dsh-tui-server/**`：通过公开 `ctx.apiProxy`/Cordis seams 提供 stdio TUI server 的插件。
- `packages/dsh-tui-embedded/**`：外部 DSH bundle/profile patch，不修改 DSH CLI 或内置 bundle。
- `packages/dsh-tui-file-reference/**`：TUI 的可选 `fileReferences.list` seam 适配；不得修改 Harness `host/apiproxy` 源码。
- 根级 TypeScript workspace/build metadata、迁移说明和第三方声明（仅在确有必要时）。
- `docs/开发进度记录/` 中与本迁移相关的后续记录。

不会覆盖或重写本仓库已有 Rust/UI 工作树改动；不会把完整 `deepseek-harness` 源码、`.git` 目录或其 lockfile 提交到本仓库。

## 来源与许可证

- 来源仓库：`https://github.com/deepseek-ai/deepseek-harness.git`。
- 官方基线：`origin/master`（迁移审计时为 `141eb6fef8`，`0.1.0-rc.8`）。
- 本地来源提交：`92568c3afa`、`334ace99d5`、`28da1dce04`、`387f950df9` 以及工作树中的 control-plane/file-reference 修改；最终以文件级迁移为准，不保留 Harness Git 历史。
- DSH 原有依赖继续以其公开 npm 包作为 peer/dependency；本仓库只声明自己的外部包，不重新发布 DSH 源码。
- 新增代码保留原始 MIT 许可边界；Rust/Grok 既有 Apache/MIT 声明不变。

## 验证门槛

- TypeScript 包可以在本仓库 workspace 中独立 typecheck/build/test（依赖公开的 DSH 包，不依赖 Harness 工作树路径）。
- bundle patch 只插入外部包和官方公开插件，不修改 DSH profile/base/CLI。
- Rust protocol/client 与 server method 名称、错误和 file-reference DTO 保持一致。
- 迁移完成后核对 Harness：`HEAD == origin/master`，工作树干净；迁移前状态保存在仓外恢复备份中。

## 记录

本记录必须随迁移提交一起提交，提交信息包含：

`Progress-Record: docs/开发进度记录/2026-08-24_00-12-29_externalize_dsh_tui_backend.md`

## 实际结果

- 新增 `packages/dsh-tui-protocol`、`packages/dsh-tui-server`、`packages/dsh-tui-embedded` 三个外置包；control-plane 保持 server 内部 library，避免创建不必要的运行时插件。
- server 不再依赖 Harness 修改过的 `ApiProxy.fileReferences` 字段；`fileReferences.list` 通过公开 `ctx.fileReferences` provider 和 `@deepseek-ai/dsh-api-remotes` resolver 适配。
- bundle 插入官方 `dsh-file-reference-local`，并保留 `api-gateway`、workspace/storage、browse picker、Code Mode 与 stdio server；未改 DSH CLI 或 base 源码。
- 根目录新增 pnpm workspace、TypeScript build/typecheck/test 脚本、MIT/第三方来源声明和外置安装文档；生成的 `lib` 与 `node_modules` 不纳入 Git。
- `pnpm run build:ts`：通过；`pnpm run typecheck:ts`：通过；`pnpm run test:ts`：通过（protocol 16、server 37、bundle 2 tests）。
- `python3 scripts/check-protocol-fixtures.py`：通过（2 fixtures）。
- Harness 恢复结果：`HEAD == origin/master == 141eb6fef83422698aef7a981029e843e8161534`，`git status --short` 为空。迁移前状态备份在仓外 `/home/leo/code/.migration-backups/deepseek-harness-2026-08-24/`，含 bundle、binary diff、untracked tarball 和 status manifest。
