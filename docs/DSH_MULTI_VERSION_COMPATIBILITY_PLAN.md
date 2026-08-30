# DSH 多版本兼容架构与迁移方案

> 状态：Proposed（待评审）
> 日期：2026-08-30
> 适用仓库：`dsh-pager-grok`
> 本文档只做设计与迁移规划，不代表已实施。

## 1. 这次设计的目的

### 1.1 核心目的

这次设计要解决的不是“怎么再升级一次 DSH”，而是“怎么让
`dsh-pager-grok` 从架构上具备持续适配多个 DSH 版本的能力”。

项目已经历了一次具有代表性的上游变化：

- DSH `0.1.0-rc.8` 使用 Host ApiProxy/Remote API。
- DSH `0.1.1-rc.2` 仍使用同一架构族，但存在局部 API 和事件语义变化。
- DSH `0.1.2-alpha.1` 删除旧 Host ApiProxy 路径，改为 Controller、
  follow/control stream 和 waterfall 交互。

如果继续用“在当前 server/gateway/profile 上直接改到新版本”的方式，
每次上游改造都会同时影响协议、server、profile、CLI、测试和发布。
这会导致：

- 旧 DSH 支持被覆盖，只能依靠长期分支保留。
- 升级改动很难判断是“DSH 适配”还是“TUI 产品行为变更”。
- 无法在同一个 `main` 上对多个 DSH 版本做持续回归。
- 一个未发布的上游 alpha 依赖会直接阻塞整个产品的 npm 发布。

因此，本次设计的核心目的是：

1. 保持一条产品主线，避免每个 DSH 版本对应一套长期 TUI 分支。
2. 冻结 Rust/Grok TUI 消费的项目自有协议，不让 DSH 类型和服务泄漏进入
   TUI 内核。
3. 把 DSH 版本差异限定在可替换的 adapter 和版本化 profile/runtime
   组合中。
4. 建立可机器读取、可验证、可发布的兼容矩阵，而不是在多个脚本中
   分散硬编码 DSH 版本。
5. 让“最新已发布 DSH”和“下一个仅源码可用 DSH”可以并行开发，
   但不混淆 npm 发布条件。

### 1.2 直接业务价值

- 已安装公开 npm DSH 的用户可以安装一个真正可用的 pager，不会遇到
  registry 404。
- 开发者可以继续在上游 alpha tag 上预适配，不必因 npm 尚未发布而
  放弃新版开发。
- 当 DSH 再次变更时，维护者可以明确回答“这是同架构族升级，还是
  需要新 adapter family”。
- 用户执行 `doctor` 时可以看到实际 DSH 版本、适配器、能力和不兼容原因，
  而不是在运行后才遇到模糊错误。

## 2. 实施后要达到的效果

### 2.1 对代码架构的效果

实施完成后，应同时满足：

1. Rust `dsh-pager-protocol` 继续作为线协议、版本和 wire DTO 语义的项目内
   所有者；TypeScript `dsh-tui-protocol` 作为 server 端镜像实现，不再导入
   任何 `@deepseek-ai/dsh-*` 类型或运行时。
2. TUI server core 不导入任何具体 DSH Controller、ApiProxy 或 Cordis service 类型。
3. `TuiGateway` 只依赖项目自有的 `TuiBackend` 接口。
4. DSH `0.1.0-rc.8`/`0.1.1-rc.2` 由 `apiproxy-v1` adapter family 承载。
5. DSH `0.1.2-alpha.1` 由 `controllers-v2` adapter family 承载。
6. 各 adapter 的 Cordis `inject`、上游 imports、语义归一化和 profile patch 物理隔离。
7. 添加一个新 DSH 版本时，默认不需要修改 Rust TUI、gateway、transport
   和 control-plane；如确实需要，必须明确说明是自有协议能力变更。

### 2.2 对兼容性的效果

- 同一个 `main` 可对 rc.8、rc.2 和 alpha.1 分别运行 adapter conformance 与
  真实 Harness E2E。
- 对用户公开的支持状态只有 `supported`、`maintenance`、`experimental`、
  `unsupported`，它们由兼容矩阵和 CI 事实决定；迁移期间允许使用内部
  `candidate` 状态，但不得把它展示成已支持。
- 未列入矩阵的 DSH 版本在启动前被拒绝，不依靠“方法看起来存在”就
  继续运行。
- 同一架构族内的版本只在公用合同测试和真实 E2E 都通过后才共享
  adapter。
- 旧版本退出主线 CI 时，依然保留最后一个可安装的 pager 版本和发布记录。

### 2.3 对用户体验的效果

- `dsh-pager doctor` 输出 DSH 版本、adapter family、runtime package、profile schema
  与 capability 集合。
- 发现不兼容时在启动前给出明确信息：检测到的版本、支持版本、推荐
  安装命令或开发模式启动方式。
- 切换 adapter family 时不在原 profile 上就地混装不兼容插件；新建或迁移到
  family 专用 profile，并保留可恢复备份。
- Rust/Grok TUI 在各后端上保持同一套会话、history、stream、approval、
  question 和 workspace 交互。
- 某个 DSH 版本不具备的功能通过 capability 显式禁用，不伪装支持后返回
  `internal error`。

### 2.4 对测试和发布的效果

- gateway/control-plane 的通用测试只运行一套 fake backend，不为每个 DSH 版本复制。
- 每个 adapter family 运行同一份 conformance suite。
- 每个声明支持的精确 DSH 版本运行独立安装、构建和真实 E2E。
- npm 发布前必须验证产物中的每个非 optional 依赖在 registry 可解析。
- 公开发布产物必须在无本地 Harness checkout、无 workspace link 的干净目录中
  完成 cold install 与启动。
- source-only alpha 可以进入 CI，但 registry gate 不通过时不得进入正式 npm
  publish job。

### 2.5 设计目的与可验证结果对应表

| 设计目的 | 实施后可验证结果 |
|---|---|
| 保持一条产品主线 | `main` 同时包含两个 adapter family；不再从 legacy 长期分支启动日常开发 |
| 隔离 DSH 架构变化 | Rust canonical protocol 与 TypeScript mirror/core 不导入 DSH；同 family 升级默认不改 Rust/Grok UI |
| 精确管理兼容性 | 每个支持版本在 registry 中有 exact tag、commit、family、status、distribution 和独立 E2E |
| 兼顾 npm 与下一代源码适配 | rc.2 可在无本地 checkout 的环境 cold install；alpha.1 继续源码 CI，但不会污染正式 publish |
| 降低用户排错成本 | `doctor` 在启动前报告实际版本、adapter、profile、capability 和可执行修复建议 |
| 防止“声称兼容但未验证” | adapter conformance、真实 DSH E2E、PTY 和 registry gate 共同决定支持状态 |

## 3. 现状与问题证据

### 3.1 当前代码和分支事实

- 当前 `main`（`04d9bd671af30eee16e72fb0ded96684db71715d`）与
  `upgrade/harness-0.1.2-alpha.1`
  （`e7b874f92de678359619265bb7de232d88f834db`）都是 alpha.1 适配；`main`
  额外包含干净 runner CI 修复。
- 旧 rc.8 适配保存于 `legacy/main-before-harness-0.1.2-alpha.1`
  （`e49c1267bc2177ac8f2019d1c04753c4992481bf`）。
- 截至 2026-08-30，[npm 版本页](https://www.npmjs.com/package/%40deepseek-ai/dsh?activeTab=versions)
  显示 `@deepseek-ai/dsh` 的 `latest` 为 `0.1.1-rc.2`。
- `dsh-v0.1.1-rc.2` 仍包含 `@deepseek-ai/dsh-host-apiproxy` 和
  `@deepseek-ai/dsh-api-remotes`；`dsh-v0.1.2-alpha.1` 已删除 Host ApiProxy，新增
  session/settings/workspace controllers。

### 3.2 当前耦合点

- `packages/dsh-tui-server/src/bridge.ts` 已是明确的 alpha.1 兼容边界，这个方向
  正确；但文件约 1400 行，同时承担 DSH service typing、unary dispatch、
  history/chunk 转换、follow/control stream、workspace cache、waterfall 交互和
  error normalization。
- `packages/dsh-tui-server/src/gateway.ts` 的构造器直接依赖具体
  `TuiHarnessBridge`，尚未形成可替换 port。
- `packages/dsh-tui-protocol/src/types.ts`、`codec.ts` 和 `ids.ts` 仍导入
  DSH `SessionId`、`SessionEvent` 和 brand，因此协议包并未真正与 DSH 解耦。
- `scripts/dsh-tui-common.sh`、CLI/runtime manifests、workspace overrides、profile patch 和 CI
  各自包含版本知识。
- 根级 `pnpm-workspace.yaml` 把所有 DSH packages 全局 override 到 alpha.1 本地
  checkout，会阻止在同一工作区中准确验证 rc.2 adapter。
- 当前 `@dsh-pager-grok/cli` 和 runtime 直接依赖未发布的
  `0.1.2-alpha.1` packages，本地 link 可用但 npm cold install 必然失败。

## 4. 设计范围与非目标

### 4.1 本方案覆盖

- TypeScript TUI wire contract 的 DSH 去耦。
- server core 与 DSH adapter 之间的 `TuiBackend` SPI。
- ApiProxy v1 和 Controllers v2 两个 adapter family。
- 版本化 Cordis profile/runtime 组合。
- DSH 精确版本兼容矩阵、capability 模型和 fail-closed 检测。
- 本地 source checkout、npm 默认安装与正式发布的边界。
- 分层测试矩阵和分阶段迁移路线。

### 4.2 本方案不承诺

- 不承诺永久支持所有历史 DSH 版本。
- 不在一个 adapter 内通过大量 `if (version)` 兼容跨架构族变化。
- 不要求所有 DSH 版本暴露完全相同的产品功能；功能差异通过 capability
  显式表达。
- 不在 DeepSeek 尚未发布 alpha packages 时重新发布或静默 vendor 上游
  packages。
- 不将本次 adapter 拆分与 Grok UI 视觉重构混在同一批次。
- 不用自动 semver 宽范围代替真实版本 E2E。

## 5. 目标架构

```text
                         Rust / Grok TUI
                                │
              TUI JSON-RPC v1（Rust canonical / TS mirror）
                                │
                  ┌───────────▼───────────┐
                  │       TUI Server Core       │
                  │ gateway / control-plane     │
                  │ buffering / transport       │
                  │ protocol errors / lifecycle │
                  └───────────┬───────────┘
                                │ TuiBackend SPI
                   ┌────────────┴────────────┐
                   │                         │
          adapter-apiproxy-v1         adapter-controllers-v2
           rc.8 / rc.2                   alpha.1 / 后续
                   │                         │
          profile-apiproxy-v1         profile-controllers-v2
                   │                         │
          runtime-apiproxy-v1         runtime-controllers-v2
                   └────────────┬────────────┘
                                │
                     CLI compatibility resolver
```

依赖方向只能从上到下：

- TUI 不能导入 adapter 或 DSH。
- server core 不能导入 adapter 的实现类型。
- adapter 可以导入 protocol/core 和它所对应的 DSH family。
- profile/runtime 负责组合 adapter 和 DSH plugins，不定义 TUI 业务语义。

## 6. 稳定协议层

### 6.1 协议所有权

遵守仓库既有架构契约：

- `crates/dsh-pager-protocol` 是 TUI 线协议、协议版本和 wire DTO 语义的
  canonical owner。
- `@dsh-pager-grok/tui-protocol` 是 TypeScript server 使用的对等 codec/type
  实现，不是第二个产品语义真源。
- 两端共同覆盖 `SessionId` 等 wire brand、`ApiResult` / `ApiError`、55 个当前
  unary method 名称及稳定投影、`MuxFrame`、`HostFrame`、interaction response、
  control-plane baseline、JSON-RPC codec 和错误编码。
- 跨语言 fixture catalog 是两端一致性的机器门禁；Phase 2 必须明确 fixture
  canonical 路径，并让检查脚本同时拒绝字段、method、version 和 error code 漂移。
  在引入可靠 codegen 前，不能只靠两份手写类型“约定一致”。

TypeScript 协议镜像不得再依赖：

- `@deepseek-ai/dsh-brand`
- `@deepseek-ai/dsh-session`
- `@deepseek-ai/dsh-invariants`
- DSH Host ApiProxy 生成类型
- Cordis service context

### 6.2 协议版本策略

- DSH 版本升级不等于 TUI protocol 升级。
- adapter 内部形状改变、新 capability 和新 `host.describe` 字段优先使用
  向后兼容方式增加。
- 只有 Rust 客户端必须修改解析/状态机的非兼容 wire 语义才提升
  `TUI_PROTOCOL_VERSION`。
- 所有新增字段先用 TypeScript/Rust 双端 fixture 证明旧客户端可安全忽略。

## 7. `TuiBackend` 适配器 SPI

### 7.1 建议接口

```ts
export interface TuiBackendInfo {
  adapterFamily: 'apiproxy-v1' | 'controllers-v2'
  dshVersion: string
  profileSchema: number
  capabilities: Readonly<TuiCapabilities>
}

export interface TuiBackend {
  readonly info: TuiBackendInfo

  call(
    method: TuiUnaryMethod,
    params: unknown,
    operationId: string,
    signal: AbortSignal,
  ): Promise<ApiResult>

  attachSession(sessionId: SessionId): void
  detachSession(sessionId: SessionId): void

  followSession(
    sessionId: SessionId,
    signal: AbortSignal,
  ): AsyncIterable<TuiMuxEnvelope>

  muxFrames(signal: AbortSignal): AsyncIterable<TuiMuxEnvelope>
  hostFrames(signal: AbortSignal): AsyncIterable<HostFrame>

  respond(
    requestId: string,
    value: unknown,
  ): Promise<TuiRespondResult>

  resetConnection(): void
  dispose(): void
}
```

`TuiUnaryMethod` 是项目自有 wire catalog 的中性名称；当前
`ApiProxyMethod`/`API_PROXY_METHOD_SET` 应在协议去耦阶段重命名，method 字符串
保持不变。该接口以当前 `TuiHarnessBridge` 已经运行的公开方法为起点，避免
首次抽取就引入大量产品行为变化。

### 7.2 core 与 adapter 的责任边界

Server core 拥有：

- `tui.hello` / attach / detach / subscribe 连接状态机。
- generation、stale request、at-most-once response。
- history/load barrier 之外的连接级 buffer 和 control-plane replay。
- JSON-RPC line transport、backpressure 和错误载体。
- capability 检查与统一 `unsupported-capability` 错误。

Adapter 拥有：

- DSH Context/service typing 和 Cordis `inject`。
- 上游 method/event 到 TUI 稳定 DTO 的转换。
- DSH 版本特有的 history/follow/pagination 语义。
- approval/question waterfall 的上游领取与应答。
- DSH 错误码、projection、tool view 和 workspace 形状归一化。
- adapter 启动时的精确版本和所需 service 断言。

### 7.3 边界数据校验

当前 alpha bridge 中有大量 `Record<string, unknown>`。迁移时不要一次把全部 DSH
内部类型镜像到项目中，而是优先为高风险边界建立 adapter-local guards：

- session list/summary
- history opening snapshot/page/chunk rows
- live follow/control frames
- approval/question requests and responses
- workspace baseline/follow frames
- model/preset/goal/subagent results

Adapter 输出 `TuiBackend` 前必须完成归一化；`RecordLike` 不应穿过 SPI 进入
gateway 或 Rust TUI。

## 8. Adapter family 设计

### 8.1 `apiproxy-v1`

目标版本：

- `0.1.0-rc.8`：maintenance
- `0.1.1-rc.2`：candidate，完成全部门禁后转为 supported，并作为首个正式
  npm 默认后端

实现起点：

- 从 `legacy/main-before-harness-0.1.2-alpha.1` 恢复 ApiProxy dispatch、mux/host
  stream 和 response 逻辑。
- 不恢复旧 gateway 副本；旧适配代码必须实现新 `TuiBackend`，共享
  当前 core gateway/control-plane。
- 对 rc.8 → rc.2 的局部事件和 API 差异使用 adapter-local normalization，
  不在 core 增加版本分支。

兼容声明：

- rc.8 和 rc.2 归为同一 family 是基于上游架构边界相同，不是直接
  声称运行时已兼容。
- rc.2 只有在全部 conformance、real E2E、PTY 和 cold npm install 通过后才标记
  `supported`。
- 如 rc.2 要求改变 TUI wire 语义，应先评估是否为 adapter 投影问题，
  不默认修改 Rust 客户端。

### 8.2 `controllers-v2`

目标版本：

- `0.1.2-alpha.1`：experimental/source-only，直到上游 packages 发布到 npm。

实现起点：

- 当前 `TuiHarnessBridge` 的行为作为已验证 baseline。
- 先完成类名和接口抽取，不立即改变 history、stream 或 waterfall 逻辑。
- 随后按责任拆分，建议内部结构：

```text
adapter-controllers-v2/src/
  plugin.ts
  context.ts
  backend.ts
  unary.ts
  history.ts
  streams.ts
  interactions.ts
  workspace.ts
  normalize.ts
```

拆分不改变一个重要原则：opening snapshot、old-page cursor、live follower 和
attach/load barrier 仍是一组具有统一正确性约束的逻辑，不能为了拆文件而将
状态所有权切碎。

## 9. Capability 模型

### 9.1 建议能力集

```ts
export interface TuiCapabilities {
  sessions: boolean
  workspaces: boolean
  settings: boolean
  credentials: boolean
  agentPresets: boolean
  goals: boolean
  subagents: boolean
  approvals: boolean
  questions: boolean
  queue: boolean
  jobs: boolean
  skills: boolean
  fileReferences: boolean
  directoryPicker: boolean
}
```

### 9.2 暴露方式

首选将 backend metadata 和 capabilities 放入现有 `host.describe` 的稳定投影，
因为它是业务方法而不是连接层协议。示例：

```json
{
  "backend": {
    "name": "deepseek-harness",
    "version": "0.1.1-rc.2",
    "adapterFamily": "apiproxy-v1",
    "profileSchema": 1
  },
  "capabilities": {
    "sessions": true,
    "workspaces": true,
    "goals": false
  }
}
```

如后续需要在 `host.describe` 之前完成协商，可再将最小 backend metadata 以可选
字段加入 `tui.hello.serverInfo`，但必须先验证所有已发布 Rust 客户端能忽略
额外字段。

### 9.3 不支持功能的行为

- method 名称可以继续存在于 TUI v1 catalog。
- adapter 不具备对应 capability 时，core 返回稳定业务错误
  `unsupported-capability`。
- Rust UI 在已知 capability 为 false 时禁用对应入口并给出版本原因。
- 不将“服务对象不存在”当作正常的运行时能力检测；adapter 启动时
  就应检查它声明必需的 services。

## 10. 包和目录拆分

建议目标布局：

```text
packages/
crates/
  dsh-pager-protocol/               # canonical TUI wire contract/version/DTO
packages/
  dsh-tui-protocol/                 # 纯 TS mirror/codec，不依赖 DSH
  dsh-tui-server-core/              # gateway/control-plane/transport
  dsh-adapter-apiproxy-v1/          # rc.8 + rc.2
  dsh-adapter-controllers-v2/       # alpha.1 + 经验证后续版本
  dsh-runtime-apiproxy-v1/          # family-specific bundle + patch
  dsh-runtime-controllers-v2/       # family-specific bundle + patch
  dsh-pager-cli/
  native/*
```

包命名应使用架构族，而不是每个 prerelease 字符串都新建一个 adapter
package。精确支持版本由兼容矩阵表达。

为避免首次迁移过大，实际实施时可先在当前 `dsh-tui-server` 内建立
`core/` 和 `adapters/` 子目录，等边界和测试稳定后再拆成 workspace packages。
这样可以将“代码责任拆分”与“npm 包拆分”分成两个可回滚批次。

## 11. 兼容矩阵作为单一真源

### 11.1 建议文件

```text
compat/dsh-support.json
```

建议 schema：

```json
{
  "schemaVersion": 1,
  "versions": {
    "0.1.0-rc.8": {
      "family": "apiproxy-v1",
      "tag": "dsh-v0.1.0-rc.8",
      "commit": "141eb6fef83422698aef7a981029e843e8161534",
      "packageManager": "pnpm@11.7.0",
      "runtimePackage": "@dsh-pager-grok/runtime-apiproxy-v1",
      "profileSchema": 1,
      "status": "maintenance",
      "distribution": "npm"
    },
    "0.1.1-rc.2": {
      "family": "apiproxy-v1",
      "tag": "dsh-v0.1.1-rc.2",
      "commit": "b150a551b8d465e31e418e1b2eaf5e79bbb7d28e",
      "packageManager": "pnpm@11.7.0",
      "runtimePackage": "@dsh-pager-grok/runtime-apiproxy-v1",
      "profileSchema": 1,
      "status": "candidate",
      "distribution": "npm"
    },
    "0.1.2-alpha.1": {
      "family": "controllers-v2",
      "tag": "dsh-v0.1.2-alpha.1",
      "commit": "cd5ef8148158c3a752a658978873241fdf8e2bbc",
      "packageManager": "pnpm@11.7.0",
      "runtimePackage": "@dsh-pager-grok/runtime-controllers-v2",
      "profileSchema": 2,
      "status": "experimental",
      "distribution": "source-only"
    }
  }
}
```

这是 Phase 0 建立清单时的建议初始状态，而不是对 rc.2 已通过测试的声明。
rc.2 必须在 Phase 4 和 Phase 6 门禁全部通过后才改为 `supported`。三个本地
上游 tag 的根 `package.json` 均已核对为 `pnpm@11.7.0`；CI 落地时仍应直接
读取清单并检查 checkout，防止后续手工漂移。

### 11.2 消费者

以下环节都必须读取或验证这份清单：

- CLI backend resolver
- `doctor`
- 本地 profile setup
- adapter startup assertion
- GitHub Actions version matrix
- runtime pack verification
- npm dependency availability gate
- release notes/support table generation

Package manifest 中无法动态生效的 exact dependencies 仍保留在各 runtime package 中，
但 CI 必须检查它们与 support registry 一致，防止双真源漂移。

### 11.3 版本选择算法

1. 从实际 DSH entry 定位 CLI `package.json`。
2. 读取精确 `version`，不使用目录名猜测。
3. 在 support registry 中执行精确查找。
4. 找不到时 fail closed，列出已测试版本。
5. 找到后选择 adapter family、runtime package 和 profile schema。
6. adapter 启动时再验证必需 Cordis services，防止版本对但 profile 组合错。
7. `doctor` 输出选择证据，但不输出凭据或用户私密路径内容。

## 12. Profile 和 runtime 隔离

### 12.1 不兼容的 profile 不就地混装

ApiProxy v1 和 Controllers v2 的插件图不同：

- ApiProxy v1 需要 Host ApiProxy/Remote 组合和旧 projection/recovery 行为。
- Controllers v2 需要 session/settings/workspace controllers、follow/control stream，
  且已退役旧 recovery。

因此建议 profile 内部名称包含 family：

```text
dsh-pager-grok-apiproxy-v1
dsh-pager-grok-controllers-v2
```

CLI 仍向用户暴露稳定的 `dsh-pager` 命令，profile family 是内部组合细节。

### 12.2 profile ownership metadata

由 pager 管理的 profile manifest 应记录：

```json
{
  "dshPagerGrok": {
    "managed": true,
    "adapterFamily": "apiproxy-v1",
    "dshVersion": "0.1.1-rc.2",
    "profileSchema": 1,
    "runtimeVersion": "0.2.0"
  }
}
```

如 profile 已存在但 family/schema 不符：

- 默认不就地覆盖。
- 先将旧 profile 重命名为带时间戳备份。
- 创建新 family profile。
- 只迁移明确定义的 pager 设置，不删除 `$DSH_HOME/sessions` 或用户凭据。

## 13. npm 产物与发布边界

### 13.1 产品版本与 DSH 版本解耦

`dsh-pager-grok` 使用自己的 semver，不把 DSH 版本直接当成 pager 版本。
一个 pager release 可以包含多个 adapter family，具体支持列表由 support registry
和 release notes 说明。

### 13.2 建议公开包

- `@dsh-pager-grok/cli`
- `@dsh-pager-grok/runtime-apiproxy-v1`
- `@dsh-pager-grok/runtime-controllers-v2`（上游依赖发布后）
- 现有五个 native packages

公开 CLI 的非 optional 依赖图只能包含 registry 可解析的包。CLI 不得为了
“附带多版本支持”直接依赖未发布的 controllers-v2 runtime 或 alpha packages；
family runtime 必须按 support registry 延迟解析，缺失时给出安装或开发配置提示。

已经存在的 `@dsh-pager-grok/runtime` 不应立即删除。建议在一个迁移周期内将
它作为“默认已发布 DSH family”的兼容 runtime，并由新 CLI 优先安装
family-specific runtime。待旧 CLI 支持周期结束后，再单独评审是否退役兼容包名。

### 13.3 默认 npm 后端

本方案建议：

- 首个可发布的多版本架构候选以 DSH `0.1.1-rc.2` 作为默认 npm 后端。
- CLI 默认安装/解析已发布的 `@deepseek-ai/dsh@0.1.1-rc.2`。
- 用户通过 `DSH_BIN_JS` 或开发配置指向 alpha.1 源码时，CLI 选择
  `controllers-v2` source runtime；该 runtime 必须由显式的开发 profile/setup
  提供，不能靠猜测 sibling checkout 路径获得。
- 在 alpha.1 所需 DeepSeek packages 完整发布前，不公开发布依赖它们的
  `runtime-controllers-v2`。

### 13.4 注册表可用性门禁

每个 publish candidate 必须生成依赖清单，对每个 non-optional dependency 执行：

```text
npm view <package>@<exact-version> version
```

任意项缺失就在 publish 前失败。门禁输出缺失包名和版本，不通过
`--force`、忽略 peer 或仅验证 `npm pack` 绕过。

## 14. 依赖与 workspace 隔离

### 14.1 取消根级单版本全局 override

当 adapter 拆分完成后，根级不应将同名 DSH packages 全部 override 到一个
alpha checkout。建议：

- 每个 adapter package 声明自己的 exact peer/dev dependencies。
- npm 已发布版本从 registry 安装。
- source-only adapter 在专用 CI/local fixture 中使用 exact `link:` checkout。
- 不在公开 tarball 的 dependencies 中留下 `link:` 或 `workspace:`。

### 14.2 隔离的兼容 fixture

建议建立：

```text
compat/fixtures/
  dsh-0.1.0-rc.8/
  dsh-0.1.1-rc.2/
  dsh-0.1.2-alpha.1/
```

每个 fixture 只组合一个 DSH 版本和对应 adapter/runtime，防止 pnpm peer resolution
或局部 link 让测试在错误版本上假通过。

## 15. CLI 选择、升级和诊断

### 15.1 默认启动流程

```text
resolve DSH entry
  → read exact DSH version
  → lookup compat/dsh-support.json
  → select adapter family/runtime/profile schema
  → verify or create family profile
  → run adapter startup assertions
  → spawn native pager
```

### 15.2 `doctor` 必须报告

- pager CLI/runtime/native 版本对齐状态
- DSH entry 来源：npm default 或显式 `DSH_BIN_JS`
- DSH 精确版本
- adapter family
- support status/distribution
- profile 路径、family 和 schema（只输出路径，不读凭据内容）
- capability 列表或缺失的必需能力
- registry dependency availability（仅 `doctor --release`或发布门禁）

### 15.3 update/repair 语义

- `update` 在同 family/schema 内可就地对齐 pager runtime 版本。
- DSH 变更导致 family/schema 变化时，`update` 必须转为可见的 profile migration。
- `repair` 继续使用重命名备份，不执行破坏性删除。
- 所有 migration 在启动前列出旧/新 family 和 profile 目录。

## 16. 测试架构

### 16.1 Core contract tests

用 fake `TuiBackend` 统一验证：

- hello 与 generation
- attach/detach/subscribe
- history/load barrier
- mux/host buffering
- control-plane baseline/replay
- request abort
- repeated response fingerprint 与 at-most-once
- reconnect/dispose 资源清理
- unsupported capability

这组测试不导入任何 DSH package。

### 16.2 Adapter conformance suite

同一份测试套件作用于每个 adapter family：

1. `session.list/search/create/history`
2. history opening snapshot 与旧页 cursor
3. history/live 无丢失、无重复 barrier
4. prompt/request identity/cancel
5. queue/jobs/projection
6. approval/question claim/respond/abort/reconnect
7. workspace baseline/follow/order/archive
8. settings/credentials
9. agent presets/goals/subagents
10. file references/directory picker/commands
11. error normalization
12. capability 缺失语义

套件输入是 adapter-specific fake DSH services，输出必须是相同 TUI DTO/golden。

### 16.3 真实 DSH 矩阵

| DSH | 来源 | Adapter | 门禁 |
|---|---|---|---|
| `0.1.0-rc.8` | exact npm/tag | `apiproxy-v1` | install/build/conformance/real E2E |
| `0.1.1-rc.2` | exact npm/tag | `apiproxy-v1` | install/build/conformance/full E2E/PTY/cold pack |
| `0.1.2-alpha.1` | exact tag + commit | `controllers-v2` | source build/conformance/full E2E/PTY |

每个 job 必须：

- 使用 support registry 中的 exact tag/commit。
- 使用该 DSH 自身要求的 package manager。
- 使用独立 `DSH_HOME` 和 profile。
- 不共享另一版本的 `node_modules`。
- 在测试结束时报告 adapter/version/profile evidence。

### 16.4 Rust/UI 回归

Rust 主体测试不必在每个 DSH 版本重复完整运行：

- protocol fixture 和 mock backend 回归在 core CI 完整运行一次。
- 每个 DSH 版本至少运行 hello/list/load/lifecycle 真实 E2E。
- 默认 supported DSH 额外运行完整 PTY 和发布 dogfood。
- adapter 变更若影响 tool presentation 或稳定 DTO，再扩大对应 Rust/UI 用例，
  不默认全量复制所有视觉测试。

## 17. CI 与发布流程

### 17.1 PR CI

```text
core lint/typecheck/unit
  ├─ Rust workspace tests
  ├─ protocol cross-language fixtures
  ├─ apiproxy-v1 conformance
  ├─ controllers-v2 conformance
  └─ exact DSH smoke matrix
```

Adapter 或 support registry 变更时运行完整 DSH matrix；纯 Rust UI 视图改动可由 path
filter 减少重复 Harness build，但定期 scheduled run 仍运行全矩阵。

### 17.2 Release candidate

1. 检查 support registry 和 package manifests 一致。
2. 构建所有 native packages。
3. 打包 TypeScript protocol mirror/core/adapters/runtimes/CLI，并校验 Rust canonical
   contract 的跨语言 fixtures。
4. 执行 registry dependency gate。
5. 在独立临时目录从 tarball/registry cold install。
6. 使用默认 supported DSH 运行 doctor、hello、list、load 和 PTY dogfood。
7. 按 native → runtime → cold install → CLI 顺序发布。
8. 只有全部包发布并验证后才移动 npm dist-tag 和创建 GitHub Release。

### 17.3 source-only experimental

- 允许每日/每周定时从 exact alpha tag 构建和 E2E。
- 不触发 npm publish。
- 当上游 packages 发布后，首先将 support registry `distribution` 候选改为
  `npm-candidate`，通过 registry/cold install 门禁后再改为 `npm`。

## 18. 支持策略

建议默认策略：

- `supported`：最新已发布 npm DSH，完整 CI/E2E/发布门禁。
- `maintenance`：上一个已发布 DSH，保留 adapter 与基本 E2E，只修复
  高优先级问题。
- `experimental`：下一个 exact source tag，可破坏、不作为 npm 默认产物。
- `unsupported`：不再进入主线 CI，使用它的用户停留在最后一个已知可用
  pager release。

当前建议初始状态：

| DSH | 状态 | 原因 |
|---|---|---|
| `0.1.0-rc.8` | maintenance | 已有 legacy 实现，但不是 npm 最新 |
| `0.1.1-rc.2` | candidate → supported | npm 最新，尚需完成 adapter/E2E |
| `0.1.2-alpha.1` | experimental | 已完成源码 E2E，但上游 npm 依赖不存在 |

`candidate` 可以作为内部迁移状态，不向最终用户承诺 supported。

## 19. 分阶段实施路线

每个阶段独立记录、独立提交/PR，不将全部重构放入一个不可回滚批次。

### Phase 0：冻结基线与建立清单

产出：

- `compat/dsh-support.json`
- 当前 TUI v1 method/frame/fixture catalog
- alpha.1 bridge 高风险行为清单
- rc.8 legacy 可恢复代码对照清单

验收：

- 清单与三个 exact DSH tag/commit 一致。
- 现有 main 所有测试不变。

回滚：删除清单与检查脚本，不影响运行时。

### Phase 1：抽取 `TuiBackend`（无行为变化）

产出：

- `TuiBackend`/`TuiBackendInfo`/capability 内部接口
- gateway/dispatch/serve 仅依赖该接口
- 当前 alpha bridge 实现该接口

验收：

- alpha.1 TypeScript、Rust、real E2E、PTY 结果与抽取前一致。
- gateway 源码不 import 具体 alpha bridge class。

回滚：可单独 revert interface extraction。

### Phase 2：protocol/core DSH 去耦

产出：

- Rust canonical + TypeScript mirror 的自有 `SessionId`、session event 投影和
  运行时 codec
- invariant 插件转移到 adapter/runtime 边界
- core fake backend contract tests

验收：

- TypeScript protocol mirror/core 中 `rg '@deepseek-ai/dsh-'` 结果为空。
- TypeScript/Rust fixtures 一致。
- alpha.1 真实 E2E 不变。

回滚：可单独 revert DTO/codec ownership 迁移。

### Phase 3：整理 `controllers-v2`

产出：

- alpha bridge 转为 controllers-v2 adapter
- 按 history/streams/interactions/workspace/normalize 拆分
- controllers-v2 conformance suite

验收：

- 所有当前 alpha.1 门禁通过。
- opening snapshot、old-page cursor、live barrier、waterfall reconnect 都有定向用例。

回滚：退回拆分前单 bridge 实现，SPI 保持不变。

### Phase 4：引入 `apiproxy-v1` 并适配 rc.2

产出：

- 从 legacy 移植的 ApiProxy adapter
- rc.8/rc.2 adapter-local normalization
- apiproxy-v1 conformance suite
- rc.8 和 rc.2 真实 E2E

验收：

- 两个版本产生相同稳定 TUI scenario outputs。
- 未修改 Rust/Grok UI 产品行为，或任何必要差异都有独立协议评审。

回滚：删除 apiproxy-v1 adapter/runtime，controllers-v2 继续可用。

### Phase 5：版本化 profile/runtime 和 CLI resolver

产出：

- family-specific patches/runtimes/profiles
- support registry-driven CLI selection
- `doctor` backend/adapter/capability 证据
- 可恢复 profile migration

验收：

- 同一台机器可分别运行 rc.2 和 alpha.1 profile，不共享不兼容插件。
- 不支持版本在启动 pager 前给出明确错误。
- 迁移不删除用户 sessions/credentials。

回滚：CLI 恢复单默认 runtime，已备份 profile 可恢复。

### Phase 6：完成 CI 矩阵与 npm rc.2 候选

产出：

- 三 DSH 版本独立 CI
- registry dependency gate
- tarball cold/warm/offline dogfood
- rc.2 默认 npm publish workflow

验收：

- PR 和 main CI 全绿。
- rc.2 所有运行时依赖 registry 可用。
- 无 sibling Harness checkout 的干净环境安装、doctor、hello、load 和 PTY 通过。
- 在明确版本/dist-tag/批准前不执行公开 publish。

回滚：发布前直接停止 workflow；发布后使用 npm deprecate/dist-tag 回退，
不尝试覆盖已发布版本。

## 20. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| SPI 过度抽象 | 首次迁移成本过高 | 以当前 bridge 公开方法为起点，先无行为变化抽取 |
| 两个 adapter 逻辑复制 | 修复只落到一边 | 只共享 TUI 稳定 DTO/core，用同一 conformance suite 防止语义漂移 |
| 过度依赖 `RecordLike` | 上游变化在运行时才暴露 | 高风险边界使用 adapter-local schema/guards |
| 同名 DSH packages 多版本混装 | Cordis service identity 或 peer 解析错 | family-specific runtime/profile，独立 fixture/node_modules |
| profile 迁移破坏用户状态 | 用户丢失配置或无法回退 | 改名备份、schema metadata、不删 sessions/credentials |
| capability 与真实实现不一致 | UI 误开功能 | conformance 要求每个 true capability 有成功场景 |
| alpha 未发布依赖混入 release | npm 包可发布但不可安装 | registry gate + distribution=`source-only` fail closed |
| CI 矩阵时间过长 | PR 反馈慢 | core/adapter path filter + scheduled 全矩阵，supported 版本始终保留完整门禁 |
| 新 DSH 变更 wire 语义 | 错误地在 adapter 内隐藏产品变更 | 独立 protocol decision/fixture 评审，必要时明确升级 protocol |

## 21. 可量化验收条件

架构迁移只有在以下条件全部满足时才算完成：

1. protocol/server core 中不存在 `@deepseek-ai/dsh-*` imports。
2. gateway/serve/control-plane 只依赖 `TuiBackend`。
3. rc.8、rc.2、alpha.1 在 support registry 中均有 exact tag、commit、family、status、
   distribution 和 profile schema。
4. ApiProxy v1 与 Controllers v2 都通过同一 conformance scenario catalog。
5. rc.8/rc.2/alpha.1 各自独立安装和真实 E2E 通过。
6. rc.2 npm 候选在无本地 checkout/link 环境完成 cold install 和 PTY dogfood。
7. alpha.1 registry 依赖未发布时，正式 publish job 在上传任何包前失败。
8. `doctor` 对 supported、maintenance、experimental、unsupported 四类公开状态和
   candidate 预发布状态给出可操作的诊断，且不会把 candidate 显示为已支持。
9. family/profile 迁移可回滚，且自动测试证明 sessions/credentials 不被删除。
10. 后续同 family DSH 升级的常规改动限定于 support registry、adapter
    normalization/tests 和 runtime manifest；如越过该范围，PR 必须解释原因。

## 22. 需要维护者评估的决策

### D1：支持窗口

推荐：“最新 npm + 上一个 npm 一个周期 + 一个 source-only next”。

可选：

- 只支持最新 npm：成本低，但旧用户升级压力大。
- 无限期支持所有版本：不推荐，CI 和安全修复成本会持续增长。

### D2：adapter 命名粒度

推荐：按架构族命名 `apiproxy-v1` / `controllers-v2`，精确 DSH 版本放入
support registry。

不推荐：为 `rc.8`、`rc.2`、`alpha.1` 每个版本复制整个 adapter。

### D3：首个可发布 npm 默认 DSH

推荐：`0.1.1-rc.2`，因为它是当前最新已发布 npm DSH。

`0.1.2-alpha.1` 继续作为 source-only experimental，不回退已完成的适配代码。

### D4：profile 切换

推荐：不同 adapter family 使用不同内部 profile 名和 schema，切换时备份旧
profile。

不推荐：在同一 `dsh-pager-grok` profile 中就地增删两代插件。

### D5：capability 暴露位置

推荐：首先放入 `host.describe`；只有确实需要连接前协商时才以可选字段
扩展 `tui.hello`。

### D6：包拆分节奏

推荐：先在当前 server package 内拆 core/adapters，通过双 adapter E2E 后再拆 npm
workspace packages。

可选：立即拆成多包；最终结构更快成形，但会将代码边界、package
resolution 和发布变更混在同一批次。

## 23. 评审清单

评审时建议重点确认：

- [ ] 是否认可“一主线 + 稳定 core + adapter family”的方向？
- [ ] 是否认可首个 npm 默认后端为 DSH `0.1.1-rc.2`？
- [ ] 是否认可 `0.1.2-alpha.1` 在上游发布前保持 source-only？
- [ ] 是否认可 supported/maintenance/experimental 支持窗口？
- [ ] 是否认可 family-specific profile/runtime package？
- [ ] 是否认可优先在单 server package 内抽边界，再做 npm 多包拆分？
- [ ] capability 集是否足够，哪些应当是所有 supported backend 的必须能力？
- [ ] 是否需要调整 Phase 0–6 的顺序或每阶段范围？

## 24. 建议结论

建议接受本方案的总体路线，并从 Phase 0 和 Phase 1 开始：先冻结事实、
抽取现有 `TuiBackend` port，不同时改产品行为和 npm 包结构。

这条路径的最终效果是：

- rc.2 成为可安装、可发布的默认 DSH 后端。
- alpha.1 适配成果保留并持续回归，不因 npm 尚未发布而丢失。
- 后续 DSH 升级大多数被限制在 adapter/runtime 边界内。
- Rust/Grok TUI 可以继续专注产品交互和视觉，不被上游 Host API 重构反复
  牵动。
- 每个“支持”和“可发布”声明都有兼容矩阵、真实 E2E 和 registry cold
  install 证据支撑。
