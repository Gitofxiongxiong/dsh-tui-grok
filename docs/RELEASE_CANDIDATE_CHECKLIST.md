# dsh-pager-grok 0.2.0 发布候选 Checklist

> 状态：七个公开包的 `0.2.0` 已发布，正式 recovery workflow 与 registry cold/PTY/final
> gate 已全绿；默认 `latest` 只完成部分移动。维护者于 2026-08-30 决定暂停剩余网页认证，
> 后续严格按本文“延期 TODO”逐包串行完成；未执行项不得提前勾选。
> 单一 DSH 支持真源：[`compat/dsh-support.json`](../compat/dsh-support.json)

## 候选范围与发布边界

- 公开单元：五个平台 `@dsh-pager-grok/native-*`、
  `@dsh-pager-grok/runtime-apiproxy-v1`、`@dsh-pager-grok/cli`。
- `@dsh-pager-grok/tui-protocol` 与 `@dsh-pager-grok/tui-server` 必须分别 `npm pack`
  并审计，因为它们是 runtime bundle 的协议/core/adapter 输入；按方案 §13.2 和 D-R10
  保持 `private`，其代码已内嵌进 family runtime，不单独 publish。
- `@dsh-pager-grok/runtime` 保持 controllers-v2/alpha source-only、`private`，不进入
  本次候选；不得把其 alpha dependencies 带入任一公开 tarball。

## §17.2 发布候选门禁

- [x] support registry、family runtime 与 CLI default DSH manifest 一致。
- [x] host native release binary 已 build/strip/pack；五平台 manifests/matrix 已校验。
- [x] protocol/server bundle 输入和 runtime/CLI 已 pack；protocol fixtures 与
  `link:`/`workspace:`/alpha 泄漏门禁通过。
- [x] rc.2 公开候选 17 个唯一 non-optional dependencies 均可由 registry 精确解析。
- [x] 独立临时目录从本地 tarball + registry rc.2 cold install，无 Harness checkout。
- [x] cold doctor、hello、list、load、warm/offline hello 与真实 DSH PTY dogfood 通过。
- [x] 维护者批准后，按下述顺序执行 Trusted Publishing/OIDC。
- [ ] 所有 npm 包实际可见后再移动 dist-tag 并创建 GitHub Release。

## 维护者人工发布顺序

以下步骤只在明确授权发布之后执行。`release.yml` 的 Tag push 只生成候选；实际发布
必须从默认分支手动 dispatch，同时输入 `release_tag=v0.2.0` 与
`confirm=publish-v0.2.0`。workflow 的 package build 与 cold rehearsal 必须 checkout
该不可变 Tag。

1. 审批 `0.2.0`、目标 commit、stable/prerelease 与目标 dist-tag；确认 PR/main CI
   三平台 Rust + 三 DSH 版本矩阵全绿。
2. push 已审批分支/PR，合并后从同一 main commit 创建并 push `v0.2.0` Tag。
3. 等待 Tag artifact run 全绿；从该 run 下载 runtime 候选并对新 package 做一次
   `release-candidate` bootstrap，随后立即绑定 `release.yml` Trusted Publisher。
4. 从默认分支 dispatch 正式 workflow，并显式指定 `v0.2.0`；各 runner 先构建/发布五个
   `@dsh-pager-grok/native-*@0.2.0`，验证 executable bit、provenance 与平台 metadata；
   workflow 再核对 bootstrap runtime 与同 Tag 候选的 SHA-512 integrity。
5. workflow 用 registry 上的 native/runtime + 同 Tag CLI 候选跑 clean-prefix
   cold/warm/offline、doctor/hello/list/load 与 PTY；不得用本地 native/runtime tarball
   冒充已发布包。
6. 上一步通过后，由 workflow 最后发布 `@dsh-pager-grok/cli@0.2.0`。
7. 验证 npm version/provenance/install 后再移动约定 dist-tag；最后创建 GitHub
   Release 并附支持表、已知限制和回滚说明。

## 回退与中止

- Tag/publish 前：停止 workflow，修复后以新 commit 重新演练；无需变更 registry。
- Tag 已 push、npm 未发布：暂停发布，记录已完成平台；不重定位 Tag。
- 部分包已经发布：已发布 version 不可覆盖/复用。若失败仅在发布编排、指定 Tag 的
  package 内容没有变化，则允许修复默认分支 workflow 后安全重入；已存在 version 必须
  比对 registry integrity 与经过身份核验的来源 release run 确切 artifact，并确认
  staging tag，绝不再次 publish。
  若 package 内容或 Tag 本身有缺陷，未发布后续单元保持停止，通过新的 patch version
  修复，必要时由维护者对有问题版本执行 `npm deprecate`。
- dist-tag 尚未移动：保留旧 dist-tag 即为首选回退。已移动时由维护者将 dist-tag
  指回最后已知良好版本；不自动 unpublish。
- GitHub Release 只在完整 npm 链验证后创建；失败 Release 应记录为 prerelease/draft
  或停止创建，而不是掩盖部分发布。

## 发布前最后人工确认

- [x] 维护者审批 `0.2.0` stable、`latest`、公开包列表和发布副作用。
- [x] push 分支/合并 PR/创建并 push 单一 `v0.2.0` Tag。
- [x] Trusted Publishing 按 native → runtime → registry cold → CLI 执行。
- [ ] 移动 npm dist-tag。
- [ ] 创建 GitHub Release。

除以上审批、push、publish、dist-tag、Release 外，不应再剩代码、依赖、fixture、
cold-install 或文档工作。

## v0.2.0 实际发布与恢复事实

- Tag artifact run `33308926704`：`v0.2.0` metadata、五平台 native 与 runtime/CLI
  candidates 全绿；publish jobs 按 tag-push 规则跳过。
- runtime bootstrap tarball SHA-512 为
  `f3d7d0c008f3d8077f7f14c17b42ad240b2682fe7b096f1590f06451f9f7067723a94fb223fb66cec2aeab3c8c9289928ba46b84caa7b0ecf2bb3e9acbe4b1a2`，
  registry `dist.integrity` 与之匹配。作为 registry 首个/唯一版本，npm 自动建立
  `latest=0.2.0`；删除唯一版本默认 tag 返回 400，因此暂时与
  `release-candidate=0.2.0` 并存。
- 首次正式 run `33309459911`：五个平台 native 已通过 OIDC 发布到
  `release-candidate`；runtime integrity job 因 workflow heredoc 解析错误 exit 2，
  registry cold/PTY、CLI publish 与最终 registry/provenance gate 未执行。
- 首次 recovery run `33310487508`：metadata、来源 artifacts 恢复、五个平台 native
  与 runtime integrity/staging 校验全绿；registry cold job 因工具准备 step 只断言
  `command -v rg`、未实际安装 `ripgrep` 而 exit 1。CLI publish 与最终 gate 继续未执行。
- 第二次 recovery run `33311024658`：metadata、来源 artifacts、五平台 native、
  runtime integrity 和 `ripgrep` 工具门禁全绿；cold install 已安装完 451 个
  packages，但审计脚本把 CLI 的 optional native 依赖误当成 prefix 根级 hoist，
  对不存在的根级 symlink 执行 `realpathSync` 而失败。lockfile、dependency graph 和
  CLI-relative `require.resolve` 均证明 registry native `0.2.0` 与 executable 实际存在。
- native 解析修复后，本地以来源 run 的精确 CLI tarball 和 registry native/runtime
  复跑已通过；证据目录 `/tmp/dsh-pager-release-candidate.ujT5UX` 包含 cold audit、
  doctor/hello/list/load、warm/offline 和 PTY `result.json`。正式 recovery 仍等待本批
  PR/main CI 通过后重入。
- 第三次 recovery run `33312297700`：metadata、原始 artifacts、五平台 native
  与 runtime integrity 全绿；cold job 已使用精确 Tag 候选工作树，但也因此
  从 `v0.2.0@77ae309` 执行了 Tag 内旧版 rehearsal/audit，没有消费已合并到
  main 的 native 解析修复。失败仍发生在旧 root-hoist `realpathSync`；CLI publish
  和 final gate 继续跳过，registry 无新增副作用。
- 后续 recovery 保留 Tag 作为 cold job 根工作树，同时把当前 workflow
  definition 的精确 `github.workflow_sha` 检出到独立
  `release-orchestration/`，只从后者运行已审核的 audit/rehearsal/PTY 工具。
  该工具 checkout 不进入 build/pack，不替换任何 Tag/source run artifact。
- 本地双来源同构演练已通过：候选树为
  `v0.2.0@77ae309f8df2547f628b26c62414885349d7fa1c`，编排树为
  `980d5f980c8fcf349e4ded8fa8dafd662fcc3193`，仍使用 source run 的精确 CLI
  artifact + registry native/runtime。证据目录
  `/tmp/dsh-pager-release-candidate.l1WAC6` 为 `passed`。
- 恢复要求：修复 workflow 经 PR/CI 合并后，从默认分支以
  `release_tag=v0.2.0`、`resume_run_id=33309459911`、
  `confirm=publish-v0.2.0` 重入。两次 run candidates 的对比显示 Linux、macOS、runtime
  与 CLI 字节级一致，Windows native tarball 不同；因此 recovery 必须复用来源 run 的
  确切 artifacts。所有已存在 package 只走 integrity/staging-tag 校验；最终门禁未绿前
  不统一移动其余 `latest`、不创建 Release。

## 正式 recovery 结果与延期 TODO（2026-08-30）

- 正式 recovery run `33313278488` 的 11 个 jobs 全绿：复用来源 run
  `33309459911` 的确切 artifacts，完成五平台 native/runtime integrity 校验、registry
  cold/warm/offline、doctor/hello/list/load、真实 DSH PTY、CLI OIDC publish 和最终
  registry/provenance gate。
- 七个公开 package 的 `0.2.0` 均已存在。五个平台 native 与 CLI 带 provenance；首次
  bootstrap 的 runtime 没有 provenance，但 registry SHA-512 与来源候选完全一致，属于
  本 checklist 已批准的一次性新 package bootstrap 例外。
- 暂停时逐包读取 registry 的精确状态：
  - `latest=0.2.0` 且 `release-candidate=0.2.0`：
    `native-linux-x64-gnu`、`native-linux-arm64-gnu`、`native-darwin-x64`、
    `runtime-apiproxy-v1`；
  - `latest=0.1.0` 且 `release-candidate=0.2.0`：
    `cli`、`native-darwin-arm64`、`native-win32-x64`。
- `release-candidate=0.2.0` 可作为无害别名保留，不是 stable 安装或 GitHub Release 的
  阻塞项；本轮不再为删除该别名触发七次额外认证。

后续由已认证维护者严格串行执行，每次只打开一个网页认证会话，并在开始下一项前先用
`npm view` 验证当前项：

1. 优先执行
   `npx --yes npm@11.19.0 dist-tag add '@dsh-pager-grok/cli@0.2.0' latest`，随后确认
   `npm view '@dsh-pager-grok/cli' dist-tags --json` 返回 `latest=0.2.0`。CLI 是默认
   `npx @dsh-pager-grok/cli` 的用户入口，因此必须先完成。
2. 以相同步骤依次移动 `@dsh-pager-grok/native-darwin-arm64@0.2.0` 和
   `@dsh-pager-grok/native-win32-x64@0.2.0`；两项之间不得并行创建认证会话。
3. 对七个 package 做最终只读 `version`、`dist-tags`、`dist.integrity` 和 provenance
   核验；确认所有 `latest` 均为 `0.2.0`，并重跑默认 CLI registry install/doctor/PTY
   smoke。不得重发、覆盖或 unpublish `0.2.0`。
4. 仅在第 3 项全绿后创建 `v0.2.0` GitHub Release，附 DSH 支持表、projection cache
   不迁移限制、来源/recovery run 和回退说明。
5. 用单独的 post-release 进度记录回写最终 tag、Release URL、实际核验命令和结果，再将
   本表剩余 checkbox 勾选完成。
