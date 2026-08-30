# DSH TUI v1 Phase 0 基线清单

> 基线日期：2026-08-30
> 当前源码基点：`68d9eca086b198b7731b933ed9a0fea28a13d329`
> legacy 基点：`legacy/main-before-harness-0.1.2-alpha.1`
> (`e49c1267bc2177ac8f2019d1c04753c4992481bf`)

本文冻结 Phase 0 的可恢复事实，不扩展协议，也不表示 rc.2 已获得支持。后续行号
均相对于上面的不可变 Git 基点；Phase 1 以后工作树行号发生移动时仍可用
`git show <基点>:<路径>` 复核。

## TUI v1 method catalog

TypeScript runtime catalog 位于
`packages/dsh-tui-protocol/src/methods.ts:14-88`。当前总数是 **65**：55 个 unary
业务方法、5 个 TUI 控制请求、5 个 server notification。catalog 使用 `as const`
冻结；统一类型名为 `TuiUnaryMethod`，method 字符串保持基线不变。

### Unary（55）

- Session（12）：`session.list`、`session.search`、`session.create`、
  `session.history`、`session.models`、`session.selectModel`、`session.rename`、
  `session.fork`、`session.prompt`、`session.attachment`、`session.updateQueue`、
  `session.cancel`。
- Subagent（4）：`subagent.list`、`subagent.history`、`subagent.prompt`、
  `subagent.interrupt`。
- Host（5）：`host.describe`、`host.pickDirectory`、`host.listDirectory`、
  `host.createDirectory`、`host.openPath`。
- File/command（3）：`fileReferences.list`、`commands/list`、`commands/execute`。
- Workspace（7）：`workspace.list`、`workspace.create`、`workspace.rename`、
  `workspace.delete`、`workspace.insertBefore`、`workspace.insertSessionBefore`、
  `workspace.archiveSession`。
- Skill（1）：`skill.list`。
- Agent preset（6）：`agentPreset.list`、`agentPreset.select`、`agentPreset.read`、
  `agentPreset.copy`、`agentPreset.openDocument`、`agentPreset.remove`。
- Goal（6）：`goal.create`、`goal.edit`、`goal.pause`、`goal.resume`、
  `goal.complete`、`goal.clear`。
- Settings（5）：`settings.describe`、`settings.openDocument`、`settings.update`、
  `settings.replace`、`settings.mutate`。
- Credentials（3）：`credentials.describe`、`credentials.set`、
  `credentials.unset`。
- LLM（3）：`llm.providers`、`llm.models`、`llm.discoverModels`。

### TUI 控制请求（5）

`tui.hello`、`tui.attach`、`tui.detach`、`tui.subscribe`、`tui.respond`。

### Server notification（5）

`tui.serverReady`、`tui.serverDraining`、`tui.controlPlaneBaseline`、`events.mux`、
`events.host`。

## Frame 类型清单

`packages/dsh-tui-protocol/src/types.ts:64-107` 冻结 server 端的 payload union：

- `MuxFrame`（10）：`session/event`、`session/subscribed`、`approval/requested`、
  `approval/resolved`、`question/requested`、`question/resolved`、`session/queue`、
  `session/jobs`、`session/projection`、`stream/error`。
- `HostFrame`（10）：`host/session-added`、`host/session-removed`、
  `host/session-status`、`host/agent-error`、`host/workspace-changed`、
  `host/workspace-removed`、`host/workspace-order-changed`、
  `host/archived-sessions-changed`、`host/remote-event`、`stream/error`。
- `TuiMuxFrame` 给 `approval/requested` 与 `question/requested` 增加原始
  `requestId`；`TuiStampedMuxFrame` / `TuiStampedHostFrame` 可增加 connection
  `generation`。

Rust canonical 包 `crates/dsh-pager-protocol/src/lib.rs:822-863` 当前定义四类 line
carrier：`JsonRpcLine::Request`、`Success`、`Failure`、`Notification`，并为 hello、
attach、subscribe、respond 及主要业务 DTO 提供强类型。当前 Rust 包没有与
`MuxFrame` / `HostFrame` 一一对应的判别 union；notification `params` 仍以
`serde_json::Value` 承载，由 `crates/dsh-pager` 的 session/control-plane reducer
按 `type` 判别。Phase 2 的跨语言 fixture canonical ownership 需要据此明确，不能
把现状误写成已有双端 frame codegen。

## alpha.1 bridge 高风险行为

文件：`packages/dsh-tui-server/src/bridge.ts`，行号基于本文顶部当前源码基点。
以下六类与多版本方案 §7.3 一一对应：

1. **session list/summary**：controller contract `30-49`；unary dispatch
   `327-351`；projection 扁平化 `1195-1209`；Host session summary frame
   `1211-1224`。风险是 roster projection、parent/origin/cwd/preset 字段在同族版本
   间漂移。
2. **history opening snapshot/page/chunk rows**：record/snapshot 状态
   `164-215`；follow opening barrier `504-549`；session/subagent history、page 与
   cold inspect `619-733`；chunk row 解码和分页 `1106-1155`。风险是 opening
   cursor、old-page `beforeSeq`、chunk expansion 和 projections 必须保持同一 barrier。
3. **live follow/control frames**：follow event 映射 `504-549`；control baseline
   与 queue/jobs/projection 增量 `882-927`；Host lifecycle 监听 `973-992`。风险是
   request identity、sequence、重复/丢失和 stream close 语义。
4. **approval/question requests and responses**：pending union `218-239`；应答校验
   与 at-most-once 删除 `583-615`；waterfall 安装、claim、abort 与 frame 投影
   `994-1084`。风险是只 claim attached runtime root、abort fallback、重复应答和
   reconnect 时 pending replay。
5. **workspace baseline/follow frames**：workspace union `191-196`；冷 baseline
   `861-879`；follow pump 与 cache mutation `929-971`。风险是 baseline 先后顺序、
   workspace order、archive 集合以及 cache 与增量收敛。
6. **model/preset/goal/subagent results**：subagent unary 与 history
   `352-367,655-703`；preset/goal unary `417-461`；model/provider projection
   `766-849`；agent resolution/goal service selection `802-821,850-854`。风险是
   optional service、agent-scoped goal service、model selection fallback 和不同版本
   result shape 被 `Record<string, unknown>` 掩盖。

## rc.8 legacy 可恢复代码对照

legacy 分支没有 `bridge.ts`。ApiProxy 适配逻辑散落在 core 文件中，Phase 4 只能
恢复职责并实现 `TuiBackend`，不能恢复整份旧 gateway：

- `packages/dsh-tui-server/src/gateway.ts`
  - `12-14`：直接导入 `ApiProxy`、`MuxFrame`、`HostFrame`、`RpcId` 和 DSH
    `SessionId`。
  - `100-104`：`TuiGateway` 直接持有 `ApiProxy` 与 `TuiDispatchExtensions`。
  - `360-392`：直接调用 `api.events.mux` / `api.events.host` 并剥离 ApiProxy envelope。
  - 其余 hello、buffer、control-plane 和 generation 逻辑属于应保留的共享 core，
    不作为 legacy adapter 复制目标。
- `packages/dsh-tui-server/src/dispatch.ts`
  - `8-17`：导入 `toFetchHandler`、ApiProxy carrier/brand 与 extension 所需 DSH 类型。
  - `19-24`：定义 file reference、agent resolver、command runtime extensions。
  - `34-68`：用 `toFetchHandler(api)` 构造 `/api/<method>` Request，封装
    `client-request` / `rpcId` / `payload` 并返回 `ServerResponse.result`。
  - `211-220`：用 `api.respond` 拼接 `client-response` carrier。
- `packages/dsh-tui-server/src/index.ts`
  - `14-16,39-43`：声明 remote resolver/command 依赖和 Cordis inject。
  - `62-76`：从 context 取得 `fileReferences`，创建 agent resolver，并把
    `resolveAgent` / `commands` extensions 传给 `serve`。
- `packages/dsh-tui-server/src/serve.ts`
  - `8-21`：把 ApiProxy 与 extensions 暴露为组装层类型。
  - `32-46`：拆出 transport options，并把 extensions 拼接进旧 gateway 构造器。

Phase 4 恢复时的责任落点：ApiProxy carrier、events mux/host 和 response 归入
`apiproxy-v1` adapter；gateway/control-plane/transport 继续复用当前 core；
fileReferences/commands 的 adapter-local normalization 通过共同 conformance suite
验证，而不是在 gateway 重新增加 DSH imports。
