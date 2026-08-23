# M2-M5 主界面、渲染与 Prompt 迁移

> 记录时间：2026-08-23 11:45:28 +0800
> 操作者/agent：Codex /root
> 状态：partial

## 目标与背景

将当前可运行但仍以 `runtime.rs` 手工布局为核心的 UI 推进到 M2-M5 的第一条生产切片：统一终端/布局失效语义，建立 Grok-derived AppView/AgentView 渲染边界，保留结构化 transcript block 并接入统一 scroll/wrap，补齐 Unicode 多行 Prompt 的渲染、提交和 resize 行为。

## 设计契约和复用依据

- 对应长期计划章节：M2、M3、M4、M5。
- Grok source path、commit、SOURCE_REV：`vendor/grok/xai-grok-pager/{views,prompt,input}`；mirror `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，SOURCE_REV `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：A1/B；保留 Grok picker/modal/status/timeline/line-editor 的状态规则，适配只消费 `GrokHostSnapshot` 和 `UiIntent`。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/app.rs`
- `crates/dsh-pager-grok-ui/src/views/agent.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- `crates/dsh-pager-grok-ui/src/views/mod.rs`
- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- `crates/dsh-pager-grok-ui/src/input/mod.rs`
- `crates/dsh-pager-grok-ui/src/geometry.rs`
- `crates/dsh-pager-grok-ui/src/render/wrapping.rs`
- `crates/dsh-pager-grok-ui/src/render/scrollbar.rs`
- `crates/dsh-pager-grok-ui/src/theme.rs`
- `crates/dsh-pager-grok-ui/src/parity.rs`
- `crates/dsh-pager-grok-ui/src/lib.rs`
- 相关 `crates/dsh-pager-grok-ui` 单测和必要的 semantic fixture。
- 本记录文件。

## 预期行为

- 所有主屏事件经 `AppShell` 路由，布局和 hit map 在内容/尺寸变化时失效。
- 主屏渲染以 Grok-derived AgentView/Transcript/Prompt 组件边界组织，不新增 host truth。
- transcript 保留 block kind/content/identity，软换行和滚动高度使用统一 width-aware 逻辑。
- Prompt 保留 Unicode grapheme、换行和尾部空格；重复提交、resize 后光标漂移和旧 hit map 命中有测试证据。

## 不在范围内

- 不复制 Grok agent loop、ACP、shell、persistence 或配置运行时。
- 不修改 DSH wire protocol、真实 Harness checkout 或 vendor 上游源码。
- 不在本批宣称完整 Grok reference golden、50k soak、真实 queue/interaction lifecycle 或 M11 fallback 删除完成。

## 风险、回滚和依赖

- 现有 `runtime.rs` 是默认入口，分拆渲染时需保持 picker/queue/interaction effect 语义不变。
- ratatui 的 wrap/coordinate 语义可能与 Grok snapshot 有差异，先用 semantic tests 固化 invariant。
- 回滚为恢复本批列出的代码文件和本记录；不修改 host/protocol authority。

## 实际修改

- `crates/dsh-pager-grok-ui/src/views/agent.rs`：新增无 host/renderer 依赖的
  AgentView 主屏几何边界，宽屏/窄屏统一 transcript/rail/prompt/footer 布局。
- `src/runtime.rs`：默认渲染路径改用 AgentView 布局；scrollback materialization、
  anchor、宽度失效和 PaintLine 几何统一驱动画面、hit map 与 selection；session
  attach/resize 时清理旧 transcript view；Prompt 接入 Queue/Steer mode、mode
  chrome、capability 检查和 Unicode paste 边界。
- `src/app.rs`：AppShell 增加 Alt-S Prompt mode intent，并保留统一 owner 路由。
- `src/views/transcript.rs`：为 materialized scrollback line 增加 Grok semantic
  role style；补充结构化 block 和工具/错误行测试。
- `src/parity.rs`：semantic/reference runner 改用 AgentView 的同源布局。
- `src/input/mod.rs`：补充控制字符过滤、tab/换行保留测试。
- `src/lib.rs`、`src/views/mod.rs`：注册并导出 AgentView 边界。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo check -p dsh-pager-grok-ui`：通过。
- `cargo test -p dsh-pager-grok-ui --locked`：通过，176 tests + doctests。
- `cargo test --workspace --locked`：通过。
- `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features -- -D warnings`：通过。
- `scripts/e2e.sh`：通过；protocol/source/matrix、workspace test、all-features
  clippy、binary build 和 full mock PTY（resize/mouse/overlay/restore）均通过。
- `git diff --check`：通过。

## Git 提交

- commit message：待补充。
- Progress-Record trailer：`Progress-Record: docs/开发进度记录/2026-08-23_11-45-28_M2-M5-main-render-prompt.md`
- 暂存区审计：待补充。

## 未解决问题和下一步

- 本批仍未完成完整 Grok AppView/AgentView 文件树迁移和 fallback shell 删除；
  `runtime.rs` 仍承载 effect orchestration，AgentView 当前先承载同源主屏几何。
- transcript 的长历史虚拟化和 anchor 已接入默认路径，但完整 Grok markdown/code/
  diff/tool renderer golden、50k 性能门禁和真实 reference 对照仍待 M4/M10。
- Prompt 的 Queue/Steer 和多行 Unicode 语义已接通；外部 editor/pager 的真实进程
  handoff、完整 mouse selection golden 和 real backend interaction matrix 仍待后续。
