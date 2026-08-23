# M4-M5 rich transcript 与 Prompt 收口切片

> 记录时间：2026-08-23 12:00:36 +0800
> 操作者/agent：Codex /root
> 状态：completed

## 目标与背景

在上一批 AgentView/scroll geometry 基础上，让结构化 transcript renderer 真正进入默认绘制链，并补齐 Prompt 的本地提交状态、mode/capability 反馈和 editor 测试边界，减少 runtime 对 Paragraph/fallback 文本的直接依赖。

## 设计契约和复用依据

- 对应长期计划章节：M4.1-M4.6、M5.1-M5.5。
- Grok source path、commit、SOURCE_REV：`vendor/grok/xai-grok-pager/{views,input}`；mirror `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，SOURCE_REV `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：A1/B；沿用 Grok line editor/modal/status primitives，DSH 只提供 rich DTO、identity、capability 和 effect receipt。

## 计划修改范围

- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- `crates/dsh-pager-grok-ui/src/input/mod.rs`
- `crates/dsh-pager-grok-ui/src/effects.rs`
- `crates/dsh-pager-grok-ui/src/parity.rs`
- 本记录文件。

## 预期行为

- 默认 transcript 绘制根据结构化 block/entry kind 选择 Grok semantic role，保留 stable entry/line identity 和 copy payload。
- Prompt 提交前后状态、Queue/Steer capability 和失败 receipt 文案可测试；失败不清空草稿。
- semantic runner 与真实绘制共享可观察的 block role 和 prompt mode 语义。

## 不在范围内

- 不改 DSH protocol/host source、vendor 上游源码或真实 Harness。
- 不在本批完成完整 Grok AppView 文件树、50k soak、完整 reference golden、外部 editor/pager 真实进程 handoff 或 M11 fallback 删除。

## 风险、回滚和依赖

- 需要保持上一提交中的 AgentView layout、scroll anchor 和 queue/interaction effect 行为不变。
- rich line style 若与滚动缓存行数不一致会造成 selection 几何回归，必须优先保证 PaintLine identity。
- 回滚为恢复本记录列出的文件和提交。

## 实际修改

- `views/transcript.rs`
  - 新增 `RichTranscript` 宽度敏感投影，默认绘制通过 `render_row` 处理 Markdown、Reasoning、ToolCall、ToolResult、Diff、Image、Unknown 等结构化 block。
  - 维持 `DshRenderEntryId + line_index` stable identity；screen y、spacer 行、anchor 和 soft-wrap 使用同一份几何。
  - 增加结构化 block、Unicode wrap、stable identity 和 fallback block coverage 测试。
- `runtime.rs`
  - AgentView transcript 绘制改用 rich projection，不再以 `Scrollback::visible_lines` 的扁平文本作为生产绘制源。
  - selection/hit map 与 rich line geometry 对齐，保留 spacer 行和滚动 anchor。
  - Prompt receipt 按 Accepted/Queued/Pending 反馈 admission 状态；Rejected/Conflict/Stale/Failed 保留草稿。
  - Steer 模式要求 host 通过 `queue_steer`/`queueSteer` projection 明确声明 capability，并增加 capability 测试。
- `parity.rs`
  - semantic runner 复用相同 rich transcript projection、wrap geometry 和 prompt mode 文案。

## 验证结果

- `cargo fmt --all`
- `cargo check -p dsh-pager-grok-ui`
- `cargo test -p dsh-pager-grok-ui --locked`：181 tests + doctests
- `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features -- -D warnings`
- `cargo test --workspace --locked`
- `scripts/e2e.sh`：protocol/source manifest、648-case semantic parity、workspace tests、M8-M10 E2E 均通过
- `git diff --check`

## Git 提交

- commit message：`feat: complete M4-M5 rich transcript and prompt slice`
- Progress-Record trailer：`Progress-Record: docs/开发进度记录/2026-08-23_12-00-36_M4-M5-rich-transcript-prompt.md`
- 暂存区审计：仅包含本记录列出的 4 个文件；未修改 DSH protocol、host source、vendor 或真实 Harness。

## 未解决问题和下一步

- 本批完成的是 M4/M5 rich transcript 与 Prompt 收口切片，不宣称完整 M2-M5 parity。
- 外部 editor/pager handoff、完整 Grok AppView 文件树、50k soak、reference golden 和 M11 fallback 清理仍不在范围内。
