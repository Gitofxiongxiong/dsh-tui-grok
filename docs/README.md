# dsh-pager-grok 文档

这里只保留当前 successor 的可执行约定，不再混放旧项目的设计草稿和调研快照。
grok build 的代码仓库在/home/leo/code/grok-build

- [GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md](GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md)：长期总控计划、不可破坏的设计契约、Grok 源码复用地图、细化里程碑、工作包、验收矩阵和风险登记；这是后续实现的最高优先级入口。
- [GROK_SCROLLBACK_CLOSURE_PLAN.md](GROK_SCROLLBACK_CLOSURE_PLAN.md)：N2/档 3 执行细则。冻结 `transcript.rs` 自造视觉算法，按 Grok `scrollback/` 闭包 vendor，并把该神文件的职责按切片删掉。
- [开发进度记录](开发进度记录/README.md)：每个工作批次在修改仓库前必须创建的时间戳记录，以及一份记录对应一个 Git commit 的提交规则。
- [ARCHITECTURE.md](ARCHITECTURE.md)：模块边界、数据流和哪些代码允许依赖 host。
- [MIGRATION_PLAN.md](MIGRATION_PLAN.md)：从旧 pager 到 Grok UI 的垂直迁移顺序。
- [SOURCE_POLICY.md](SOURCE_POLICY.md)：Grok 源码固定版本、保留范围和本地修改规则。
- [TESTING.md](TESTING.md)：编译、单元、协议 smoke 与终端验证。

阅读顺序建议：先读长期主计划的第 0、2、3、5、7、10、13 节，再按需要查阅
架构、来源和测试细节。旧项目的分析材料只作为历史背景，不得覆盖本 successor
的设计契约。
