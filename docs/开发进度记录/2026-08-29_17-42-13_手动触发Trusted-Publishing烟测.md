# 手动触发 Trusted Publishing 烟测

> 记录时间：2026-08-29 17:42:13 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

将 `.github/workflows/publish.yml` 从 Tag/`ci/trust-smoke` 分支可自动触发的
Trusted Publishing 烟测，收紧为只能手动触发。保持 npm OIDC 绑定所需的
workflow 文件名、`npm-release` environment 和 `id-token: write`，但防止普通
push 或发布 Tag 意外创建公网预发布包。

## 设计契约和复用依据

- 对应规范：`docs/GIT_PR_RELEASE_WORKFLOW.md` 的“开发/发布分离”和 agent
  授权边界。
- 对应发布设计：`docs/PRODUCT_PLUGIN_LAUNCHER.md` Trusted Publishing/OIDC；
  本批次仍是 `tui-protocol` 烟测，不实现正式 native/runtime/CLI 发布链。
- Grok source path、commit、SOURCE_REV：不适用。
- 复用等级：D（GitHub Actions/npm 供应链治理）。
- 安全契约：workflow 仅保留 `workflow_dispatch`，并要求显式输入
  `publish-trust-test`；错误确认值必须在 publish 前失败。

## 计划修改范围

- 文件：
  - `.github/workflows/publish.yml`
  - `docs/GIT_PR_RELEASE_WORKFLOW.md`
  - `docs/开发进度记录/2026-08-29_17-42-13_手动触发Trusted-Publishing烟测.md`
- 预期行为：
  - push `main`、普通分支、`ci/trust-smoke` 或 `v*` Tag 都不触发
    `publish.yml`；
  - 只有 GitHub Actions 页面的 Run workflow 或
    `gh workflow run publish.yml -f confirm=publish-trust-test` 可触发；
  - 重跑同一 Actions run 时把 `GITHUB_RUN_ATTEMPT` 加入预发布版本，避免
    重用已发布 npm version。
- 不在范围内：不发布真实 npm 包，不手动运行 workflow，不 push，
  不改变 npm Trusted Publisher 网站配置或 GitHub environment 保护规则。

## 风险、回滚和依赖

- 风险：手动 workflow 仍会向公网 npm 发布预发布版本。处置：显式确认字符串、
  `npm-release` environment 和文档警告三层门禁。
- 风险：同一 run 重跑会尝试重发同版本。处置：版本包含
  `${GITHUB_RUN_ID}.${GITHUB_RUN_ATTEMPT}`。
- 风险：修正烟测触发后，`v*` Tag 只构建 artifact，仍不是正式 npm 发布。
  处置：文档继续明确正式发布链尚未实现。
- 回滚：恢复 `push.tags` 和 `push.branches` 触发配置；不涉及代码或已发布包删除。
- 依赖：GitHub Actions `workflow_dispatch` inputs、npm Trusted Publishing OIDC、
  npm CLI 11.15.0。

## 预期验证

- 解析 workflow YAML，确认顶层仅有 `workflow_dispatch` 触发。
- 静态检查 confirmation step 位于 checkout/build/publish 之前。
- 检查版本字符串同时包含 run ID 和 run attempt。
- `git diff --cached --check` 通过，暂存区只包含本记录列出的 3 个文件。

## 实际修改

- `.github/workflows/publish.yml`：
  - 删除 `push.tags` 和 `push.branches` 触发器，只保留手动
    `workflow_dispatch`；
  - 增加必填 string input `confirm`，第一个 step 要求其精确等于
    `publish-trust-test`，否则在 checkout/build/publish 前失败；
  - 增加 `npm-trust-smoke` concurrency group，不取消已在执行的发布；
  - workflow 显示名改为 `publish-smoke`，文件名仍为 npm OIDC 绑定的
    `publish.yml`；
  - 预发布版本改为
    `0.1.1-trust.<run_id>.<run_attempt>`，使同一 run 的手工重跑使用新版本。
- `docs/GIT_PR_RELEASE_WORKFLOW.md`：更新真实触发表、Tag 副作用、手动
  `gh workflow run` 命令和正式发布未完成边界。
- 本记录：回写实际修改、验证和未解决项。

## 验证结果

- 使用项目已安装的 `js-yaml` 解析 workflow：顶层 trigger 只有
  `workflow_dispatch`，confirmation 是第一个 step，`TRUST_VERSION` 同时包含
  `github.run_id` 和 `github.run_attempt`，通过。
- 对照 GitHub 官方 workflow syntax/context 文档：`workflow_dispatch.inputs`、
  `inputs` context 和 workflow-level `concurrency` 写法有效；手动 workflow 可从
  Actions 页面、GitHub CLI 或 REST API 触发。
- 本机未安装 `actionlint`，因此未运行 actionlint；以 YAML 解析、结构断言、
  官方语法对照和提交后 GitHub Actions 解析作为验证边界。
- 本地模拟 confirmation shell 判断：`wrong` 被拒绝，
  `publish-trust-test` 被接受，通过。
- `gh workflow run --help`：当前 `gh 2.46.0` 支持 `-f key=value` 和 `--ref`，
  文档命令可用。
- `git diff --check` 和精确暂存后的 `git diff --cached --check`：退出码 0，
  无空白错误，已覆盖新进度记录。
- 未手动运行该 workflow，未请求 OIDC token，未创建任何 npm 版本。

## Git 提交

- commit message：`ci: make Trusted Publishing smoke manual`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-29_17-42-13_手动触发Trusted-Publishing烟测.md`
- 暂存区审计：只包含 `.github/workflows/publish.yml`、
  `docs/GIT_PR_RELEASE_WORKFLOW.md` 和本进度记录，共 3 个文件；其他已有
  未跟踪文件未纳入。

## 未解决问题和下一步

- `publish.yml` 仍只是烟测，不是正式 native/runtime/CLI 发布 workflow。
- `npm-release` environment 当前的 required reviewer/等待时间是 GitHub 远程设置，
  本批次未改动；若需要人工二次审批，应在 Settings 中另行启用。
- workflow 只有合并到 GitHub 默认分支后才能在 Actions 页面被手动触发；本批次
  不 push 也不做真实烟测。
