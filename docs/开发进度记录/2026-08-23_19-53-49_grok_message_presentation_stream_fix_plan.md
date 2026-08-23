# Grok 消息展示与流式终结问题调研及方案记录

- 时间：2026-08-23 19:53:49 +08:00
- 目标：深度调研 `/home/leo/code/grok-build` 的消息分类、Block 组件、折叠/分组和 streaming 生命周期，并将当前 DSH TUI 的“系统/上下文泄漏”和“首个流式消息卡住”两个问题转化为可执行的修复方案文档。
- 背景：真实 DeepSeek Harness 体验显示，DSH 当前把 agent 注入内容按普通可见 Context 行展示；partial assistant 只在最终 `assistant/message` 到达时清理，异常结束或缺失 final frame 时可能长期保持 running/partial。用户要求先完成调研和方案，不在本批次直接改 Rust 实现。
- 设计契约：Grok 源码是前端语义与组件真相；DSH 协议/会话状态是数据与序列真相；适配层负责将 `user/message`、`assistant/chunk`、`assistant/message`、tool、turn/error/EOF 映射到稳定的展示状态。方案必须保留原始历史可追溯性，视图层只做可逆的折叠/隐藏/分组投影，partial 必须有明确的终结态。
- 计划文件：`docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md`、`docs/GROK_RENDERER_PIXEL_PARITY_PLAN.md`。
- 允许修改文件：
  - `docs/开发进度记录/2026-08-23_19-53-49_grok_message_presentation_stream_fix_plan.md`
  - `docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md`
- 风险：Grok-build 是外部源码快照，不能把其内部实现细节直接当作 DSH 协议；上下文可见性若在历史层删除会破坏审计与 replay；streaming 的 fallback 若误把正常延迟当作终止会丢失后续 chunks。
- 回滚：本批次仅新增两份 Markdown 文档；如方案需要调整，可删除本批次提交或回退该提交，不影响 Rust、协议和运行数据。
- 预期校验：文档写入后执行 `git diff --check`，核对允许文件范围，检查引用的 Grok/DSH 路径和 M0-M10/P03/P14/P15 门禁；不运行与代码无关的完整编译。

## 完成记录

- 状态：已完成（仅方案文档）
- 实际修改文件：
  - `docs/GROK_MESSAGE_PRESENTATION_AND_STREAM_FIX_PLAN.md`
  - `docs/开发进度记录/2026-08-23_19-53-49_grok_message_presentation_stream_fix_plan.md`
- 已完成内容：
  - 梳理 Grok `RenderBlock`、`BlockContent`、`ScrollbackEntry`、`ScrollbackState`、`EntryRenderer`、`BlockRenderer`、`ScrollbackPane`、`groups`/`verb_group` 和 finish/streaming 生命周期；
  - 对照 DSH `presentation.rs`、`scrollback.rs`、`transcript.rs`、`session.rs`、`runtime.rs` 定位两个问题的根因；
  - 写出 visibility/classification、stable surface/finalize、view-time fold/group、单测和 Harness/PTY P19/P20 门禁；
  - 明确本批次没有修改 Rust 实现，也没有宣称 bug 已修复。
- 校验结果：
  - `git diff --check`：通过；
  - 文档引用已核对 Grok mirror/source revision、DSH 文件路径、M4.2/M4.6/M9.7、N2/N3、P03/P14/P15；
  - 当前变更仅限本记录声明的两份 Markdown 文件。
- 未解决与下一步：按方案 Phase 0 -> Phase 1 -> Phase 2 实现；先落地 P20 流式终结，再落地 P19 上下文可见性，之后才继续 renderer pixel parity。
