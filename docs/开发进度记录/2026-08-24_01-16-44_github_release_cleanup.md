# dsh-tui-grok 独立仓库整理与 GitHub 发布

> 记录时间：2026-08-24 01:16:44 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

将 `/home/leo/code/dsh-pager-grok` 整理成可独立维护的 Git 仓库，保留本仓库已有的
Grok transcript/scrollback/stream 修复和外置 DSH TUI 插件代码，不修改
`/home/leo/code/deepseek-harness`，并提交到
`git@github.com:Gitofxiongxiong/dsh-tui-grok.git`。

## 设计契约与边界

- `dsh-pager-grok` 是唯一要发布的仓库；DeepSeek Harness 只作为外部 backend 和
  profile 宿主，不复制进本仓库，也不回写其 checkout。
- Grok-derived UI、DSH-neutral adapter、协议/transport 和三个外置 TypeScript
  包继续按现有架构边界维护。
- 现有工作树中的连续代码、锁文件、测试 fixture、计划文档和开发进度记录属于
  本项目历史工作，整体纳入本次独立仓库提交；不提交 target/node_modules/lib
  构建产物、临时日志或凭据。
- README 和顶层 npm metadata 补充独立 GitHub 仓库及源码 profile 的可复现入口。

## 计划修改范围

- `README.md`
- `package.json`
- `Cargo.lock`
- `Cargo.toml`
- `crates/dsh-pager-grok-ui/Cargo.toml`
- `crates/dsh-pager-grok-ui/src/host_adapter.rs`
- `crates/dsh-pager-grok-ui/src/parity.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- `crates/dsh-pager/src/lib.rs`
- `crates/dsh-pager/src/loader.rs`
- `crates/dsh-pager/src/presentation.rs`
- `crates/dsh-pager/src/scrollback.rs`
- `crates/dsh-pager/src/session.rs`
- `crates/dsh-pager/tests/presentation_fixtures.rs`
- `crates/dsh-pager/tests/snapshots/replay-presentation.json`
- `docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md`
- 当前工作树中已有的 9 份 `docs/开发进度记录/2026-08-23_*.md` 未跟踪记录
- 本进度记录

Git remote 配置和 push 是仓库外部状态，不写入项目文件；目标 remote 为用户指定的
GitHub SSH 地址。

## 风险、回滚与验证

- 风险：当前工作树包含多个历史批次的未提交修改，提交前必须通过 staged diff
  审计，确认没有无关文件或构建输出。
- 风险：真实 backend 凭据位于 Harness 自己的 `$DSH_HOME`，只做扫描确认，不把
  凭据复制到本仓库。
- 回滚：本地可回退本次提交；远端 push 前先核对 remote、分支和 commit 内容，避免
  覆盖其他远端历史。
- 预期验证：`git diff --check`、`cargo fmt --all -- --check`、
  `cargo test --workspace --locked`、`pnpm run verify:ts`、协议 fixture 检查、
  必要的 mock PTY/E2E 与 remote push 后的分支核对。

## 实际修改

- 保留并纳入本次独立仓库提交的既有实现：
  - DSH presentation/session/scrollback 的 stable streaming surface、finish/EOF/
    generation 收敛、source visibility 和零高度 projection seam；
  - Grok transcript 的 User/Agent chrome、Markdown AST、thinking/tool block 投影、
    dense group、双击折叠、选择框和间距行为；
  - `pulldown-cmark` workspace 依赖、回归 fixture/snapshot 与对应 9 份历史进度记录；
  - 三个外置 TypeScript TUI 包及之前已提交的本地 profile/E2E 边界。
- `README.md` 改为独立 `dsh-tui-grok` 仓库说明，补充 GitHub SSH 地址、外部 Harness
  边界和源码 profile 的本地联调入口。
- `package.json` 增加 GitHub repository metadata。
- 构建产物、`target/`、`node_modules/`、TypeScript `lib/`、临时日志和凭据均保持
  被 `.gitignore` 排除，未进入暂存区。
- `/home/leo/code/deepseek-harness` 未修改，checkout 保持干净。

## Git 提交

完成；commit 必须包含：

`Progress-Record: docs/开发进度记录/2026-08-24_01-16-44_github_release_cleanup.md`

## 未解决问题与下一步

本仓库仍保留长期计划中明确列出的 Grok reference geometry、special-tool viewer
和完整 P19/P20 真实矩阵缺口；这些不是本次 GitHub 整理新增的阻塞。远端首次检查
发现默认 SSH agent 未加载 key，已使用本机 `/home/leo/.ssh/id_ed25519_github`
显式验证 GitHub SSH 权限；push 时继续使用同一 key。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test --workspace --locked`：通过，workspace 单元测试、集成测试和 doctest
  全部通过。
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`：通过。
- `pnpm run verify:ts`：通过，protocol 16、server 37、embedded 2 个测试通过，
  三个包 build/typecheck 通过。
- `python3 scripts/check-protocol-fixtures.py`：通过，2 个 fixture 同步。
- `bash scripts/e2e.sh`：通过，source manifest、972-case M10 matrix、workspace
  tests/clippy/build 和 full mock PTY 门禁全部通过。
- `git diff --check`：通过；暂存区审计确认只包含本记录列出的项目文件和历史进度记录。
- `GIT_SSH_COMMAND='ssh -i /home/leo/.ssh/id_ed25519_github -o IdentitiesOnly=yes'
  git ls-remote git@github.com:Gitofxiongxiong/dsh-tui-grok.git`：通过，远端当前为空，
  可安全首次 push。
