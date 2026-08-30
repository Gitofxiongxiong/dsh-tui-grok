# DSH 多版本 Phase 5a capability 门禁

> 记录时间：2026-08-30 16:15:35 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

按 D-R8 在 protocol 中建立 method→capability 单一映射，在 core dispatch
调用 adapter 前执行门禁；不支持时返回稳定的
`ApiResult { ok:false, error.code:'unsupported-capability' }`，不改 Rust wire、
`TUI_PROTOCOL_VERSION`、method 字符串或 `TuiErrorKind`。取消两个 adapter
conformance 中的 Phase 5 skip，并使 fixture 对完整 65-method catalog 的
capability 归属做跨语言门禁。

## 设计契约和复用依据

- 对应长期计划：`DSH_MULTI_VERSION_COMPATIBILITY_PLAN.md` §9、§16、
  §19 Phase 5、§20 capability 风险、§21.4。
- protocol catalog 实际由 55 unary、5 control、5 notification 组成，共 65
  个 method；运行时映射仅门禁 unary，`tui.*` control 不设 capability。
- rc.2 `@deepseek-ai/dsh-host-apiproxy@0.1.1-rc.2` 的 fetch handler 已逐项
  核对：53 个 catalog unary 为原生 ApiProxy route，`fileReferences.list`
  和 `commands/*` 由已登记 profile extension 补齐。
- Grok source path/commit/SOURCE_REV：不涉及 Grok UI 源码或视觉变更。
- 复用等级：C（DSH protocol/core/adapter 能力真源与业务错误）。

## 计划修改范围

- `packages/dsh-tui-protocol/src/methods.ts`、`src/index.ts`、
  `tests/fixture-catalog.spec.ts`、`tests/fixtures/capability-map.json`
- `crates/dsh-pager-protocol/tests/fixture_catalog.rs`、
  `tests/fixtures/capability-map.json`
- `scripts/check-protocol-fixtures.py`
- `packages/dsh-tui-server/src/core/backend.ts`、`src/core/dispatch.ts`
- `packages/dsh-tui-server/src/adapters/controllers-v2/plugin.ts`、
  `src/adapters/apiproxy-v1/plugin.ts`
- `packages/dsh-tui-server/tests/gateway.spec.ts`、
  `tests/conformance/suite.ts`
- 如真实 rc.2 false-capability 抽验需要：
  `compat/fixtures/dsh-0.1.1-rc.2/index.js`、`run-e2e.sh`、`README.md`。
- 本进度记录。
- 不在范围：CLI/profile migration/runtime 包、依赖清理、CI/发布、
  方案正文、Rust UI 和用户数据。

## 风险、回滚和依赖

- 风险：method 归属错误会拒绝 adapter 实际支持的功能；通过 65
  项 fixture 完整性、两套 conformance 成功场景与 false 路径测试防护。
- 风险：把 ApiProxy optional extension 当成 DSH 原生能力；保留
  `fileReferences` 条件声明，并在真实 profile 已挂载时才报 true。
- 回滚：revert 本批 commit 即恢复直接 dispatch 和原 fixture catalog；
  不涉及 profile/会话/凭据或机器状态。

## 预期验证

- `pnpm run typecheck:ts`、`pnpm run test:ts`
- `cargo fmt --all -- --check`、`cargo check --workspace`、
  `cargo test --workspace --locked`
- `python3 scripts/check-protocol-fixtures.py`
- `node scripts/check-dsh-support.mjs` 及 `git diff --check`
- alpha.1：`DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness-latest
  DSH_TUI_PROFILE=dsh-pager-grok-e2e scripts/real-e2e.sh`
- rc.2：`corepack pnpm@11.7.0 --dir compat/fixtures/dsh-0.1.1-rc.2 run e2e`
- 如 rc.2 真实 profile 存在 false capability，抽验其稳定业务错误。

## 实际修改

- protocol 新增 14 项 `TUI_CAPABILITY_SET`、55 项完整
  `TUI_METHOD_CAPABILITY_MAP` 与 `capabilityForTuiUnaryMethod`。`session.updateQueue`
  单独归属 `queue`，其他 session/commands/host describe 归属 sessions；
  workspace/settings/credentials/preset/goal/subagent/skill/file-reference/directory
  方法按产品面归属。`tui.*` control 和 server notification 明确为
  `null`，不设产品能力门禁。
- Rust canonical 与 TypeScript mirror 同时新增 `capability-map.json`；
  Rust/TS fixture tests 断言其与 55+5+5 method catalog 完整对齐，
  `check-protocol-fixtures.py` 通过全文件集同步自动将其纳入门禁，
  因此脚本本身无需修改。
- core `dispatchUnary` 在 adapter call 前检查 `backend.info.capabilities`；
  false 时返回 `unsupported-capability`、method/capability/family/DSH 版本
  details，且 adapter 零调用。这是 `ApiError.code` 业务错误，没有
  修改 `TuiErrorKind`、Rust wire 或协议版本。
- 两个 conformance suite 各自的 capability skip 改为可执行场景；
  同时维护 capability→成功场景的穷举表，所有声明 true 的能力均
  对应已实际通过的 scenario 1/5/6/7/8/9/10。
- ApiProxy v1 保留真实动态差异：仅当 `resolveAgent` 与
  `fileReferences` 扩展均挂载时声明 fileReferences=true；缺任意
  一项时为 false。rc.2 真实 fixture 已挂载该扩展与 commands，
  其 ApiProxy handler 也提供其余 53 个 unary，因此该发布组合 14 项
  capability 全为 true，没有伪造 false 或用全 true 遮蔽缺失路由。

## 验证结果

- `pnpm run typecheck:ts`：通过，protocol/server/embedded 全绿。
- `pnpm run test:ts`：通过；protocol 17 passed，server 71 passed
  （两套 conformance 各 12 passed、0 skip），embedded/runtime/CLI 全绿。
- `cargo fmt --all -- --check`：首轮只报新增 Rust fixture test 需
  rustfmt；执行 `cargo fmt --all` 后复验通过，无范围外文件变化。
- `cargo check --workspace`：通过。
- `cargo test --workspace --locked`：通过，workspace unit/doc tests 全绿。
- `python3 scripts/check-protocol-fixtures.py`：通过，显示
  `protocol fixtures in sync (6 JSON files)`。
- 指定本机实际 rc.8/rc.2/alpha.1 checkout 运行
  `node scripts/check-dsh-support.mjs`：3 版本 tag/commit/packageManager 与
  22 项 exact runtime dependency 全部通过。首次使用的 rc.8/rc.2
  猜测路径不存在；只读定位后用
  `/home/leo/code/deepseek-harness`、
  `/home/leo/code/deepseek-harness-dsh-v0.1.1-rc.2`、
  `/home/leo/code/deepseek-harness-latest` 重跑通过。
- alpha.1 真实 E2E：`DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness-latest
  DSH_TUI_PROFILE=dsh-pager-grok-e2e scripts/real-e2e.sh` 通过 hello/list/
  dashboard/load/PTY，新建了隔离空 session
  `session-8f6f50fc-b156-4a34-8655-90580df2b9eb`；没有删除或改写
  既有 session/credentials/profile。
- rc.2 真实 E2E：`corepack pnpm@11.7.0 --pm-on-fail=ignore --dir
  compat/fixtures/dsh-0.1.1-rc.2 run e2e` 通过 hello/list/dashboard/
  load/PTY，隔离 home 为 `/tmp/dsh-pager-phase4b-rc2.cxKx5E`，退出后
  已由 runner 清理。该真实 profile 无 false capability，因此 D-R8
  “如存在 false”的真实抽验条件未触发；false 路径已由 core
  contract 和两套 conformance 三处执行验证。
- `git diff --check`：通过。Grok UI 视觉与交互零变更，本批
  不触发浏览器像素门禁。

## Git 提交

- 预计 commit message：`feat(protocol): enforce backend capabilities`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-30_16-15-35_DSH多版本Phase5aCapability门禁.md`
- 暂存区审计：只包含本记录列出且实际变更的 protocol
  source/fixture/tests、server core/tests 与本记录；不包含方案正文、
  compat profile、CLI/runtime、registry、Rust UI 或构建产物。

## 未解决问题和下一步

- rc.2 发布组合没有 false capability；当后续 profile 移除
  file-reference 扩展时，`doctor` 必须展示 fileReferences=false，core 会
  保证该 unary 稳定失败而不是 internal error。
- Phase 5b 实现 registry-driven CLI resolver/doctor，并让 doctor 直接展示
  本批建立的 capability 真源。
