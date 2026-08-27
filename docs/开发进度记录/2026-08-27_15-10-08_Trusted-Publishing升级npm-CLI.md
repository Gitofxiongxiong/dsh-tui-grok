# Trusted Publishing 烟测升级 Actions 上的 npm CLI

> 记录时间：2026-08-27 15:10:08 +0800
> 操作者/agent：main agent
> 状态：completed

## 目标与背景

`ci/trust-smoke` 上 `trust-smoke` job 在 `publish prerelease via OIDC` 失败（exit 1）。
checkout、pnpm install、tui-protocol build 都绿。官方要求 Trusted Publishing 使用
npm CLI ≥ 11.5.1；GitHub `actions/setup-node` + Node 22 默认常是 npm 10，
不会走 OIDC，PUT 会 401。本批次在 publish 前安装 `npm@11.15.0`。

## 设计契约和复用依据

- https://docs.npmjs.com/trusted-publishers/ ：npm ≥ 11.5.1，Node ≥ 22.14。
- Grok：不适用。D。

## 计划修改范围

- 文件：`.github/workflows/publish.yml`、本记录
- 预期：job 打印 `npm -v` ≥ 11.5.1 后 `npm publish --tag trust-test`。
- 不在范围内：unpublish、改 latest、native/CLI。

## 风险、回滚和依赖

- 风险：仍可能因 Trusted Publisher 声明不匹配失败。
- 回滚：还原 workflow。
- 依赖：上一条烟测记录的 job 结构。

## 实际文件与摘要

- `.github/workflows/publish.yml`：publish 步前 `npm install -g npm@11.15.0` 并打印版本。

## 验证

- 推 `ci/trust-smoke` 后再看 Actions `publish` / `trust-smoke`。

## Git 提交

- commit message：`ci: install npm 11.15 for Trusted Publishing OIDC`
- Progress-Record：`docs/开发进度记录/2026-08-27_15-10-08_Trusted-Publishing升级npm-CLI.md`
