# 外置 DSH 插件套件

本仓库中的 TypeScript 目录是外置集成层，不是 `deepseek-harness` 的 fork：

```text
@dsh-pager-grok/tui-protocol   纯协议/codec library
@dsh-pager-grok/tui-server     Cordis plugin，stdio gateway + control-plane
@dsh-pager-grok/tui-embedded   dsh.bundle profile patch
```

它们只依赖 DSH 发布的 `0.1.0-rc.8` 包；没有修改 DSH 的 CLI、`dsh-base`、`host/apiproxy` 或任何官方源码。`fileReferences.list` 是 server 自己适配的 TUI method：server 读取公开的 `ctx.fileReferences` provider，并用公开的 `dsh-api-remotes` resolver 找到 session 对应 agent。因此官方 Harness checkout 可以保持干净。

## 本地验证

```bash
cd /home/leo/code/dsh-pager-grok
pnpm install
pnpm run verify:ts
```

`pnpm run verify:ts` 会构建三个包并运行 protocol、gateway、control-plane、transport 和 bundle patch 测试。

## 安装到 DSH profile

发布到 npm 后，推荐只把 bundle 加到一个自定义 profile；profile 的依赖会带入 protocol/server 和官方 host 插件：

```bash
dsh plugin --profile grok-tui add @dsh-pager-grok/tui-embedded@0.1.0
dsh --profile grok-tui
```

开发期不发布 npm 时，先在本仓库打包三个 npm tarball，再按依赖顺序安装：

```bash
mkdir -p /tmp/dsh-pager-grok-pack
pnpm --filter @dsh-pager-grok/tui-protocol pack --pack-destination /tmp/dsh-pager-grok-pack
pnpm --filter @dsh-pager-grok/tui-server pack --pack-destination /tmp/dsh-pager-grok-pack
pnpm --filter @dsh-pager-grok/tui-embedded pack --pack-destination /tmp/dsh-pager-grok-pack

dsh plugin --profile grok-tui add /tmp/dsh-pager-grok-pack/dsh-pager-grok-tui-protocol-0.1.0.tgz
dsh plugin --profile grok-tui add /tmp/dsh-pager-grok-pack/dsh-pager-grok-tui-server-0.1.0.tgz
dsh plugin --profile grok-tui add /tmp/dsh-pager-grok-pack/dsh-pager-grok-tui-embedded-0.1.0.tgz
dsh --profile grok-tui
```

三个 tarball 应保持同一版本；实际发布时应由 GitHub Actions 统一发布，避免 profile 装到不匹配的 protocol/server。

## 与 Rust pager 的边界

Rust crates 仍然拥有 transport、session、control-plane 真源和 Grok UI。TypeScript server 只提供 DSH 进程内的 protocol carrier 与公开 host seam；Rust 客户端通过 `tui.hello`、`tui.attach`、`tui.subscribe`、ApiProxy methods 和 `tui.respond` 与它通信。
