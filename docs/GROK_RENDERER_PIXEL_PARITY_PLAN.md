# Grok renderer 复用与像素级前端对齐方案

> 适用仓库：`/home/leo/code/dsh-pager-grok`
> 上游仓库：`/home/leo/code/grok-build`
> 固定 mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`
> 固定 `SOURCE_REV`：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`

## 1. 方案结论

后续前端迁移不再手写 Grok 的等价 renderer。Grok Build 的视觉实现直接作为
前端 reference 和生产 renderer：布局、主题、Prompt、Scrollback、selection、
status、快捷键、焦点和终端降级规则优先复用上游代码。

DSH 仍然是唯一的数据和副作用真源。DSH 只负责把 `SessionState`、
`ControlPlaneStore` 和 capability 投影成 Grok renderer 消费的 DTO，并把 Grok
view 产生的 `UiIntent` 转换为 `UiEffect`。Grok agent、shell、ACP、RPC、配置、
持久化和 telemetry 不进入 DSH UI。

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
   AgentView  Prompt  Scrollback
   Layout     Widget  Pane/Blocks
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

以下核心视觉模块尚未直接进入生产路径：

| 模块 | 当前替代物 | 对齐目标 |
|---|---|---|
| `PromptWidget` | `views/agent.rs` 自建 Prompt chrome | 直接复用上游 `PromptWidget::draw` |
| `AgentViewLayout` | DSH 手写 row stack | 直接复用上游 `LayoutConfig`/`ScrollbarConfig`/layout solver |
| 完整 Theme | `dsh-pager-render::Theme` 简化字段 | 引入 Grok 完整语义 token |
| Appearance | DSH 常量和局部参数 | 引入 Grok prompt/scrollback/layout 配置的只读投影 |
| ScrollbackPane/block renderer | `RichTranscript` | 直接复用 Grok scrollback block/layout/render 路径 |
| AgentView render orchestration | `runtime.rs` 手工串联 | 复用 Grok `AgentView::draw` 的绘制顺序 |

因此，当前界面“接近但不一致”的根因不是缺少 ratatui 能力，而是核心 renderer
仍然是 DSH 自建实现。

## 3. 复用边界

### 3.1 必须直接复用的前端代码

第一阶段优先引入以下上游路径：

```text
xai-grok-pager/src/views/prompt_widget/
xai-grok-pager/src/views/agent.rs
xai-grok-pager/src/app/agent_view/mod.rs
xai-grok-pager/src/app/agent_view/render.rs
xai-grok-pager/src/views/agent_status.rs
xai-grok-pager/src/views/turn_status.rs
xai-grok-pager/src/scrollback/
xai-grok-pager-render/src/appearance/
xai-grok-pager-render/src/theme/
xai-grok-pager-render/src/glyphs.rs
xai-grok-pager-render/src/render/
```

这些模块共同决定：

- row constraints、outer padding、prompt gap 和 short-terminal 断点；
- prompt 的 divider、accent、prefix、title、info line、cursor 和 unfocused dimming；
- transcript block 的结构、折行、颜色、selection、scrollbar 和 timeline；
- status、turn status、spinner、shortcut 和 capability fallback；
- 主题 token、glyph 和终端颜色角色。

不能只复制 `PromptWidget` 或 `agent.rs` 单文件。必须以编译和行为所需的最小
renderer dependency closure 为单位 vendor，否则会再次出现“看起来像上游、实际
几何和颜色仍由本地代码决定”的结果。

### 3.2 明确不得复制的 Grok 代码

以下模块即使被 renderer 间接引用，也不得进入 DSH production dependency：

- Grok agent loop、tool/shell orchestration；
- ACP client/runtime 和 Grok session model；
- Grok workspace/config/auth/persistence/telemetry；
- Grok RPC transport、foreign session storage 和后台 worker；
- 只为 Grok 产品业务存在的 dispatch/effect 实现。

这些能力仍由 `crates/dsh-pager` 和 DSH protocol 拥有。

### 3.3 Adapter 是唯一接缝

Grok view 不得 import `SessionState`、`RpcTransport` 或 DSH wire DTO。适配只允许
经过以下 DTO：

```rust
pub struct GrokRenderSnapshot {
    pub session: GrokSessionVisual,
    pub prompt: GrokPromptVisual,
    pub scrollback: GrokScrollbackVisual,
    pub status: GrokStatusVisual,
    pub capabilities: TerminalCapabilities,
}
```

DTO 只表达绘制和交互所需事实，不保存 DSH runtime 引用，不发起副作用。

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

1. 从固定 Grok revision 计算 Prompt、AgentView、Scrollback、Theme、Appearance
   和 render helper 的依赖闭包。
2. 按 `vendor/grok` 原始目录结构复制，保留 Apache-2.0 `LICENSE/NOTICE`。
3. 更新 `SOURCE_MANIFEST.md`：每个文件记录 upstream path、source hash、vendor
   hash 和本地适配原因。
4. 保留上游 tests；如果 tests 依赖 Grok runtime，则提取纯 renderer fixture，
   不复制业务测试 harness。

完成标准：所有新增视觉文件都可追溯；没有未登记的 vendor 文件；不引入 Grok
agent/shell/runtime。

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

### P3：AgentViewLayout 和主绘制顺序

目标是让所有 pane 的几何来自 Grok solver。

工作项：

1. 使用上游 `AgentViewLayout::compute` 和 `LayoutConfig`。
2. 将 DSH 数据映射到 Grok pane：scrollback、turn status、banner、prompt、status
   line、shortcuts、timeline/scrollbar。
3. 用同一份 layout snapshot 同时驱动绘制、hit map、cursor 和 mouse routing。
4. 删除当前 runtime 的手写 header/prompt/footer 几何回推逻辑。
5. 保留 DSH 的 overlay owner 和 effect reducer，只更换其视觉落点。

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

### P5：runtime 收敛和旧 renderer 删除

目标是生产路径只保留一套 Grok 视觉实现。

删除或降级为测试 oracle：

- 当前 `views/agent.rs` 自建 Prompt chrome；
- 当前 `RichTranscript` 生产绘制路径；
- runtime 中重复的 status/header 文案拼接和尺寸计算；
- 与 Grok 等价的 DSH Theme、spacing、glyph 和 scrollbar 算法。

保留：

- `GrokHostSnapshot` 和 DSH host adapter；
- `UiIntent -> UiEffect -> receipt/notification`；
- `AppShell` 的 DSH-neutral focus/overlay reducer，直到 Grok focus 状态机完全接管；
- parity/reference 工具和 DSH 协议测试。

完成标准：生产 runtime 不再存在第二套同职责 renderer；所有临时 fallback 都有
删除记录或明确保留理由。

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
