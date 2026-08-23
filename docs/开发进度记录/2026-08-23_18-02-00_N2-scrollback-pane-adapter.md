# N2 ScrollbackPane adapter

> 记录时间：2026-08-23 18:02:00 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

Grok renderer parity 的 transcript 生产路径不能同时维护 RichTranscript 的
本地高度/anchor 索引和 DSH Scrollback 的 Fenwick 索引。本批让 DSH Scrollback
继续拥有稳定 entry identity、partial replacement、宽度相关高度与 anchor，
让 Grok-derived transcript projection 只缓存结构化 block 行并消费同一布局。

## 设计契约和复用依据

- `dsh_pager::Scrollback` 是 host-owned presentation/layout authority；UI 不复制
  entry identity 或协议 payload。
- `ScrollbackPane` 仅把 `DshRenderEntry` 投影为 Grok semantic lines，并将实际
  semantic height 回报给 Scrollback；viewport、anchor 和 paint window 来自
  `ScrollbackLayout`。
- structured Markdown/Diff/tool/image/unknown block 仍经 `render_row` 绘制，
  不将结构化数据重新 flatten 成普通文本。

## 计划修改范围

- `crates/dsh-pager/src/scrollback.rs`：增加 renderer-owned measured-height
  回报边界与测试。
- `crates/dsh-pager-grok-ui/src/views/transcript.rs`：新增 ScrollbackPane，
  缓存语义行并消费 DSH layout。
- `crates/dsh-pager-grok-ui/src/views/mod.rs`：导出 pane。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：production transcript 改用 pane。
- 本记录文件：收尾回写。

## 风险、回滚和依赖

- 风险：语义 block 行数与 plain fallback 不同；必须在每次 entry upsert/宽度
  变化时回报实际高度，不能继续使用旧估算值绘制。
- 风险：entry replacement 后旧行缓存残留；缓存 key 同时比较 stable entry DTO
  和 width。
- 回滚：回退本记录单一 commit，保留已完成的 N1 controller/effect commits。

## 实际修改

- `Scrollback::set_rendered_height` 增加纯 renderer adapter 边界；DSH Fenwick
  layout、stable anchor 和 partial/upsert identity 仍由 core 持有，外部语义
  renderer 可以回报真实 block 行高。
- 新增 `ScrollbackPane`：按 stable `DshRenderEntry` 缓存 Grok semantic lines，
  将 Markdown/Diff/tool/image/unknown block 的实际高度回传给 Scrollback，
  paint window、anchor restore、scroll position 全部读取共享 `ScrollbackLayout`。
- runtime production transcript 已切换到 `ScrollbackPane`，不再为每帧构造并
  使用独立的 RichTranscript 高度/anchor 索引；selection/hit geometry 继续
  使用同一 stable entry/line identity。
- 增加 core renderer-height/anchor 测试和 UI pane semantic block/identity 测试。

## 验证结果

- `cargo fmt --all`
- `cargo test -p dsh-pager --lib scrollback --quiet`（11 passed）
- `cargo test -p dsh-pager-grok-ui --lib --quiet`（228 passed）
- `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features -- -D warnings`
- `git diff --check`

## Git 提交

- commit message：`feat: route production transcript through scrollback pane`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_18-02-00_N2-scrollback-pane-adapter.md`

## 未解决问题和下一步

- N3 继续迁移 workspace/dashboard 与 task/subagent pane；N4 再删除 parity 和
  production 中剩余简化 renderer，并执行完整 E2E/PTY 矩阵。
