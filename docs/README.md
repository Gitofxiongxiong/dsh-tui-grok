# dsh-pager-grok 文档

这里只保留当前 successor 的可执行约定，不再混放旧项目的设计草稿和调研快照。
grok build 的代码仓库在/home/leo/code/grok-build

- [GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md](GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md)：长期总控计划、不可破坏的设计契约、Grok 源码复用地图、细化里程碑、工作包、验收矩阵和风险登记；这是后续实现的最高优先级入口。
- [DSH_MULTI_VERSION_COMPATIBILITY_PLAN.md](DSH_MULTI_VERSION_COMPATIBILITY_PLAN.md)：DSH 多版本兼容的总控方案，定义稳定协议、adapter family、支持矩阵、测试与迁移路线。
- [DSH_SUPPORT.md](DSH_SUPPORT.md)：由 `compat/dsh-support.json` 生成的当前精确版本、family、状态与 distribution 支持表。
- [RELEASE_CANDIDATE_CHECKLIST.md](RELEASE_CANDIDATE_CHECKLIST.md)：`0.2.0` 本地发布候选门禁、公开包顺序、人工发布步骤与回退边界。
- [GROK_SCROLLBACK_CLOSURE_PLAN.md](GROK_SCROLLBACK_CLOSURE_PLAN.md)：N2/档 3 执行细则。冻结 `transcript.rs` 自造视觉算法，按 Grok `scrollback/` 闭包 vendor，并把该神文件的职责按切片删掉。
- [开发进度记录](开发进度记录/README.md)：每个工作批次在修改仓库前必须创建的时间戳记录，以及一份记录对应一个 Git commit 的提交规则。
- [GIT_PR_RELEASE_WORKFLOW.md](GIT_PR_RELEASE_WORKFLOW.md)：日常分支、push、Pull Request、rebase merge、Tag 与 npm/GitHub Release 规范，包含 `git`/`gh` 可执行命令和当前 Actions 副作用。
- [ARCHITECTURE.md](ARCHITECTURE.md)：模块边界、数据流和哪些代码允许依赖 host。
- [MIGRATION_PLAN.md](MIGRATION_PLAN.md)：从旧 pager 到 Grok UI 的垂直迁移顺序。
- [SOURCE_POLICY.md](SOURCE_POLICY.md)：Grok 源码固定版本、保留范围和本地修改规则。
- [TESTING.md](TESTING.md)：编译、单元、协议 smoke 与终端验证。
- [`compat/README.md`](../compat/README.md)：rc.8、rc.2、alpha.1 三个独立 fixture/matrix 的可重复本地命令。
- [PRODUCT_PLUGIN_LAUNCHER.md](PRODUCT_PLUGIN_LAUNCHER.md)：树外 `dsh-pager` 产品启动器方案；当前推荐私有精确版本 DSH/pnpm、单一 runtime Bundle 与 npm 平台原生包。

阅读顺序建议：先读长期主计划的第 0、2、3、5、7、10、13 节，再按需要查阅
架构、来源和测试细节。兼容性变更后运行 `scripts/check-dsh-support.mjs`；发布候选
使用 `scripts/pack-release-candidates.mjs` 与
`scripts/rehearse-release-candidate.sh`。旧项目的分析材料只作为历史背景，不得
覆盖本 successor 的设计契约。
