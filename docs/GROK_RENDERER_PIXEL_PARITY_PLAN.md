# Grok renderer 复用、完整能力迁移与像素级前端对齐方案

> 适用仓库：`/home/leo/code/dsh-pager-grok`
> 上游仓库：`/home/leo/code/grok-build`
> 固定 mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`
> 固定 `SOURCE_REV`：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`

## 1. 方案结论

后续前端迁移不再手写 Grok 的等价 renderer。Grok Build 的视觉实现直接作为
前端 reference 和生产 renderer：布局、主题、Prompt、Scrollback、selection、
status、快捷键、焦点、Markdown、Diff、File Search、Suggestion、Image、
Workspace、Agent/Task/Subagent 和终端降级规则优先复用上游代码。

DSH 仍然是唯一的数据、身份、能力和副作用真源。DSH 负责把 `SessionState`、
`ControlPlaneStore`、搜索结果、workspace/job/subagent 状态、媒体元数据和
capability 投影成 Grok renderer 消费的 DTO，并把 Grok view 产生的 `UiIntent`
转换为 `UiEffect`。新项目必须保留上述完整用户能力，但不把 Grok 的 agent loop、
shell、ACP、RPC、配置、持久化和 telemetry runtime 带进来。

这里的“排除 Grok agent”只表示不复制 Grok 的业务执行引擎；AgentView、agent
消息、运行状态、task、subagent、interrupt 和 streaming 视觉/交互仍然是必须
迁移的 TUI 能力，由 DeepSeek Harness 提供真实状态。

目标不是把 Grok 应用整体复制进 DSH，而是复制 Grok 的视觉实现闭包：

```text
DSH SessionState / ControlPlaneStore
                |
                v
       GrokHostSnapshot / DTO
                |
                v
       GrokRenderAdapter
        |       |        |
        v       v        v
   AgentView  Prompt  Scrollback  Capability surfaces
   Layout     Widget  Blocks      Search/Suggest/Image/Workspace/Agent
        \       |        /
         \      v       /
          ratatui Buffer
                |
                v
              PTY
```

## 2. 当前基线

仓库已经直接复用了以下 Grok 模块：

| 模块 | 当前来源 | 当前状态 |
|---|---|---|
| TextArea / EditBuffer | `crates/dsh-grok-textarea` | Grok-derived workspace crate，生产使用 |
| key / line editor | `vendor/grok/xai-grok-pager/src/input/` | 已 vendor |
| Picker | `vendor/grok/xai-grok-pager/src/views/picker.rs` | 已 vendor |
| ModalWindow | `vendor/grok/xai-grok-pager/src/views/modal_window.rs` | 已 vendor |
| ShortcutsBar | `vendor/grok/xai-grok-pager/src/views/shortcuts_bar.rs` | 已 vendor |
| StatusBar | `vendor/grok/xai-grok-pager/src/views/status_bar.rs` | 已 vendor |
| Timeline | `vendor/grok/xai-grok-pager/src/views/timeline.rs` | 已 vendor |

以下核心视觉模块尚未完整进入生产路径；PromptWidget 的纯 draw core 已先进入
production，其 controller 闭包仍按 P6.1 门禁继续迁移：

| 模块 | 当前替代物 | 对齐目标 |
|---|---|---|
| `PromptWidget` | upstream-derived `views/prompt_widget.rs` draw core + Grok `TextArea` 已用于 production | 接入 file-search/suggestion/history/image controller 闭包并完成 reference cell diff |
| `AgentViewLayout` | DSH 手写 row stack | 直接复用上游 `LayoutConfig`/`ScrollbarConfig`/layout solver |
| 完整 Theme | `dsh-pager-render::Theme` 简化字段 | 引入 Grok 完整语义 token |
| Appearance | DSH 常量和局部参数 | 引入 Grok prompt/scrollback/layout 配置的只读投影 |
| ScrollbackPane/block renderer | `RichTranscript` | 直接复用 Grok scrollback block/layout/render 路径 |
| AgentView render orchestration | `runtime.rs` 手工串联 | 复用 Grok `AgentView::draw` 的绘制顺序 |
| Markdown / Diff renderer | `views/transcript.rs` 的简化文本投影 | 复用 Grok markdown/diff AST、样式、hunk 和复制语义 |
| File Search | 目前只有 line viewer 和 session search 基础 | 迁移 Grok file-search overlay、结果列表、预览和命中几何，并接 DSH file-search contract |
| Suggestion / history | `PromptEditor` + 简化 projection | 迁移 Grok suggestion controller、候选 viewport、slash/history 状态机 |
| Image / media | placeholder + capability shim | 迁移 Grok image block、preview、尺寸约束和显式 fallback |
| Workspace / dashboard | DSH `DashboardModel` + 简化 modal | 复用 Grok workspace/list/tree/focus/peek 组件，DSH 提供 authority |
| Agent / task / subagent | status/task DTO + 简化文案 | 迁移 Grok AgentView/status/task/subagent pane，DSH 提供 lifecycle 和 effect |

因此，当前界面“接近但不一致”的剩余根因不是缺少 ratatui 能力，而是 Scrollback、
完整 AgentView/pane solver 和 capability controller 仍有 DSH 简化 renderer。
Prompt 的 chrome/TextArea/info/cursor 已不再由 `views/agent.rs` 平行实现。

### 2.1 2026-08-23 执行状态

本表只记录能力链路是否闭合，不把它等同于 Grok 像素级 renderer 已迁移：

| 能力 | Host/Effect 状态 | Renderer 状态 | 下一出口 |
|---|---|---|---|
| Prompt core | draft、mode、model/title projection 已接入 | 固定 Grok TextArea + upstream-derived draw core 已用于 production；height/draw/cursor/mouse 共用同一状态 | reference cell oracle；接 controller 闭包 |
| Markdown / Diff | 结构化 block、stable ID、copy/link 已保留 | 仍是 `RichTranscript` 简化实现 | vendor Grok markdown/diff + scrollback block 闭包 |
| File Search | `fileReferences.list` 正式 ApiProxy/TUI 方法；query revision、stable row、unsupported/error 已接入 | overlay 仍是过渡 renderer | vendor Grok file-search controller/list，并接同一 result DTO |
| Suggestion / history | host projection、候选 viewport、accept/dismiss、prompt history 已接入 | 仍是 runtime 简化 banner/controller | vendor Grok suggestion/history 状态机 |
| Image / media | `session.attachment`、1 MiB 有界读取、独立 preview buffer 已接入 | 当前只显示列表和已加载元数据 | vendor Grok image block/preview，按终端能力绘制或 fallback |
| Workspace | list/group/peek/attach、archive、session reorder 已接真实 RPC | 当前是简化 Dashboard modal | vendor Grok workspace/dashboard tree；补 create/rename/delete/workspace reorder |
| Agent / task / subagent | job/subagent catalog、stream status、interrupt effect 已接 | 当前是简化 task surface | vendor Grok AgentView/task/subagent pane 与主绘制顺序 |

已经建立并必须继续沿用的动作边界：

```text
Grok-derived input
    -> UiIntent
    -> UiEffect
    -> UiEffectReceipt
    -> authoritative query/snapshot/notification
    -> renderer DTO
```

其中 File Search 的权威结果来自有 revision 的 unary query response，Media preview
来自有界的 attachment response；两者都只进入 host adapter 的临时 result buffer，
不写入 `SessionState`，也不允许 view 直接调用 RPC。

## 3. 复用边界

### 3.1 必须直接复用的前端代码和能力闭包

第一阶段优先引入以下上游路径：

```text
xai-grok-pager/src/views/prompt_widget/
xai-grok-pager/src/views/agent.rs
xai-grok-pager/src/app/agent_view/mod.rs
xai-grok-pager/src/app/agent_view/render.rs
xai-grok-pager/src/views/agent_status.rs
xai-grok-pager/src/views/turn_status.rs
xai-grok-pager/src/scrollback/
xai-grok-pager/src/scrollback/blocks/
xai-grok-pager/src/scrollback/wrappers/
xai-grok-pager/src/views/file_search/
xai-grok-pager/src/views/suggestion_controller/
xai-grok-pager/src/prompt_images/
xai-grok-pager/src/views/dashboard/
xai-grok-pager/src/views/tasks_pane.rs
xai-grok-pager/src/views/subagent* / app/agent_view/task_status*
xai-grok-pager-render/src/appearance/
xai-grok-pager-render/src/theme/
xai-grok-pager-render/src/glyphs.rs
xai-grok-pager-render/src/render/
```

这些模块共同决定：

- row constraints、outer padding、prompt gap 和 short-terminal 断点；
- prompt 的 divider、accent、prefix、title、info line、cursor 和 unfocused dimming；
- transcript block 的结构、折行、颜色、selection、scrollbar 和 timeline；
- Markdown、code、Diff/hunk、tool/result、reasoning、image 和 unknown block 的
  结构、样式、折行、selection、link/copy 语义；
- file search 的输入、过滤、结果列表、line preview、命中高亮、焦点和 overlay；
- suggestion/history 的候选排序、viewport、光标、accept/dismiss 和 Esc ladder；
- image/media 的 inline/preview/placeholder、尺寸裁剪和 capability fallback；
- workspace/session tree、dashboard、peek/back、job/task/subagent 视觉和焦点；
- status、turn status、spinner、shortcut 和 capability fallback；
- 主题 token、glyph 和终端颜色角色。

不能只复制 `PromptWidget` 或 `agent.rs` 单文件。必须按“纯 renderer + 交互状态
机 + 上游测试”的最小能力闭包 vendor；闭包中确实属于 Markdown、Diff、file
search、suggestion、image、workspace 或 agent UI 的依赖必须保留，只有业务执行
runtime 才在 adapter seam 外重接，否则会出现“功能还在但视觉和行为已经分叉”。

### 3.2 明确不得复制的 Grok 代码

以下模块即使被 renderer 间接引用，也不得以 Grok runtime 形式进入 DSH production
dependency：

- Grok agent loop、tool/shell orchestration；
- ACP client/runtime 和 Grok session model；
- Grok workspace/config/auth/persistence/telemetry runtime（workspace UI、数据
  DTO 和 action 视觉仍然必须保留）；
- Grok RPC transport、foreign session storage 和后台 worker；
- 只为 Grok 产品业务存在的 dispatch/effect 实现。

这些运行时能力仍由 `crates/dsh-pager`、DSH protocol 和 DeepSeek Harness 拥有；
对应的用户可见功能由 Grok-derived UI 完成。

### 3.3 Adapter 是唯一接缝

Grok view 不得 import `SessionState`、`RpcTransport` 或 DSH wire DTO。适配只允许
经过以下 DTO：

```rust
pub struct GrokRenderSnapshot {
    pub session: GrokSessionVisual,
    pub prompt: GrokPromptVisual,
    pub scrollback: GrokScrollbackVisual,
    pub status: GrokStatusVisual,
    pub file_search: GrokFileSearchVisual,
    pub suggestions: GrokSuggestionVisual,
    pub media: GrokMediaVisual,
    pub workspace: GrokWorkspaceVisual,
    pub agent: GrokAgentVisual,
    pub capabilities: TerminalCapabilities,
}
```

DTO 只表达绘制和交互所需事实，不保存 DSH runtime 引用，不发起副作用。

每个 capability surface 都必须区分三种状态：

- `available`：DTO 数据和对应 host effect 都已具备；
- `pending`：host 正在加载或等待 authoritative snapshot；
- `unsupported`：当前 Harness/terminal 没有能力，UI 必须走 Grok fallback 并给出诊断。

不能把“没有数据”当成空列表，也不能把未实现的 effect 伪造成成功。

动作方向保持：

```text
Grok input/state
    -> UiIntent
    -> UiEffect
    -> DSH transport
    -> receipt/notification
    -> SessionState snapshot
    -> next render
```

## 4. 分阶段实施

### P0：源码闭包与 provenance

目标是先确定“直接复用什么”，不改视觉行为。

工作项：

1. 从固定 Grok revision 计算 Prompt、AgentView、Scrollback、Theme、Appearance、
   Markdown、Diff、File Search、Suggestion、Image、Workspace 和 Agent UI 的
   依赖闭包；逐项标记 pure renderer、local state、host DTO 和 host effect。
2. 按 `vendor/grok` 原始目录结构复制，保留 Apache-2.0 `LICENSE/NOTICE`。
3. 更新 `SOURCE_MANIFEST.md`：每个文件记录 upstream path、source hash、vendor
   hash 和本地适配原因。
4. 保留上游 tests；如果 tests 依赖 Grok runtime，则提取纯 renderer/interaction
   fixture，不复制业务执行 harness，但不得删除对应用户能力的行为断言。
5. 为每个能力建立 source map：上游源码、当前 DSH DTO、需要的 UiIntent/UiEffect、
   stable ID/generation、capability fallback 和旧实现删除条件。

完成标准：所有新增视觉文件都可追溯；没有未登记的 vendor 文件；Markdown、Diff、
File Search、Suggestion、Image、Workspace、Agent 的 renderer 闭包都有明确入口；
不引入 Grok agent/shell/runtime。

### P1：完整 Theme、Appearance 和 glyph

目标是消除颜色和 spacing 的系统性偏差。

工作项：

1. 将 Grok `Theme` 的完整语义字段引入 renderer 层。
2. 将 `LayoutConfig`、`ScrollbarConfig`、`PromptStyle` 等配置设为 Grok view
   的输入，而不是散落在 DSH runtime 的常量。
3. DSH 只提供配置快照和 capability；不在 DSH UI 重新定义同职责 Theme。
4. 为颜色建立稳定 role 名称，避免 parity 直接比较终端 RGB 偶然值。

完成标准：同一 Theme 下，reference 与 DSH 的 prompt/status/scrollbar 关键 cell
   颜色角色一致；旧简化 Theme 不再被生产 renderer 使用。

### P2：PromptWidget 直接接线

目标是删除当前自建 Prompt renderer。

工作项：

1. 用上游 `PromptWidget::draw` 替换 `views/agent.rs::render_prompt`。
2. 用 `PromptStyle` 传入 focus、compact、chrome、title、prefix、border 和
   placeholder 规则。
3. 用 `PromptInfo` 传入 model、mode flags、multiline 和 usage/status caption。
4. 继续使用现有 `dsh-grok-textarea`，保证输入编辑和 Grok TextArea 的状态模型
   一致。
5. DSH `PromptEditor` 只负责 host adapter 的 draft/state 投影，不复制 textarea
   绘制逻辑。

完成标准：`40x12`、`80x24`、`120x40` 的 prompt border、title、prefix、info line、
   cursor 和 multiline frame 与 Grok reference 逐 cell 一致。

### P2A：完整能力闭包接线

P2A 与 P2/P3 并行，但它是 TUI 功能完整性的硬门禁，不允许因为主 prompt 或
transcript 已经显示就把其它产品面推迟到“以后再说”。

| 能力 | Grok UI 闭包 | DSH host seam | 最低出口 |
|---|---|---|---|
| Markdown | markdown AST、代码/列表/链接 renderer、wrap/copy | `DshRenderBlock::Markdown` + content revision | block semantic cells、链接和复制一致 |
| Diff | hunk、增删行、路径 header、折叠/复制 | `DshRenderBlock::Diff` + stable entry/block ID | diff geometry、颜色 role、selection 一致 |
| File Search | query editor、结果列表、line viewer、命中高亮、overlay/focus | typed search snapshot + search effect；不得把 `session.search` 冒充 filesystem search | loading/empty/error/result/preview 以及 stale query |
| Suggestion | candidate controller、history/slash、viewport、accept/dismiss | prompt suggestion DTO + capability；副作用仍走 intent/effect | cursor、候选焦点、Esc/Enter 状态机 |
| Image | image block、prompt attachment、preview、尺寸裁剪、placeholder | attachment ID/media metadata/capability；数据由 Harness 提供 | supported/unsupported/missing 三种状态明确 |
| Workspace | tree/list、group、peek/back、archive、reorder、焦点 | workspace/session/job DTO + mutation effect | stable ID、refresh/attach/back race |
| Agent | AgentView、streaming、turn/status、task/subagent、interrupt | session event、job/subagent snapshot + generation | running/complete/error/reconnect/interrupt |

这些能力都必须进入 reference fixture 和真实 backend matrix。没有对应 DSH
authority 的能力只能显示 `pending` 或 `unsupported`，不能用静态假数据伪造完成。

### P3：AgentViewLayout 和主绘制顺序

目标是让所有 pane 的几何来自 Grok solver。

工作项：

1. 使用上游 `AgentViewLayout::compute` 和 `LayoutConfig`。
2. 将 DSH 数据映射到 Grok pane：scrollback、turn status、banner、prompt、status
   line、shortcuts、timeline/scrollbar。
3. 用同一份 layout snapshot 同时驱动绘制、hit map、cursor 和 mouse routing。
4. 删除当前 runtime 的手写 header/prompt/footer 几何回推逻辑。
5. 保留 DSH 的 overlay owner 和 effect reducer，只更换其视觉落点。

P3 的“主绘制顺序”还必须包含 file-search/suggestion/image/workspace/agent 的
overlay 和 pane owner；它不是只替换 header、transcript、prompt 三段 row stack。

完成标准：任何支持的终端尺寸下，pane rect 不重叠、不越界；resize 后 layout、
   hit map、scrollbar 和 cursor 同步失效并重建。

### P4：ScrollbackPane 和结构化 block renderer

目标是使 transcript 从“DSH 自建文本 renderer”变为 Grok 原生 block renderer。

工作项：

1. 将 `DshPresentationModel` 映射为 Grok block DTO，不在 adapter 中压平文本。
2. 保留 user、assistant、reasoning、tool call/result、error、diff、image、
   partial、replacement 和 compaction 等结构。
3. 直接复用 Grok scrollback layout、block render、selection、sticky header、
   scrollbar 和 timeline 逻辑。
4. 为每个 block 保留 DSH 稳定 ID，显示顺序和动作目标不能依赖数组索引。
5. DSH 的 replay/live/generation 逻辑继续在 host 层完成，renderer 只消费当前
   snapshot。

完成标准：同一结构化 fixture 下，Grok reference 与 DSH 的 block glyph、颜色、
   wrapping、selection geometry 和 scrollbar 语义一致；partial replacement 不
   产生重复 block。

P4 的 block 闭包至少包括 Markdown、Reasoning、ToolCall、ToolResult、Diff、Image、
Subagent/Agent status 和 Unknown。File Search 预览中的代码/文本行必须复用同一
width/grapheme/hit-map 规则，不能另造 line viewer 几何。

### P5：runtime 收敛和旧 renderer 删除

目标是生产路径只保留一套 Grok 视觉实现。

删除或降级为测试 oracle：

- ~~当前 `views/agent.rs` 自建 Prompt chrome~~（已删除；production/parity 共用
  `views/prompt_widget.rs::GrokPromptRenderer`）；
- 当前 `RichTranscript` 生产绘制路径；
- runtime 中重复的 status/header 文案拼接和尺寸计算；
- 与 Grok 等价的 DSH Theme、spacing、glyph 和 scrollbar 算法。
- 只支持“主会话能显示”的 feature fallback；在 file search、suggestion、image、
  workspace 或 agent surface 仍缺失时，不得关闭对应旧路径。

保留：

- `GrokHostSnapshot` 和 DSH host adapter；
- `UiIntent -> UiEffect -> receipt/notification`；
- `AppShell` 的 DSH-neutral focus/overlay reducer，直到 Grok focus 状态机完全接管；
- parity/reference 工具和 DSH 协议测试。

完成标准：生产 runtime 不再存在第二套同职责 renderer；所有临时 fallback 都有
删除记录或明确保留理由。

### P6：执行顺序（当前）

能力 seam 已经具备后，不再继续扩展 `runtime.rs` 的平行视觉实现。后续按以下顺序
推进，每一项都必须同时迁移上游 renderer、上游交互测试和 DSH adapter fixture：

1. `PromptWidget` + `AgentViewLayout`，替换手写 prompt/header/footer 几何；
2. `ScrollbackPane` + Markdown/Diff/Image block，替换 `RichTranscript` 生产路径；
3. File Search + Suggestion/history controller，复用同一 prompt/focus/Esc ladder；
4. Workspace/Dashboard + Agent/Task/Subagent pane，替换当前简化 modal；
5. reference runner、semantic cell diff、PTY/backend matrix 全通过后删除 frozen
   `runtime.rs` fallback shell。

门禁：前四项任一能力仍依赖简化 renderer 时，不得执行第 5 项；也不得以“真实
RPC 已接通”为理由把对应 Grok renderer 闭包标记为完成。

P6.1 进一步拆成以下可审计门禁，避免把“字段 contract 已存在”误报成 renderer
迁移完成：

1. 固定上游 `PromptStyle`、`PromptInfo`、height 和 chrome rect split 的
   DSH-neutral contract；
2. 将 `PromptEditor` 收敛到现有 Grok `TextArea`，由同一个 TextArea 负责 wrap、
   selection、cursor、mouse 和 viewport；
3. vendor/抽取固定上游 draw core，替换
   `views/agent.rs::render_prompt_buffer`，同时迁移 semantic cell tests；
4. 迁移完整 `AgentViewLayoutParams`（tasks/catalog/todo/queue/btw/banner/CTA/
   follow-ups/voice/status/scrollbar/timeline），让绘制与 hit map 共用一个 snapshot；
5. File Search、Suggestion/history 和 Image controller 接入后，P6.1 才可标记完成。

当前第 1-4 项完成：`PromptEditor` 已由 Grok `TextArea` 持有编辑/selection/
viewport/cursor/mouse 状态，production 与 semantic runner 共用 upstream-derived
`GrokPromptRenderer`，`views/agent.rs::render_prompt_buffer` 和 `PromptViewport` 已
删除；完整 `LayoutConfig`/`ScrollbarConfig`、`AgentViewLayoutParams`、pane solver、
`rows_available_for_prompt`、scrollbar/timeline carve-out 和 `PaneAreas` 已接入，
runtime/parity 的主 pane、prompt、hit map、cursor 使用同一布局快照。第 5 项仍未
完成，因此这里只声明 P6.1 第 4 门禁收敛，不把 P6.1、完整 TUI 或像素级 Renderer
标记为完成。

### 3.3 P6.1 第 4 门禁实际结果（2026-08-23）

- 固定上游 `AgentViewLayout::compute` 的 pane 顺序已迁移：status bar、tasks、
  catalog、todo、scrollback、btw、queue、turn status、banner、CTA、follow-ups、
  prompt、voice、status line、shortcuts。
- 只有 scrollback 使用 `Min(5)`；optional pane 高度为 0 时其 gap 同时省略；短
  终端抑制 CTA/follow-ups 并取消底部 padding；status line 自我 clamp。
- `rows_available_for_prompt` 先 probe 再 clamp；scrollbar/timeline 共用 gutter，
  `scrollback_content` 不会越过 rail/track；`PaneAreas::hit_test` 保持上游优先级。
- runtime 的 tasks/catalog/queue 高度只来自当前 `GrokHostSnapshot`；subagent catalog
  的异步列表先经 host effect reducer 合并到单帧 snapshot。没有 authoritative DTO 的
  todo、btw、CTA、follow-ups、voice 仍显式为 0，没有伪造内容。
- 保留现有 File Search、Suggestion/history、Image、Workspace、Agent task overlay
  和对应 UiIntent/UiEffect/receipt 边界；本门禁没有删除或重写这些能力。

第 5 门禁仍需迁移 File Search、Suggestion/history、Image controller；随后还要迁移
Scrollback Markdown/Diff/Image block renderer、Workspace dashboard 和专用 Agent/Task/
Subagent pane renderer，才能继续关闭完整 TUI 门禁。

## 4A. 能力状态和 host contract

每个 feature snapshot 都必须有稳定的 `status`：

```rust
pub enum FeatureStatus {
    Available,
    Pending,
    Unsupported,
}
```

规则：

1. `Available` 只能来自 DSH snapshot/capability 和可执行 effect 的共同确认；
2. `Pending` 表示正在加载、等待 attach barrier、搜索结果或媒体内容，不代表成功；
3. `Unsupported` 必须落到 Grok 定义的 fallback/diagnostic；不得静默隐藏入口；
4. 所有动作目标使用 stable ID，并携带 session/request/generation；
5. view 不得直接读取 `SessionState`、解析 RPC JSON 或调用 loader；
6. UI 功能可以完整迁移，Grok 业务执行 runtime 不得迁移。

目标 snapshot 分区：

```text
GrokHostSnapshot
  ├─ session / prompt / scrollback / status
  ├─ file_search
  ├─ suggestions
  ├─ media
  ├─ workspace
  └─ agent (tasks / subagents / interactions)
```

## 5. 像素级 parity 方案

### 5.1 Reference 来源

reference 必须来自固定 Grok source snapshot 和相同输入 fixture，不依赖运行完整
Grok agent。为此建立纯 renderer harness：

```text
Grok fixture DTO + terminal area
              -> Grok renderer
              -> ratatui Buffer
```

DSH adapter 使用同一个 fixture DTO 形状生成另一份 Buffer。两份 Buffer 只比较稳定
   的视觉语义，不比较 ANSI 控制序列。

### 5.2 Cell signature

每个 cell 至少记录：

```rust
pub struct SemanticCell {
    pub x: u16,
    pub y: u16,
    pub symbol: String,
    pub fg_role: ColorRole,
    pub bg_role: ColorRole,
    pub modifiers: u16,
}
```

每个 frame 还必须记录：

- terminal width/height；
- component rects；
- cursor position/visibility；
- focus owner；
- overlay owner；
- hit map regions；
- dynamic cells mask（仅 spinner、clock、animation 等明确动态区域）。

### 5.3 必测尺寸和状态

尺寸：

- `40x12`
- `80x24`
- `120x40`
- 至少一个极窄宽度和一个超宽高度回归样本。

状态：

- empty/unfocused prompt；
- focused prompt；
- multiline prompt；
- streaming/running；
- completed/error/reconnecting；
- queue、picker、modal、interaction；
- selection、hover、scrollbar/timeline；
- capability fallback（mouse、paste、OSC52、image）。

### 5.4 差异分类

Parity failure 必须按以下类别报告，不允许只给截图：

| 类别 | 示例 |
|---|---|
| Geometry | rect、padding、gap、scrollbar x、cursor y |
| Glyph | `❯`、divider、spinner、bullet、ellipsis |
| Color role | border active、accent、dim、selection、background |
| Modifier | bold、dim、italic、underline、reverse |
| State | focus owner、overlay、selected、running、multiline |
| Content | title、model、mode flag、block text |
| Dynamic | spinner/tick/animation，仅允许在 mask 内差异 |

## 6. 验收门禁

每个阶段都必须通过：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 scripts/check-protocol-fixtures.py
python3 scripts/check-source-manifest.py
python3 scripts/parity-matrix.py
scripts/e2e.sh
git diff --check
```

P2 以后增加：

```bash
cargo test -p dsh-pager-grok-ui semantic_cell
python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full
```

对于每个上游 renderer 文件：

1. 上游 hash 与 vendor hash 可审计；
2. 本地修改有明确注释和 manifest 记录；
3. 至少有一个 reference fixture 和一个 DSH adapter fixture；
4. 失败报告包含坐标、reference cell、actual cell 和所属 component；
5. 不得用修改 reference 或放宽全局比较规则来掩盖差异。

## 7. 风险与控制

### 依赖闭包过大

按 renderer 责任拆包，先引入 Prompt/Theme/Layout，再引入 Scrollback；任何只为
agent 业务存在的依赖都在 vendor 边界外解决。

### DSH 数据模型与 Grok block 模型不同

新增 DSH-neutral block DTO，不把 DSH 类型直接塞进 Grok view，也不把 Grok session
类型带回 host。缺失字段显示 Grok 定义的 fallback，而不是重新设计外观。

### 上游升级导致视觉漂移

固定 source snapshot；升级必须单独建立记录，更新 manifest/hash，重新运行完整
parity matrix，并审核所有 golden 变化。

### 迁移期双 renderer 并存

旧 renderer 只能作为 behavior oracle 或显式 fallback。每个 fallback 必须有删除
条件、负责人和测试门槛；P5 完成后不能继续从旧路径添加功能。

## 8. 预期结果

完成本方案后，用户可见的 Grok Build 前端不再由 DSH 自己“仿写”，而是由固定
Grok renderer 直接绘制；DSH 只改变数据和副作用，不改变视觉组件的几何和语义。

这才是可验证的像素级对齐：当 DSH 和 Grok 输入相同的 render DTO、终端尺寸和
能力集合时，除明确标记的动态 cell 外，Buffer 的 glyph、颜色角色、modifier、
cursor 和 rect 都应一致。

本方案不宣称迁移已经完成；它是下一阶段源码复用和 renderer 接线的执行基线。
