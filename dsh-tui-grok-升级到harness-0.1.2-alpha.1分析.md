# dsh-tui-grok 升级到 DeepSeek Harness 0.1.2-alpha.1 调整分析

> 分析日期：2026-08-29
> 方法：只读 git 比对（`deepseek-harness` 0.1.0-rc.8 `141eb6fef8` ↔
> `DeepSeek-Harness-latest` 0.1.2-alpha.1 `cd5ef81481`，1286 commits）+ 逐项源码实测
> 上游差异总览见同目录 [deepseek-harness-版本差异分析.html](deepseek-harness-版本差异分析.html)。

## 一、结论摘要

**dsh-tui-grok 目前整体钉死在 Harness `0.1.0-rc.8`，升级到 `0.1.2-alpha.1` 是
一次"TS 插件适配层重写"，Rust TUI 核心几乎不用动。**

- Rust 与 Harness 之间隔着 TS Cordis 插件（dsh-tui-server 等），Rust 只消费自有
  TUI 线协议（JSON-RPC v1：`tui.*` 控制方法 + `session.*`/`workspace.*` 等 unary +
  `events.mux`/`events.host` 通知流）。只要 TS 适配层把新 Harness 表面翻译回同一套
  帧词汇，**Rust 侧协议层零改动**。
- 真正的破坏面全部在 TS 包、profile patch、依赖版本和启动链：
  **`ctx.apiProxy` 整体消失**、**code-mode 改名 ptc**、**preset 目录迁移**、
  **`@deepseek-ai/dsh-*` 依赖必须 0.1.0-rc.8 → 0.1.2-alpha.1**。

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

### 3.4 方法映射表：TUI 的 43 个 unary 方法逐一对应

依据官方迁移笔记 `.agents/notes/implemented/architecture/2026-08-10-unary-apiproxy-remote-migration.md`
+ 新版源码实测：

| TUI 线方法 | 新版目的地 | 备注 |
|---|---|---|
| session.list / search / create / selectModel | `sessionController.list/search/create/selectModel` | cold-safe，不激活 agent |
| session.models | `sessionController.modelCatalog` | 名字变化 |
| session.rename / fork / prompt / attachment / updateQueue / cancel | `sessionController` 同名 | |
| **session.history** | `sessionController.page`（unary 反向页）+ `follow`（流快照） | **形状变了**：旧 `{events, hasMore, projections}`；新 `{records, hasMore}`，records 为 `event \| chunks`（chunk run 需展开） |
| host.openPath | `sessionController.openWorkspacePath` | 路径解析上移客户端 |
| host.describe | **删除** | ready 帧 + 能力查询替代 |
| host.pickDirectory / listDirectory / createDirectory | `workspaceController` directory-picker `pick/list/createDirectory` | |
| workspace.list / create / rename / delete / insertBefore / insertSessionBefore / archiveSession | `workspaceController` 同名 | list 语义变为 detached snapshot |
| skill.list | `sessionController` 内 SessionSkillCatalog | |
| fileReferences.list | `sessionController` 内 SessionFileReferences | agent 解析改用 `sessionController.resolveAgent()`（替代已删除的 resolver） |
| commands/list、commands/execute | `CommandRuntime`（`@Remote` 同名） | `ctx.commands` 仍在 |
| agentPreset.list / select / read / copy / remove | `dsh-agent-presets` 的 `agentPresets/*` Remote | |
| agentPreset.openDocument | `settingsController` 的 `settings/openAgentPresetDirectory` | |
| goal.* / llm.providers / llm.discoverModels / credentials.* / settings.* | `dsh-goal` / `dsh-llm`（listProviders、listConfigurableProviders、discoverModels）/ `credentialsController` / `settingsController` | llm.models 并入 modelCatalog |
| subagent.list / history / prompt / interrupt | `dsh-subagent`（`subagents/list/prompt/interruptByParent`） | interrupt 授权语义保留 |

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
- **TUI 线协议不用改**：新 TS 适配层把以上帧翻译回旧词汇即可，Rust
  `apply_mux/apply_host` 原样工作；tui-server 的 control-plane store（重复帧折叠、
  watermark、历史加载屏障）可复用，只换输入源。

### 3.6 审批/问答应答：`api.respond` → waterfall 事件应答

- 旧：`dispatchRespond` 走 `apiProxy.respond({type:'client-response', rpcId, result})`。
- 新：UI 作为 answerer 注册 `ctx.remote.$on('approval/request', (request, next) => outcome)`
  与 `'user-questions/request'`；**返回值即答案，调用 `next()` 表示委托**；审批服务把
  `approval/asked`、`approval/decided` 写入 session 日志。
- TUI 适配：tui-server 注册两个 waterfall 监听 → 转成 `approval/requested` /
  `question/requested` 帧（带 requestId）→ `tui.respond` 到达时 resolve 对应
  waterfall。注意**无 answerer 时 fail-closed**（旧版可能静默挂起）。

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

### 3.9 展示数据缺口：`session.list` 不再带 `agentPreset`；history 的宿主 `view` 消失

- 新 `SessionSummary` 无 `agentPreset` 字段；preset 在 `SessionHeader.agentPreset`
  （持久化）和 `agent-preset/selected` 事件中。TUI 的 roster/子代理面板消费
  `summary.agent_preset` + host 帧的 agentPreset（host_adapter.rs 多处）。适配层需从
  header（sessionQuery）或 `agent-preset/selected` 事件补 `host/session-status` 帧的
  agentPreset，否则 preset 显示退化为 "-"。
- 旧 `session.history` entry 带**宿主计算的工具展示 `view`**（presenter 分页时求值）；
  新版 `page` records **无 view**。新版仍有 `packages/core/agent-tool-presentation`
  （presentation 服务搬到 core）。适配层用其重算 view 拼回 HistoryEntry，或接受工具
  卡片降级为原始参数渲染（Rust 侧 `view` 是 Option 有 fallback，但视觉 parity 下降）。

### 3.10 其他

- `dsh plugin add` / `dsh.profile.bundles` / `dsh.bundle.patch` 机制新版**原样保留**
  （`apps/cli/src/plugin.ts` + `dsh-app-boot/profile.ts` 实测；自定义 profile 默认
  `['@deepseek-ai/dsh-base']`、patchReload=live）——profile 自举流程不用改。
- 新 base 挂载 `session-log-deepseek`（向官方 API 增量上传会话日志）、
  `web-fetch-http` 等新行——TUI profile 无感，但**真实会话会上报日志**，升级后需在
  patch 显式评估（禁用或接受）。
- 遥测默认 DISABLED → FEEDBACK_ONLY（仅 /feedback 时上传），TUI 场景无感。
- `dsh-tui-protocol` 的 `satisfies Record<keyof TuiRpcMethodMap, true>` 编译期断言随
  dsh-host-apiproxy 类型消失而失效——方法集合改为自由列表（TUI 方法集由 TUI 自己掌控）。

## 四、Rust 侧影响（很小，逐项确认）

| 区域 | 影响 |
|---|---|
| dsh-pager-protocol（TUI 线协议 v1） | 零改动——线方法名/帧词汇全部保留，由 TS 适配层翻译 |
| dsh-pager（session/control_plane/loader） | 零改动；agentPreset 由适配层补齐后行为不变 |
| dsh-pager-grok-ui（host_adapter 等） | 基本零改动；测试桩 `agent_preset: Some("standard")` 不受影响 |
| dsh-pager-bin | 默认 backend `dsh --profile dsh-pager-grok` 不变；release 链依赖 3.1 构建前置 |
| 潜在 Rust 改动 | 新会话 `ptc` 前缀事件/tool 名是否命中 presentation.rs 的 `code` 硬编码（旧词表未变，大概率不用动） |

## 五、改造工作量排序（建议实施顺序）

1. **依赖升版 + 启动链**（阻塞项）：所有 package.json 升 `0.1.2-alpha.1`、删
   dsh-host-apiproxy、加新 api 控制器依赖；`dsh-tui-common.sh` 加 latest 检出候选/
   构建检查；`@dsh-pager-grok/cli` 的 `@deepseek-ai/dsh` 升版。
2. **cordis.patch.yml 重排**（3.8）：删 4 行、加 3 个控制器行、（可选）api-remotes；
   退役 recovery 插件（profile 安装列表 4 包 → 3 包）。
3. **tui-server 重写 dispatch 层**（最大工作量，3.4）：`dispatchUnary` 从
   `toFetchHandler(api)` 改为按方法路由到各控制器进程内调用；错误面从
   `ApiResult{ok,error}` 映射新版 `TypertRemoteFailure`。
4. **tui-server 重写流层**（3.5/3.6）：`pumpMux/pumpHost` 改为消费
   follow/control/workspace-follow + `api-session/*` 事件 + waterfall 审批事件，
   翻译回旧帧词汇；`dispatchRespond` 改为 waterfall resolve。
5. **数据补齐**（3.9）：list 行 preset 补帧；history view 用 agent-tool-presentation
   重算或接受降级（需决策）。
6. **验证**：`pnpm verify:ts`（同步更新 gateway/control-plane 单测 mock 面）、
   `cargo test`、`real-e2e.sh`、PTY smoke；TUI 用户可见变更按仓库约定走
   浏览器终端截图对比门禁（ttyd + Playwright，DSH 与 Grok 基准并排对比）。

## 六、风险提示

- **最大风险在流语义**（3.5/3.6）：新版 follow/control 是"每代际一个完整基线 +
  gap-free 替换帧"，比旧 mux/host 双流更强；翻译层小心重复帧折叠与 watermark 恢复
  （tui-server 的 control-plane store 已有此逻辑，重接输入即可复用）。
- **view 降级**是唯一可能"看起来不一样"的点，优先用 agent-tool-presentation 重算，
  保持 Grok 工具卡片视觉 parity。
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
| 持久化 | 每会话文件 + JSONL | SQLite schema 19 冻结物理契约 + FTS 查询 + ZIP 导出 |
| 审批 | mux 帧 + respond | waterfall 事件 `approval/request` / `user-questions/request` |
| 遥测默认 | DISABLED | FEEDBACK_ONLY |
