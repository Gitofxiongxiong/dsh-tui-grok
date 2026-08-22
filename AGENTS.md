# dsh-pager-grok Agent Instructions

本文件是 /home/leo/code/dsh-pager-grok 的项目级执行约定。

## 开始任何工作前

先阅读：

1. docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md；
2. docs/ARCHITECTURE.md；
3. docs/SOURCE_POLICY.md；
4. docs/TESTING.md；
5. docs/开发进度记录/README.md。

如果任务涉及 Grok 源码，先核对：

- 上游目录：/home/leo/code/grok-build；
- mirror commit：19d42e35c07a9c9244f03f6df0c4c353f970d4f9；
- SOURCE_REV：7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa；
- crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md、许可证和对应上游测试。

## 绝对目标

本项目的终极目标是：保持 Grok Build TUI 原有的前端样式、布局、交互状态机、
快捷键、鼠标行为、滚动/选择、主题、动效和终端生命周期，把它适配到
DeepSeek Harness。

所有实现必须遵守以下分工：

- Grok 决定用户如何看到和操作：AppView、AgentView、组件树、Theme/Appearance、
  focus、Esc ladder、scrollback、selection、picker、queue、modal、status、
  timeline、viewer 和终端呈现优先复用 Grok；
- DSH 决定系统真实发生了什么：protocol、transport、session、control plane、
  replay/live、generation、queue authority、interaction、RPC effect 和
  DeepSeek 产品数据由 dsh-pager 保持；
- adapter 负责两者接线：SessionState → DSH-neutral DTO，以及
  UiIntent → UiEffect → receipt/notification。

数据不同不是重写 UI 的理由。Grok 有同职责实现时，必须先复制、移植或结构性
适配；不得因为适配超过 100 行、现有 DSH 代码已经能运行或名称不同，就新建
第二套同职责 UI。

## 修改仓库前的强制记录

任何会改变仓库状态的操作都适用本规则：代码、配置、文档、脚本、vendor、
Cargo.lock、测试 fixture、格式化结果和生成文件都包括在内。

只读搜索、分析、编译和测试可以先做。第一次目标文件修改前，必须在
docs/开发进度记录 创建新的时间戳记录：

~~~text
YYYY-MM-DD_HH-mm-ss_<简短主题>.md
~~~

时间使用 Asia/Shanghai（+0800），精确到秒。记录必须先列出目标、背景、设计
契约、计划文件、风险、回滚方式和预期验证。之后只能修改记录列出的范围；
范围扩大前先创建新记录。

完成或失败后，在同一份记录中写入实际文件、摘要、验证结果、阻塞和下一步。
回写同一份记录是该批次的收尾动作，不需要递归创建记录。历史记录不删除、
不静默改写。

## Git 提交规则

一份主进度记录对应一个 Git commit，一个 commit 只能对应一个主进度记录。
commit message 必须包含：

~~~text
Progress-Record: docs/开发进度记录/YYYY-MM-DD_HH-mm-ss_<简短主题>.md
~~~

提交前必须检查：

- 暂存区只有该记录和记录中列出的目标文件；
- 没有第二份主记录、无关格式化或生成物；
- trailer 路径与实际文件名完全一致；
- 需要拆分的工作先创建新的记录；
- 需要 squash 时先合并记录范围，再生成最终提交。

记录和目标文件应在同一个 commit 中提交。不要为了事后写 commit hash 产生
无意义的额外提交；记录路径、trailer 和提交文件列表是绑定依据。

当前 successor 尚没有 Git 元数据。Git 根确认/初始化、历史记录审计和
trailer/范围自动校验属于 M0.1；在此之前不能声称已有 commit 对应验收。

## 架构所有权

| 区域 | 负责内容 | 规则 |
|---|---|---|
| crates/dsh-pager-protocol | 线协议、版本、wire DTO | 保持向后兼容和 fixture；不放 UI |
| crates/dsh-pager | transport、loader、SessionState、control plane、presentation | 只拥有 DSH 真源；不得 import Grok view 或 ratatui Frame |
| crates/dsh-pager-grok-ui | Grok-derived view/input/layout、host adapter、effect reducer | view 不调用 RPC；适配逻辑集中在 adapter |
| crates/dsh-grok-inline / dsh-grok-textarea | Grok 低耦合终端和编辑器 | 保留来源、许可证和上游测试 |
| crates/dsh-pager-primitives / render | 中性渲染 helper、terminal lifecycle glue | 已有 Grok 实现时不要再维护平行算法 |
| crates/dsh-pager-bin | 参数、backend shell、非交互 smoke、UI 入口 | 不实现布局和业务 view |
| crates/dsh-pager-test-support | fixture、mock Harness、PTY screen、semantic snapshot | 新行为必须在合适层级留下证据 |
| docs/开发进度记录 | 每批变更的审计记录 | 先记录、后修改；一记录一 commit |

旧 app.rs、block_viewer.rs、picker.rs、queue_view.rs、selection.rs 只能作为
行为 oracle 或迁移 fallback，不能继续堆新的 Grok parity 功能，也不是默认 UI
入口。

## Grok 复用规则

复用等级如下：

- A0：固定 snapshot 原样复制；
- A1：路径/import/别名/少量 DTO 接线；
- B：结构性适配，但保留 Grok 组件、状态机、不变量、测试和视觉规则；
- C：只对 DSH 真源、协议、身份、DeepSeek 字段自建；
- D：排除 Grok agent、shell、tools、ACP、auth、persistence、telemetry 和
  foreign session runtime。

每个新 UI 模块进入前，必须回答：

~~~text
Grok 对应源码和测试：
复用等级：
DSH-neutral seam：
新增 DTO/Intent/Effect：
稳定 ID 和 generation：
将替换的旧模块：
失败测试、PTY/golden 和回滚：
~~~

vendor 修改必须更新 SOURCE_MANIFEST.md、hash、许可证/NOTICE 和本地修改说明。
不得把 Grok runtime crate 引入 DSH 生产依赖。

## 异步、身份和效果

- session、request、generation、sequence、render entry、queue item 和
  interaction 都要有稳定 identity；
- replay/live、attach/detach、reconnect、partial replacement 和 queue revision
  必须由 host authoritative state 收敛；
- UI local draft、focus、hover、pending indicator 不能冒充最终真源；
- 任何 effect 都要经过 Intent → Effect → receipt/notification，不能因重绘重复
  发送 RPC；
- stale/gone/duplicate/unsupported/error 必须显式进入诊断或 Grok status/modal；
- view 不得直接持有 RpcTransport、解析 RPC JSON 或修改 session log。

## 验证门禁

按变更风险运行最小相关集合，并在进度记录中写出实际命令：

~~~bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 scripts/check-protocol-fixtures.py
python3 scripts/pty-smoke.py --binary target/debug/dsh-pager
~~~

UI 变更还应覆盖 semantic buffer、geometry、focus、wrap、scroll、mouse、
paste、resize、terminal restore 和必要的 mock/real Harness 场景。不要只用
最终文字或 ANSI 字节声称视觉 parity。

当前已知基线问题：

- fast clippy 会报告未修改 vendor 测试的 needless_borrow；
- all-features clippy 还会触发 optional markdown fixture 依赖和 ratatui
  Backend/safe-buffer API 缺口；
- 这些问题必须登记并修复，不能通过修改上游 vendor 内容或放宽所有 lint
  来隐藏。

## 文档和代码风格

- 代码、行为、协议和测试变更同步更新相关文档；
- 长期决策写入 docs 主计划，逐批事实写入开发进度记录；
- 使用直接、可验证的中文/英文描述，避免没有验收含义的“差不多”“已完成”；
- 文件保持一个末尾换行；提交前运行 diff 检查；
- 优先复用已有依赖和 Grok-derived 代码，不为已有能力增加新 crate；
- 不提交密钥、真实后端凭据、临时日志、target 输出或用户私有数据。

## 安全和操作边界

- 保留用户已有修改；先读后改，避免覆盖未审阅内容；
- 不运行 broad destructive commands，不使用未经确认的 reset/checkout/recursive delete；
- 外部 editor/pager、PTY、raw mode、reader thread 和子进程必须有恢复/清理路径；
- 不为完成测试而伪造 Harness 成功、吞掉错误或改变协议语义；
- 发现任务范围需要新权限、真实凭据或外部协调时，先报告阻塞。

## 完成定义

一个任务只有在以下条件都满足时才算完成：

1. 预先进度记录存在且范围匹配；
2. Grok source/reuse 决策和 DSH contract delta 可追溯；
3. 相关测试、semantic/PTY 验证和失败结果已记录；
4. 无未登记的平行 UI、真源或 vendor 漂移；
5. 若有 Git 根，暂存区审计通过并创建带 Progress-Record trailer 的对应 commit；
6. 未解决问题、fallback 删除条件和下一步已写回记录。
