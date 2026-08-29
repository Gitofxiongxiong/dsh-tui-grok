# dsh-tui-grok 升级到 DeepSeek Harness 0.1.2-alpha.1 调整分析

> 分析日期：2026-08-29
> 方法：只读 git 比对（`deepseek-harness` 0.1.0-rc.8 `141eb6fef8` ↔
> `DeepSeek-Harness-latest` 0.1.2-alpha.1 `cd5ef81481`，1286 commits）+ 逐项源码实测
> 校正：2026-08-29 18:49 +0800（补充字段级契约、历史流生命周期与改动规模）
> 上游差异总览见同目录 [deepseek-harness-版本差异分析.html](deepseek-harness-版本差异分析.html)。

## 一、结论摘要

**dsh-tui-grok 目前整体钉死在 Harness `0.1.0-rc.8`，升级到 `0.1.2-alpha.1`
是一次大型 Host 集成层改造：TypeScript server 的 dispatch/stream/interaction 核心
需要重接，profile、依赖和发布链需要系统调整；Rust/Grok UI 不需要全面重写，
但 Rust 侧不是零改动。**

- Rust 与 Harness 之间隔着 TS Cordis 插件（dsh-tui-server 等），Rust 只消费自有
  TUI 线协议（JSON-RPC v1：`tui.*` 控制方法 + `session.*`/`workspace.*` 等 unary +
  `events.mux`/`events.host` 通知流）。只要 TS 适配层把新 Harness 表面翻译回同一套
  帧词汇，就能保留现有 session/control-plane/Grok 视图主体。
- Rust 必须补齐新契约：`session.prompt` 和 `subagent.prompt` 均新增必填
  `requestId`；异步 UI effect 应直接使用已有 `OperationKey.request_id`。
- 真正的大面积破坏集中在 TS 包、profile patch、依赖版本和启动链：
  **`ctx.apiProxy` 整体消失**、**code-mode 改名 ptc**、**preset 目录迁移**、
  **`@deepseek-ai/dsh-*` 依赖必须 0.1.0-rc.8 → 0.1.2-alpha.1**。
- 推荐保留 TUI v1 作为稳定边界，在 `dsh-tui-server` 内新增
  `TuiHarnessBridge`；不要让 Rust 直接追踪 Typert Remote 和浏览器客户端内部类型。

## 二、现状：dsh-tui-grok 与 Harness 的接线（实测确认）

```
Rust TUI (dsh-pager-bin / dsh-pager / dsh-pager-grok-ui)
   │  JSON-RPC 2.0 行协议（stdio），TUI_PROTOCOL_VERSION=1
   ▼
dsh-tui-server（Cordis 插件，inject ['apiProxy','agents','sessions','commands']）
   │  ctx.apiProxy（unary fetch 载波 + events.mux/host 流 + respond）
   ▼
Harness 进程内（profile dsh-pager-grok，dsh.bundle patch 组合）
   ├─ dsh-tui-embedded/cordis.patch.yml：挂 storage、workspace、directory-picker、
   │    api-gateway(@deepseek-ai/dsh-host-apiproxy)、agent-presets、code-runtime 等
   ├─ dsh-tui-session-projection-recovery：装饰 apiProxy.sessions.list 补冷行投影
   └─ dsh-pager-runtime：把 20+ 个 @deepseek-ai/dsh-*@0.1.0-rc.8 钉进 profile
启动链：node <harness>/apps/cli/lib/bin.js --profile dsh-pager-grok（--backend/--backend-arg）
产品链：@dsh-pager-grok/cli 依赖 @deepseek-ai/dsh@0.1.0-rc.8 + @dsh-pager-grok/runtime
```

Rust 侧调用方式为**方法名字符串 + JSON**（`api_call(transport, "agentPreset.list", …)`），
不绑定 Harness 编译期类型——这是升级成本集中在 TS 层的前提。

## 三、破坏面清单（逐项验证）

### 3.1 启动链：`apps/cli/lib/bin.js` 在新检出中不存在（阻塞项）

- 旧检出有构建产物 `apps/cli/lib/bin.js`（git 不跟踪，tsdown 产物，src 入口
  `apps/cli/src/bin.ts`）；新检出 `apps/cli/` 只有 `src/`，**`lib/` 未构建**。
- `apps/cli/package.json` 的 bin 声明仍是 `"dsh": "lib/bin.js"`；`args.ts` 逐字节未变
  （`--profile`/`--backend`/`--backend-arg`/`plugin` 均保留）。
- 影响：`scripts/dsh-tui-common.sh` 的 `dsh_tui_resolve_harness_root` /
  `dsh_tui_require_harness_entry` 会直接报 "checkout not found"。
- 处置：新检出先 `pnpm install && pnpm build`（或产品链改用 npm 发布的
  `@deepseek-ai/dsh@0.1.2-alpha.1`）；脚本候选路径加入 latest 检出或靠
  `DSH_HARNESS_ROOT` 显式指定。

### 3.2 依赖版本：`@deepseek-ai/dsh-*` 0.1.0-rc.8 → 0.1.2-alpha.1（阻塞项）

涉及所有 TS 包的 package.json（dsh-tui-server、dsh-tui-embedded、dsh-pager-runtime、
dsh-pager-cli、dsh-tui-protocol、dsh-tui-session-projection-recovery）：

| 旧依赖 | 新状态（实测） |
|---|---|
| `@deepseek-ai/dsh-host-apiproxy` | **已从仓库删除**，无 alpha.1 版本；改用 base 自带的 `dsh-api-gateway`（typert-gateway）+ 三个域控制器 |
| `@deepseek-ai/dsh-api-remotes` | 仍在（`packages/api/remotes`），但重构为 Typert 组装面（`inject: ['typertGateway']`），**`createApiRemoteAgentResolver` 已不存在** |
| `@deepseek-ai/dsh-commands` | 仍在（`packages/interaction/commands`），`ctx.commands` 保留，新增 `/remote` 导出 |
| `@deepseek-ai/dsh-agent-presets` | 仍在，preset 改为包内自带 shipped root；`apps/cli/config/agent-presets/` 目录已删 |
| `@deepseek-ai/dsh`（dsh-pager-cli 产品依赖） | 0.1.0-rc.8 → 0.1.2-alpha.1 |
| storage / workspace / file-reference-local / code-runtime-worker-thread / cordis-host-runner / directory-picker-browse 等 | 包名全部保留 |

> 版本不一致后果：新 CLI（alpha.1）加载 rc.8 插件会产生**双份核心包**（dsh-session
> 等，品牌/服务不互通），且 `ctx.apiProxy` 不存在导致 tui-server `inject` 直接失败。

### 3.3 服务面：`ctx.apiProxy` 消失，替换为 Typert Remote 域控制器

旧：`ctx.apiProxy`（每域手写 zod schema + `POST /api/<method>` unary +
`events.mux/host` 流 + `respond`）。
新：base patch 已自带 typert 栈（`typert` / `typert-loader` / `typert-gateway`），
域控制器需 profile 挂载：

- `@deepseek-ai/dsh-api-session-controller` → 服务名 `sessionController`（namespace `session`）
- `@deepseek-ai/dsh-api-settings-controller` → settings + credentials
- `@deepseek-ai/dsh-api-workspace-controller` → workspace + directory-picker
- `@deepseek-ai/dsh-subagent`、`@deepseek-ai/dsh-goal`、`@deepseek-ai/dsh-llm`、
  `@deepseek-ai/dsh-agent-presets` 各自有 `/remote` 导出

浏览器侧组装在 `@deepseek-ai/dsh-api-remotes`（`ctx.remote.session` 等）。
**TUI 适配层不需要走 HTTP/WS**：控制器是普通 Cordis 服务，方法签名
`(request, signal?)`，流方法返回 `AsyncIterable`，可直接进程内调用——这保持了
嵌入 profile"不挂 webserver、不挂浏览器运行时"的设计。

`@deepseek-ai/dsh-api-remotes` 现在是浏览器/BFF 的转发事件源，依赖
`typertGateway`；它不再提供 `createApiRemoteAgentResolver`。嵌入式 TUI 不应为了
兼容而挂载这一层，应直接调用 Host service 并用
`sessionController.resolveAgent(sessionId)` 做 agent 解析。

### 3.4 方法映射表：TUI catalog 的 55 个 unary 方法逐一对应

依据官方迁移笔记 `.agents/notes/implemented/architecture/2026-08-10-unary-apiproxy-remote-migration.md`
+ 新版源码实测：

| TUI 线方法 | 新版目的地 | 备注 |
|---|---|---|
| session.list / search / create / selectModel | `sessionController.list/search/create/selectModel` | list 行的 preset/model 信息需从 `projections.values` 展平到旧 wire |
| session.models | `sessionController.modelCatalog` + `modelSelection` projection | 不只是改名；新值无 `current/routable`，需用 `next ?? lastUsed ?? default` 重建 |
| session.rename / fork / attachment / updateQueue / cancel | `sessionController` 同名 | 返回成功值需包装为旧 `ApiResult` |
| **session.prompt** | `sessionController.prompt` | 新请求必填 client-minted `requestId`，当前 Rust payload 缺少 |
| **session.history** | `sessionController.follow` opening snapshot + `page` 反向页 | 请求改为 `{address, throughSeq, beforeSeq?, maxMessages?}`；返回 `records: event \| chunks`，chunk run 必须无损展开 |
| host.openPath | `sessionController.openWorkspacePath` | 路径解析上移客户端 |
| host.describe | **删除** | ready 帧 + 能力查询替代 |
| host.pickDirectory / listDirectory / createDirectory | `directoryPickerController.pick/list/createDirectory` | 该 controller 由 workspace-controller 包挂 |
| workspace.create / rename / delete / insertBefore / insertSessionBefore / archiveSession | `workspaceController` 同名 | 请求主体大体保留 |
| workspace.list | `workspaceController.follow` opening baseline | 新版无 list unary，桥层缓存 `{items, archivedSessionIds}` 回答旧请求 |
| skill.list | `sessionController` 内 SessionSkillCatalog | |
| fileReferences.list | `sessionController` 内 SessionFileReferences | agent 解析改用 `sessionController.resolveAgent()`（替代已删除的 resolver） |
| commands/list、commands/execute | `CommandRuntime`（`@Remote` 同名） | `ctx.commands` 仍在 |
| agentPreset.list / select / read / copy / remove | `ctx.agentPresets` 直接方法 | list 的 `hasDocument` 需与 `settingsController.canOpenAgentPresetDirectory()` 合并；select 先 resolve agent |
| agentPreset.openDocument | `settingsController.openAgentPresetDirectory` | 旧/新请求与返回形状需包装 |
| goal.* / llm.* / credentials.* / settings.* | `ctx.goals` / `ctx.llm` / `credentialsController` / `settingsController` | 多个新方法改为位置参数或 `void`，桥层必须恢复旧 value shape |
| subagent.list / prompt / interrupt | `ctx.subagents.remoteExportList/prompt/interruptByParent` | prompt 同样必填 `requestId` |
| subagent.history | `sessionController.page` + subagent `address` | 新版已无独立 history 方法，必须保留 parent/mode 授权语义 |

所有新 controller/Remote 失败通过 `TypertRemoteFailure.failure` 暴露
`{code,message,details}`。`TuiHarnessBridge` 应在一个公共边界统一翻译为旧
`{ok:false,error}`，不要让每个 dispatch case 各自发明错误映射。

### 3.5 流：`events.mux` / `events.host` 的替代（tui-server 重写核心）

- 旧 mux 帧（session/event、session/subscribed、session/projection、session/queue、
  session/jobs、approval/requested、question/requested、approval/resolved、
  question/resolved）→ 新：
  - `sessionController.follow`（stream）：**opening snapshot（header+cursor+records+
    projections 基线）+ 顺序事件帧** —— 等价于"旧 history 尾页 + live 帧"，cursor
    可替代 session/subscribed 的 lastSeq；
  - `sessionController.control`（stream）：**baseline（queues/jobs/projections）+
    queue/jobs/projection 替换帧**；
  - **审批/问答不再走帧**：改为转发的 waterfall 事件 `approval/request`、
    `user-questions/request`（见 3.6）。
- 旧 host 帧（host/session-added、session-removed、session-status、agent-error、
  workspace-changed、workspace-removed、workspace-order-changed、
  archived-sessions-changed）→ 新：
  - `workspaceController.follow`（stream）：baseline + `upsert/remove/order/archived` 帧；
  - session 生命周期：`api-session/added|removed|status|error` 宿主事件。
- **TUI v1 的方法名和通知帧词汇不需重做/升版**：新 TS 适配层把以上
  帧翻译回旧词汇即可，Rust `apply_mux/apply_host` 原样工作；但 prompt
  params 要做向后可兼容的 `requestId` 字段扩展。tui-server 的 control-plane store
  （重复帧折叠、watermark、历史加载屏障）可复用，只换输入源。

#### 3.5.1 history/follow 生命周期（不能只做字段重命名）

现有 Rust 加载顺序是 `tui.attach` → `session.history` → 释放缓冲 live 帧。
新桥层应保持这个可观测契约：

1. `tui.attach` 为该 session 启动一个 follower，记录 opening snapshot 的
   `header/cursor/records/hasMore/projections`。
2. 第一个旧 `session.history` 请求从 opening snapshot 组装
   `{events,hasMore,projections}`；这之前到达的 live 帧仍进现有 load barrier。
3. `load_older` 的 `beforeSeq` 请求转为 `sessionController.page`，并使用该
   follower 的 opening `cursor` 作为 `throughSeq`，防止翻页期间新事件改变页面截面。
4. `records.type === 'chunks'` 的 `chunkrow/text-chunks`、`reasoning-chunks`、
   `tool-call-chunks` 必须还原为原始 `assistant/chunk` 事件，保持 seq/time/
   token 边界；否则 Rust 只会将 chunkrow 当未知事件。
5. detach/重连/generation 切换时 abort follower，丢弃旧代缓冲；重连必须从新
   snapshot 重建 baseline，不宣称无损 stream resume。

Dashboard 的 `peek_session_tail` 有一个新版 API 缺口：它要求不 attach、
不激活 Agent，而普通 session 的 `follow` 在 snapshot 交付后会后台激活。
短期只能用 `sessionController.inspect(sessionId)` 冷读全日志再本地分页
（语义正确，长历史成本高）；长期应向 Harness 补一个正式 cold-tail/cursor
能力，不能用 follow 偷换 peek 的“不激活”契约。

### 3.6 审批/问答应答：`api.respond` → waterfall 事件应答

- 旧：`dispatchRespond` 走 `apiProxy.respond({type:'client-response', rpcId, result})`。
- 新：同进程 UI 作为 answerer 通过 `ctx.on('approval/request', ...)` 与
  `ctx.on('user-questions/request', ...)` 注册 scoped waterfall listener；**返回值即
  答案，调用 `next()` 表示委托**；审批服务把
  `approval/asked`、`approval/decided` 写入 session 日志。
- TUI 适配：tui-server 注册两个 waterfall 监听 → 转成 `approval/requested` /
  `question/requested` 帧 → `tui.respond` 到达时 resolve 对应 waterfall。
- 新 waterfall request 不携带可直接作为 TUI carrier id 的字段；桥层必须
  为每个 pending request 生成稳定合成 ID，保存 resolver 和原始 request，并在重复
  hello/重连时用同一 ID 重放未解决请求。
- listener 只能领取当前 TUI 已 attach 的 runtime-root session；不属于它的
  request 必须 `next()`，abort 时清理 pending 并发 resolved/cancelled 兼容帧。
  无 answerer 时保持 fail-closed。

### 3.7 code-mode → PTC（破坏性重命名，持久化词表未动）

- tools 配置 `mode` 枚举：`'native'|'code'|'both'` → `'native'|'ptc'|'both'`
  （`packages/core/tools/src/index.ts` 实测）。**设 `code` 校验失败**。
  tui-embedded patch 的 `mode: !!js process.env.DSH_TOOLS_MODE` 保留，但任何
  `DSH_TOOLS_MODE=code` 必须改 `ptc`（仓库内当前无脚本设置该变量，默认安全）。
- preset id：`code` preset → `ptc`；session 持久化词表（`tool/code-dispatch-*` 等）
  **有意保留**（等 v0→v1 迁移），旧日志仍可读。
- 事件词表 `tools/code-dispatch-log` → `tools/ptc-dispatch-log`：新会话出现 ptc 前缀
  事件，需确认 Rust presentation.rs 是否硬编码 `code` 匹配。

### 3.8 tui-embedded / cordis.patch.yml 行级调整

| 旧行 | 处置 |
|---|---|
| `api-gateway: @deepseek-ai/dsh-host-apiproxy` | **删除**（typert-gateway 已在 base 挂载） |
| `storage` / `storage-json` / `storage-domain` / `session-projection-cache` | **删除**（新 base 已挂载，重复 insert 冲突） |
| `session-list-projection-recovery` | **退役**（新 `session.list` 原生做 cold 投影恢复：`summarizeCold` + `probeSmallCold` + projections block） |
| 新增 `session-controller` / `settings-controller` / `workspace-controller` | 按 web-app patch 规范行（`@deepseek-ai/dsh-api-*`），检查 inject 闭包（agents/sessions/llm/sessionQuery/sessionProjections/workspaceRegistry/credentials/settings 均在 base 有） |
| `directory-picker: ...-browse` | 保留（browse 包仍在） |
| `agent-presets` | 保留，注释更新：不再依赖 CLI 注入 `config/agent-presets/`；新机制 `includeShippedRoot`（system trust）+ `$DSH_HOME/.agent-presets`（user trust） |
| 全部 `disabled: true` 行（tool-bash 等约 30 行） | **行 id 在新 base 全部仍存在**（实测 comm 对比零删除），原样保留 |
| tools 行 `mode` | 保留（枚举值语义见 3.7） |

`dsh-pager-runtime/cordis.patch.yml`（与 embedded 平行的那份）需同步调整。

### 3.9 展示数据形状变化：preset/model 投影和 history tool view

- 新 `SessionSummary` 没有顶层 `agentPreset`，但在 agent-presets 组合存在时，
  `summary.projections.values.agentPreset` 是正式冷/热投影来源。适配层应优先
  展平该值，follow snapshot 的 `header.agentPreset` 只作创建时 fallback；不要
  忽略 blank session 中 `agent-preset/selected` 已经改变当前 preset 的情况。
- `session.models` 的 `current` 同样应从 `modelSelection` projection 取
  `next ?? lastUsed`，均为空时再用 `modelCatalog.default`；`routable` 通过
  `modelCatalog.routableProviders.includes(current.provider)` 组装。
- 旧 `session.history` entry 带**宿主计算的工具展示 `view`**（presenter 分页时求值）；
  新版 `page/follow` records **无 view**。`agent-tool-presentation` 只选择
  native/PTC 工具暴露模式，不是旧 render-intent 服务；不应用它重算。
- 正确复用点是新版仍保留的 Host-local `ctx.tools.get(name, scope)` 和工具
  定义上的纯函数 `presentCall/presentResult`。可把旧 ApiProxy 的 `viewFor` +
  call/result 配对表逻辑迁入桥层；presenter 抛错、JSON 无法解析或跨页无法
  配对时软降级为无 view，不中断事件交付。

### 3.10 其他

- `dsh plugin add` / `dsh.profile.bundles` / `dsh.bundle.patch` 机制新版**原样保留**
  （`apps/cli/src/plugin.ts` + `dsh-app-boot/profile.ts` 实测；自定义 profile 默认
  `['@deepseek-ai/dsh-base']`、patchReload=live）——profile 自举流程不用改。
- 新 base 挂载 `session-log-deepseek` 作为 session-log 格式/能力组合；不应仅凭
  包名将它描述为网络上传器。
- 真正需要做隐私决策的是 base 中的 `session-telemetry-otel`：新默认为
  `FEEDBACK_ONLY`，用户触发 `/feedback` 时可导出原始捕获记录。TUI profile 应
  显式决定接受或禁用，不应把这项政策变化称为“无感”。
- `dsh-tui-protocol` 的 `satisfies Record<keyof TuiRpcMethodMap, true>` 编译期断言随
  dsh-host-apiproxy 类型消失而失效——方法集合改为自由列表（TUI 方法集由 TUI 自己掌控）。

## 四、Rust 侧影响（有限但必须，不是零改动）

| 区域 | 影响 |
|---|---|
| dsh-pager-protocol（TUI 线协议 v1） | 保留方法名/帧词汇，但 `SessionPromptParams` 和 `SubagentPromptParams` 必须增加 `requestId` |
| dsh-pager（session/control_plane/loader） | session/control-plane 主体不变；同步 loader prompt 路径需传入或生成 request identity，相关 fixture 需更新 |
| dsh-pager-grok-ui（host_adapter/effects） | view 基本不动；`encode_async_request` 在 `session.prompt` payload 中发送已有 `OperationKey.request_id` |
| dsh-pager-bin | 默认 backend `dsh --profile dsh-pager-grok` 不变；release 链依赖 3.1 构建前置 |
| 必须回归 | prompt 提交回显关联、重试/去重、subagent follow-up、新 `ptc` 词汇与旧日志回放 |

## 五、目标边界与实施顺序

这次升级不应把 55 个方法 switch、四条流和 interaction pending map
全部堆进 `gateway.ts`。建议在 `dsh-tui-server` 内建立唯一的新 Harness 边界：

```text
Rust/Grok UI
   │  TUI JSON-RPC v1（项目自有、稳定）
   ▼
TuiGateway / ControlPlaneStore
   │
   ▼
TuiHarnessBridge
   ├─ call(method, params, operationId, signal) -> legacy ApiResult
   ├─ followSession(address, signal) -> legacy history + MuxFrame
   ├─ controlFrames(signal) / hostFrames(signal)
   └─ respond(pendingId, interaction) -> accepted receipt
             │
             ▼
session/settings/workspace controllers + Host domain services
```

`dsh-tui-protocol` 应自己定义并冻结兼容期的 `ApiResult`、`MuxFrame`、
`HostFrame` 和 method catalog，不再 import 已删除的
`@deepseek-ai/dsh-host-apiproxy/api` 类型。

按可独立验证的提交分段：

1. **依赖、协议类型与 profile 编译闭环**：所有 package.json 升
   `0.1.2-alpha.1`；删 dsh-host-apiproxy/api-remotes 旧用法，加 controller/domain
   依赖；protocol 改为项目自有 wire DTO；patch 删重复 storage/ApiProxy/recovery，
   加 session/settings/workspace controller。
2. **unary bridge**：`dispatchUnary` 改为调用 `TuiHarnessBridge.call`，完成常用
   session/workspace/commands/preset/credentials 与剩余 catalog 方法的请求重塑、
   value 恢复和统一错误映射。
3. **session history/follow**：完成 per-session follower map、opening snapshot、
   `throughSeq` 分页、chunk run 展开、工具 view 重算、history/live barrier、detach 与
   generation 清理。
4. **control/host/interaction 流**：接入 `sessionController.control`、
   `workspaceController.follow`、`api-session/*` 和两类 waterfall，翻译回旧
   MuxFrame/HostFrame，实现 pending interaction broker 与 at-most-once respond。
5. **Rust 字段契约与发布链**：prompt requestId 贯通 protocol/loader/effects；退役
   recovery 包和 runtime `./recovery` 出口，更新 assemble/pack/profile 检查；启动脚本
   优先 latest 检出并校验版本，防止新 checkout 未 build 时静默回落到 rc.8。
6. **验证**：`pnpm verify:ts`（同步更新 gateway/control-plane 单测 mock 面）、
   `cargo test`、`real-e2e.sh`、PTY smoke；TUI 用户可见变更按仓库约定走
   浏览器终端截图对比门禁（ttyd + Playwright，DSH 与 Grok 基准并排对比）。

规模分类：**大型适配，但不是全项目重写**。大改动集中在 TS Host
边界、profile 与验证系统；Grok view/renderer 和 Rust session/control-plane 主体
应保持稳定。

## 六、风险提示

- **最大风险在流语义**（3.5/3.6）：新版 follow/control 是"每代际一个完整基线 +
  gap-free 替换帧"，比旧 mux/host 双流更强；翻译层小心重复帧折叠与 watermark 恢复
  （tui-server 的 control-plane store 已有此逻辑，重接输入即可复用）。
- **history 不是简单分页换名**：必须保持 opening cursor、chunk 无损展开、
  follower 生命周期与现有 load barrier；任一环错误都可导致丢帧、重帧或长历史错乱。
- **Dashboard cold peek 暂无新 API 等价物**：短期全日志 `inspect` 有性能代价，
  用 follow 替代又会违反不激活 Agent 的产品契约，需要明确接受临时方案
  并跟踪上游 cold-tail 能力。
- **prompt `requestId` 是硬性字段破坏**：不补时真实 prompt/subagent follow-up
  直接失败；应绑定现有 OperationKey，不只在 TS 内临时 random。
- **tool view 可以保持 parity**：优先迁移旧 ApiProxy 的 presenter 配对逻辑并使用
  `ctx.tools`；只有配对不可能时才按旧契约降级为通用卡。
- 升级后 base 组合已变，**旧 profile 需重建**（`dsh plugin` 重装）；
  `dsh_tui_profile_manifest_is_ready` 的 4 包校验需同步改。
- 新版 `session.list` 跳过无 cwd 会话、blank 语义微调
  （`blank: metadata?.blank ?? false`），roster 首屏可能有预期差异，截图对比时单独归类。

## 七、上游版本对照（速查）

| 项 | 旧 0.1.0-rc.8 | 新 0.1.2-alpha.1 |
|---|---|---|
| 网关 | `packages/host/apiproxy`（unary fetch） | `packages/api/{gateway,remotes,session-controller,settings-controller,workspace-controller}`（Typert RPC） |
| 客户端运行时 | `packages/client/runtime`（大对象树） | `client/store` + `client/ui-session` + 控制器上移 |
| 模式 | code | ptc |
| preset 目录 | `apps/cli/config/agent-presets/` | `packages/preset/agent-presets/presets/{standard,ptc,minimal,cordis}`（包内 shipped root） |
| profile | web / headless | web / headless / acp / sdk / sdk-minimal（自定义 profile 不变） |
| 会话持久化/查询 | JSONL 会话文件 + projection cache | 会话真源仍为 JSONL；SQLite session-query 作为可选查询/FTS 索引，base 默认 `openAt: never` |
| 审批 | mux 帧 + respond | waterfall 事件 `approval/request` / `user-questions/request` |
| 遥测默认 | DISABLED | FEEDBACK_ONLY |
