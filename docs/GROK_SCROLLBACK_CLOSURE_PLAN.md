# Grok Scrollback 闭包移植计划（档 3）

> 用途：把转录生产路径从自造的 `transcript.rs` 换回 Grok `scrollback/` 视觉闭包，
> 拦住继续漂移。
> 上级计划：[`GROK_RENDERER_PIXEL_PARITY_PLAN.md`](GROK_RENDERER_PIXEL_PARITY_PLAN.md) N2。
> 视觉契约：[`GROK_TUI_TRANSCRIPT_SURFACE.md`](GROK_TUI_TRANSCRIPT_SURFACE.md)。
> 上游：`/home/leo/aidreamschool/grok-build`，mirror
> `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，`SOURCE_REV`
> `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
> 本文只规划，不宣称闭包已迁完，也不授权继续在 `transcript.rs` 里发明视觉算法。

## 1. 为什么现在写

2026-08-25 同一上午，运行态左侧 `┃` 在 `transcript.rs` 里被连续改了三轮：

1. 按线速度的彗星（8 行/秒、4 行带、6 行空档）；
2. 误把卡顿当速度快，把速度改成 4；
3. 改回 8，再换成 16 行高斯。

Codex 独立审核确认：整列突然变黑再从头亮，是这段自造包络走离轨道后 `%` 折回，
再把亮度 0 画成背景色。Grok 从来不是这种模型。Grok 是
`sin²(tick × 0.15 + 2π × row / 32)`，空间周期固定，**不吃 rail 长度**。

这不是行波一个 knobs 调错。这是结构问题：

- Grok 把 chrome、block、layout、state 拆在 `scrollback/` 一棵目录里；
- DSH 把对等职责塞进
  `crates/dsh-pager-grok-ui/src/views/transcript.rs`（约 3908 行），
  占整个 `views/` 的将近一半；
- 每次「看起来差一点」都在这个文件里再写一套公式，和上游越来越远，
  以后要 drop-in `EntryRenderer` 时，这些补丁全部是冲突。

档 3 的目标：**生产绘制走 Grok scrollback 闭包；`transcript.rs` 只剩 host 胶水，
然后删除。** 行波只是第一块必须交还的算法，不是可以单独打磨的产品差异。

## 2. 和非本文的关系

| 文档 | 职责 | 本文不重复什么 |
|---|---|---|
| `GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md` | 总控契约 | 里程碑编号、协议 |
| `GROK_RENDERER_PIXEL_PARITY_PLAN.md` | 全 TUI 复用与 N1–N4 | Prompt、File Search、Workspace、Agent pane |
| `GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md` | DTO、stream、fold 语义 | 协议投影规则 |
| `GROK_TUI_TRANSCRIPT_SURFACE.md` | 人眼看到的 chrome 契约 | 截图级现象描述 |
| 本文 | **N2 的执行细则**：目录对照、冻结规则、vendor 切片、删文件顺序 | 不改 N1/N3/N4 范围 |

已完成的
[`2026-08-23_18-02-00_N2-scrollback-pane-adapter.md`](开发进度记录/2026-08-23_18-02-00_N2-scrollback-pane-adapter.md)
只接了 **DSH Fenwick 布局 authority + 语义行缓存**。它没有迁
`EntryRenderer`，也没有迁 Grok `scrollback_pane.rs`。名字碰巧都叫
`ScrollbackPane`，这是下一节必须先解开的碰撞。

## 3. 立刻生效的冻结规则

从本文落地开始，转录视觉的默认动作是 **vendor 或删除**，不是在
`transcript.rs` 里加函数。

1. **禁止** 在 `transcript.rs` 新增视觉算法：行波、脉冲、blend、glyph 选择、
   fold chrome、verb-run header、vpad、timestamp overlay 几何。
2. **禁止** 再调 `WAVE_ROWS_PER_SECOND` / `WAVE_BAND_ROWS` / `WAVE_GAP_ROWS`
   作为产品方案。这三个常数是自造彗星的旋钮，档 3 要删掉它们。
3. **禁止** 把 Grok `scrollback/` 的某一文件「简化重写」进 `views/`。
   允许的是 vendor 原文件 + adapter。
4. **允许** 的 `transcript.rs` 改动只有：
   - 修 DSH identity/layout 接线 bug；
   - 把一整块职责搬到 vendor/adapter 后删除对应函数；
   - 测试改为断言 Grok 闭包的输出。
5. 若必须临时补像素，新代码放在
   `crates/dsh-pager-grok-ui/src/scrollback_adapter/` 下、文件头标明
   `TEMPORARY — delete when <upstream path> is wired`，并在本文第 8 节挂账。
   不得再扩大 `transcript.rs`。
6. 并行批次若要改 Tool 展开箭头、配色、思考耗时，先问：Grok 对应文件是哪一个？
   有则改 vendor 或等该切片；没有才允许 adapter。禁止第三套平行实现。

违反冻结 = 继续漂移。行波三连补丁已经证明，在神文件里「先看一眼再改常数」
会把错误模型写进测试，之后更难替换。

## 4. 两边现在各是什么

### 4.1 Grok：一棵目录，一条绘制链

```text
xai-grok-pager/src/scrollback/
  block.rs / entry.rs / types.rs     类型：RenderBlock、ScrollbackEntry、AccentStyle
  blocks/                            每种消息自己的 output / accent / bullet / fold
    thinking.rs user.rs agent.rs
    tool/{execute,read,edit,search,...}.rs
  wrappers/
    entry_renderer.rs                chrome：┃ ◆ vpad 行波 group header timestamp
    block_renderer.rs                只把 BlockOutput 打进 Buffer
  layout.rs                          列结构：accent | pad | content | pad
  render.rs                          按滚动窗口调用 EntryRenderer
  scrollback_pane.rs                 会话主 widget、sticky、scratch
  state/                             运行集、verb_group、tick、flash、selection
  sticky.rs selection.rs text_selection.rs
```

行波不拥有自己的子系统。它是 `EntryRenderer` 在 `AccentStyle.animated == true`
时对每一行调用 `theme::wave_brightness`。公式在
`xai-grok-pager-render/src/theme/tokyonight.rs`，混色在
`render/color.rs::blend_color`，节奏在 `appearance/config.rs`
（`fps=30`，`wave_rows=32`）和 `WAVE_SPEED=0.15`。

绘制链：

```text
event_loop 30fps
  → ScrollbackState.tick()           // tick += 1；仅 viewport 内 running 才要重绘
  → scrollback/render.rs
  → EntryRenderer
       ┃  : wave_brightness(tick, skip_rows+row, 32, 0.15)
       ◆  : wave_brightness(tick, 0, 32, 0.15)   // 与第 0 行同步
  → blend_color(block_bg, accent, brightness)
```

`logical_row` 是 **entry 内行号**（含滚出屏幕的 `skip_rows`），不是可见高度，
也不是 `rail_len + band + gap`。

### 4.2 DSH：一个文件，一条平行宇宙

`views/transcript.rs` 当前同时承担：

| 职责 | 代表符号 | Grok 对应文件 |
|---|---|---|
| host 行缓存、高度回报 | `ScrollbackPane` | 无；这是 DSH Fenwick 胶水，应留下但改名 |
| fold / group / verb-run 投影 | `ProjectionInfo` `GroupKind` `build_projection` | `state/verb_group.rs` `state/groups.rs` |
| thinking 截断/耗时 | `thinking_running_lines` `format_elapsed_ms` | `blocks/thinking.rs` |
| tool 摘要/Execute 面板 | `render_tool_call` `render_execute_tool_call` | `blocks/tool/*.rs`（Execute 已有 B vendor） |
| Markdown / Diff | `render_markdown` `render_diff` | `blocks/markdown_content.rs` `tool/edit.rs` |
| chrome：┃ ❙ ◆ 时间戳 | `decorate_line` | `wrappers/entry_renderer.rs` |
| 自造行波 | `wave_brightness` `assign_rail_wave_geometry` | `theme::wave_brightness` + EntryRenderer 循环 |
| 自造混色 | `blend_color` | `render/color.rs` |
| 选中 / 完成闪 | `apply_dynamic_accent` | EntryRenderer + `state` flash |
| 测试 | `mod tests` 约占文件后半 | 上游各文件 `#[cfg(test)]` + snapshots |

生产路径已经是「DSH `Scrollback` 管 identity，pane 管语义行」，这一点要保留。
错的是语义行的 **生成和上色** 在本地重写，而不是调用 Grok block + EntryRenderer。

### 4.3 必须先解开的命名碰撞

| 名字 | Grok | DSH 现在 | 档 3 之后 |
|---|---|---|---|
| `ScrollbackPane` | 主会话 widget | `transcript.rs` 里的 host 缓存 | vendor 保留 Grok 名；DSH 胶水改名为 `TranscriptHost` 或 `DshScrollbackGlue` |
| `Scrollback` | `ScrollbackState` 拥有 entries | `dsh_pager::Scrollback` 拥有 Fenwick + `DshRenderEntry` | **DSH 继续拥有 identity**；不引入第二份 entry 真源 |
| `DisplayMode` | `types.rs` | `transcript.rs` 私有 enum | 用 Grok 的 |
| `wave_brightness` | theme 导出 | transcript 私有高斯 | 删除本地函数，调用 theme |

Grok `ScrollbackState` **不**作为 DSH 历史真源。Harness 事件 → `DshRenderEntry`
这条链不动。迁的是「给定一条 entry，像素怎么画」。

## 5. 目标拓扑

```text
SessionState / assistant chunks / tool views
        │
        ▼
dsh_pager::Scrollback                // identity、partial replace、Fenwick、anchor
        │  DshRenderEntry[]
        ▼
src/scrollback_adapter/              // 唯一允许的新本地代码
  project_entry.rs                   // DshRenderEntry → Grok ScrollbackEntry
  tick.rs                            // Instant → u64 tick（见 5.1）
  host_pane.rs                       // 原 DSH ScrollbackPane 改名后的胶水
        │
        ▼
vendor/grok/.../scrollback/          // A0/A1 原文件 + 必要 B 接缝注释
  blocks/*  wrappers/*  render.rs  layout.rs  types.rs
        │
        ▼
ratatui Buffer / 现有 hit-map
```

`views/transcript.rs` 最终应消失，或只 re-export `host_pane`。不允许它再作为
视觉实现的家。

### 5.1 时钟：抄公式，不抄 tick++

Grok 每帧 `tick += 1`。额外按键/通知重绘会加速行波。DSH 已经用
`rail_wave_started_at: Instant` 避开了这个问题，**这是应保留的 adapter**，
不是第二套波形。

接缝固定为：

```text
tick = floor(elapsed_ms * appearance.animation.fps / 1000)
brightness = grok_wave_brightness(tick, logical_row, wave_rows, 0.15)
```

默认 `fps=30`、`wave_rows=32`。禁止再引入 `rail_len` 参与周期。

### 5.2 颜色：抄 AccentStyle，不继续灰白特判当算法

Grok thinking 运行波用 `ThinkingConfig.accent`；Execute 用
`ExecuteConfig.running_accent`；bullet 在 running 时与 accent 同相位。
DSH 把运行波混到 `theme.text_primary`、右侧文字锁白，是另一次本地产品分叉。
档 3 以 Appearance 配置为准。若 DeepSeek 品牌要改色，改 Appearance 投影，
不改 `wave_brightness`。

## 6. 必须 vendor 的闭包（按依赖顺序）

复用等级遵守 `SOURCE_POLICY.md`。路径相对
`crates/codegen/`（pager）或 `crates/codegen/xai-grok-pager-render/`。

### 6.1 行波与混色（无它则 EntryRenderer 编不过）

| 上游 | 等级 | 本地去向 | 删除的自造物 |
|---|---|---|---|
| `theme/tokyonight.rs` 的 `wave_brightness` / `pulse_brightness` | A0 | 随 Theme constructors 切片，或先抽 `vendor/.../wave.rs` 再合并 | `transcript.rs` 高斯三常数 |
| `render/color.rs::blend_color` | A1 | vendor；Indexed 量化可保留 | 本地三通道 round blend |
| `appearance/config.rs` 的 `AnimationConfig` `ThinkingConfig` `ToolConfig` | B | 已有 Layout 投影上补字段，不要平行 struct | 写死的 gray / text_primary 波 |
| `glyphs.rs` 剩余 registry | A1 | 扩 `src/glyphs.rs` 或 vendor 全表 | 继续硬编码 `⌘`/`⌄` 等 |

`tokyonight.rs` 整文件还带着 Theme 构造。生产主题已是
`dsh-pager-render::Theme`。**不要为了 12 行公式吞掉整份 Theme 再打一场。**
先 A0 拷两个函数；Theme constructors 仍按 SOURCE_MANIFEST 的 planned 切片合并。

### 6.2 chrome（行波像素级的最小闭包）

| 上游 | 等级 | 说明 |
|---|---|---|
| `scrollback/types.rs` | A1 | `AccentStyle` `DisplayMode` `BlockContext` |
| `scrollback/layout.rs` | A1 | accent 列几何 |
| `wrappers/entry_renderer.rs` | B | 保留 WAVE_SPEED、pending 冻结、bullet 同步；tick 从 adapter 注入 |
| `wrappers/block_renderer.rs` | A1 | |
| `wrappers/padded.rs` `accented.rs` | A1 | 若 EntryRenderer 仍引用 |

没有 `ScrollbackEntry` / `RenderBlock`，EntryRenderer 不能单独编译。所以 6.2
必须和 6.3 的 **投影目标类型** 同一批出现，哪怕第一刀只投影 thinking +
execute + markdown。

### 6.3 blocks（按现在生产里真正会画的切）

第一刀覆盖当前 `transcript.rs` 已经在画、且用户每天看见的：

- `blocks/thinking.rs`
- `blocks/user.rs`
- `blocks/agent.rs`
- `blocks/tool/execute.rs`（已有 B vendor，改接到 EntryRenderer，不要第二套 header）
- `blocks/tool/read.rs` `search.rs` `edit.rs` 以及 verb-run 需要的 collapsed 摘要
- `blocks/markdown_content.rs`（markdown crate 已 vendor，缺的是 block 包装）

随后：`web_search` `web_fetch` `list_dir` `subagent` `bg_task` `session_event`
`context_info` `btw` `workflow`。没有对应 DSH 数据的，画显式 empty/unsupported，
禁止用本地文案冒充。

### 6.4 窗口绘制

| 上游 | 等级 | 接缝 |
|---|---|---|
| `render.rs` | B | 可见窗口、skip_rows 来自 DSH `ScrollbackLayout`，不来自 Grok state.scroll |
| `sticky.rs` | A1/B | 用户消息 sticky header |
| `selection.rs` `text_selection.rs` | B | 与现有 DSH selection model 对 identity，不并行两套 copy 真源 |
| Grok `scrollback_pane.rs` | B | 替换 runtime 里手写的 transcript 绘制循环 |

### 6.5 state：只迁视觉状态机，不迁历史库

| 上游 | 迁 | 不迁 |
|---|---|---|
| `state/verb_group.rs` | 是，替换 `build_projection` | |
| `state` 里 finish flash / running set | 是，或由 adapter 按 `DshRenderFinish` 提供 | |
| `state/mod.rs` 整份 entry 存储 | **否** | DSH `Scrollback` 已是真源 |
| Grok session/tool orchestration | **否** | D |

`verb_group.rs` 是 chrome 连续 rail 的来源。继续在 `transcript.rs` 里维护
`GroupKind` 就是继续漂移。

## 7. Adapter 契约

唯一允许理解 DSH 的代码在 `src/scrollback_adapter/`。

```text
DshRenderEntry
  id, kind, finish, visibility, blocks, timestamps, tool views
        │  project_entry
        ▼
ScrollbackEntry
  EntryId ← 从 DshRenderEntryId 稳定映射，禁止数组下标
  RenderBlock ← thinking/user/agent/tool/markdown/...
  display_mode ← 本地 fold pin，不写回 history
  is_running / finished_at ← DshRenderFinish
```

硬规则：

- vendor 文件不得 `use dsh_pager::` 或 `SessionState`。
- `EntryId` 必须能 round-trip 回 `DshRenderEntryId`，hit-map / copy / fold 才不会断。
- 宽度变化仍由 DSH Fenwick 收 renderer 回报的高度（已有 N2 边界），Grok 闭包只
  负责「这个 entry 现在多少行」。
- generation / 幂等键不进 renderer。

## 8. 切片顺序（每一刀都要缩小 transcript.rs）

每一刀的验收：**本刀迁走的职责在 `transcript.rs` 里对应函数删除或变为
`#[cfg(test)]` oracle 调用，禁止双路径 production。**

| 切片 | 迁入 | 必须从 transcript.rs 删除 | 用户可见出口 |
|---|---|---|---|
| S0 | 本文 + 冻结 | 无代码 | 新视觉 PR 不再进神文件 |
| S1 | Grok `wave_brightness` + Instant→tick | 高斯、`WAVE_BAND_*`、`WAVE_GAP_*`、`rail_wave_len` 周期 | 灭灯消失，短 rail 呈 32 行相位 `sin²` |
| S2 | `AccentStyle` + Appearance thinking/tool 色 | `apply_dynamic_accent` 里写死 primary 波 | thinking/execute 波色与 Grok 一致 |
| S3 | `EntryRenderer` + `BlockRenderer` + 最小 `project_entry`（thinking/execute/markdown） | `decorate_line` 生产路径 | ┃ ◆ vpad 由 EntryRenderer 画 |
| S4 | thinking/user/agent 全量 block | `thinking_running_lines` 等 | Thought for / 截断规则来自上游 |
| S5 | 其余 tool blocks + `verb_group` | `GroupKind` `render_tool_call` 生产路径 | Read 2 files 等 header 来自上游 |
| S6 | `render.rs` + sticky + 改名后的 host glue | 手写 visible_lines 上色循环 | skip_rows/窗口与 Grok 一致 |
| S7 | 删除 `RichTranscript` 生产路径 | 文件降到 glue 或移除 | N2 出口：glyph/颜色/wrap/copy 对齐 |

S1 可以单独先做，因为它删除的是已经被证伪的模型，且不扩大神文件（只替换函数体，
随后 S3 会连函数一起删）。S1 **不是** 档 3 完成。

禁止的顺序：先把 `transcript.rs` 拆成 `wave.rs` `tools.rs` `thinking.rs` 三个
本地文件再谈 vendor。那是把平行宇宙正规化，漂移不会减。

## 9. 工作量（量级，不是承诺人天）

只计「为了让 EntryRenderer 在生产里画当前可见转录」的闭包，不含 N1 File Search、
N3 dashboard。

| 范围 | 上游大约行数 | 本地删除/改写 |
|---|---|---|
| S1 公式 | ~12 + 测试 | `transcript.rs` 行波段 ~150 行替换 |
| S2–S3 chrome | types/layout/wrappers ~3k | decorate/accent ~400 行删除 |
| S4–S5 blocks | thinking/user/agent/tool 约 6–8k（Execute 已部分在） | tool/thinking 投影 ~1.5k 行删除 |
| S6 窗口 | render/sticky/pane ~2k | visible_lines 绘制循环 |
| Grok `ScrollbackState` 整库 | ~3.5k | **不搬存储** |

相对「再在 transcript.rs 里调三个 float」，这是一次换血。相对「像素级模仿
Grok 全部功能」，这是已经写在 N2 里、现在必须当真执行的那一块。

## 10. 若继续在 transcript.rs 打补丁会发生什么

可预期的漂移（已经在发生）：

- 行波测试锁死彗星周期 `2.875/3.25/4.25/5.75`，替换公式会先打红测试，
  然后有人把错误周期当成契约；
- Tool 展开箭头、灰白配色、思考耗时各自改同一文件，vendor EntryRenderer 时
  三路冲突；
- `ScrollbackPane` 两个定义让 code search 和后续 PR 审阅者无法判断权威；
- 3908 行文件没有模块边界，审查只能看 diff 片段，无法对照上游函数。

冻结 + S1 立刻减少新债。S3 之后行波不再有本地函数可调。S7 之后这个文件不应
再出现在「视觉 bug 去哪改」的答案里。

## 11. 验收

档 3 / N2 可以声称「转录闭包已接上」的最低条件：

1. 生产路径绘制 running thinking/execute 的 `┃` 调用的是 vendor
   `theme::wave_brightness`，仓库内不再存在高斯/`WAVE_GAP_ROWS` 生产代码；
2. `EntryRenderer` 在 production 绘制 accent/bullet/vpad/group header；
3. `transcript.rs` 不再包含 `fn wave_brightness` / `fn decorate_line` /
   `fn render_tool_call` 的生产实现；
4. 同一 `DshRenderEntryId` 的 fold、copy、hit-map 在迁前后稳定；
5. `SOURCE_MANIFEST.md` 为迁入的每个 scrollback 文件登记 hash 与 B 接缝原因；
6. 行波/折叠/thinking 截断有确定性测试；浏览器静帧不能证明行波，但必须证明
   不再整列消隐（连续多帧采样，全 rail 亮度不同时为 0 超过 Grok `sin²` 窗口
   所允许的范围）。

未完成 S7 不得写「pixel parity 完成」。

## 12. 挂账（TEMPORARY adapter）

当前已知、允许短暂存在、必须在对应切片删除的本地实现：

| 项 | 现在位置 | 删除切片 |
|---|---|---|
| 高斯行波 | `transcript.rs` `wave_brightness` | S1 |
| `rail_wave_len` 周期 | 同上 + `assign_rail_wave_geometry` | S1 |
| 本地 `blend_color` | `transcript.rs` | S1/S2 |
| `apply_dynamic_accent` | `transcript.rs` | S3 |
| `decorate_line` | `transcript.rs` | S3 |
| `thinking_running_lines` | `transcript.rs` | S4 |
| `GroupKind` / `build_projection` | `transcript.rs` | S5 |
| DSH `ScrollbackPane` 这个名字 | `transcript.rs` | S6 改名 |

新增 TEMPORARY 必须改本表，不能只在 PR 描述里写「以后再迁」。
