# Trusted Publishing 用 tui-protocol 预发布做烟测

> 记录时间：2026-08-27 15:08:21 +0800
> 操作者/agent：main agent
> 状态：in-progress

## 目标与背景

用户已为 8 个 `@dsh-pager-grok/*` 包配置 Trusted Publisher，并确认
GitHub environment `npm-release` 无 Required reviewers。要验证 OIDC 能否
**直接** `npm publish`（不走 staged approve）。用 `@dsh-pager-grok/tui-protocol`
打一个不影响 `latest` 的预发布版本：`0.1.1-trust.<run_id>`，dist-tag
`trust-test`。CLI/runtime/native 的 `0.1.0` latest 不动。

## 设计契约和复用依据

- 对应计划：PRODUCT_PLUGIN_LAUNCHER.md Trusted Publishing / OIDC。
- 官方：trusted-publishers 要求 workflow 文件名 `publish.yml`、environment
  `npm-release`、`permissions.id-token: write`；npm CLI ≥ 11.5.1。
- Grok：不适用。复用等级 D。

## 计划修改范围

- 文件：
  - `.github/workflows/publish.yml`
  - 本进度记录
- 预期行为：在分支 `ci/trust-smoke` 上 push 触发 job `trust-smoke`：
  构建 tui-protocol、去掉 `private`、版本改为 `0.1.1-trust.${{ github.run_id }}`、
  `npm publish --access public --tag trust-test`。不 bump latest。
- 不在范围内：发 CLI/runtime/native；改 package 源码 version；unpublish；
  GitHub Environment reviewers。

## 风险、回滚和依赖

- 风险：OIDC 配错会 publish 失败，不破坏 0.1.0。
- 风险：预发布版本会留在 npm；可 72h 内 unpublish 该 version。
- 回滚：还原 `publish.yml`；必要时 `npm unpublish @dsh-pager-grok/tui-protocol@<prerelease>`。
- 依赖：npm Trusted Publisher 已绑 `publish.yml` + `npm-release`（已用
  `npm trust list` 核对 8/8）。
