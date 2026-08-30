# dsh-pager-grok 0.2.0 发布候选 Checklist

> 状态：本地候选演练；未 publish、未移动 dist-tag、未 push Tag、未创建 Release。
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
- [ ] 维护者批准后，按下述顺序执行 Trusted Publishing/OIDC。
- [ ] 所有 npm 包实际可见后再移动 dist-tag 并创建 GitHub Release。

## 维护者人工发布顺序

以下步骤是明确授权发布之后的人工操作；本次 agent 演练不得执行。

1. 审批 `0.2.0`、目标 commit、stable/prerelease 与目标 dist-tag；确认 PR/main CI
   三平台 Rust + 三 DSH 版本矩阵全绿。
2. push 已审批分支/PR，合并后从同一 main commit 创建并 push `v0.2.0` Tag。
3. 在各自 runner 从该 Tag 构建并发布五个 `@dsh-pager-grok/native-*@0.2.0`；验证
   executable bit、provenance 与平台 metadata。
4. 发布 `@dsh-pager-grok/runtime-apiproxy-v1@0.2.0`。
5. 用 registry 上的 native/runtime 再跑一次 clean-prefix cold install；不得用本地
   tarball冒充已发布包。
6. 最后发布 `@dsh-pager-grok/cli@0.2.0`。
7. 验证 npm version/provenance/install 后再移动约定 dist-tag；最后创建 GitHub
   Release 并附支持表、已知限制和回滚说明。

## 回退与中止

- Tag/publish 前：停止 workflow，修复后以新 commit 重新演练；无需变更 registry。
- Tag 已 push、npm 未发布：暂停发布，记录已完成平台；不重定位 Tag。
- 部分包已经发布：已发布 version 不可覆盖/复用。未发布后续单元保持停止；通过新的
  patch version 修复。必要时由维护者对有问题版本执行 `npm deprecate`。
- dist-tag 尚未移动：保留旧 dist-tag 即为首选回退。已移动时由维护者将 dist-tag
  指回最后已知良好版本；不自动 unpublish。
- GitHub Release 只在完整 npm 链验证后创建；失败 Release 应记录为 prerelease/draft
  或停止创建，而不是掩盖部分发布。

## 发布前最后人工确认

- [ ] 维护者审批版本、commit、dist-tag、包列表和副作用。
- [ ] push 分支/合并 PR/创建并 push 单一 `v0.2.0` Tag。
- [ ] Trusted Publishing 按 native → runtime → registry cold → CLI 执行。
- [ ] 移动 npm dist-tag。
- [ ] 创建 GitHub Release。

除以上审批、push、publish、dist-tag、Release 外，不应再剩代码、依赖、fixture、
cold-install 或文档工作。
