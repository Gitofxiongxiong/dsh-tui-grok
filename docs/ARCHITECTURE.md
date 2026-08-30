# 架构边界

长期目标、不可破坏的设计契约和分阶段执行门禁见
[GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md](GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md)。
本文件描述目标边界；当前 dsh-pager-grok-ui 的 runtime、host adapter 和
UiEffect 仍是迁移中的最小 scaffold，不应被当作最终 Grok parity 实现。

## 目标

优先复刻 Grok Build 的完整 UI 组件交互和视觉效果，而不是重新实现一个“类似”的 pager。Grok UI 的 view、状态和布局是产品层；DSH 只提供会话数据和副作用。

## 分层

### 1. Host runtime

`crates/dsh-pager` 保留不依赖具体视觉组件的能力：

- `protocol` DTO 与版本协商；
- `RpcTransport`、`loader`、`SessionState`；
- `control_plane`、`dashboard` 等会话/工作区投影；
- `presentation` 与滚动布局这类可被任何 renderer 消费的数据模型。

这一层不能 import Grok 的 view，也不能把 ratatui `Frame` 或终端按键写进 session。

### 2. Grok UI

`crates/dsh-pager-grok-ui` 是默认 UI：

- 直接保留 Grok 的 picker、modal、status bar、timeline、line editor 等模块；
- 通过 `host_adapter` 把 `SessionState` 映射为 Grok view 所需的 rows、blocks、shortcuts；
- 通过 `effects::UiEffectSink` 把“发送 prompt、确认 approval、编辑 queue、切换 session”等动作回传给 host；
- UI 状态（焦点、搜索词、弹窗、timeline viewport）只在本 crate 内管理。

复制的 Grok 文件放在 `vendor/grok`，适配代码集中在 `src/host_adapter.rs` 和 `src/runtime.rs`。视图文件不应散落 DSH 特有判断。

### 3. Binary shell

`crates/dsh-pager-bin` 只负责参数解析、后端启动、非交互 smoke 和把已加载的 runtime 交给 `dsh-pager-grok-ui::run_interactive`。它不实现布局。

## TypeScript server 边界

TypeScript server 的目标分层是稳定 core 与可替换 DSH adapter。core 拥有
gateway、连接级 control-plane、JSON-RPC line transport、buffering、错误载体和
lifecycle；它只依赖项目自有的 `TuiBackend` SPI，不导入具体 Controller、ApiProxy
或 Cordis service 类型。

DSH 的精确版本差异由 adapter family 隔离：`apiproxy-v1` 承载 rc.8/rc.2，
`controllers-v2` 承载 alpha.1 及经验证的后续同族版本。adapter 负责 service
typing、上游事件和稳定 DTO 的归一化、history/follow 语义、交互 waterfall 与启动
断言；profile/runtime 只负责组合，不拥有 TUI 业务语义。详细依赖方向、稳定协议和
SPI 契约见 [DSH 多版本兼容方案 §5–§7](DSH_MULTI_VERSION_COMPATIBILITY_PLAN.md#5-目标架构)。

## 数据流

```text
RPC notifications
       │
       ▼
SessionState / ControlPlaneStore
       │  snapshot + commands
       ▼
GrokHostAdapter ──────► Grok views/widgets ──────► terminal
       ▲                         │
       └────── UiEffect ◄────────┘
```

唯一允许跨层的方向是 runtime → adapter → Grok view；Grok view 不反向调用 RPC。这样未来可以把同一个 host adapter 接到 Codex CLI 或其他兼容 harness。

## 迁移期间的兼容规则

旧 `app.rs`、`block_viewer.rs`、`picker.rs`、`queue_view.rs`、`selection.rs` 不属于新 UI 的默认编译边界。需要比较行为时在原项目或 `git` 历史中查阅，不把旧实现重新接回 successor 的入口。

`scrollback`、`presentation` 如果仍被 session 用作中间数据结构，可以暂时保留；它们不是 Grok 的视觉入口，待 adapter 完成后再按测试覆盖逐步缩小。
