# dsh-tui-grok

DeepSeek Harness 的原生终端 UI 实验项目。

这是一个独立仓库（`git@github.com:Gitofxiongxiong/dsh-tui-grok.git`），从 `dsh-pager`
拆出并接入 Grok Build UI。DeepSeek Harness 原仓库只作为外部 backend/profile 宿主；
本仓库包含自己的 Rust TUI、DSH-neutral adapter、协议/transport 和三个外置 TypeScript
插件包，不需要把 Harness 整个仓库复制进来。

## 当前边界

```text
backend / RPC
    -> dsh-pager (session、loader、control-plane、协议 DTO)
    -> dsh-pager-grok-ui (Grok 视图、输入、布局和交互状态)
    -> dsh-pager-bin (进程入口与命令行 smoke)
```

复用优先级是：Grok 的视图和交互状态机 > Grok 的渲染辅助模块 > 本项目已有的 runtime/protocol；只有 host 数据模型、RPC effect 和无法直接匹配的终端生命周期才写适配代码。旧的 monolithic `app` 不再是新项目的 UI 入口。

## 构建与验证

```bash
cargo check --workspace
cargo test --workspace
cargo run -p dsh-pager-bin -- --help

# TypeScript external DSH packages
pnpm install
pnpm run verify:ts
```

## 产品安装（v2）

```bash
npm install -g @dsh-pager-grok/cli
dsh-pager
```

CLI 自带钉死版本的 DeepSeek Harness 与 pnpm，首次运行会把 `@dsh-pager-grok/runtime`
装进 profile `dsh-pager-grok`，并启动当前平台的预编译 pager。不要在 TTY 上直接
`dsh --profile dsh-pager-grok`。

## 启动新对话

本机开发环境可直接运行。首次运行会自动检查并构建三个 TypeScript 包，然后把它们
一次性 link 到项目专属的 `dsh-pager-grok-dev` profile；profile 位于 Harness 的
`$DSH_HOME`，不会修改 `deepseek-harness` checkout：

```bash
./scripts/start-new-chat.sh
```

脚本随后会构建最新 `dsh-pager`，连接该 profile，并使用 `--new` 创建新会话。只检查
backend/profile 是否可用而不创建会话时运行：

```bash
./scripts/start-new-chat.sh --check
```

需要显式准备 profile 时运行：

```bash
./scripts/setup-dev-profile.sh
```

启动脚本默认会自举 profile；`--skip-setup` 只适合已经准备好 profile 的场景。默认
backend 是 `--backend <node>`、`--backend-arg <absolute apps/cli/lib/bin.js>`、
`--backend-arg --profile`、`--backend-arg <profile>`，不设置 `DSH_TUI_SERVER`。
可用 `DSH_HARNESS_ROOT`、`DSH_HOME`、`DSH_TUI_PROFILE` 和 `DSH_TUI_CARGO`
覆盖本机默认路径。`DSH_TUI_SERVER` 是完整 backend 命令的高级覆盖（空白拆分，
路径不得含空格）；设置后不再注入默认 `--backend` 链。指定已有的非本项目
profile 时，初始化脚本默认拒绝覆盖；确认后可设置 `DSH_TUI_PROFILE_ALLOW_UPDATE=1`。

真实 E2E 使用同一套 profile 自举逻辑。下面的命令显式使用隔离 profile；脚本会一次性
link 本仓库的三个 TypeScript 包：

```bash
DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness \
DSH_TUI_PROFILE=dsh-pager-grok-e2e \
DSH_TUI_INSTALL_LOCAL=1 \
bash scripts/real-e2e.sh
```

正式安装时，也可以将构建后的 `@dsh-pager-grok/tui-embedded` 安装到 profile，
再运行 `dsh --profile <profile>`；需要自定义 backend 时使用 `--backend` 和
`--backend-arg` 覆盖。真实 prompt/多轮联调的安全边界见 [验证策略](docs/TESTING.md)。

## 文档

- [架构边界](docs/ARCHITECTURE.md)
- [迁移计划](docs/MIGRATION_PLAN.md)
- [源码复用与许可证策略](docs/SOURCE_POLICY.md)
- [验证策略](docs/TESTING.md)
- [外置 DSH 插件安装与 profile](docs/EXTERNAL_DSH_PLUGIN.md)

## 许可证

项目自身沿用 Apache-2.0/MIT 兼容声明；复制进来的 Grok 源文件及其许可证见 `crates/dsh-pager-grok-ui/vendor/grok/LICENSE` 和 [源码策略](docs/SOURCE_POLICY.md)。
