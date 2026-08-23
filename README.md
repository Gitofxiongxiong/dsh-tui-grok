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

需要连接真实后端时，开发期可以把本仓库的三个 TypeScript 包一次性 link 到隔离
profile，再运行只读真实 E2E：

```bash
DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness \
DSH_TUI_PROFILE=dsh-tui-grok-dev \
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
