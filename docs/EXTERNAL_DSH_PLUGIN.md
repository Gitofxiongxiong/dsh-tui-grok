# 外置 DSH 插件套件

> [!IMPORTANT]
> **状态：rc.8 历史基线，请勿照抄执行。** 本文描述的是 `0.1.0-rc.8` Host
> ApiProxy 时代的树外接缝；当前实现已采用 registry 驱动的多 family 架构，且
> `session-projection-recovery` 已退役。本文后续仅作为
> [DSH 多版本兼容方案](DSH_MULTI_VERSION_COMPATIBILITY_PLAN.md) 中
> `apiproxy-v1` adapter family 的恢复与对照基线。可执行命令以
> [`compat/README.md`](../compat/README.md) 和 [TESTING.md](TESTING.md) 为准。

本仓库中的 TypeScript 目录是外置集成层，不是 `deepseek-harness` 的 fork：

```text
@dsh-pager-grok/tui-protocol   纯协议/codec library
@dsh-pager-grok/tui-server     Cordis plugin，stdio gateway + control-plane
@dsh-pager-grok/tui-session-projection-recovery  Cordis plugin，恢复冷会话列表投影
@dsh-pager-grok/tui-embedded   dsh.bundle profile patch
```

它们只依赖 DSH 发布的 `0.1.0-rc.8` 包；没有修改 DSH 的 CLI、`dsh-base`、`host/apiproxy` 或任何官方源码。`fileReferences.list` 是 server 自己适配的 TUI method：server 读取公开的 `ctx.fileReferences` provider，并用公开的 `dsh-api-remotes` resolver 找到 session 对应 agent。因此官方 Harness checkout 可以保持干净。

## 本地验证

```bash
cd /home/leo/code/dsh-pager-grok
pnpm install
pnpm run verify:ts
```

`pnpm run verify:ts` 会构建四个包并运行 protocol、gateway、control-plane、transport、冷会话投影恢复和 bundle patch 测试。

## 安装到 DSH profile

### 开发期推荐入口

开发期推荐使用仓库脚本创建项目专属 profile。它会构建四个 TypeScript 包，并在一次
`dsh plugin add` 中 link protocol、server、embedded 和 session-projection-recovery，避免依赖
本机遗留 profile：

```bash
DSH_TUI_PROFILE=dsh-pager-grok-dev \
  scripts/setup-dev-profile.sh
```

`scripts/start-new-chat.sh` 默认会自动执行同样的准备；`--skip-setup` 仅用于已经
准备好的 profile。profile 位于 `$DSH_HOME/profiles/<name>`，不会写入 Harness checkout。

### 发布包

发布到 npm 后，推荐只把 bundle 加到一个自定义 profile；profile 的依赖会带入 protocol/server 和官方 host 插件：

```bash
dsh plugin --profile dsh-pager-grok add @dsh-pager-grok/tui-embedded@0.1.0
dsh --profile dsh-pager-grok
```

开发期不发布 npm 时，不要把四个 tarball 分次装进 profile。`pnpm pack` 会把
`workspace:*` 转成普通的 `0.1.0` semver；profile 安装最后一个 bundle 时，pnpm
会尝试从 npm registry 重新下载 `@dsh-pager-grok/tui-server`，而不是复用前面装的
本地 tarball。直接一次性 link 四个源码包即可：

```bash
pnpm run build:ts
dsh plugin --profile dsh-pager-grok add \
  /home/leo/code/dsh-pager-grok/packages/dsh-tui-protocol \
  /home/leo/code/dsh-pager-grok/packages/dsh-tui-server \
  /home/leo/code/dsh-pager-grok/packages/dsh-tui-embedded \
  /home/leo/code/dsh-pager-grok/packages/dsh-tui-session-projection-recovery
dsh --profile dsh-pager-grok
```

真实联调脚本也可以自动完成这一步。它仍然只改 profile 的依赖，不改
`deepseek-harness` checkout；建议使用隔离的项目 profile：

```bash
DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness \
DSH_TUI_PROFILE=dsh-pager-grok-e2e \
DSH_TUI_INSTALL_LOCAL=1 \
scripts/real-e2e.sh
```

只有正式发布到 npm 后，才使用上面的 `tui-embedded@0.1.0` 单包安装方式。

四个 tarball 应保持同一版本；实际发布时应由 GitHub Actions 统一发布，避免 profile 装到不匹配的 protocol/server。

`real-e2e.sh` 的默认真实联调只做只读加载和隔离空 session 的 PTY 生命周期，
不会提交模型 prompt。`--smoke-interactions` 等非交互 smoke 依赖仓库内的
mock-server；对真实 Harness 的模型 prompt 必须另行确认权限和费用边界，并显式
设置 `DSH_ALLOW_REAL_SMOKE=1`。

## 与 Rust pager 的边界

Rust crates 仍然拥有 transport、session、control-plane 真源和 Grok UI。TypeScript server 只提供 DSH 进程内的 protocol carrier 与公开 host seam；Rust 客户端通过 `tui.hello`、`tui.attach`、`tui.subscribe`、ApiProxy methods 和 `tui.respond` 与它通信。
