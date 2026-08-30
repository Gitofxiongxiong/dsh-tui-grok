# DSH 多版本 Phase 3b Controllers v2 Conformance

> 记录时间：2026-08-30 15:37:17 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

为 Phase 3a 拆出的 `controllers-v2` adapter 建立 family-neutral conformance
scenario catalog。套件只依赖 `TuiBackend` 和稳定 TUI DTO；controllers-v2
提供最小 fake DSH services 与 adapter 工厂，Phase 4 的 apiproxy-v1 将复用同一
场景定义而不是复制断言。

## 设计契约和复用依据

- 对应长期计划：§7.3、§16.2、§19 Phase 3。
- 场景目录覆盖 §16.2 的 12 类；输出与共享 golden 比较，adapter-specific
  fixture 只暴露输入控制、调用记录和上游事件注入。
- 六类高风险边界优先定向覆盖：session summary、history snapshot/page/chunk、
  live/control、approval/question、workspace follow、model/preset/goal/subagent。
- opening snapshot、old-page cursor、live frame barrier 和 waterfall reconnect
  必须是显式测试，不以普通 unary 冒烟替代。
- controllers-v2 所有 capability 当前均为 true；“capability 缺失语义”没有本
  family 的负例，按规格登记为 skip，不能借此实现 Phase 5 的 core enforcement。
- 复用等级：C（项目自有 adapter 契约测试）。

## 计划修改范围

- `packages/dsh-tui-server/tests/conformance/types.ts`：family-neutral fixture 契约。
- `packages/dsh-tui-server/tests/conformance/goldens.ts`：稳定 TUI DTO golden。
- `packages/dsh-tui-server/tests/conformance/suite.ts`：§16.2 共享场景实现。
- `packages/dsh-tui-server/tests/conformance/controllers-v2.fixture.ts`：最小
  controllers service fake 与 adapter 工厂。
- `packages/dsh-tui-server/tests/controllers-v2-conformance.spec.ts`：注册本 family。
- 本进度记录。
- 不修改生产源码、现有测试断言、manifest、lockfile、Rust、wire、profile、
  CI 或方案正文。

## 风险、回滚和依赖

- 风险：共享套件泄漏 controllers 专有类型；通过 `suite.ts`/`types.ts` 不 import
  adapter 目录以及后续 apiproxy fixture 可直接注册来约束。
- 风险：异步流测试挂起；每个 fixture 使用独立 AbortController，并在 finally
  dispose backend。
- 风险：golden 过宽掩盖漂移；关键 frame/result 使用深度全等，动态 request id
  只通过 fixture 捕获后验证 reconnect 稳定与 at-most-once。
- 回滚：单独 revert 本批测试提交，不影响 Phase 3a 产品实现。

## 预期验证

- `pnpm --filter @dsh-pager-grok/tui-server test`
- `pnpm run typecheck:ts`
- `pnpm run test:ts`
- `cargo test --workspace --locked`
- `python3 scripts/check-protocol-fixtures.py`
- `node scripts/check-dsh-support.mjs`
- `DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness-latest DSH_TUI_PROFILE=dsh-pager-grok-e2e scripts/real-e2e.sh`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full --timeout 45`
- `rg '@deepseek-ai' packages/dsh-tui-server/tests/conformance/suite.ts packages/dsh-tui-server/tests/conformance/types.ts`
- `git diff --check` 与暂存区审计。

## 实际修改

- 新增 family-neutral `AdapterConformanceFixture` 与 factory 契约；共享 suite 只
  import 稳定 protocol/core 类型，不 import controllers-v2 或任何 DSH package。
- 新增 controllers-v2 fake context，提供结构化 session/workspace/settings/
  credentials/preset/goal/subagent/command services、可控 follow/control frame、
  waterfall event 注入、调用记录、取消和错误注入；用真实
  `ControllersV2Backend` 工厂注册共享 suite。
- 新增共享 golden 与精确 DTO 断言。动态 waterfall request id 由输出捕获，
  随后验证 reset/attach replay 保持同一 id、respond 成功 frame 和重复应答拒绝。
- 首轮测试发现新预期与既有基线有两处偏差：chunk-row 的真实投影使用
  `data.chunk`，且成功 approval 会先发布 `approval/resolved`；只修正新测试
  golden/读取顺序，没有修改生产实现。
- 生产源码、现有测试、manifest、lockfile、Rust、wire、profile 与方案正文均未改。

## §16.2 场景审计

| # | 场景 | 结果/用例 |
|---|---|---|
| 1 | session.list/search/create/history | 通过：共享用例 1，含 cold inspect history golden |
| 2 | opening snapshot 与旧页 cursor | 通过：共享用例 2，断言 chunk 展开与 `throughSeq=4` |
| 3 | history/live barrier | 通过：共享用例 3，history seq 4 后 live seq 5，无丢失/重复 |
| 4 | prompt/request identity/cancel | 通过：共享用例 4，缺 id 拒绝、原 id 透传、abort 归一化 |
| 5 | queue/jobs/projection | 通过：共享用例 5，baseline 三类 frame |
| 6 | approval/question claim/respond/abort/reconnect | 通过：共享用例 6，stable id/replay/at-most-once/question abort |
| 7 | workspace baseline/follow/order/archive | 通过：共享用例 7，baseline/upsert/order/archive/mutation |
| 8 | settings/credentials | 通过：共享用例 8，describe/update/set/unset DTO |
| 9 | presets/goals/subagents | 通过：共享用例 9，并含 model selection 投影 |
| 10 | file refs/directory picker/commands | 通过：共享用例 10，并含 skill catalog |
| 11 | error normalization | 通过：共享用例 11，`GOAL_STALE` → stable internal + detail |
| 12 | capability 缺失语义 | skip：controllers-v2 全部 capability=true；core enforcement 属 Phase 5 |

## 验证结果

- `pnpm --filter @dsh-pager-grok/tui-server test`：通过，6 files，54 passed，
  1 documented skip；新增 11 个可执行 conformance 场景全绿。
- `pnpm --filter @dsh-pager-grok/tui-server typecheck`：通过。
- `pnpm run typecheck:ts`：通过。
- `pnpm run test:ts`：通过；既有 protocol/server/embedded/runtime/CLI 与新增
  conformance 全绿。
- `cargo test --workspace --locked`：通过，workspace unit/integration/doc tests
  零失败。
- `python3 scripts/check-protocol-fixtures.py`：通过，5 份 fixture 一致。
- `node scripts/check-dsh-support.mjs`：通过，3 个版本、22 个 exact runtime
  dependency 声明通过；未注入 checkout 路径的 HEAD 检查按设计 skip。
- `DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness-latest DSH_TUI_PROFILE=dsh-pager-grok-e2e scripts/real-e2e.sh`：
  通过，默认 45 秒完成真实 hello/list/dashboard/load/PTY，收到 `SessionLoaded`。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full --timeout 45`：
  通过。
- `rg '@deepseek-ai' packages/dsh-tui-server/tests/conformance/suite.ts packages/dsh-tui-server/tests/conformance/types.ts`：
  无结果，scenario catalog 为 family-neutral。
- `git diff --check`：通过。

## Phase 3 §19 验收

- [x] alpha bridge 已成为 controllers-v2 adapter（`dc2c35e`）。
- [x] history/streams/interactions/workspace/normalize 已建立责任边界。
- [x] controllers-v2 所有适用 conformance 场景全绿；仅 Phase 5 capability
  enforcement 负例按设计 skip。
- [x] 当前 alpha.1 静态、TS/Rust、fixture、真实 E2E 与 PTY 门禁通过。
- [x] opening/cursor/live barrier/waterfall reconnect 有共享 suite 定向用例。

## Git 提交

- commit message：`test(server): add controllers v2 conformance`。
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-30_15-37-17_DSH多版本Phase3bControllersConformance.md`
- 暂存区审计：提交前仅暂存本记录和计划列出的五个 conformance 测试文件。

## 未解决问题和下一步

- Phase 3 §19 验收已满足。下一批 4a 由 apiproxy-v1 fixture 注册同一 suite；
  不复制 scenario 断言。
- 代表性 mux/host frame 已由共享 scenario 的精确 DTO 断言建立；fixture canonical
  文件仍按 Phase 2 决策由跨语言目录管理，本批未改变 Rust frame 所有权。
