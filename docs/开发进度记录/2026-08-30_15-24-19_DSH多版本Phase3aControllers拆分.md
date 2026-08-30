# DSH 多版本 Phase 3a Controllers v2 拆分

> 记录时间：2026-08-30 15:24:19 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

在已恢复的 alpha.1 真实 E2E/PTY 基线上，将 `dsh-tui-server` 的 DSH-neutral
core 与 `controllers-v2` adapter 建立物理目录边界，并把单体 `bridge.ts` 按
方案 §8.2 的责任拆分。此批只改变源码组织，不改变 TUI wire、方法字符串、
错误语义或运行时行为。

前置证据：

- `f381d6c7a8b70c3e742ad3cd5c45e3feebaa9847` 记录了原始真实 PTY blocker。
- `b394c7018522a84e187ac0739f9afc1412c4594d` 修复隔离 profile 链接后，原始
  默认 45 秒 alpha.1 `real-e2e.sh` 已完整全绿。

## 设计契约和复用依据

- 对应长期计划：§7.2、§7.3、§8.2、§16.3、§19 Phase 3。
- core 拥有 gateway/control-plane/dispatch/serve/transport/errors/backend；
  adapter 拥有 DSH context、unary/history/stream/waterfall/workspace/normalize。
- opening snapshot、old-page cursor、live follower 与 attach/load barrier 的状态
  继续由单个 `ControllersV2Backend` 实例统一持有；拆出的模块提供类型、纯转换或
  受该实例调用的操作，不复制或分叉 follower 状态。
- 根 `index.ts` 仍是唯一 runtime 实例化/组装层；包根入口现有导出全部兼容。
  私有 `src/*` 深路径随内部目录迁移，不承诺兼容，避免同名 shim 破坏 Git rename
  检测与 `--follow` 历史。
- 复用等级：C（内部结构重排）；不涉及 Grok UI 或外部协议变更。

## 计划修改范围

- 新目录 `packages/dsh-tui-server/src/core/`：移动
  `backend.ts`、`gateway.ts`、`control-plane.ts`、`dispatch.ts`、`serve.ts`、
  `transport.ts`、`errors.ts`。
- `packages/dsh-tui-server/src/adapters/controllers-v2/`：
  `plugin.ts`、`context.ts`、`backend.ts`、`unary.ts`、`history.ts`、`streams.ts`、
  `interactions.ts`、`workspace.ts`、`normalize.ts`。
- `packages/dsh-tui-server/src/index.ts`：从新 core/adapter 边界组装并保留旧导出。
- `packages/dsh-tui-server/tests/bridge.spec.ts`、`control-plane.spec.ts`、
  `gateway.spec.ts`、`transport.spec.ts`：只机械更新私有源码 import 到新目录，
  不修改断言或场景。
- 本进度记录。
- 不修改测试断言、manifest、lockfile、Rust、协议包、profile 或方案正文。

## 风险、回滚和依赖

- 风险：ESM 相对路径移动后出错；通过 typecheck、单包测试和全量 TS 测试检查。
- 风险：不恰当拆分 history/follower 状态会制造丢帧或重复；状态集合和 opening
  promise 均保留在 backend 实例，并复跑现有定向测试与真实 alpha.1 PTY。
- 风险：文件历史丢失；主要实现与 core 文件使用 `git mv`，提交后用
  `git log --follow` 抽查。
- 回滚：单独 revert 本批 commit 即恢复扁平源码；SPI 与前两阶段不受影响。
- 依赖：alpha.1 checkout、专用 `dsh-pager-grok-e2e` profile、Node/pnpm、Rust。

## 预期验证

- `pnpm run typecheck:ts`
- `pnpm run test:ts`
- `cargo test --workspace --locked`
- `python3 scripts/check-protocol-fixtures.py`
- `node scripts/check-dsh-support.mjs`
- `DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness-latest DSH_TUI_PROFILE=dsh-pager-grok-e2e scripts/real-e2e.sh`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full --timeout 45`
- `rg '@deepseek-ai' packages/dsh-tui-server/src/core`
- `git diff --check`、`git log --follow` 与暂存区文件审计。

## 实际修改

- 将七个 DSH-neutral core 实现移动到 `src/core/`；测试的私有源码 import
  机械指向新目录，包根入口导出保持兼容。core 不包含任何 `@deepseek-ai` import。
- 将原 `bridge.ts` 主实现移动为
  `src/adapters/controllers-v2/backend.ts`，类名改为 `ControllersV2Backend`；
  `TuiHarnessBridge` 作为 deprecated 包根导出别名保留。
- controllers-v2 adapter 拆成方案建议的九个模块：
  - `plugin.ts`：精确 alpha.1 identity、profile schema、capability 与 Cordis inject；
  - `context.ts`：adapter-local 结构化 DSH service 类型；
  - `backend.ts`：唯一连接/attach/follower/interaction/cache 状态所有者；
  - `unary.ts`：稳定 TUI unary method 到 controllers service 的映射；
  - `history.ts`：chunk-row 展开、pagination、opening 类型与 tool-call 回扫；
  - `streams.ts`：mux/host async queue；follower opening promise 与 abort 联动仍在
    backend 状态所有者内；
  - `interactions.ts`：approval/question pending 结构类型；领取、应答、reconnect
    与 frame 投影仍由 backend 的同一状态所有者处理；
  - `workspace.ts`：baseline clone 与 follow delta cache；
  - `normalize.ts`：边界读取、错误与 session summary 投影。
- opening snapshot、old-page `throughSeq`、live follower 和 attach/load barrier 的
  `followers`/`subagentOpenings` 状态仍只存在于一个 `ControllersV2Backend` 实例；
  拆分模块没有建立第二份状态或版本分支。
- 根 `index.ts` 仍是唯一调用 `new ControllersV2Backend(...)` 的组装层；现有
  `TuiHarnessBridge`、core class/type 和 `serve` 导出全部保留。
- 测试文件只改内部 import；未改断言、场景、manifest、lockfile、Rust、协议、
  wire、method string、profile 或方案正文。

## 验证结果

- `pnpm --filter @dsh-pager-grok/tui-server typecheck`：通过。
- `pnpm --filter @dsh-pager-grok/tui-server test`：通过，5 files / 43 tests；
  现有 9 个 bridge 定向用例与 16 个 gateway 契约用例均未改断言。
- `pnpm run typecheck:ts`：通过，protocol/server/embedded 全绿。
- `pnpm run test:ts`：通过；protocol 17、server 43、embedded 3、runtime 1、
  CLI 11 tests 全绿。
- `cargo test --workspace --locked`：通过；workspace unit/integration/doc tests
  零失败。
- `python3 scripts/check-protocol-fixtures.py`：通过，5 份 JSON fixture 一致。
- `node scripts/check-dsh-support.mjs`：通过，3 个版本、22 个 exact runtime
  dependency 声明通过；未注入 checkout 环境变量的 HEAD 检查按设计 skip。
- `DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness-latest DSH_TUI_PROFILE=dsh-pager-grok-e2e scripts/real-e2e.sh`：
  通过，默认 45 秒内完成真实 hello/list/dashboard/load/PTY 并收到
  `SessionLoaded`；结构拆分前后结果一致。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full --timeout 45`：
  通过。
- `rg '@deepseek-ai' packages/dsh-tui-server/src/core`：无结果。
- `git diff --check`：通过；core 使用显式 `git mv`，adapter 主实现保留原
  bridge 的可追踪主体，提交后以 `git log --follow --find-renames=20%` 抽查。

## Git 提交

- commit message：`refactor(server): split controllers v2 adapter`。
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-30_15-24-19_DSH多版本Phase3aControllers拆分.md`
- 暂存区审计：提交前仅暂存本记录、列出的 `src/core/`、
  `src/adapters/controllers-v2/`、`src/index.ts` 与四个机械 import 测试文件。

## 未解决问题和下一步

- Phase 3 的 conformance 产出尚未完成；下一批 3b 使用 family-neutral scenario
  catalog 补齐 §16.2，并把 waterfall reconnect 与 history/live barrier 作为
  controllers-v2 定向门禁。
- 本批未发现方案与代码事实的新冲突。
