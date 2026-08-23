# Grok 消息分类、折叠与流式终结：DSH 两个体验问题修复方案

本文回答两个问题：

1. Grok Build 如何区分用户消息、agent 回复、thinking、tool、系统/上下文和会话状态，以及每类消息由哪些组件负责展示、折叠和分组？
2. 结合这套语义，DSH 当前“系统提示词和 agent 注入内容全部露出”和“第一个流式消息卡住”的问题，应该怎样落地修复？

本文是 `/home/leo/code/dsh-pager-grok` 的 bug tranche 设计，不在本批次直接改 Rust。源码调研基于 Grok Build 镜像：

- Grok mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`
- Grok source revision：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`
- DSH 适配目标：`docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md`
- 像素/布局目标：`docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`

## 结论先行

这两个问题应该先修，再继续扩大 M4/M9 的 renderer parity 范围。但两个现有计划不会自动解决它们：

- `GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md` 已经把 M4.2（稳定 ID/lineage/partial replacement）、M4.6（running block/streaming/compaction/retry）、M9.7（error/EOF/reconnect）列为门禁，却没有把“注入上下文默认不可见/可折叠”写成明确的展示契约，也没有指定 `turn/end`、stream error、EOF 如何终结 partial。
- `GROK_RENDERER_PIXEL_PARITY_PLAN.md` 的 N2 只规定了 `ScrollbackPane`、结构化 block 和 Markdown/Diff/Image/Reasoning/ToolCall/ToolResult 等视觉目标；它依赖一个已经正确分类且已经终结的 presentation model，不能替代分类器和流状态机。

因此本方案先建立两条不变量：

1. **可见性不变量**：原始 history 永不删除；系统指令、agent-instructions、plugin 注入等内容不能被适配器当成普通 `User/Context` 可见行直接绘制，而必须进入 `SystemInstruction`/`AgentContext` 的隐藏或折叠投影。用户真实输入、agent 最终答案、tool 生命周期和错误状态分别进入不同的 block/组件。
2. **流终结不变量**：每个 `(turn, step)` 的 streaming surface 只能有一个稳定身份；delta 更新同一 surface；正常 final、turn end、abort、provider error、stream EOF 都必须把它转为 `Completed/Interrupted/Failed/Eof` 之一，不能留下永久 `partial=true/running=true` 的幽灵行。

预期用户体验是：提交一条消息后看到清晰的 You/Assistant/Tool/状态层级；系统提示词和 agent 内部注入内容默认不占用 transcript 的主体空间，必要时可通过一个折叠摘要展开；agent 首个 token 连续流入同一条回答 block，结束、错误或断流时都有确定的完成/中断提示。

## 一、Grok Build 的真实展示模型

### 1. `RenderBlock` 是消息类型的真相，不是通用字符串行

Grok 的核心枚举在 `crates/codegen/xai-grok-pager/src/scrollback/block.rs:366-397`。当前源码中的 canonical block 至少包括：

| 语义 | Grok block | 主要组件/文件 | 视图职责 |
|---|---|---|---|
| 用户真实输入 | `UserPrompt` | `scrollback/blocks/user.rs`、`UserPromptBlock` | 用户气泡/前缀、提示词折叠、粘性 prompt header |
| agent 最终/流式文本 | `AgentMessage` | `scrollback/blocks/agent.rs`、`AgentMessageBlock`、`MarkdownContent` | Markdown、代码、图片/mermaid；流式追加到同一 block |
| thinking/reasoning | `Thinking` | `scrollback/blocks/thinking.rs`、`ThinkingBlock` | 运行计时、截断/展开/折叠、`show_thinking_blocks` 开关 |
| 工具调用 | `ToolCall` | `scrollback/blocks/tool/mod.rs` 及 `execute/read/edit/search/list/web_*` | 工具名、参数、diff、输出预览、成功/失败和运行态 |
| 系统状态 | `System` | `scrollback/blocks/system.rs`、`SystemMessageBlock` | 紧凑 muted 文本；不是用户消息，也不是 agent 正文 |
| 会话事件 | `SessionEvent` | `scrollback/blocks/session_event.rs` | turn 完成、重试、认证、上下文 recap 等会话级状态 |
| 后台任务 | `BgTask` | `scrollback/blocks/bg_task.rs` | 后台任务进度/完成态 |
| 子 agent | `Subagent` | `scrollback/blocks/subagent.rs` | 单行运行/完成/失败/取消摘要，可加入工具 verb group |
| 工作流/旁注 | `Workflow`、`Btw` | `scrollback/blocks/workflow.rs`、`btw.rs` | 工作流和中途用户交互的独立 surface |
| 上下文信息 | `ContextInfo` | `scrollback/blocks/context_info.rs` | `/context` 的结构化快照，不是原始 system prompt 回显 |
| 额度/限制 | `CreditLimit` | `scrollback/blocks/credit_limit.rs` | 紧凑配额提示 |

`BlockContent` trait（`scrollback/block.rs:46-187`）要求每类 block 自己声明：

- `output(ctx)`：内容和行级样式；
- `is_foldable`、`next_fold_mode`、`collapse_mode`、`default_display_mode`、`finished_display_mode`：折叠和运行结束策略；
- `is_selectable`、`has_bullet`、`is_groupable`：导航、bullet 和分组行为；
- `accent`、`background`、`has_vpad`：视觉边界和空间。

这意味着 Grok 的“分类”不是先输出一条统一 row 再猜颜色，而是先选择一个 block 类型，再由 block 和 wrapper 决定内容、状态和交互。

### 2. 各类 block 的具体规则

#### UserPrompt

`UserPromptBlock`（`blocks/user.rs:461-535`）只有在视觉行数超过三行或单行过长时才 `is_foldable=true`；可折叠时默认 `Collapsed`，否则 `Expanded`。折叠模式显示最多三行，并通过用户 fold 操作恢复全文。它是可选中、可复制的主要内容块，不应承载隐藏 system prompt。

#### AgentMessage

`AgentMessageBlock`（`blocks/agent.rs:10-85,170-231`）提供 `streaming()`、`push_chunk()`、`push_chunk_deferred()` 和 `finish()`。chunk 只追加到同一 `MarkdownContent`，不会为每个 delta 新建一条 transcript row。agent message 本身 `is_foldable=false`，因此最终回答默认保持可读正文；需要隐藏的是内部上下文或 thinking，而不是用户能看到的回答。

#### Thinking

`ThinkingBlock`（`blocks/thinking.rs:428-540`）是最完整的流式状态示例：

- `streaming()` 开始本地计时；`streaming_replay()` 用于历史回放，不重复启动本地计时；
- `push_chunk()` 追加 reasoning；`finish()` 完整重渲染并冻结耗时；
- 默认 `DisplayMode::Truncated`，运行时可在 `Collapsed -> Truncated -> Expanded` 间切换；
- 完成后 `finished_display_mode=Collapsed`；
- `is_groupable=true`；
- `ScrollbackEntry::is_hidden_thinking(show_thinking)` 在 `show_thinking_blocks=false` 时让既有历史也立即变成零高度，而不是删除历史。

这正是 DSH 需要借鉴的生命周期：运行中的 surface 仍存在且可更新，完成时调用 block 的终结逻辑，隐藏是 view-time projection。

#### ToolCall / ToolResult

`ToolCallBlock`（`blocks/tool/mod.rs:210-274`）把折叠、运行态、完成态和分组委托给具体工具变体。Read/Edit/Search/Execute/Web 等使用不同组件绘制参数、diff、stdout、引用和错误；它们可以在 collapsed 状态下形成密集工具组。工具结果不是普通 assistant 文本，应该保持 tool result 语义，以便错误颜色、复制和后续折叠都正确。

#### System 与 ContextInfo

`SystemMessageBlock`（`blocks/system.rs:25-94`）和 `ContextInfoBlock`（`blocks/context_info.rs:596-663`）都具有以下特征：

- muted、无垂直 padding，视觉上是 compact chrome；
- `is_selectable=false`；
- `is_foldable=false`；
- `is_groupable=true`。

注意：这两个 block 不是“把整段 system prompt 作为普通消息显示”的许可。Grok 的 system block 用于短的会话通知；`ContextInfo` 用于结构化 `/context` 快照。原始 system instruction 若需要审计，应进入独立的隐藏/折叠上下文层，而不是伪装成 `UserPrompt` 或普通 `Context` 行。

#### Subagent / SessionEvent

`SubagentBlock`（`blocks/subagent.rs:173-323`）固定为一行 `Collapsed`，可显示 running/completed/failed/cancelled，并 `is_groupable=true`。`SessionEventBlock`（`blocks/session_event.rs:567-686`）则承担 recap、turn terminal、认证和生命周期状态；这类状态不应该和 assistant markdown 拼在同一个 surface。

### 3. Entry、wrapper 和 pane 如何把 block 变成屏幕

Grok 的渲染链是：

```text
host/acp event
  -> AgentView / queue / tracker 选择 RenderBlock
  -> ScrollbackState.push/replace/update
  -> ScrollbackEntry { stable EntryId, block, is_running, display_mode, finished_at }
  -> layout/group scan
  -> EntryRenderer -> BlockRenderer -> ratatui Buffer
  -> ScrollbackPane（viewport、sticky header、selection、mouse、link/media overlay）
```

关键组件：

- `ScrollbackEntry`（`scrollback/entry.rs:72-110,174-236,254-297`）保存稳定 `EntryId`、`is_running`、`display_mode`、`display_mode_pinned`、完成时间和渲染缓存。它是 block 的状态壳，而不是一次性字符串。
- `EntryRenderer`（`scrollback/wrappers/entry_renderer.rs:25-83,231-376,428-515`）负责 accent、bullet、padding、timestamp、group header 和 hidden thinking 的高度；`BlockRenderer`（`wrappers/block_renderer.rs:16-170`）只把 `BlockContent` 输出到 Buffer。
- `ScrollbackState`（`scrollback/state/mod.rs:617-679,963-1075,1399-1498`）负责 push、对指定 EntryId 追加 chunk、标记 running、finish streaming block，并在 finish 时应用 `finished_display_mode`。
- `ScrollbackPane`/`scrollback/render.rs` 读取布局缓存，使用 `EntryRenderer` 绘制可见行；不会自己猜消息类型。

## 二、Grok 的折叠与分类不是一个开关，而是两级 view-time projection

### 1. Entry 级 display mode

每个 entry 有 `Collapsed`、`Truncated`、`Expanded` 三种模式。模式由 block 自己决定默认值和运行结束值，用户操作通过 `selection.rs` 的 `toggle_fold_selected`、`toggle_group_expansion` 和鼠标路径修改；`respect_manual_folds` 时用户手势通过 `display_mode_pinned` 保留下来。

### 2. Group 级 synthetic header

`scrollback/state/groups.rs:31-61,69-124,126-191,255-383` 将折叠扫描集中到一个模型中，只有两个 fold family：

- `VerbRun`：连续 collapsed 的 tool/subagent 生命周期行聚合成 “Read 2 files / Ran 3 commands” 等 synthetic header；finished collapsed thinking 可以被吸收但不计数；
- `Truncation`：超过可见预算的 dense group 用 “N more” header，旧成员高度变成 0，展开时恢复成员高度。

`verb_group.rs:31-93,121-175` 是唯一的 run classifier：collapsed tool/subagent 是 `Member`，finished collapsed thinking 是 `ThoughtMember`，hidden/running/opened thinking 是 `Transparent`，其它类型是 `Break`。`group_tool_verbs` 和 `show_thinking_blocks` 在一次 layout rebuild 内只读取一次，确保 scan 和 projection 一致。

`project_to_layout` 将 span 投影为 `height=0/1`、`group_header_count`、`verb_group_header`、`group_collapse_header`。`render.rs:373-444` 每帧根据 span 计算 live label，`EntryRenderer::render_group_header`（`wrappers/entry_renderer.rs:231-376`）绘制 diamond/chrome、运行波纹和可选中 label。也就是说，“折叠”不删除 entry，而是让 layout 和 renderer 共同投影。

## 三、DSH 当前实现与两个症状的对应关系

### 问题一：系统提示词/agent 注入内容全部展示

当前分类入口是 `crates/dsh-pager/src/presentation.rs:702-731,745-758`：

```text
source.kind == "user"        -> DshRenderKind::User
source.plugin == "compact"   -> DshRenderKind::Compaction
其它 user/message             -> DshRenderKind::Context
```

所以 plugin、agent-instructions、system/context 注入会产生正常的 `Event { seq }`，并且 `ScrollbackPane::sync`（`crates/dsh-pager-grok-ui/src/views/transcript.rs:73-94`）把 `scrollback.render_entries()` 中的每一条都缓存、布局和绘制。`render_row`（`transcript.rs:325-348`）对每条 row 统一输出 `label + #source_seq`，再把所有 block 逐个画出；它没有 Grok 的 hidden/collapsed/group projection。

因此根因是 **presentation contract 缺少 visibility/display/group 状态**，不是单纯换一个颜色或把 `Context` 标签改掉。即使 `DshRenderKind` 已有 `Thinking/ToolCall/ToolResult/Error/Compaction`，它们仍然被当作平面 entry 渲染。

### 问题二：第一个流式消息卡住

当前 partial reducer 在 `presentation.rs:782-872`：

- partial key 是 `(turn, step)`，这是正确方向；
- `block-start`、text/reasoning delta、tool-call delta、block-end 会更新 `PartialMessage` 并 upsert `partial=true`；
- `usage`、`finish` 等未知 chunk 走 `_: return`（`presentation.rs:843-852`），终结元数据被静默丢弃；
- 只有 `assistant/message` 才会在 `presentation.rs:760-780` remove `Partial{turn,step}` 并追加 final entry；
- `turn/end`（`presentation.rs:951-963`）只在非 completed 时追加 Status，不清理 partial、不改变 running/finish；
- stream error 只在 `SessionState::accept_stream_error`（`session.rs:752-770`）标记 reconnecting/diagnostic，没有通知 presentation reducer 终结当前 surface；
- seq gap 仍会被 `accept_live` 缓存在 `pending_events`（`session.rs:791-816`），等待 repair；这本身不是卡住 bug，但 adapter 不能再把“还没收到 final”当成永远 running。

runtime 的通知和 redraw 路径本身存在：`dsh-pager-grok-ui/src/runtime.rs:108-169` 在 `update.changed` 时 invalidate content 并重新绘制。因此首个流式消息卡住的主要风险是 **terminal lifecycle 不完整**，不是缺 redraw。

DeepSeek Harness 的实际协议也支持这个判断：`agent.ts` 正常完成会追加 `assistant/message`，但 provider error/EOF/abort 路径可能只有已累计的 `assistant/chunk` 和最后的 `turn/end`，没有可供当前代码匹配的 final assistant message。

## 四、具体修复方案

### A. 先固定 DSH presentation contract

在 `crates/dsh-pager/src/presentation.rs` 扩展中立 DTO；不要把 Grok 的 Rust 类型直接搬进 DSH。建议字段如下（名称可调整，但语义必须保留）：

```rust
enum DshRenderVisibility {
    Visible,
    Collapsed, // 只显示摘要/header，正文可由用户展开
    Hidden,    // 仍在 history/model，当前 transcript 高度为 0
}

enum DshRenderFinish {
    Running,
    Completed,
    Interrupted,
    Failed,
    Eof,
}

struct DshRenderEntry {
    // 现有 id/source_seq/kind/text/partial/lineage/content 保留
    visibility: DshRenderVisibility,
    finish: DshRenderFinish,
    group_key: Option<String>,
    selectable: bool,
}
```

设计边界：

- `kind`/`finish`/默认 visibility 由 DSH adapter 根据 host event 决定；
- `display_mode`、group 展开状态、`show_thinking_blocks` 属于 UI 本地 state，不能写回 history；
- `Hidden` 不等于丢弃，copy/debug/fullscreen 可以从 history 或显式诊断入口读取；
- `group_key` 只作为 view projection 的稳定锚点，不以数组 index 作为身份。

### B. 建立消息分类矩阵，解决 system/context 泄漏

在 `adapt_user_message` 中把当前“其它都 Context”的分支改为显式分类。第一版建议：

| Host 条件 | DSH 语义 | 默认 visibility | Grok 对应 |
|---|---|---|---|
| `source.kind == "user"` | `User` / UserPrompt | `Visible`；超长才 local collapse | `UserPromptBlock` |
| `source.kind` 为 `system`、`developer`、`agent-instructions`、`instructions` 或明确的 system/plugin 注入 | `SystemInstruction`（可作为新的 `Context` subtype） | `Hidden`；诊断模式可展开 | 不直接映射到 UserPrompt；短通知才用 `SystemMessageBlock` |
| `source.kind == "plugin"`、插件返回的上下文、工具前置注入 | `AgentContext` | `Collapsed`；按 source/plugin 聚合成一个摘要 header | view-time `ContextInfo`/synthetic group |
| `source.plugin == "compact"` | `Compaction` | `Collapsed` | `SessionEvent`/compact chrome |
| assistant text/reasoning/tool blocks | `Assistant`/`Thinking`/typed tool | Assistant visible；Thinking 按 setting hidden/truncated | `AgentMessageBlock`/`ThinkingBlock`/`ToolCallBlock` |

实际字段名以 Harness fixtures 为准，不要只依赖字符串标签。分类器必须保留 `source.kind`、`source.plugin` 和 lineage，遇到新 kind 时进入 `UnknownContext` 的 **collapsed** 安全默认值，而不是可见普通行。

`ScrollbackPane`/`render_row` 的替换方向：

1. 先按 entry 的 visibility 生成 view-time `VisibleEntry` 列表；`Hidden` 不进入布局，`Collapsed` 只生成一个 synthetic summary row；
2. 连续的 `AgentContext`/`SystemInstruction` 形成 `ContextGroup`，header 显示来源和数量，例如 `Context · 4 injected messages`，而不是打印正文；
3. header 的展开状态以首个 entry 的稳定 id/group key 保存；展开只改变 projection，不修改 history；
4. 删除 `render_row` 中无条件的 `label + #source_seq` 主视觉。`source_seq` 仅保留在 debug/diagnostic 或复制元数据里，避免把协议内部编号当成 Grok UI；
5. Grok 风格的颜色、bullet、Markdown/Diff/Image/Reasoning/ToolCall renderer 继续消费 `DshRenderContent`，但由 kind/visibility 选择 wrapper，而不是由一条通用 row 猜样式。

这一步的最小可交付不是完整 pixel parity，而是：一次正常对话中只显示 User、Assistant、必要的 Tool/Result 和 compact status；注入上下文在默认 transcript 中不再逐条出现；可通过一个 fold 操作展开并保持 stable anchor。

### C. 用稳定 surface 状态机修复流式卡住

#### C.1 稳定身份

把 `(turn, step)` 定义为 assistant surface 的稳定身份。实现上有两种可接受方案，推荐第一种：

1. 新增 `DshRenderEntryId::AssistantSurface { turn, step }`，从首个 chunk 到 final/error/EOF 始终复用同一个 id；
2. 若暂时不能改 enum，则保留 `Partial { turn, step }` 作为 surface id，并在 final upsert 时将 `partial=false`、`finish=Completed`，禁止“Remove partial + 新建 Event”导致行重排。

`source_seq` 记录最新导致更新的事件，`lineage` 追加所有相关 seq；它们不能替代 surface id。

#### C.2 chunk reducer

在 `adapt_assistant_chunk` 中：

- 保留 block index 和 block lineage；`text-delta`、`reasoning-delta`、`tool-call-delta` 追加到同一个 partial surface；
- `usage` 和 `finish` 不再静默丢弃，至少写入 `PartialMessage` 的 terminal metadata；
- `block-end` 若带完整 block，替换对应 partial block；否则只标记 block closed，不能清空已有 delta；
- 缺少 turn/step/chunk 的 malformed frame 生成 diagnostic/status，并带 generation/seq，不能静默 return；
- 只含 reasoning 的 surface 仍显示为 `Thinking`，一旦出现 text/tool block，kind 变为 `Assistant`，但 surface id 不变。

#### C.3 所有终结路径都必须调用同一 finalize 函数

新增一个类似 `finalize_partial(key, finish, reason, seq)` 的 adapter/session seam：

| 终结来源 | finish | 处理 |
|---|---|---|
| `assistant/message` | `Completed` | 用 final content 替换 partial blocks，`partial=false`，调用 block finish；没有 content 时保留已收到内容并标记 completed |
| `turn/end` + reason completed | `Completed` | 若仍有 partial，先 finalize；有 final message 时幂等，不重复生成 |
| `turn/end` + abort/interrupted/cancelled | `Interrupted` | 保留已收到正文，停止 running，追加 compact status；不等待不存在的 assistant/message |
| `host/agent-error` / provider error | `Failed` | 保留 partial 内容，追加 error/status block，surface 不再 running |
| stream EOF/transport close | `Eof` | 保留 partial 内容，追加 “stream ended before final message” 的 compact status；触发 reconnect/repair，但不把 surface 留在 running |
| session/generation 变化 | `Eof` 或 `Interrupted`（按原因） | 先终结旧 generation 的所有 partial，再接受新 generation，防止 stale chunk 回写 |

终结要幂等：同一 key 第二次收到 final/error/EOF 只能更新诊断，不得重复 header、重复 assistant row 或重新进入 running。若 surface 没有任何可见 block，允许移除空 partial，但必须保留 status/diagnostic 以便解释。

`SessionState::accept_stream_error` 不能只改 reconnecting 字段；它必须向 `Scrollback`/presentation 发送 finalize signal。runtime 的 redraw 保持现状即可，`SessionUpdate.changed=true` 会驱动重绘。

### D. 在 view 层实现 Grok 风格的 fold/group 投影

在分类和终结契约稳定后，再改 `crates/dsh-pager-grok-ui/src/views/transcript.rs` 与 DSH scrollback：

1. `Scrollback::render_entries()` 保留全量 canonical entries；新增 `project_view(appearance, local_folds)`，输出可布局的 `VisibleEntry`/`GroupHeader`；
2. `AgentContext`/`SystemInstruction` 使用独立 `ContextGroup`，默认 `Collapsed` 或 `Hidden`；不要复用 `ToolCall` 的 verb group 计数；
3. `Thinking` 默认 `Truncated`，完成后 `Collapsed`，`show_thinking_blocks=false` 时既有 entry 高度变为 0；
4. ToolCall/Subagent 等 typed tool 才加入 `VerbRun`，只聚合同类可命名成员；Assistant/UserPrompt 是 run breaker；
5. group header 是 synthetic row，selection/mouse 以稳定首 entry id 作为 anchor；展开/折叠后恢复 viewport anchor；
6. `style_for_paint` 只负责 semantic role 的样式，不能承担 visibility 或生命周期判断；
7. narrow terminal（80x24、40x12）必须先保证 header/text 不溢出，再做像素细节。

这个顺序与 Grok 的 `groups.rs -> project_to_layout -> render.rs -> EntryRenderer` 一致：先扫描和生成 span，再投影高度和 header，最后绘制。

## 五、测试与验收门禁

### 1. Presentation adapter 单元测试

在 `crates/dsh-pager/src/presentation.rs` 现有测试附近增加 fixture，至少覆盖：

- `user/message` 的 `source.kind=user` 是 Visible User；
- `agent-instructions`、system/developer、plugin 注入默认 Hidden/Collapsed，不产生普通可见 Context row；
- compaction 只产生 compact event；
- text + reasoning + tool-call 交错时仍只有一个 stable assistant surface，block index 不乱；
- `usage`/`finish` 后 surface 进入终结元数据；
- `assistant/message`、`turn/end`、abort、agent-error、EOF 分别只 finalize 一次；
- malformed chunk 有 diagnostic，不静默吞掉；
- final frame 缺失时已收到内容仍可复制，且 `partial=false` 或明确 `finish != Running`。

### 2. Scrollback/view projection 测试

新增 projection/snapshot 断言：

- canonical entries 数量不因 Hidden 而减少；
- default view 不包含 system prompt 正文；
- ContextGroup 折叠只占一行，展开后恢复原始 block 顺序；
- thinking setting 切换会影响既有历史；
- group header 展开/折叠不改变 stable ids，viewport anchor 不跳；
- user/assistant/tool/result/status 的 copy payload 不包含 synthetic header chrome。

### 3. Real Harness/PTY gate

沿用主计划的 P03/P14/P15，并新增本 bug tranche 的两个门禁：

- **P19 Context visibility**：启动 DeepSeek Harness，发送一条普通 prompt；transcript 默认不逐条显示 system prompt、agent-instructions 和 plugin 注入正文；可见区域包含用户输入、assistant 回复和必要 tool/status；显式展开后上下文可读且不破坏顺序。
- **P20 Stream terminal**：第一条 assistant stream 在首个 delta、多个 delta、block-end、final、turn/end-only、provider error、EOF、Ctrl-C/abort 八种路径下都能结束；没有永久 running/partial；异常路径保留可见已收到内容和 compact 状态。

PTY 场景至少跑 80x24 和窄屏；记录事件 seq、turn/step、surface id、finish、visibility、最终屏幕文本。只靠 snapshot 不足以覆盖 Harness 的 error/EOF timing。

## 六、执行顺序与停止条件

### Phase 0：契约和 fixture（先做）

- 锁定上面的 `visibility/finish/stable surface` DTO；
- 从 Harness 采集正常、abort、provider error、EOF、missing final 的最小 JSONL；
- 把 Grok block/component mapping 写成测试 fixture，而不是直接复制 Grok UI 代码。

### Phase 1：先修流式终结（解决问题二的根）

- 实现 stable assistant surface 和 finalize seam；
- 接入 `turn/end`、stream error、EOF、generation change；
- 完成 P20 与 adapter unit gates。

### Phase 2：修分类与上下文可见性（解决问题一的根）

- 显式分类 source kind/plugin；
- 加入 Hidden/Collapsed projection 和 ContextGroup；
- 移除通用 `label + #seq` 主视觉依赖；
- 完成 P19 与 view projection tests。

### Phase 3：补 Grok fold/group 和像素 parity

- Thinking 三态、finished collapse、show-thinking live toggle；
- Tool/Subagent verb group 和 N-more truncation；
- EntryRenderer/ScrollbackPane 的 selection、mouse、sticky header、narrow width；
- 对应 `GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md` 的 M4.2/M4.3/M4.6、M9.7 和 `GROK_RENDERER_PIXEL_PARITY_PLAN.md` N2/N3 门禁。

### Phase 4：回归和 rollout

- normal live/replay、surface replace、stale generation、seq gap repair、reconnect、malformed frame 全量回归；
- 若 host 字段仍不稳定，未知 source/block 使用安全默认：context collapsed、unknown block 可复制但不自动提升为 User/Assistant；
- 任一终结路径不能证明幂等前，不继续扩展更多 block 类型或做像素微调。

## 七、为什么现在应先修，而不是继续堆 renderer

当前 UI 已经能把 `DshRenderContent` 中的 Markdown/Diff/Image/Reasoning/ToolCall/ToolResult 画出来，runtime 也会在通知变化后重绘；继续增加颜色、边框或更多 block renderer，只会把错误分类和永久 partial 更精确地画出来。先完成本文件的两条不变量，才有资格进入 M4.3/M4.6 和 pixel parity：

- 没有 visibility projection，N2 的 `ScrollbackPane` 只能继续展示泄漏的 system/context；
- 没有 terminal finalize，任何 streaming snapshot 都可能停在 `partial=true`；
- 没有 stable surface id，final replacement 会造成 transcript 重排，后续 sticky/group/selection 都不可靠。

本批次只新增方案文档，不宣称两个 bug 已修复。下一批实现应严格按 Phase 0 -> Phase 1 -> Phase 2 推进，并以 P19/P20 作为继续扩大 renderer parity 的前置门禁。
