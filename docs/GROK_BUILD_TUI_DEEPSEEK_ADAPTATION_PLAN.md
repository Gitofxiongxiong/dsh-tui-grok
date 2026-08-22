# Grok Build TUI × DeepSeek Harness 长期适配主计划

> 文档类型：长期总控计划、设计契约和执行清单  
> 适用仓库：/home/leo/code/dsh-pager-grok  
> 上游来源：/home/leo/code/grok-build  
> 当前 Grok mirror commit：19d42e35c07a9c9244f03f6df0c4c353f970d4f9  
> 当前 Grok SOURCE_REV：7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa  
> 上游派生代码许可证：Apache-2.0（以 vendor license/NOTICE 为准）  
> 文档状态：执行基线（2026-08-22）  
> 最高目标：**把 Grok Build 原生 TUI 的前端体验适配到 DeepSeek Harness，同时让 DeepSeek Harness 成为唯一的运行时真源；能复用 Grok 的地方坚决不重新造轮子。**

本文件是 successor 的长期主计划。ARCHITECTURE.md 说明稳定的模块边界，
SOURCE_POLICY.md 说明来源和许可证，TESTING.md 说明已有验证命令；如果这些
文件与本计划出现措辞差异，以本文件中的设计契约和验收门禁为准，并在同一
次变更中同步修正文档。

---

## 0. 一页结论

### 0.1 终极交付物

最终交付物不是一个“看起来有点像 Grok 的 DeepSeek pager”，而是：

1. 用户看到的 AppView、AgentView、transcript、prompt、queue、picker、
   dashboard、modal、status、timeline、viewer、selection、鼠标和终端
   动效，遵守固定版本 Grok Build 的布局、颜色、间距、焦点、快捷键和状态机；
2. 这些 Grok-derived 组件不读取 Grok session、agent、shell、ACP 或持久化
   类型，只消费 DSH-neutral 的视图 DTO；
3. DeepSeek Harness 负责协议、session、control plane、历史重放、实时事件、
   generation、队列权威、approval/question 和所有 RPC 副作用；
4. UI 发出的每个用户意图都经过 UiIntent → UiEffect → receipt/notification
   边界，最终状态以 Harness 的 authoritative snapshot 为准；
5. 换 renderer、换终端尺寸、重连、切 session 或退出程序，都不会改变 DSH
   协议语义，也不会留下 raw/alternate-screen/子进程资源。

### 0.2 当前判断

当前 successor 已经有可编译的 crate 边界、Grok-derived 的低耦合模块和一个
最小 dsh-pager-grok-ui 入口，但当前 runtime 仍是 scaffold：

- vendor 目前主要覆盖 picker、modal、status bar、timeline、line editor 等
  小模块；Grok 的 AppView/AgentView、完整 scrollback、selection、focus/
  dispatch 还没有成为生产路径；
- host_adapter 把部分结构化内容压扁成纯文本，UiEffect 目前只有 SubmitPrompt；
- prompt、scroll direction、resize、paste、picker attach、modal/input owner、
  rich block 和真实 DeepSeek backend 还没有达到最终契约；
- cargo check --workspace、cargo fmt --all -- --check、cargo test
  --workspace --locked 和当前 mock PTY smoke 可通过；scripts/test-fast.sh
  的 clippy 阶段仍会在未修改的 vendor 测试中报告 needless_borrow，这应
  通过 wrapper lint 配置处理，而不是篡改上游源码。直接运行
  cargo clippy --workspace --all-targets --all-features 还会触发 optional
  upstream-markdown fixture 缺失依赖，以及 ratatui Backend 新 trait 方法/
  safe-buffer API 不匹配；这些是独立的 M0/M2 门禁，不能用修改 vendor 的方式
  掩盖。

因此，后续工作必须先把“可运行的过渡 UI”变成“可回滚的迁移基线”，再按本文件
的垂直切片逐步替换。**现有能跑不等于最终方向正确，也不授权继续扩展平行 UI。**

### 0.3 三个不可改变的决策

| 决策 | 具体含义 |
|---|---|
| Grok 前端是视觉和交互真源 | 组件树、布局算法、主题 token、focus/Esc ladder、滚动/选择/鼠标状态机优先从 Grok 复制或结构性移植 |
| DSH 是数据和副作用真源 | session、事件顺序、连接、队列、交互请求、generation、RPC receipt 只能来自 DeepSeek Harness |
| 适配层是唯一接缝 | view 不调用 RPC，runtime 不把 Grok 类型带进 host；所有差异集中在 DTO、projection、Intent/Effect adapter |

---

## 1. 名词、范围和成功标准

### 1.1 名词

| 名称 | 本计划中的含义 |
|---|---|
| Grok | 固定 source snapshot 的 Grok Build TUI 及其可抽取的 renderer、view、input、scrollback 和测试 |
| DSH | DeepSeek Harness 的协议、服务端、session/control-plane 和客户端 host runtime |
| Host | crates/dsh-pager 提供的 DSH runtime；它拥有真源和副作用 |
| Grok UI | crates/dsh-pager-grok-ui 及其 vendor/derived crate；它拥有前端状态、绘制和交互呈现 |
| DTO | 供 UI 消费的稳定、无 runtime 依赖的 view model |
| Intent | UI 对用户动作的语义表达，例如 SubmitPrompt、AttachSession、RespondInteraction |
| Effect | host 执行的副作用请求，例如 DSH RPC、session attach、queue mutation |
| Receipt | host 接受、拒绝、排队或失败的明确结果；receipt 不是最终 session 状态 |
| generation | 一次 session attach/reconnect 的代际标识；旧代事件不能覆盖新代 |
| parity | 在同一输入、尺寸、状态和能力下，Grok reference 与 DSH UI 的行为、几何和语义 buffer 可比对 |

### 1.2 必须覆盖的产品表面

- 主会话的 AppView/AgentView 组合和窄终端断点；
- user、assistant、reasoning/thinking、tool call、tool result、error、todo、
  retry、turn、compaction、image/media 等 transcript block；
- 流式 partial、完成、替换、重放与实时事件重叠；
- prompt editor、光标、换行、粘贴、历史、建议/命令入口和提交模式；
- session picker、dashboard、roster、attach/detach、peek/back；
- queue pane、编辑、删除、排序、steer/context 分组和 authoritative revision；
- approval/question/permission/task/timeline/status/modal/shortcut；
- 鼠标命中、滚轮、拖选、跨 block 复制、链接和可选媒体能力；
- terminal raw/alternate screen、resize、外部 pager/editor、退出恢复；
- 连接中、重连中、失败、stale、能力不支持等反馈状态。

### 1.3 明确不在目标内

以下 Grok 业务运行时不得因为 UI 迁移而带入：

- Grok agent loop、shell/tool orchestration、ACP runtime；
- Grok workspace/config/auth/persistence/telemetry；
- Grok 的 session 类型、foreign session 存储和与 DSH 无关的后台服务；
- 为了“先跑起来”而永久保留一套第二版 AppView、scrollback、selection 或
  modal 系统；
- 用静态截图替代真实键盘、鼠标、重连、异步行为；
- 只改变品牌文案却声称已经完成 Grok parity。

### 1.4 最终成功标准

在固定 Grok source snapshot、固定 fixture、固定终端能力和固定尺寸下：

1. 关键路径的 semantic screen buffer、组件几何、焦点 owner、可见状态和
   shortcut 文案与 Grok reference 对齐；
2. 同一用户操作序列得到同样的 UI 状态转移（允许 DSH 后端数据内容不同）；
3. DSH 的真实协议测试、session load/reconnect、队列权威和 PTY smoke 全绿；
4. 删除临时 fallback UI 后，协议、session log 和 host contract 不发生变化；
5. 每个 Grok-derived 文件都能追溯到 source path、source revision、hash、
   license 和测试；
6. 性能、内存、终端恢复和错误行为不低于当前 baseline，或有明确的批准记录。

---

## 2. 不可破坏的设计契约

本节使用规范性词语：

- **必须（MUST）**：违反即不能合并；
- **不得（MUST NOT）**：即使短期能减少工作量也不能采用；
- **应该（SHOULD）**：除非有记录充分的例外；
- **可以（MAY）**：由实现切片决定。

### 2.1 视觉契约：Grok 是唯一前端参考

1. 必须以固定的 Grok source snapshot 及其 fixture/snapshot 作为视觉参考，
   不能以“肉眼差不多”作为完成条件。
2. 必须保持 Grok 的组件层级、边框/分隔线、padding、spacing、颜色语义、
   glyph、文本权重、光标、focus ring、hover/highlight、loading/streaming
   状态和窄宽度断点。
3. markdown、代码、diff、reasoning、tool/result、图片占位、错误和空状态
   必须沿用 Grok 的 block renderer 和视觉 token；缺少 DSH 数据时显示空状态，
   不能替换成一套通用 Paragraph 样式。
4. 动效/反馈（流式光标、spinner、进度、toast、overlay、modal transition）
   必须保留其触发条件和视觉节奏。终端不支持某能力时，只能走显式 capability
   fallback，并保持 Grok 的降级层级。
5. 新增 DSH dashboard/workspace surface 时，数据可以是 DSH-owned，但 pane、
   list、tree、modal、focus、selection 和 spacing 必须组合 Grok-derived
   primitives。
6. 不得在 DSH UI 中另定义与 Grok 等价的 Theme、Appearance、spacing、scrollbar
   或 icon registry。若已有 shim，只能作为迁移期兼容层，并有删除任务。

### 2.2 交互契约：状态机和焦点优先于按键分支

1. 每一帧只能有一个明确的 focus/key owner；事件按 Grok 的 owner 优先级路由，
   不能在顶层 match key 中散落同一快捷键的多个解释。
2. Esc 必须遵守 Grok 的退出阶梯：先关闭最内层输入/选择/下拉，再关闭 modal/
   overlay，再回到 pane，最后才退出或 detach。草稿存在时不得无提示丢失。
3. Key、Mouse、Paste、Resize、Tick 和通知事件必须进入同一 dispatch/reducer
   模型；不能只实现 Key 而把 Mouse/Resize 当作“以后再说”。
4. 滚动位置、sticky header、timeline、选择端点和命中几何必须来自同一个
   layout snapshot；绘制和鼠标命中不得各自计算一套坐标。
5. 输入 editor 必须保留 Unicode grapheme、软换行、多行光标、粘贴和可见
   viewport；提交不得无条件 trim 导致用户内容改变。
6. picker、queue、modal 和 prompt 的打开条件、关闭条件、shortcut bar、
   selected/hovered 状态必须由 Grok 组件状态机决定，host 只提供数据和效果。
7. 任何异步操作完成后，必须检查 session_id + request_id + generation；
   旧结果只能变成诊断，不能覆盖当前 UI。

### 2.3 数据契约：DSH 是唯一真源

1. SessionState、ControlPlaneStore 和 Harness snapshot 是 session、队列、
   交互请求、连接相位、事件顺序和运行状态的唯一权威来源。
2. UI 可以有短暂的 local draft、focus、hover、pending indicator，但不得把
   local state 当成已提交的 session truth。
3. replay 与 live notification 必须可合并、去重、排序并保留稳定 entry identity；
   partial replacement、surface replacement、重连和 attach 切换必须有显式 generation。
4. 每个可交互对象必须有稳定 ID：session、render entry/block、queue item、
   interaction、request、workspace/job。数组索引只能作为显示排序，不能作为动作目标。
5. DshPresentationModel 只能做 SessionEvent → view DTO 投影，不得反向写
   session log，也不得在 UI 内推测缺失的 host 事实。
6. 富内容必须保留结构：Markdown、Reasoning、Plain、Image、ToolCall、
   ToolResult、Diff、Unknown 等 block 不能在 adapter 中提前扁平化为一行字符串。
7. host refresh、attach、detach、queue mutation 和 interaction response 必须
   产生可诊断的 accepted/rejected/pending/conflict/stale 结果。

### 2.4 副作用契约：Intent、Effect、Receipt 分离

目标边界：

~~~text
Grok view event
    -> UiIntent（纯语义、可测试）
    -> reducer 更新 local UI state
    -> UiEffect（带 session/request/generation 的 host 操作）
    -> DSH loader/transport
    -> Receipt + notification
    -> SessionState snapshot
    -> 下一帧 render
~~~

规则：

1. view 不得直接持有 RpcTransport、调用 loader 或解析 RPC JSON。
2. UiIntent 不得携带 ratatui Frame、终端坐标以外的 renderer 实现细节；
   UiEffect 不得暴露 Grok session 类型。
3. Effect 必须可取消、可超时、可记录 request identity；同步 RPC 不能阻塞绘制
   线程超过既定 timeout。
4. receipt 只说明 host 对请求的处理结果；UI 必须等待 authoritative notification/
   snapshot 决定最终文本、队列位置和运行状态。
5. 不支持的 DSH 能力必须返回 typed capability/contract error，并由 Grok
   status/modal 组件显示；不得静默吞掉或伪造成功。
6. 同一 effect 的重试必须遵守幂等键或明确的 duplicate policy；不能因重绘重复
   发送 prompt、queue mutation 或 interaction response。

### 2.5 依赖和源码契约

1. L1-L6 若 Grok 有同职责实现，默认判为 A0/A1/B 复用；“适配代码超过 100
   行”“当前 DSH 已经能跑”“名称不同”都不能作为自建理由。
2. 每个新增 UI 模块开始前，必须在 Grok 源码和测试中登记对应 source path、
   复用等级、runtime seam、预期删除的旧模块。
3. vendor 文件必须保留上游 license、版权、SOURCE_REV 和 SHA-256；本地修改
   必须写入 manifest，不得直接悄悄改 vendor。
4. 生产 crate 不得依赖 xai-grok-shell、xai-grok-agent、xai-grok-tools
   或 Grok persistence/config/telemetry 来换取编译通过。
5. adapter 之外不得出现 DSH/Grok 类型的交叉渗透；Grok-derived view 不得
   import host runtime。
6. 旧 app.rs、block_viewer.rs、picker.rs、queue_view.rs、selection.rs
   只能作为行为 oracle/fallback；迁移期间禁止在其上继续添加新的 parity 功能。

### 2.6 终端和资源契约

1. 进入 raw/alternate screen 后，无论正常退出、错误、Ctrl-C、panic hook、
   外部 editor/pager 返回或 backend 断线，都必须恢复终端。
2. resize 必须触发同一 layout invalidation；不能使用旧宽度的 height cache、
   hit map 或 scrollbar。
3. terminal writer、reader thread、子进程和 temporary file 必须有明确 owner
   和 Drop/取消路径；退出时不能留下后台 reader 或锁住 PTY。
4. 大历史必须采用增量/窗口化布局，不能每个 tick 克隆完整 transcript；
   render 和 notification drain 要有可测上限。
5. capability（鼠标、bracketed paste、OSC52、Kitty image、bidi 等）必须是
   host snapshot 的显式字段，不能用环境猜测替代探测结果。

### 2.7 兼容和演进契约

1. UI 迁移不得破坏已有 protocol version、hello、load-only、dashboard smoke、
   session log 或 Harness API。
2. 新增 DTO 字段优先向后兼容；协议 breaking change 必须有 version gate 和
   fixture migration。
3. 每次上游 Grok 升级必须是一次可审计同步：更新 source revision/hash、跑上游
   tests、跑 DSH parity matrix，再决定是否接受行为变化。
4. 所有“暂时保留”的 fallback 必须有 owner、退出条件、覆盖率门槛和删除切片；
   没有删除条件的临时代码视为架构债务，不得进入完成状态。

### 2.8 变更前进度记录契约：先记录，再动仓库

这是与视觉 parity、DSH 真源和源码复用同等级别的工程契约。

1. **任何会改变仓库状态的操作**都必须先创建进度记录，包括 Rust、配置、
   文档、脚本、vendor、Cargo.lock、格式化结果、测试 fixture 和生成文件。
   只读检查、搜索、编译和测试可以在记录前进行。
2. 每个“工作批次”在第一次目标文件变更前，先在 docs/开发进度记录 创建一份
   唯一文件，格式为 YYYY-MM-DD_HH-mm-ss_<简短主题>.md。时间统一使用
   Asia/Shanghai（+0800），精确到秒；不得覆盖已有记录。
3. 记录必须写在目标变更之前，并至少包含：记录时间、操作者/agent、目标、
   背景、设计契约、计划修改文件、Grok source/revision（如适用）、风险、
   回滚方式和预期验证。
4. 后续只能修改记录中列出的范围。若发现需要增加文件、改变目标或扩大
   影响面，必须先创建一份新的进度记录，再继续工作。
5. 目标修改完成后，在同一份记录中补齐实际文件、摘要、测试命令/结果、
   未解决问题和下一步；回写同一份记录属于该工作批次的收尾，不需要再递归
   创建记录。不能用“计划”冒充“已完成”。
6. 工作取消、失败、被阻塞或只完成一部分时，也必须把状态和原因写回记录；
   历史记录原则上只追加事实，不删除或静默改写。

### 2.9 Git 提交契约：一份记录对应一个 commit

1. 一个独立工作批次必须形成一个主进度记录和一个 Git commit；一个 commit
   只能对应一个主进度记录，不得把两个无关批次合并。
2. commit message 必须包含可机器检索的 trailer：

~~~text
Progress-Record: docs/开发进度记录/YYYY-MM-DD_HH-mm-ss_<简短主题>.md
~~~

3. commit 前必须检查暂存区：只能包含该记录文件以及记录中列出的目标文件；
   发现未登记文件、无关格式化或生成物时，先拆分范围或创建新记录。
4. 若一个工作需要拆成多个 commit，必须在第一个额外 commit 前创建新的
   时间戳记录；每个 commit 都要有自己的 trailer。若需要 squash，先把
   记录范围合并成一份，再生成最终 commit。
5. 记录文件与目标修改应在同一个 commit 中提交。commit hash 不在提交前
   硬编码到记录；记录路径加 commit trailer 是绑定依据，避免为回写 hash
   制造无意义的后续提交。
6. commit 之后用 Git 检查 trailer、记录路径和 commit 文件列表；若提交失败，
   保留记录并标记 in-progress/blocked，不伪造完成。
7. 当前 successor 尚无 Git 元数据；在 M0.1 初始化/确认 Git 根之前，只能
   执行“先写时间戳记录”的部分，不能声称已经完成 commit 对应验收。

推荐的最小工作顺序：

~~~text
只读检查
  -> 创建带时间戳的进度记录（列出本批次范围）
  -> 修改列出的目标文件
  -> 运行验证
  -> 回写同一份记录的实际结果/阻塞/后续
  -> 暂存区审计
  -> 使用 Progress-Record trailer 创建一个对应 commit
~~~

---

## 3. 目标架构和所有权

### 3.1 七层分工

| 层 | 责任 | 所属 crate/目录 | 禁止事项 |
|---|---|---|---|
| L1 终端能力 | raw mode、PTY、能力探测、OSC、resize、writer 生命周期 | dsh-grok-inline、dsh-pager-render、少量 DSH terminal adapter | 不把 Grok agent/runtime 带入；不重复实现 terminal parser |
| L2 渲染内核 | Buffer、cell diff、cursor、safe buffer、wrap、scrollbar、semantic theme | dsh-grok-inline、dsh-pager-primitives、dsh-pager-render、Grok-derived vendor | 不建立第二套 Theme/draw contract |
| L3 文档/虚拟化 | entry、block、height cache、viewport、sticky、group、search | Grok-derived scrollback crate/模块 + DSH presentation adapter | 不把 session event JSON 直接交给 widget；不扩展 Fenwick fallback 成产品引擎 |
| L4 几何/指针 | grapheme、table/bidi geometry、selection、hit map、copy | Grok-derived selection/geometry + DSH stable ID alias | 绘制坐标和命中坐标分离；不自建另一套 selection state machine |
| L5 输入/派发 | event loop、KeyOwner、focus、Esc ladder、Action/Effect/TaskResult、mouse routing | Grok-derived app/event modules + DSH effect adapter | 不在 main.rs 或一个上帝函数中分发所有键 |
| L6 视图 | AppView/AgentView、prompt、queue、picker、dashboard、modal、status、timeline、viewer | dsh-pager-grok-ui vendor/derived | 不以 DSH widget 重画 Grok 组件树 |
| L7 真源/控制面 | protocol、transport、loader、session、generation、reconnect、queue authority、dashboard data | dsh-pager、dsh-pager-protocol、Harness | 不依赖 Grok view；不让 UI 修改真源 |

### 3.2 目标数据流

~~~text
DeepSeek Harness
  ├─ JSON-RPC response/notification
  ▼
dsh-pager protocol + RpcTransport + loader
  ▼
SessionState / ControlPlaneStore / DashboardSnapshot
  ▼
DshPresentationModel + GrokHostSnapshot
  ▼
Grok-derived AppView / AgentView / widgets / scrollback
  ▼
TerminalSurface / semantic buffer / PTY

Key / Mouse / Paste / Resize / Tick
  ▼
Grok event dispatcher + focus owner
  ▼
UiIntent
  ▼
DSH UiEffect adapter
  ▼
loader / RPC / receipt / authoritative notification
~~~

### 3.3 推荐的 crate 边界

- dsh-pager-protocol：线协议、版本和 wire DTO；
- dsh-pager：transport、session、loader、control plane、presentation、host
  projection；不依赖 ratatui Frame；
- dsh-pager-primitives：只保留确实中性的纯 helper；已经有 Grok 对应物时
  逐步改为 re-export/adapter；
- dsh-pager-render：终端生命周期和 renderer glue，不能成为第二套 view；
- dsh-grok-inline、dsh-grok-textarea：Grok 低耦合抽取物；
- dsh-pager-grok-ui：Grok-derived vendor、AppView/AgentView、输入/焦点/
  视图组合、host adapter、effect reducer；
- dsh-pager-test-support：mock Harness、PTY screen、fixture、semantic
  snapshot 和 identity/race scenario；
- dsh-pager-bin：启动参数、backend shell、非交互 smoke 和调用新 UI。

---

## 4. Grok 源码复用地图

### 4.1 复用等级

| 等级 | 定义 | 可接受改动 |
|---|---|---|
| A0 原样复制 | 固定 snapshot vendoring，保留测试/许可证 | 路径、crate 名、版权头 |
| A1 小改 | 单模块轻量接线 | import、别名、feature、少量 DTO 字段映射；有效逻辑通常不超过 100 行 |
| B 结构性适配 | 仍然复用 Grok 的组件、状态机、不变量和视觉规则 | 允许跨文件，但必须保留源码组织和测试，并把 DSH seam 单独隔离 |
| C DSH 自建 | 只有 DSH 真源、协议、身份和 DeepSeek 产品语义 | 不复制 Grok runtime；新 surface 仍组合 Grok UI primitives |
| D 排除 | 与目标无关或会引入另一套 agent runtime | 不复制、不依赖 |

### 4.2 文件级地图

| 目标能力 | Grok 首选来源 | 等级 | DSH 适配内容 | 完成出口 |
|---|---|---:|---|---|
| inline terminal、segment、resize、terminal scroll | xai-grok-pager-render/src/terminal/*、xai-ratatui-inline | A0/B | DSH terminal owner、capability、退出策略 | 上游测试 + terminal/PTY 生命周期绿 |
| textarea、cursor、viewport、wrapping | xai-ratatui-textarea/src/*、xai-grok-pager/src/input/line_editor.rs | A0/B | DSH prompt mode、effect metadata、外部 editor | 多行/Unicode/paste/history/submit 场景绿 |
| safe buffer、line/width、scrollbar | xai-grok-pager-render/src/*、scrollback/wrappers/* | A0/A1 | DSH stable ID 和 capability 字段 | 软换行、Unicode、窄屏 geometry parity |
| semantic Theme/Appearance | xai-grok-pager/src/appearance*、theme/*、render theme | B | 品牌字段和 capability fallback | 修改 palette 不改 widget；snapshot parity |
| draw frame、cursor、link flush | xai-grok-pager-render/src/render/* | B | TerminalSurface/writer seam | 空帧、cursor、OSC/link、resize parity |
| scrollback entry/state/layout | scrollback/entry.rs、scrollback/state/*、scrollback/layout.rs、sticky.rs、search.rs | B | DshRenderEntry、generation、content adapter | 长历史、sticky/group/search/anchor/partial parity |
| block renderer | scrollback/block.rs、scrollback/blocks/*、wrappers/block_renderer.rs | B | DSH Markdown/Reasoning/Tool/Image/Unknown DTO | 结构化 block semantic snapshot + copy |
| selection/geometry | scrollback/text_selection.rs、table_geometry.rs、app/agent_view/selection.rs | B | DSH stable entry ID、copy intent | grapheme/table/bidi/drag/autoscroll/mouse parity |
| event loop/focus | app/event_loop.rs、app/mouse.rs、input/mouse.rs、agent_view/key_owner.rs | B | DSH effect dispatcher、generation guard | owner/Esc ladder/mouse routing/property tests |
| AppView/AgentView shell | app/app_view.rs、app/agent_view/{mod,panes,render,interactions}.rs | B | host snapshot、layout capability | 同一组件树、断点、焦点和 overlay parity |
| prompt UI | app/agent_view/prompt.rs、views/prompt_widget/*、input/line_editor.rs | B | DSH submit/steer/queue intent | 草稿、cursor、suggestion、paste、错误反馈 parity |
| status line/bar | app/status_line*、views/status_line/*、views/status_bar.rs | B | DeepSeek model/connection/status projection | 状态 token、进度、错误、shortcut parity |
| picker/list/dashboard | views/picker.rs、views/session_picker.rs、views/list_pane/*、views/dashboard/* | B/C | DSH roster/session/workspace DTO、attach effect | stable ID、filter、peek/back、attach race parity |
| queue | app/agent_view/queue.rs、views/queue_pane.rs、app/dispatch/queue.rs | B | queue DTO、revision、mutation effect | edit/delete/reorder/steer/conflict parity |
| approval/question/permission | views/permission_view.rs、views/question_view.rs、app/dispatch/permissions.rs | B | interaction request/response DTO | modal focus、expiry、stale response、receipt parity |
| task/timeline/subagent | views/tasks_pane.rs、views/timeline.rs、views/agent_status.rs、app/agent_view/task_status* | B/C | DSH jobs/subagents/task projection | running/complete/error/expand/collapse parity |
| modal/overlay/shortcut | views/modal.rs、modal_window.rs、views/overlay*.rs、shortcuts_bar.rs | A0/B | DSH text/action labels only | back stack、dim layer、focus and close parity |
| media/link/clipboard | app/agent_view/media.rs、scrollback/link_map.rs、input/*paste*、render image modules | B/C | capability gate、OSC52、local path policy | supported capability parity + explicit fallback |
| Grok agent/ACP/shell/config | app/acp_handler/*、agent.rs、shell、tools、persistence | D | 无；仅参考行为 | 依赖树中不存在 |

### 4.3 现有 vendor 与下一批来源

当前 crates/dsh-pager-grok-ui/vendor/grok 已登记 picker、modal_window、
shortcuts_bar、status_bar、timeline、line_editor、key 和 modal_window_state。
下一批应优先迁移与主路径直接相关的完整模块，而不是继续增加孤立小组件：

1. app/agent_view/key_owner.rs 及其测试；
2. app/app_view.rs 和 app/agent_view/{mod,panes,render,interactions}.rs；
3. scrollback/entry.rs、scrollback/state/*、scrollback/wrappers/*；
4. views/prompt_widget/*、views/status_line/*、views/queue_pane.rs、
   views/session_picker.rs、views/block_viewer.rs；
5. scrollback/text_selection.rs、table_geometry.rs、app/mouse.rs；
6. permission_view.rs、question_view.rs、tasks_pane.rs、dashboard/peek 相关模块；
7. media、bidi、外部 editor/pager 等 capability-gated 模块。

每一批都必须连同相邻测试、snapshot 和 source manifest 迁移；只拷贝一个
render 函数而丢掉其 state/geometry/test，不算“复用 Grok”。

---

## 5. 当前基线审计

| 区域 | 当前事实 | 判断 | 必须转向 |
|---|---|---|---|
| workspace/build | workspace 已包含 dsh-pager-grok-ui、Grok-derived primitives、protocol、host runtime | 基础边界已成立 | 固定 CI 和 source drift 检查 |
| binary entry | dsh-pager-bin 已把 loaded session 交给 dsh_pager_grok_ui::run_interactive | 入口方向正确 | 不再把旧 run_interactive 接回默认路径 |
| vendor | 现有 vendor 文件可追溯，hash 对应 source manifest | provenance 基础完成 | 扩展到完整主路径，并保留上游测试 |
| host projection | SessionState::presentation_model() 有结构化 DSH 内容 | 真源方向正确 | host_adapter 不得丢弃 block/content/identity |
| host adapter | 当前 snapshot 有 title/model/status/transcript/picker，但 transcript 主要变成 label + text，picker 只展示当前 attached session | 过渡层 | 保留 rich DTO、stable IDs、roster、capability 和 generation |
| effects | 目前只有 SubmitPrompt，sink 直接带 SessionState，receipt 只有 accepted bool | 契约过窄 | 引入 neutral Intent/Effect/Receipt/OperationKey |
| runtime loop | 当前 loop drain notification、draw、poll key；Resize 基本忽略，Mouse 仅 picker，未完整处理 modal/timeline/paste | scaffold | 按 Grok event/dispatch/focus 重组 |
| transcript scroll | 当前 Paragraph scroll 与 bottom 语义、软换行高度不完全一致 | 已知行为缺口 | 接入 Grok layout/paint window/anchor |
| prompt | 当前 prompt 渲染/提交仍是简化路径，提交前 trim，光标/多行/可见 viewport 不完整 | 不能宣称 parity | 以 Grok editor 为生产实现 |
| picker | 已能显示/过滤并消费 vendor picker；Selected 只关闭，session switching 是下一切片 | 半接通 | stable session ID + attach/load barrier/back |
| modal/status/timeline | 可组合但数据和事件 owner 仍是静态/简化适配 | 视觉基线 | 迁入 AppView/AgentView modal/focus 状态机 |
| shims | clipboard、dim、Theme、wrapping/scrollbar 存在 DSH 自建简化 shim | 迁移债务 | 对照 Grok 原实现，统一 primitive，逐个删除 |
| backend | protocol、transport、loader、session、control plane、queue/presentation 已有基础 | L7 方向正确 | 补齐真实 backend、dashboard、jobs、workspace 等合同 |
| tests | workspace check/fmt/test 和 mock PTY 基线可跑；fast script 的 clippy 被 vendor test needless_borrow 卡住，all-features clippy 还暴露 optional markdown/ratatui API 编译缺口 | 门禁未闭合 | wrapper allow/配置、feature 依赖/API 修复 + 每切片 fixture/PTY/golden |
| version control | successor 当前目录需确认并纳入版本控制 | 治理风险 | 在 M0 建立可审计提交、source manifest 和变更记录 |

### 5.1 目前不能接受的“完成”表述

下列状态只能称为 baseline/scaffold，不能称为 Grok TUI 适配完成：

- 能显示三行文本；
- 能打开一个 picker 或 modal；
- 只有一个 SubmitPrompt effect；
- 只通过 mock backend 的 load/exit smoke；
- 只比较最终文字，不比较 geometry、focus、wrap、mouse、resize 和 terminal restore；
- 把 DSH DshRenderContent 转成纯字符串后再画 Paragraph；
- 保留 DSH 自建同职责组件，却把它们改名为 Grok adapter。

---

## 6. 总体路线和依赖关系

工作按依赖而非按文件数量推进：

~~~text
M0 治理/基线
  └─> M1 DSH-neutral contract + projection/effects
        ├─> M2 terminal/theme/render primitives
        └─> M3 Grok AppView/AgentView + focus/dispatch
               ├─> M4 rich scrollback/block/selection
               ├─> M5 prompt/input
               ├─> M6 picker/dashboard/attach
               └─> M7 queue/interaction/status/task
                      └─> M8 mouse/media/capability completeness
                             └─> M9 reconnect/concurrency/performance
                                    └─> M10 parity/real backend/release
                                           └─> M11 fallback removal/other harness
~~~

L7 的 backend workstream 与 M1、M6、M7、M9 并行，但任何 backend 新能力都必须
先有 DTO/Intent/Effect 设计，不能先在 UI 里写“假数据页面”。

### 6.1 当前里程碑状态

状态只描述 successor 当前可核对的事实；旧仓库开发记录中的“已完成”不会自动
转化为 successor 的完成状态，必须重新通过本文件的出口门禁。

| 里程碑 | 当前状态 | 证据/说明 | 下一步 |
|---|---|---|---|
| M0 治理/基线 | 已完成（M0 baseline） | Git 根/初始提交、source manifest checker、baseline.sh、fallback semantic fixtures、协议 fixture、all-features clippy 和 mock PTY 已闭合；baseline 仍明确不是 Grok reference matrix | M4 继续 reference parity |
| M1 contract/projection/effects | 已完成（M1 contract） | typed identity、rich block/partial/lineage DTO、分区 GrokHostSnapshot、UiIntent→UiEffect→Receipt、dedupe guard、capability matrix、projection/effect/fixture tests 已闭合 | M2/M3 消费 neutral contract |
| M2 terminal/render | 已完成（迁移基线） | `TerminalSurface` 统一 capability-aware raw/alternate/paste/mouse/cursor lifecycle、resize epoch、cell diff/link-map draw；semantic Theme 收归 renderer；prompt 使用 grapheme-aware viewport/cursor；PTY restore、Unicode/wrap、inline link tests 全绿 | M4 接入 Grok scrollback/block renderer 和 reference golden |
| M3 AppView/AgentView | 已完成（迁移基线） | 默认入口已切换为 `AppShell` reducer：KeyOwner/overlay/back state、统一 key/mouse/paste/resize/tick/notification dispatch、picker-owned Esc ladder、adaptive pane layout、semantic focus snapshot；runtime 不再维护 picker_open 平行状态 | M4/M5 继续接入完整 AgentView pane/block/prompt surface |
| M4 scrollback/blocks | 已完成（vertical slice 基线） | `DshRenderBlock` 保留 Markdown/Reasoning/Tool/Result/Image/Diff/Unknown；Grok-derived block projection、结构化 copy、`Scrollback::visible_lines`/materialization 已进入默认 runtime；仍缺完整 reference golden、selection/link map 和 50k 性能门禁 | M4.5-M4.8 rich renderer/reference/selection/perf |
| M5 prompt | 已完成（vertical slice 基线） | 基于 Grok `EditBuffer` 的 multiline `PromptEditor`、Shift+Enter、grapheme-safe cursor、soft-wrap、paste policy、draft receipt 保留已进入默认 runtime；仍缺历史/suggestion/slash、外部 editor/pager 与完整 mouse selection golden | M5.6-M5.8 capability/history/external process/tests |
| M6 picker/dashboard | 已完成（attach vertical slice） | control-plane roster rows 带 stable session ID；picker refresh/filter 不依赖数组索引；Selected 编译 Attach intent/effect 并穿过 `load_session_id` history/live barrier，成功替换 session、失败保留原状态；仍缺完整 dashboard pane/workspace hierarchy/peek-back golden | M6.6-M6.8 dashboard/workspace/race/reference |
| M7 queue/interactions | host 基础存在，UI 未闭合 | queue/pending interaction DTO 有基础，Grok pane/modal 尚未完整接线 | queue authority + modal state machine |
| M8 mouse/media | 未开始（主路径之外） | 当前 mouse 仅局部 picker 路径，媒体/selection parity 未完成 | geometry/selection/capability |
| M9 lifecycle/performance | host 基础存在，UI scheduler 未闭合 | transport/loader/reconnect 部分存在，真实长时 soak 未完成 | generation、bounded scheduler、soak |
| M10 parity/release | 未开始 | 目前只有 mock PTY 基线，不是 Grok reference matrix | reference runner、real backend、golden |
| M11 cleanup/other harness | 未开始 | fallback 和重复模块仍保留 | 删除旧 UI、稳定 host trait |

### 6.2 每个切片的统一工作协议

每个 W/M 任务都按以下顺序执行：

1. **Source map**：列出 Grok 对应源码、测试、source revision 和复用等级；
2. **Contract delta**：只写本切片新增的 DTO、intent、effect、capability 或
   identity 不变量；
3. **先写失败测试**：至少一个 unit/fixture，加一个需要时的 PTY/golden；
4. **移植再适配**：先复制 Grok 结构和测试，再在 adapter seam 接 DSH；
5. **并行实现审查**：搜索是否新增了同职责的 DSH view/scroll/selection；
6. **门禁**：fmt、check、clippy、unit、contract、PTY、snapshot、source
   manifest；
7. **记录回滚点**：旧 fallback 仍可开时，明确 feature/入口和删除条件；
8. **更新本文件状态**：完成、阻塞、延期和下一依赖必须写入任务表。

---

## 7. 里程碑和细化任务

以下里程碑是顺序依赖；同一里程碑内的任务可并行，但出口条件必须全部满足。

### M0：治理、来源和可回滚基线

**目标**：让后续迁移可审计、可比较、可回滚，先消除“代码能跑但不知道是否
复用了正确来源”的风险。

**任务**

- M0.1 确认 successor 的 Git 根、默认分支和提交策略；将 docs、vendor、
  source manifest、license 纳入版本控制。
- M0.2 为 SOURCE_MANIFEST.md 增加自动 hash 校验脚本，分别报告 upstream
  drift、local modification 和缺失 license。
- M0.3 固定 Grok source snapshot、DSH protocol fixture 版本和测试工具版本；
  在 manifest 顶部记录更新日期。
- M0.4 建立 baseline command：workspace fmt/check/test、clippy、协议 fixture、
  mock PTY、真实 backend smoke（如果 backend 可用）。
- M0.5 处理 vendor tests 的 clippy needless_borrow：优先在 wrapper module/
  lint 配置上 allow，不改上游 vendor 内容，不伪造“vendor hash 未变”；同时
  单独修复 all-features 下 optional markdown fixture 的依赖声明和 ratatui
  Backend/safe-buffer API 适配。
- M0.6 保存当前简化 UI 的 semantic screen baseline，标记为 fallback，不把
  它当作 Grok reference。
- M0.7 建立 parity fixture 目录：empty、user/assistant、streaming tool、
  queue、approval/question、two sessions、reconnect、narrow terminal。
- M0.8 冻结旧 UI 的新增功能；给 app.rs 等文件增加迁移注释和预计删除出口。

**门禁**

- cargo fmt --all -- --check
- cargo check --workspace
- cargo test --workspace --locked
- clippy 不因 vendor 测试噪声失败；
- source manifest 校验通过；
- 至少一个 80×24 和一个窄屏 semantic baseline 可重放。

**出口**：任何开发者能从干净 checkout 重建同一来源、运行同一基线并知道
某项行为属于 fallback 还是 Grok parity。

### M1：DSH-neutral UI contract、projection 和 effect reducer

**目标**：先把“Grok UI 消费什么、DSH host 提供什么”固定下来，避免后续每个
view 自己发明 DTO 和副作用。

**任务**

- M1.1 定义 DshSessionId、DshRequestId、DshGeneration、DshSeq、
  DshRenderEntryId、DshQueueItemId、DshInteractionId 的不可混淆类型
  或明确的 newtype。
- M1.2 扩展 DshPresentationModel：保留 block kind/content、source seq、
  partial/completed、tool/result metadata、lineage 和 capability，不允许
  adapter 降级成 String。
- M1.3 定义 GrokHostSnapshot 的分区：session header、agent view、
  scrollback entries、prompt state、queue、interactions、dashboard roster、
  diagnostics、terminal capabilities。每个分区注明 authoritative/local。
- M1.4 将 UiEffectSink 改为 host-neutral trait；view 只提交 UiIntent，
  adapter 负责把 intent 编译成带 identity 的 UiEffect。
- M1.5 设计 UiEffectReceipt：accepted、queued、pending、rejected、stale、
  conflict、unsupported、failed，携带 operation key、诊断和 retry policy。
- M1.6 为 effect 增加幂等/去重键，至少覆盖 submit、attach、queue mutation、
  interaction response、rename/fork/archive 等动作。
- M1.7 为每个 projection 写 fixture：replay/live overlap、partial replacement、
  stale generation、queue revision conflict、session gone、unknown block。
- M1.8 加入 capability matrix：mouse、paste、OSC52、image、bidi、external
  editor/pager、queue steer、workspace action 等明确 supported/blocked。
- M1.9 写 host adapter contract tests，确保同一 SessionState 生成确定性 snapshot，
  不在 render 中修改 session。

**门禁**

- projection fixture 对 entry/queue/interaction identity 做精确断言；
- stale/duplicate effect 不会改变 authoritative model；
- UI crate 不再 import RpcTransport 到 view module；
- 无富内容丢失的字符串化路径；
- 旧 protocol/loader/session tests 全绿。

**出口**：后续任何 Grok view 都可以只依赖 neutral DTO、local state 和
Intent/Effect，不需要知道 DeepSeek RPC 细节。

### M2：终端、主题和基础渲染 parity

**目标**：先把影响所有页面的底层视觉和终端行为统一，消除当前 shim 与
Grok 原实现的分叉。

**任务**

- M2.1 对照 Grok TerminalSurface/draw contract，统一 raw mode、alternate
  screen、cursor、flush、zero-byte idle frame 和退出恢复。
- M2.2 把 Grok semantic Theme/Appearance token 迁入唯一来源；DSH 只注入品牌/
  capability 配置，不再在 dsh-pager-grok-ui/src/theme.rs 另造一套 palette。
- M2.3 接通 draw_frame、safe buffer、cell diff、link map 和 cursor placement；
  记录每个被裁剪/降级的终端能力。
- M2.4 统一 width/grapheme/soft-wrap/line-height 算法；所有 height cache key
  必须含 viewport width、theme/appearance 和 content revision。
- M2.5 接入 Grok scrollbar、sticky rail、accent/divider、hover/focus paint；
  删除只设置背景而忽略 alpha/semantic role 的 shim。
- M2.6 处理 Resize：清空过期 layout/hit map/cache，保持 anchor 和 prompt
  cursor 语义。
- M2.7 处理 Key/Mouse/Paste normalization，启用 bracketed paste，保证 paste
  作为一次语义输入而非逐字符丢失。
- M2.8 增加 terminal capability probe fixture 和不支持能力的 Grok 风格 fallback。

**门禁**

- 80×24、100×30、120×40、40×12 四种尺寸 semantic buffer parity；
- Unicode CJK、emoji、combining mark、RTL/bidi（若 capability 开启）；
- resize storm 后无旧几何命中；
- 正常退出、Ctrl-C、backend error、外部命令返回后 terminal 恢复；
- dsh-pager-grok-ui 不再同时依赖两套 Theme/wrapping/scrollbar 实现。

**出口**：所有上层 view 使用同一套 Grok-derived visual primitives，新增页面
不需要重复处理颜色、宽度、光标和 terminal lifecycle。

### M3：Grok AppView/AgentView shell、focus 和 dispatch

**目标**：替换当前简化 runtime loop 的结构，使 Grok 的组件树和交互状态机
成为生产路径。

**任务**

- M3.1 按 source map 迁移 app/app_view.rs、app/agent_view/mod.rs、
  panes.rs、render.rs、interactions.rs 及相邻 tests。
- M3.2 定义 DSH host snapshot 到 Grok pane model 的单向 adapter；view 不读取
  SessionState 内部字段。
- M3.3 迁移 KeyOwner、focus stack、hover owner、modal/back stack；
  把每个 owner 的优先级写成表驱动测试。
- M3.4 迁移 event_loop.rs、mouse routing、tick/notification scheduling；
  UI thread 只做 reducer/render，effect 在 host boundary 执行。
- M3.5 实现 Esc ladder：prompt draft、completion、selection、picker、queue、
  interaction、modal、dashboard、detach/quit 逐层退出。
- M3.6 迁移 Grok 的宽度断点、pane composition、status/prompt/footer placement；
  不用固定三行 layout 代替。
- M3.7 对每个旧 DSH view 建立 replacement map，禁止同职责模块同时成为默认
  生产路径。
- M3.8 加入 focus/overlay semantic snapshots，断言 dim layer、z-order、cursor
  owner 和 shortcut bar。

**门禁**

- KeyOwner/Esc ladder 单元和 property tests；
- modal/picker/queue/prompt 叠加时每个事件只有一个 owner；
- resize、mouse、paste、notification 在统一 dispatcher 中可重放；
- Grok app/view tests 和 DSH adapter tests 同时通过；
- 旧 app fallback 仍可显式启用，但默认入口只有新 shell。

**出口**：主屏幕不是 runtime.rs 的手工布局，而是 Grok AppView/AgentView
组件树；所有后续 block/prompt/queue 功能都有正确的 owner 和 back stack。

### M4：结构化 transcript、Grok scrollback 和 block renderer

**目标**：在不丢 DSH 富内容的前提下，接入 Grok 长对话、虚拟化、sticky、
分组、搜索和 block 视觉。

**任务**

- M4.1 将 DshRenderContent 一一映射到 Grok block input：Markdown、
  Reasoning、Plain、Image、ToolCall、ToolResult、Diff、Unknown。
- M4.2 为每个 entry/block 建立 stable ID、source sequence、surface/turn lineage；
  partial 更新只能替换同一 identity 或按明确 replacement contract 处理。
- M4.3 迁移 scrollback/entry.rs、state/*、layout.rs、sticky.rs、
  groups.rs、wrappers/* 和相邻 snapshots；当前 Fenwick scrollback 只保留
  fallback/adapter 角色。
- M4.4 接入 Grok paint window、incremental height cache、overscan、prepend/
  append anchor 和 live follow policy。
- M4.5 迁移 block renderer、markdown/code/diff/tool/result role、link map、
  copy reconstruction 和 empty/error states。
- M4.6 接入 timeline/turn navigation、sticky header、running block、streaming
  cursor、compaction/retry 等状态。
- M4.7 对长历史做 1k/10k/50k entry fixture，验证不复制全量内容、不漂移 anchor。
- M4.8 为 DSH 独有 block 保留 Grok wrapper/spacing/role；没有等价样式时新增
  最小 renderer，而不是新增整套 block framework。

**门禁**

- 同宽度下 Grok/DSH 的 block 几何、软换行、sticky、scrollbar、follow 状态一致；
- partial→final、replay→live、surface replacement、stale generation fixture 全绿；
- tool/diff/reasoning/unknown/image 的 semantic snapshot 和完整复制正确；
- 上下滚、PageUp/PageDown、跳转、搜索、resize 后 anchor 稳定；
- p95 render/内存指标不劣于 baseline，且无每 tick 全量 clone。

**出口**：长对话主屏幕使用 Grok scrollback/layout/block engine，DSH 只提供
结构化内容和 identity；当前自建 scrollback 不再承载新的 parity 功能。

### M5：Prompt、输入和提交语义

**目标**：让 prompt 的视觉和操作达到 Grok 原生体验，同时准确表达 DSH 的
queue/steer/submit 能力。

**任务**

- M5.1 以 Grok line editor/textarea 为唯一 editor；补齐 cursor、selection、
  multiline、viewport、soft-wrap、Unicode grapheme。
- M5.2 迁移 prompt widget 的 border、placeholder、mode indicator、suggestion/
  completion、shortcut 和 error/status paint。
- M5.3 处理 bracketed paste、newline、超长粘贴、控制字符和 clipboard fallback；
  不把粘贴拆成可能改变语义的按键序列。
- M5.4 设计 prompt local draft 与 host accepted/pending/rejected 的状态机；
  receipt 失败时保留草稿和可重试信息。
- M5.5 明确 Submit、Queue、Steer、Context 等 PromptMode 的映射和 capability；
  提交时不得无条件 trim、丢尾部空格或改写换行。
- M5.6 接入历史/建议/slash（仅在 DSH capability 支持时启用），不复制 Grok
  agent command executor。
- M5.7 接入 Ctrl-X 外部 editor（若产品需要）和 Ctrl-P 外部 pager，所有外部
  进程返回都走 terminal restore/re-enter。
- M5.8 增加空 prompt、只空白、二进制/超长文本、断线提交、重复 Enter 的测试。

**门禁**

- editor unit + Grok prompt snapshots；
- prompt 在不同宽度、滚动、粘贴、鼠标选择下 cursor/viewport 正确；
- submit 不重复、不丢内容，receipt 与 notification 收敛；
- unsupported mode 显示明确 capability feedback；
- 外部进程和异常路径 terminal 恢复。

**出口**：用户感受到的是 Grok prompt，而不是当前 Paragraph 文本框；DSH
提交模式差异只出现在 intent/effect 和明确的状态文案。

### M6：Session Picker、Dashboard、Attach/Detach 和 Workspace 投影

**目标**：把 Grok picker/list/dashboard 的前端体验接到 DSH roster 和
control plane，处理异步切换的身份安全。

**任务**

- M6.1 定义 DshSessionRow、workspace/parent/job/subagent row、last activity、
  running/status/diagnostic 和 capability 字段。
- M6.2 迁移 Grok picker/session_picker/list pane 的查询、过滤、tab、selection、
  empty/loading/error、shortcut 和 modal geometry。
- M6.3 picker row 必须携带 stable session_id；刷新和过滤不得以数组位置
  恢复 selection。
- M6.4 把 Selected 编译成 AttachSession {session_id, request_id}，经过
  load barrier；旧 session 的通知不能覆盖新 session。
- M6.5 实现 attach pending、load error、session gone、back/detach 和恢复原
  picker scroll/query 的状态机。
- M6.6 迁移 dashboard pane、peek/back、roster grouping、workspace hierarchy；
  DSH 只投影数据，不自建 table/focus/selection engine。
- M6.7 接入多 session、子 agent/job、workspace action 的 capability gating；
  未实现动作显示 Grok 风格 disabled/diagnostic。
- M6.8 用 mock Harness 做 A→B→A 快速切换、并发 refresh、旧 generation 注入。

**门禁**

- picker/dashboard semantic snapshots（宽屏、窄屏、空/加载/错误）；
- attach/back/reconnect identity fixture；
- selected row stable ID 在 refresh/filter 后仍正确；
- two-session PTY 场景无串线、无旧 transcript 闪回；
- real backend 至少完成 list→attach→display→detach。

**出口**：session 切换是 Grok picker/dashboard 的完整体验，DSH load barrier
和 control plane 是唯一真源；不再只关闭 picker 而不 attach。

### M7：Queue、Approval/Question、Task、Timeline 和 Status

**目标**：补齐主路径以外但决定“像不像 Grok”的交互面，并让异步 authority
与视觉 pending 状态一致。

**任务**

- M7.1 迁移 queue pane 的分组、选中展开、多行/rich content、编辑 cursor、
  reorder、delete、steer/context 行为。
- M7.2 为 queue mutation 搭建 queue_revision/item ID/operation key；local
  pending 只能显示 intent，最终顺序必须由 authoritative snapshot 确认。
- M7.3 迁移 permission/question modal、选项焦点、自由文本回答、取消、过期和
  stale response；绑定 interaction_id + generation。
- M7.4 迁移 tasks pane、agent status、progress、running/error/completed 状态；
  DSH jobs/subagents 只提供 projection。
- M7.5 迁移 timeline/turn status、jump、sticky indicator 和 shortcut bar，
  与 M4 scrollback 同用 entry/turn geometry。
- M7.6 统一 status line/bar 的 connection、model、queue、diagnostic、pending
  receipt、capability blocked 文案和颜色语义。
- M7.7 加入 conflict/retry/timeout/permission denied/error modal 的完整路径；
  不以“accepted=true”代替真实收敛。
- M7.8 将旧 queue_view.rs、block_viewer.rs、dashboard.rs 的行为测试
  转成 Grok-derived component/adapter tests 后，标记旧实现可删除。

**门禁**

- queue revision conflict 和 stale interaction fixture；
- approval/question key/mouse/modal focus snapshots；
- task/timeline/status 在 stream/reconnect/error 下正确；
- queue edit 不重复 RPC，最终 UI 与 host snapshot 收敛；
- Grok visual hierarchy、dim layer、selected/hovered/pending 样式一致。

**出口**：所有主要异步交互都有 Grok UI 状态机和 DSH authoritative effect，
而不是散落在 runtime 的布尔值和临时字符串中。

### M8：鼠标、选择、复制、链接、媒体和高级终端能力

**目标**：完成长对话中最容易被“看起来能用”掩盖的高密度前端行为。

**任务**

- M8.1 迁移 Grok hit map：entry/block/line/column/row/overlay 的命中矩形
  由 render-time geometry 统一产出。
- M8.2 迁移 ResolvedSelectionModel、grapheme boundary、跨行/跨 block 拖选、
  edge autoscroll、selection highlight 和 copy reconstruction。
- M8.3 统一 OSC52/system clipboard fallback、复制完整 block 与当前可见片段
  的语义；能力不可用时显示明确反馈。
- M8.4 接入 link hover/open、tool result link、file search hit；不把 URL/path
  解析复制到 DSH host。
- M8.5 接入 Kitty/inline image 或安全 placeholder；capability、尺寸、生命周期
  和清理策略必须是显式 contract。
- M8.6 迁移 table/bidi geometry、宽字符、组合字符和 RTL 命中；不以 byte offset
  直接当 terminal column。
- M8.7 完成 dashboard/picker/queue/modal 的 mouse routing、wheel capture、
  drag threshold 和 click-vs-drag 规则。
- M8.8 为每个 hit target 写 geometry fixture，resize 后重新计算并验证旧 hit
  map 不会误触发。

**门禁**

- mouse/selection/property tests；
- Unicode、CJK、emoji、RTL/table fixtures；
- PTY 拖选→复制→外部读取场景；
- OSC52、无 clipboard、无鼠标、无 image capability 的 fallback；
- 与 Grok reference 的 hover/focus/highlight semantic snapshots。

**出口**：鼠标和选择不是附加功能，而是与 Grok 同源的 L4 几何/状态机；所有
高级能力都有可验证的 capability fallback。

### M9：通知、重连、并发、取消和性能

**目标**：在 UI parity 之上保证真实 Harness 长时间运行不丢事件、不串 session、
不阻塞输入且可恢复。

**任务**

- M9.1 把 notification drain、tick、render、effect completion 组织成有界
  scheduler；每个 tick 记录处理数量和延迟。
- M9.2 迁移 load barrier/replay-live stitching；attach 期间缓存和排序通知，
  完成 barrier 后一次性提交可见 snapshot。
- M9.3 所有异步回调检查 session/request/generation；stale/gone 进入 diagnostics。
- M9.4 实现 reconnect backoff、retry/cancel、transport timeout、reader thread
  shutdown 和 terminal feedback。
- M9.5 对 queue/interaction/attach 等 effect 定义幂等、取消和 duplicate semantics。
- M9.6 测量长历史内存、render p50/p95、notification backlog、resize latency、
  selection copy time；为 regression 设门槛。
- M9.7 处理 backend 子进程退出、protocol version mismatch、malformed frame、
  writer error 和 partial stream EOF。
- M9.8 运行 soak test：双 session、持续 stream、频繁 resize/scroll/picker/
  reconnect，验证资源稳定。

**门禁**

- replay/live/stale/gone/duplicate/timeout fixture；
- reconnect PTY 场景和 terminal restore；
- scheduler 无无界队列、UI 输入不被单个 RPC 阻塞；
- 性能指标在基线阈值内；
- cargo test --workspace --locked、clippy、协议/PTY/真实 backend 全绿。

**出口**：Grok shell 的异步视觉状态与 DSH 的连接/代际状态稳定收敛，长时间
运行不需要重启 pager。

### M10：Grok parity、真实后端和发布验收

**目标**：把“实现了很多功能”转成可重复、可审计的 parity 证据。

**任务**

- M10.1 建立 Grok reference runner：同一 fixture、同一终端尺寸、同一事件序列
  输出 semantic screen buffer、geometry、focus、state trace。
- M10.2 建立尺寸矩阵：40×12、60×20、80×24、100×30、120×40、160×50；
  覆盖宽屏/窄屏断点和高内容/低内容。
- M10.3 建立状态矩阵：empty、loading、running、streaming、completed、error、
  reconnecting、modal、picker、queue edit、selection、dashboard peek。
- M10.4 建立输入矩阵：key repeat、Ctrl/Alt/Shift、mouse wheel/click/drag、
  bracketed paste、resize storm、Ctrl-C/Esc/back。
- M10.5 snapshot 断言稳定文字、cell style role、rect、cursor、focus owner、
  scroll anchor、hit map；ANSI 控制序列只在 terminal integration 测试断言。
- M10.6 对 mock 与真实 DeepSeek Harness 各跑完整主路径：hello→list/dashboard
  →attach→stream→prompt→queue/interaction→reconnect→detach→restore。
- M10.7 做一次 source/license/dependency 审计，确认没有误引 Grok runtime、
  许可证误标或未登记 vendor 修改。
- M10.8 记录已知 parity 差异，每一项有 capability、优先级、owner 和下一版本
  计划；没有记录的差异不能被默认为“可接受”。

**门禁**

- parity matrix 全部 critical 场景通过；
- real backend smoke 和 soak 通过；
- 所有 workspace checks、source checker、PTY golden、license audit 通过；
- 文档、迁移记录和 fallback 删除计划齐全。

**出口**：能够向评审提供可复现的 Grok reference 对照证据，而不是口头承诺
“样式已经一样”。

### M11：清理、稳定化和其他 Harness 复用

**目标**：完成迁移闭环，删除过渡实现，并把 host seam 稳定成可复用产品接口。

**任务**

- M11.1 关闭旧 UI fallback 的默认/显式入口，先在一个 release cycle 保留
  可诊断 feature gate，再删除代码。
- M11.2 删除重复 Theme、scrollback、selection、picker、queue、modal、block
  viewer、event loop 和无用依赖；每次删除都由 parity test 保护。
- M11.3 将 Grok-derived UI 对外暴露为稳定的 host trait/adapter API；文档化
  DshPresentationModel、Intent/Effect/Receipt、capability 和 identity。
- M11.4 完成 source upgrade playbook：上游 diff、manifest/hash、license、
  upstream tests、DSH parity、回滚。
- M11.5 评估接入其他兼容 harness（例如 Codex CLI）时，只实现新的 host
  adapter，不复制 Grok UI 或另造 runtime。
- M11.6 发布前做 clean checkout、最小权限、无真实凭据、终端恢复和依赖
  license 检查。

**出口**：Grok UI 是唯一前端生产实现，DSH host 是可替换真源适配器，旧平行
实现已删除或有明确保留理由。

---

## 8. DSH Host 后端工作流（与 UI 里程碑配套）

这些工作是 UI 的数据/副作用基础，不能由 UI 临时模拟。

| 编号 | Host 工作 | 主要输出 | 依赖 UI 里程碑 | 关键门禁 |
|---|---|---|---|---|
| H1 | protocol/version/hello/能力协商 | typed DTO、version gate、fixture | M1 | wire contract、unknown capability |
| H2 | session load barrier、replay/live stitching | session snapshot、generation、stale/gone | M1/M6/M9 | attach race、duplicate/ordering |
| H3 | control plane/roster/dashboard | session/workspace/parent/job projection | M6 | list/filter/attach/back |
| H4 | queue authority | item ID、revision、mutation receipt | M1/M7 | conflict/retry/authoritative convergence |
| H5 | approval/question/permission | interaction ID、expiry、response receipt | M1/M7 | stale response、timeout、cancel |
| H6 | subagent/jobs/tasks | task lifecycle、progress、diagnostic | M7 | stream/reconnect/status projection |
| H7 | workspace/peek/rename/fork/archive | typed action and capability | M6/M7 | unsupported action feedback |
| H8 | SharedAuto/connect-or-spawn/daemon lifecycle | identity digest、owner、shutdown | M9/M10 | no orphan process, reconnect |
| H9 | worktree/rewind/media/advanced operations | contract-blocked typed effects | M8/M11 | capability gate, no fake success |

每一项 H 工作必须先补 host contract 和 fixture，再让 Grok view 暴露入口。
如果 Harness 暂未支持某动作，UI 应显示 disabled/unsupported，而不是在本地
修改 transcript 或 queue 伪造结果。

### 8.1 旧 M2 开发记录的回收映射

旧仓库的 M2 记录仍然是 backend 需求来源，但它们必须按当前 successor 的
contract 和 parity 门禁重新验收。对应关系如下：

| 旧记录 | 新计划映射 | 说明 |
|---|---|---|
| M2-0 dashboard | H3 + M6 | roster/dashboard 数据由 DSH 提供，pane/layout/focus 由 Grok 复用 |
| M2-1 control plane | H2/H3 + M1 | snapshot、generation、load barrier 先稳定，再接 picker |
| M2-2 live roster | H3 + M6.1–M6.3 | stable session ID、filter、running/status projection |
| M2-3 peek/back | M6.4–M6.6 | attach、detach、back stack 和旧通知隔离 |
| M2-4 reply/dispatch/actions | H5 + M7 | interaction response、Intent/Effect、receipt 和 modal |
| M2-5 subagents/jobs | H6 + M7.4 | task/status/timeline projection，不带入 Grok agent loop |
| M2-6 workspace | H7 + M6.6 | workspace hierarchy/action 是 DSH 数据，视觉组件仍用 Grok primitives |
| M2-7 SharedAuto | H8 + M9 | connect-or-spawn、identity digest、daemon owner 和恢复 |
| M2-8 worktree/rewind/media | H9 + M8/M11 | 只有协议支持且有 capability 时开放，不做本地伪实现 |

---

## 9. 详细工作包清单

以下工作包可作为 issue/任务卡标题。完成一个工作包时，必须链接到对应里程碑、
source map、测试和删除/回滚出口。

### A. 来源和工程治理

- A01：source manifest hash checker；
- A02：vendor clippy wrapper policy；
- A03：Grok snapshot/reference fixture importer；
- A04：license/NOTICE/dependency audit；
- A05：旧 fallback feature gate 和删除追踪；
- A06：parity result 格式（screen/geometry/state trace）。

### B. Contract 和 projection

- B01：identity newtypes；
- B02：rich render content contract；
- B03：host snapshot 分区；
- B04：UiIntent enum 和 reducer；
- B05：UiEffect/Receipt/OperationKey；
- B06：capability matrix；
- B07：replay/live/stale fixtures；
- B08：queue/interaction authority fixtures；
- B09：host adapter determinism tests。

### C. Grok rendering

- C01：TerminalSurface/draw frame；
- C02：Theme/Appearance consolidation；
- C03：wrap/width/scrollbar/sticky；
- C04：cursor/zero-byte/OSC/link；
- C05：resize invalidation；
- C06：media/bidi/table capability；
- C07：semantic screen buffer utilities。

### D. Grok shell and interaction

- D01：AppView/AgentView module port；
- D02：KeyOwner/focus stack；
- D03：Esc ladder/modal stack；
- D04：event loop/tick/notification scheduler；
- D05：mouse routing/hit map；
- D06：prompt widget；
- D07：queue/picker/dashboard；
- D08：status/task/timeline；
- D09：permission/question/modal；
- D10：selection/copy/viewer。

### E. Host lifecycle

- E01：attach/detach/load barrier；
- E02：generation/stale/gone；
- E03：reconnect/backoff/cancel；
- E04：queue mutation authority；
- E05：interaction response；
- E06：jobs/subagents/workspace；
- E07：SharedAuto/daemon；
- E08：protocol blocked capabilities。

### F. 验证和发布

- F01：unit/property fixture；
- F02：adapter integration；
- F03：mock PTY scenarios；
- F04：real backend scenarios；
- F05：visual/golden matrix；
- F06：performance/soak；
- F07：source/license audit；
- F08：fallback removal and release checklist。

---

## 10. 测试和验收矩阵

### 10.1 测试层级

| 层级 | 测试对象 | 必须验证 |
|---|---|---|
| T0 静态/来源 | fmt、check、clippy、manifest、license、依赖树 | 来源未漂移、无禁用 runtime、代码可编译 |
| T1 Grok-derived unit | 上游 editor、layout、geometry、view state、snapshot | 迁移没有改变 Grok 算法和状态机 |
| T2 DSH contract | protocol、projection、identity、receipt、capability | 真源/代际/权威/错误语义 |
| T3 adapter integration | SessionState→snapshot、Intent→Effect、notification→reducer | 单向边界、确定性、无富内容丢失 |
| T4 semantic render | buffer cells、style role、rect、cursor、focus、anchor、hit map | 不依赖 ANSI 的视觉/几何 parity |
| T5 PTY integration | terminal bytes、mouse/paste/resize、restore、外部进程 | 真实终端生命周期和输入能力 |
| T6 end-to-end | mock/real Harness 主路径 | UI、host、协议、重连整体收敛 |
| T7 property/soak | 随机事件、长历史、并发 session、resize storm | invariant、无 panic、无串线/无资源泄漏 |

### 10.2 必备场景编号

| 场景 | 输入/状态 | 验收 |
|---|---|---|
| P01 | 空 session，80×24 | Grok empty/welcome geometry、prompt focus |
| P02 | user→assistant 完成 | block role、spacing、status、scroll bottom |
| P03 | assistant partial→final | 同一 entry identity、streaming indicator、anchor |
| P04 | tool call→result→diff | structured card、折行、复制、viewer |
| P05 | 1k/10k entries | virtualized paint window、sticky、p95 render |
| P06 | PageUp/Down、wheel、jump | scroll direction、anchor、timeline 同源 |
| P07 | prompt multiline/paste | cursor、viewport、原文保留、submit once |
| P08 | picker filter/refresh | stable session ID、selection 不漂移 |
| P09 | attach A→B→A | load barrier、generation、back state |
| P10 | queue edit/reorder/delete/steer | revision/receipt/conflict convergence |
| P11 | approval/question | modal focus、response identity、stale/expiry |
| P12 | mouse drag/copy | grapheme/column/hit map/OSC52 |
| P13 | resize storm | invalidation、wrap、cursor、selection、no stale hit |
| P14 | disconnect/reconnect | status、backoff、replay/live、terminal remains usable |
| P15 | malformed/unsupported RPC | typed diagnostic、无伪成功、可恢复 |
| P16 | Ctrl-C/Esc/external pager/editor | Esc ladder、draft policy、terminal restore |
| P17 | two sessions + subagent/job | no cross-session events、roster/status |
| P18 | capability matrix | mouse/paste/image/bidi unsupported fallback |

### 10.3 Parity 断言的优先级

1. P0：不能错——session identity、generation、effect duplication、terminal
   restore、panic、数据丢失；
2. P1：必须一致——布局 rect、颜色 role、spacing、focus owner、Esc ladder、
   scroll/anchor、prompt cursor、modal z-order；
3. P2：应一致——动画节奏、glyph 细节、hover、shortcut 文案、媒体降级；
4. P3：可记录差异——仅由 DeepSeek 数据模型或终端能力造成且有 capability/
   decision record 的差异。

任何 P0/P1 未通过，不得以 P2/P3 差异为理由宣布里程碑完成。

### 10.4 推荐命令

~~~bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 scripts/check-protocol-fixtures.py
python3 scripts/pty-smoke.py --binary target/debug/dsh-pager
~~~

当某条命令因环境、真实 backend 或上游测试而无法运行时，必须在结果中写明：
命令、失败层级、是否影响本切片出口、临时 workaround 和回归任务；不能只贴
“测试通过/失败”。

---

## 11. 风险登记表

| 风险 | 迹象 | 影响 | 缓解/门禁 |
|---|---|---|---|
| Grok source drift | manifest hash 变化、snapshot 行为变化 | 视觉和行为基准漂移 | 固定 revision、自动 hash、一次性升级流程 |
| 平行 UI 继续膨胀 | 新增 *_view.rs/scroll/selection 状态机 | 永远无法达到 parity、维护双份 bug | reuse review、旧模块冻结、删除出口 |
| adapter 扁平化内容 | DshRenderContent 变 String | tool/diff/image/selection 丢失 | rich DTO contract、结构化 snapshots |
| identity race | 旧 session/旧 generation 内容闪回 | 串 session、错误操作 | newtype、load barrier、stale fixture |
| effect 重复发送 | 重绘/重试触发多次 RPC | 重复 prompt/queue mutation | operation key、idempotency、receipt test |
| RPC 阻塞 UI | 输入卡顿、tick backlog | TUI 不可用 | bounded scheduler、timeout、cancel |
| scroll 几何分叉 | draw 与 hit map 不同源 | 鼠标/选择错误 | render-time hit map、geometry fixture |
| Theme/shim 分叉 | 同一颜色/spacing 多处定义 | 前端不再像 Grok | 单一 Theme/Appearance、删除 shim |
| terminal 泄漏 | raw mode 未恢复、子进程残留 | 用户终端损坏 | RAII/Drop、PTY restore test、soak |
| 真实 backend 与 mock 不同 | mock 通过、真实 notification 顺序不同 | 发布后失效 | real backend smoke、replay/live fixture |
| vendor clippy/测试污染 | 上游测试阻断 workspace gate | 误改 vendor 或放松门禁 | wrapper lint allow、保留上游 hash |
| 性能退化 | 全量 clone、长历史 render 变慢 | 长会话不可用 | paint window、cache metrics、p95 gate |
| 许可证误标 | Apache-derived 被声明为 MIT | 发布合规风险 | NOTICE、依赖审计、release gate |
| 能力伪造 | unsupported action 显示成功 | 数据/用户信任损坏 | typed capability blocked、无 fake success |

---

## 12. 变更决策和评审模板

### 12.1 新增 UI 代码前必须回答

~~~text
职责：
Grok 对应源码和测试：
复用等级（A0/A1/B/C/D）：
为什么不能直接复用：
DSH-neutral seam：
新增 DTO/Intent/Effect：
稳定 identity 和 generation：
将替换/删除的旧模块：
失败测试和 PTY/golden 场景：
回滚方式：
source manifest/license 变化：
~~~

若“Grok 对应源码和测试”为空，必须证明 Grok 没有同职责实现；若只是因为
适配工作量大、当前实现已经存在或需要改文案，不得判为 C。

### 12.2 代码评审门禁

- [ ] UI 组件没有直接调用 host/RPC；
- [ ] 视图没有创建第二个 session/queue/scrollback truth；
- [ ] Grok source path、revision、license、hash 和测试已登记；
- [ ] DSH 富内容、稳定 ID、generation 未丢失；
- [ ] Key/Mouse/Paste/Resize/Tick 的 owner 明确；
- [ ] Esc/back/modal/focus 行为有测试；
- [ ] receipt 和 authoritative snapshot 的收敛路径明确；
- [ ] 不支持能力不是伪成功；
- [ ] semantic snapshot + 必要 PTY 场景已补；
- [ ] fallback 有删除条件，或决策记录说明为什么永久保留；
- [ ] 没有为了过 lint 修改上游 vendor source；
- [ ] 文档、manifest、测试结果和回滚点已同步。

### 12.3 上游升级流程

1. 选择新的 Grok commit，生成 source diff；
2. 复制到临时 vendor snapshot，保留旧 snapshot 可回滚；
3. 更新 source revision/hash/license/NOTICE；
4. 先跑上游 tests，再跑 DSH contract/adapter tests；
5. 跑完整 parity matrix、PTY 和真实 backend；
6. 审核视觉/交互变化是否是有意升级；
7. 合并后更新本计划和变更记录；任何未解释差异都进入风险登记。

---

## 13. 立即执行顺序（下一轮可直接开工）

下面是从当前 baseline 到第一条真正 Grok vertical slice 的最短安全路径：

1. **M0.1**：确认 successor Git 根并提交当前 docs/vendor 基线；
2. **M0.2/M0.5**：加入 source manifest checker，处理 vendor test clippy，
   使 fast gate 可重复；
3. **M0.7**：在 dsh-pager-test-support 建立 empty、assistant、tool、queue、
   interaction、two-session fixture；
4. **M1.1**：落地 identity newtypes 和 operation key；
5. **M1.2/M1.3**：扩展 rich GrokHostSnapshot，禁止 transcript 字符串化；
6. **M1.4/M1.5**：把 UiEffectSink 改成 UiIntent/UiEffect/Receipt 中性边界；
7. **M2.1/M2.2**：统一 TerminalSurface 和 Theme/Appearance，标记并删除重复 shim；
8. **M2.4/M2.6**：先修复软换行高度、bottom scroll、resize invalidation；
9. **M2.7**：接通 Paste/Mouse/Resize 事件，不再只处理 picker mouse；
10. **M3.1**：迁移 KeyOwner、AppView/AgentView skeleton 和相邻 tests；
11. **M3.4/M3.5**：将 runtime loop 改为 Grok dispatch/focus/Esc ladder；
12. **M4.1**：用一个 assistant block + 一个 tool/result block 完成结构化
    vertical slice；**已完成（2026-08-23 vertical slice）**。
13. **M4.3/M4.4**：接入 Grok scrollback layout/paint window/anchor；**已完成迁移
    基线，reference/性能门禁保留**。
14. **M5.1**：让 Grok textarea 成为真实 prompt renderer，去掉 trim/data loss；
    **已完成迁移基线**。
15. **M6.3/M6.4**：完成 picker stable session ID→attach/load barrier/back；
    **已完成 attach/load vertical slice**。
16. **M7.1/M7.3**：完成 queue 和 approval/question 的一条真实 effect 路径；
17. **M10.1/M10.2**：建立 Grok reference 与 80×24/窄屏 semantic golden；
18. **M10.6**：对真实 DeepSeek Harness 跑 hello→attach→stream→prompt→exit；
19. 只有上述 critical slice 全绿后，才继续扩展 dashboard、jobs、media、bidi；
20. 每完成一个切片，回写本文件的状态、证据、遗留差异和下一删除出口。

第一条垂直切片的最低定义是：

~~~text
真实 DSH session
  -> user prompt
  -> assistant streaming block
  -> 一个 tool/result block
  -> Grok AppView/AgentView layout
  -> prompt / status / scroll / picker or modal
  -> key + mouse + resize
  -> effect receipt + authoritative notification
  -> PTY restore
~~~

只完成其中的“显示文字”部分，不算垂直切片完成。

---

## 14. Definition of Done

### 14.1 单个任务完成

- 源码复用等级和 source map 已登记；
- DSH contract delta 已定义；
- 失败测试先于实现存在；
- Grok 相邻测试/snapshot 已保留或有书面省略理由；
- unit、adapter、必要 PTY/golden 和真实/mock backend 通过；
- 没有新增未登记的平行 UI；
- 文档、manifest、风险和回滚点已更新。

### 14.2 一个里程碑完成

- 所有子任务和出口门禁通过；
- P0/P1 parity 无未解释失败；
- fallback 的使用范围和下一删除步骤明确；
- 性能、资源、license、依赖和真实 backend 结果已记录；
- 下一里程碑不需要绕过本节设计契约。

### 14.3 最终项目完成

- Grok Build TUI 的前端样式、布局、交互状态机和终端效果是生产路径；
- DeepSeek Harness 是唯一 session/control-plane/effect 真源；
- dsh-pager-grok-ui 与 host 之间只有稳定、可测试、无 Grok runtime 依赖的
  adapter；
- 所有可复用的 Grok 代码、测试、许可证和来源均可追溯；
- 旧平行实现已删除，或每个保留项都有明确、可复核的非重复职责；
- mock 与真实 Harness、短会话与长会话、宽屏与窄屏、键盘与鼠标、正常与异常
  生命周期均通过验收；
- 换一个兼容 harness 只需实现 host adapter，不需要重写 Grok 前端。

最终判断只有一句话：

> **Grok Build 决定用户如何看到、如何操作；DeepSeek Harness 决定系统真实发生了什么。适配层只负责把两者准确接起来。**
