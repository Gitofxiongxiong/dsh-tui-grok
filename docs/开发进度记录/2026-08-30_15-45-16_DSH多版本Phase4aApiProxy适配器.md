# DSH 多版本 Phase 4a ApiProxy v1 Adapter

> 记录时间：2026-08-30 15:45:16 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

从 `legacy/main-before-harness-0.1.2-alpha.1` 恢复 ApiProxy v1 的适配职责，
实现新的 `TuiBackend`，复用当前 core 与 Phase 3b 的同一 conformance scenario
catalog。此批只落 adapter 与 fake conformance，不建立 rc.2 真实 profile；真实
install/E2E/PTY 是下一批 4b 的阻塞门禁。

## 设计契约和复用依据

- 对应长期计划：§7、§8.1、§13、§16.2、§19 Phase 4。
- legacy 来源按基线清单读取：gateway `events.mux/host` envelope 剥离；dispatch
  `toFetchHandler` carrier、respond、fileReferences/commands extensions；index/serve
  composition wiring。只恢复这些职责，不复制旧 gateway。
- D-R4：源码只使用 adapter-local `*Like` 结构类型；不静态 import rc.2
  `@deepseek-ai/*`，不在根 manifest 引入 rc.2。`toFetchHandler` 由调用方的
  profile-local `require` 在 adapter runtime helper 内解析，并在构造前断言。
- ApiProxy 单 mux 流承载 session/live/control/interaction；`followSession` 是不
  产帧且等待 abort 的空流，避免 core 把正常单流模型误判为 follower 提前关闭。
- info 必须由调用方传入实际版本；family=`apiproxy-v1`、profileSchema=1，
  capability 依据 ApiProxy method map 与 extensions 是否挂载声明。
- 复用等级：B/C（legacy 行为移植与本地结构化类型重写）；TUI wire 不变。

## 计划修改范围

- `packages/dsh-tui-server/src/adapters/apiproxy-v1/context.ts`
- `packages/dsh-tui-server/src/adapters/apiproxy-v1/plugin.ts`
- `packages/dsh-tui-server/src/adapters/apiproxy-v1/runtime.ts`
- `packages/dsh-tui-server/src/adapters/apiproxy-v1/normalize.ts`
- `packages/dsh-tui-server/src/adapters/apiproxy-v1/unary.ts`
- `packages/dsh-tui-server/src/adapters/apiproxy-v1/streams.ts`
- `packages/dsh-tui-server/src/adapters/apiproxy-v1/backend.ts`
- `packages/dsh-tui-server/src/index.ts`：仅新增 adapter class/type/runtime helper
  导出；controllers-v2 仍是唯一默认实例化对象。
- `packages/dsh-tui-server/tests/conformance/types.ts`、`suite.ts`、
  `controllers-v2.fixture.ts`：把 session frame iterator 抽成 fixture hook，使同一
  suite 可覆盖 controllers follow 与 ApiProxy mux carrier；断言不分叉。
- `packages/dsh-tui-server/tests/conformance/apiproxy-v1.fixture.ts`
- `packages/dsh-tui-server/tests/apiproxy-v1-conformance.spec.ts`
- `packages/dsh-tui-server/tests/apiproxy-v1-runtime.spec.ts`
- 本进度记录。
- 不修改 core、controllers-v2 生产实现、manifest、lockfile、Rust、protocol、
  runtime package、profile、CI 或方案正文。

## 风险、回滚和依赖

- 风险：空 `followSession` 若立即结束会触发 core stream/error；实现必须等待其
  AbortSignal，不产生 frame。
- 风险：ApiProxy envelope/receipt 形状漂移；adapter 边界做最小运行时 guard，
  malformed carrier fail closed，并由 fake conformance 覆盖。
- 风险：静态解析误用 alpha workspace 模块；runtime helper 只接收 profile-local
  `NodeRequire`，并断言 `toFetchHandler` export，不自行从仓库根解析。
- 风险：共享 suite 因 carrier 差异分叉；只允许 fixture hook 翻译 fake upstream，
  stable DTO 断言保持同一份。
- 回滚：单独 revert 本批即删除 apiproxy-v1；controllers-v2 与 core 不受影响。

## 预期验证

- `pnpm --filter @dsh-pager-grok/tui-server typecheck`
- `pnpm --filter @dsh-pager-grok/tui-server test`
- `pnpm run typecheck:ts && pnpm run test:ts`
- `cargo test --workspace --locked`
- `python3 scripts/check-protocol-fixtures.py`
- `node scripts/check-dsh-support.mjs`
- `rg "from ['\"]@deepseek-ai|import\\(['\"]@deepseek-ai" packages/dsh-tui-server/src/adapters/apiproxy-v1`
  （动态解析所需的模块字符串允许存在，静态 import 必须无结果）。
- `rg 'new ApiProxyV1Backend' packages/dsh-tui-server/src/index.ts`：无结果。
- `git diff --check` 与暂存区审计。

## 实际修改

- 新增 adapter-local `ApiProxyV1Like`、fetch carrier、extension 和 profile require
  结构类型；主 workspace 未引入 rc.2 类型或依赖。
- `ApiProxyV1Backend` 实现完整 `TuiBackend`：unary 通过 legacy 同构的
  `toFetchHandler` carrier，mux/host 剥离 ApiProxy envelope，respond 保留
  `client-response` carrier，attach/detach/reset/dispose 为无状态操作。
- `followSession` 实现为等待 abort 的空流；session/live/control/interaction 均从
  ApiProxy mux 单流进入 core，与 legacy 行为一致，且不会制造 stream closed。
- fileReferences、commands 和 resolveAgent 留在 adapter extensions；capability 仅在
  resolver 与服务同时存在时声明 `fileReferences=true`，其余 rc.2 ApiProxy method
  map 已有能力按实际声明。
- 新增 profile-local runtime resolver，对
  `@deepseek-ai/dsh-host-apiproxy.toFetchHandler` 做启动前 fail-closed 断言；包入口只
  导出 adapter class/types/helper，`index.ts` 未实例化它。
- family-neutral conformance fixture 增加 `sessionFrames` hook，使 controllers-v2 从
  follow stream、apiproxy-v1 从 mux stream 驱动同一组稳定 DTO 断言。两侧 goal
  golden 统一为上游真实 `{ ref: { id, revision } }` 形状。
- 新增 ApiProxy fake services 和 runtime resolver 单测；共享 §16.2 场景无
  family-specific 断言分叉。prompt 的 requestId 与取消路径在 adapter 边界保持
  controllers-v2 的稳定错误语义。

## 验证结果

- `pnpm --filter @dsh-pager-grok/tui-server typecheck`：通过。
- `pnpm --filter @dsh-pager-grok/tui-server exec vitest run
  tests/apiproxy-v1-conformance.spec.ts tests/controllers-v2-conformance.spec.ts`：
  通过，22 passed、2 skipped；两项 skip 均为 Phase 5 capability enforcement。
- `pnpm run typecheck:ts`：通过（protocol/server/embedded）。
- `pnpm run test:ts`：通过；server 为 67 passed、2 skipped，其他 TS package 全绿。
- `cargo test --workspace --locked`：通过，workspace unit/doc tests 全绿。
- `python3 scripts/check-protocol-fixtures.py`：通过，5 个 JSON fixture 一致。
- `node scripts/check-dsh-support.mjs`：通过，3 个注册版本与 22 个 exact runtime
  dependency declaration 校验成功；未设置 checkout env 的三项按脚本契约 skip。
- adapter 与新增 ApiProxy 测试中无 `@deepseek-ai/*` 静态 import；runtime 文件中的
  包名只用于调用方注入的 profile-local require。
- `rg 'new ApiProxyV1Backend' packages/dsh-tui-server/src/index.ts`：无结果。
- `git diff --check`：通过。

## Git 提交

- commit message：`feat(server): add apiproxy v1 adapter`。
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-30_15-45-16_DSH多版本Phase4aApiProxy适配器.md`
- 暂存区审计：仅包含本记录“计划修改范围”列出的 adapter、共享 conformance
  hook/fixture/spec 和包入口文件；无 manifest、lockfile、core、controllers 生产
  实现、Rust 或 profile 文件。

## 未解决问题和下一步

- 4b 必须用真实 rc.2 npm/profile 证明 install/build/E2E/PTY；fake conformance
  不能提升 registry status。
- 代表性 goal 响应经 rc.2 与 alpha.1 上游源码核对均为 `{ ref }`；原 Phase 3b
  fake 里的扁平 `{ id, revision }` 是测试夹具事实偏差，本批已修正 golden，未改
  项目 wire 实现。
- Phase 4 尚未完成：rc.2 真实验证是下一批阻塞门禁；rc.8 按 D-R2 随后尽力验证。
