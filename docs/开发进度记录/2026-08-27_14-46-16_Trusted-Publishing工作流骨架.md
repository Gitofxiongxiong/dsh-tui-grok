# 增加 Trusted Publishing 用的 publish.yml 骨架

> 记录时间：2026-08-27 14:46:16 +0800
> 操作者/agent：main agent
> 状态：completed

## 目标与背景

用户要先把 npm Trusted Publishing（GitHub Actions OIDC）配好，避免以后
再用长期 `NPM_TOKEN` 和本机 2FA 发版。npm 要求 trusted publisher 绑定的
workflow 文件名必须存在于 `.github/workflows/`。本批次只落骨架：OIDC
权限、GitHub Environment 名、不实际 `npm publish`。

## 设计契约和复用依据

- 对应计划：PRODUCT_PLUGIN_LAUNCHER.md v2 发布顺序第 5 步、Trusted Publishing。
- 官方：https://docs.npmjs.com/trusted-publishers/ （npm CLI ≥ 11.5.1，Node ≥ 22.14）。
- Grok source path、commit、SOURCE_REV：不适用。
- 复用等级：D。

## 计划修改范围

- 文件：
  - `.github/workflows/publish.yml`
  - 本进度记录
- 预期行为：workflow 文件名为 `publish.yml`；`permissions.id-token: write`；
  job 使用 environment `npm-release`。默认不发 npm。
- 不在范围内：真正的 native→runtime→CLI publish job、GitHub Environment
  保护规则（需在 GitHub 网页创建）、npmjs.com 上每包 Trusted Publisher 表单
  （需用户在浏览器填写）。

## 风险、回滚和依赖

- 风险：environment `npm-release` 尚未在 GitHub 创建时，第一次 Run 会新建
  一个无保护的 environment。应先在 Settings → Environments 建好再跑。
- 回滚：删除 `publish.yml`。
- 依赖：用户在 npmjs.com 为每个 `@dsh-pager-grok/*` 包填写同一套
  GitHub user/repo/workflow/environment。
