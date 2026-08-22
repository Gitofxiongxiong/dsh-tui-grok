# dsh-pager-grok

DeepSeek Harness 的原生终端 UI 实验项目。

这个目录是从 `dsh-pager` 复制出的干净 successor。原项目保留作协议、运行时和回归测试参考；本项目把 Grok Build 的 UI 组件作为默认视觉与交互实现，通过一层很薄的 host adapter 接到 DSH 的 session/runtime。

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
```

需要连接真实后端时，默认命令是 `dsh --profile tui-embedded`，也可以使用 `--backend` 和 `--backend-arg` 覆盖。

## 文档

- [架构边界](docs/ARCHITECTURE.md)
- [迁移计划](docs/MIGRATION_PLAN.md)
- [源码复用与许可证策略](docs/SOURCE_POLICY.md)
- [验证策略](docs/TESTING.md)

## 许可证

项目自身沿用 Apache-2.0/MIT 兼容声明；复制进来的 Grok 源文件及其许可证见 `crates/dsh-pager-grok-ui/vendor/grok/LICENSE` 和 [源码策略](docs/SOURCE_POLICY.md)。
