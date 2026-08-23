# N1 nonblocking effect executor 与 RPC pending boundary

> 记录时间：2026-08-23 17:25:00 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

N1-0 的 effect ledger 已经持久化，但 UI action 仍直接调用同步
`RpcTransport::call`。本批把 effect-only RPC 迁移到可轮询的 pending boundary：
键盘/鼠标 dispatch 只提交 request 并返回 `Pending`，下一帧由 runtime 轮询
receipt；通知继续由同一 transport 路由，不能复制 Grok runtime 或引入第二条
backend 连接。

## 设计契约和复用依据

- 对应长期计划：GROK_RENDERER_PIXEL_PARITY_PLAN N1、适配计划 2.4 副作用契约。
- `RpcTransport` 保留现有同步 `call` 给启动/load barrier；新增 request id、
  non-blocking pump 和 completion store，不以缩短 timeout 冒充异步。
- `AsyncEffectExecutor` 只编码 DSH-neutral `UiEffect` 到 JSON-RPC，保存
  operation/session/generation；receipt 先是 `Pending`，最终结果仍需
  authoritative notification/snapshot 收敛。
- File Search/media completion 只在 operation 仍对应当前 query/attachment 时
  写入 UI local surface；旧 generation/旧 revision 不覆盖新状态。

## 计划修改范围

- `crates/dsh-pager/src/transport.rs`：pending JSON-RPC request/completion API，
  保持通知和同步 call 兼容。
- `crates/dsh-pager/tests/transport_contract.rs`：乱序 response、pending
  completion 与 notification 保留的 transport contract fixture。
- `crates/dsh-pager-grok-ui/src/effects.rs`：新增 async executor、effect request
  编码、ApiResult/preview 解码和 typed completion。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：UiState 持有 executor；File Search、
  Image、queue、interaction、workspace mutation、prompt、subagent interrupt
  effect 改走 pending submit；每帧轮询 completion。
- 本记录文件：收尾时写入实际结果、测试和提交审计。

## 风险、回滚和依赖

- 风险：响应可能乱序；completion 必须按 transport request id 匹配，不能按数组
  位置；旧 query/attachment 只能变成诊断。
- 风险：同步启动/attach barrier 与异步 effect 共用 transport；pump 必须把响应
  放回 completion store，`try_notification` 不得把 response 误报成 notification。
- 回滚：回退本记录单一 commit，保留上一批持久 ledger/File Search contract。
- 依赖：上一批 `EffectLedger` commit `1a4257466d4e305c014dfc959b71aea065d6756e`。

## 实际修改

- `RpcTransport` 新增 `begin_call_value`/`poll_call_value` 和乱序 response
  completion store；同步 `call` 复用同一 pump，notification 不再把 pending
  response 当成错误帧。
- `AsyncEffectExecutor` 编码并追踪 Prompt、Queue、Interaction、File Search、
  Media、Workspace mutation 和 Subagent interrupt；提交立即返回 `Pending`，
  下一帧按 request id 解码 typed receipt，保留 file/media typed payload。
- `UiState` 持有 executor，在每个 loop tick 轮询 completion；旧 session/generation、
  File Search revision 和 attachment id 的 completion 会被丢弃并留下诊断。
- Prompt draft 不再因 `Pending` receipt 被清空；只有 host `Accepted/Queued` 才清空。
- 增加 transport 乱序/notification fixture，以及 async request/result 解码单测。

## 验证结果

- `cargo fmt --all -- --check`
- `cargo test -p dsh-pager --test transport_contract`
- `cargo test -p dsh-pager-grok-ui --lib`
- `cargo clippy -p dsh-pager -p dsh-pager-grok-ui --all-targets --all-features -- -D warnings`
- `git diff --check`
- 结果：transport 2 tests、UI 221 tests 通过，clippy/fmt/diff check 通过。

## Git 提交

- commit message：`refactor: add nonblocking effect executor`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_17-25-00_N1-nonblocking-effect-executor.md`

## 未解决问题和下一步

- attach/load-session 仍是显式 barrier，需在 workspace/controller 批次中迁移为
  generation-guarded coordinator；本批不会伪造 attach 已异步完成。
- 后续 N1 controller 仍需 vendor Grok File Search、Suggestion/history、Image
  视觉闭包与 reference fixtures。
