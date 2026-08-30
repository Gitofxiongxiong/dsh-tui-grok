# DSH 多版本 Phase 5b CLI 解析与 Doctor

> 记录时间：2026-08-30 16:21:20 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

实现 `compat/dsh-support.json` 驱动的 CLI DSH entry/version/family/runtime/
profile schema 解析。未登记精确版本在启动 pager 前 fail closed，并
给出已测版本与可执行安装命令。扩展 `doctor` 输出 §15.2 的
版本选择证据、family profile/schema、能力或缺失 runtime 诊断，不读取
凭据内容。shell 启动脚本不再硬编码 alpha 版本。

## 设计契约和复用依据

- 对应长期计划：`DSH_MULTI_VERSION_COMPATIBILITY_PLAN.md` §11.2–§11.3、
  §13.2–§13.3、§15、§19 Phase 5、§21.8。
- canonical registry 仍只有根级 `compat/dsh-support.json`。CLI 源码开发态
  读该文件；`prepack` 将同一文件复制为 tarball 内的派生
  `lib/dsh-support.json`，不提交第二份手写源。
- family runtime 按 registry 延迟解析；本批不把 alpha/source-only runtime
  加入 CLI 公开依赖。缺失 runtime 时显示安装或显式开发配置提示。
- user-provided `--backend`/`DSH_TUI_SERVER` 仍是显式覆盖，CLI 不伪造
  无法检查的 DSH 版本。默认 npm 与显式 `DSH_BIN_JS` 均执行
  精确 package version 查找。
- adapter 启动时只在 CLI 传入 expected family/version/schema 时二次断言；
  现有脚本/fixture 未设该 env 时行为不变。
- Grok source/reuse：不涉及 Grok UI，复用等级 C（产品 CLI/DSH 真源）。

## 计划修改范围

- `packages/dsh-pager-cli/lib/launcher.js`、`lib/main.js`、
  `tests/launcher.spec.ts`、`package.json`
- `scripts/copy-support-registry.mjs`、`scripts/dsh-tui-common.sh`、
  `scripts/check-dsh-support.mjs`
- `packages/dsh-tui-server/src/core/backend-selection.ts`、`src/index.ts`、
  `src/adapters/controllers-v2/backend.ts`、
  `src/adapters/apiproxy-v1/backend.ts`、相关 server tests。
- 本进度记录。
- 不在范围：profile migration/ownership 实施、新 runtime-apiproxy-v1
  package、根 overrides/依赖清理、CI/发布、registry 状态、方案正文、
  Rust wire/UI 和用户数据。

## 风险、回滚和依赖

- 风险：从 bin.js 向上定位错误 package；必须验证 package name
  为 `@deepseek-ai/dsh` 且 version 是精确 semver。
- 风险：candidate 被显示成 supported；doctor 原样输出 registry status，
  并对 supported/maintenance/experimental/unsupported/candidate 都有定向测试。
- 风险：新 family profile 尚未创建；doctor 可见地报 missing，实际迁移
  留给 5c，不就地覆盖旧 profile。
- 回滚：revert 本批即恢复单 runtime/profile CLI；本批不修改或删除
  profile/session/credentials。

## 预期验证

- CLI unit：default supported、unknown fail-closed、显式 `DSH_BIN_JS`，
  以及五种 status 诊断。
- 用 rc.8/rc.2/alpha.1 三个本地 checkout 的 bin.js 各运行一次
  `doctor`，记录实际输出；使用临时 `DSH_HOME`，不读真实凭据。
- `pnpm run typecheck:ts`、`pnpm run test:ts`、fixture/support 门禁。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager`。
- `cargo test --workspace --locked`、`git diff --check`。

## 实际修改

- CLI 从真实 entry 向上定位 `@deepseek-ai/dsh/package.json`，检查
  exact semver，再精确查找 support registry。默认 `require.resolve`
  与显式 `DSH_BIN_JS` 共用同一解析算法。
- 未登记版本抛出 `UnsupportedDshVersionError`，信息包含 entry/
  package 证据、所有已测版本及 `npm install -g
  @deepseek-ai/dsh@<recommended> @dsh-pager-grok/cli`。registry 内状态为
  unsupported 时同样在 pager spawn 前拒绝。
- 解析结果包含 family/runtime/profile schema/status/distribution，profile
  内部名改为 `dsh-pager-grok-<family>`。warm check、update、uninstall、
  repair 都使用选中的 family runtime/profile；source-only 组合禁止从
  registry 自动安装，改为开发 profile 提示。
- native spawn 注入 expected adapter family/DSH version/profile schema；两个
  adapter constructor 在存在这些 env 时做第二次 fail-closed 断言。直接
  fixture/script 启动不设 env，原行为不变。
- `doctor` 新增 CLI↔native 版本对齐、DSH entry 来源/绝对路径、
  exact package version/manifest、family/runtime、status/distribution 与可操作
  语义、family profile 路径/schema、runtime metadata/capability 列表。
  runtime 尚未安装时显示安装或 `DSH_PAGER_RUNTIME_ROOT` 开发提示，
  capability 不可证时非零退出，不伪报健康。`doctor --release` 接口
  已保留，本批明确输出 Phase 6 gate placeholder 并失败。
- doctor 仅检查 API key 是否设置与 credentials 文件是否存在，
  不打开或输出凭据内容。
- CLI prepack 调用新脚本将 canonical registry 复制到包内 `lib/`；
  postpack 用同一脚本精确清理该派生 JSON，不作为第二份提交真源。
- `dsh-tui-common.sh` 改为从 canonical registry 选取唯一
  controllers-v2/source-only 版本；`check-dsh-support.mjs` 扫描 CLI/main/
  shell 三个消费者，任一 registry exact version 字面量都会失败。

## 验证结果

- `node --check` 两个 CLI lib、`bash -n scripts/dsh-tui-common.sh`：通过。
- CLI unit：15 passed，覆盖 npm-default supported、unknown fail-closed +
  tested/recommendation、显式 `DSH_BIN_JS`、五种 status 文本、family
  profile/backend argv、runtime warm alignment 和 `doctor --release` argv。
- server：75 passed，新增 4 个 adapter startup assertion 用例，覆盖
  family/version/schema 三种 mismatch 与直接启动兼容。
- `pnpm run typecheck:ts`、`pnpm run test:ts`：通过，所有 TS 包全绿。
- `python3 scripts/check-protocol-fixtures.py`：通过，6 JSON fixtures 一致。
- 带三个 exact checkout env 的 `node scripts/check-dsh-support.mjs`：通过，
  3 registry versions、22 exact DSH runtime declarations、3 个 CLI/script consumer 均通过。
- `source scripts/dsh-tui-common.sh && dsh_tui_required_harness_version
  /home/leo/code/dsh-pager-grok`：输出 `0.1.2-alpha.1`，值来自 registry。
- `cargo test --workspace --locked`：通过，Rust unit/doc tests 全绿。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager`：通过。本批
  仅 CLI/doctor 文本，Grok UI 视觉零变更，按硬决策不触发浏览器像素门禁。
- `node scripts/verify-cli-pack.mjs`：通过，prepack 从 canonical registry 产生
  tarball 内副本；验证用临时 tarball
  `/tmp/dsh-cli-pack-1ox6E9/dsh-pager-grok-cli-0.1.0.tgz` 由脚本退出时清理。
  首次审计发现 prepack 派生 JSON 未被现有 ignore 覆盖，补上
  `postpack --clean` 后重跑通过（第二个临时 tarball
  `/tmp/dsh-cli-pack-OgElyT/dsh-pager-grok-cli-0.1.0.tgz`），工作树不再
  留下该文件；临时 tarball 也已由 verifier 清理。
- `git diff --check`：通过。三个 doctor 临时 `DSH_HOME` 在运行后用
  `rmdir` 删除，没有打开真实 credentials/profile/session。

### 三版本 doctor 实际输出

rc.8（exit 1 只因 family runtime/capability 尚未安装且当前非 TTY）：

```text
✓ pager CLI ↔ native  @dsh-pager-grok/native-linux-x64-gnu@0.1.0
✓ DSH entry  DSH_BIN_JS /home/leo/code/deepseek-harness/apps/cli/lib/bin.js
✓ DSH package  0.1.0-rc.8 /home/leo/code/deepseek-harness/apps/cli/package.json
✓ support  maintenance/npm · maintenance: compatibility fixes and matrix coverage
✓ adapter  apiproxy-v1 runtime=@dsh-pager-grok/runtime-apiproxy-v1
✓ profile  /tmp/dsh-pager-doctor-rc8.6ixgNC/profiles/dsh-pager-grok-apiproxy-v1 family=apiproxy-v1 schema=1
✗ pager CLI ↔ runtime  missing @dsh-pager-grok/runtime-apiproxy-v1; install it or set DSH_PAGER_RUNTIME_ROOT in development
✗ capabilities  unavailable until @dsh-pager-grok/runtime-apiproxy-v1 is installed and asserts its adapter
```

rc.2 source checkout 是干净 detached tree，但未构建 `apps/cli/lib/bin.js`；首次
doctor 正确输出 `DSH_BIN_JS does not exist` 并在解析前失败。本批没有
为诊断改写外部 checkout，而是改用 Phase 4 已锁定的 exact rc.2 npm
fixture entry：

```text
✓ DSH entry  DSH_BIN_JS /home/leo/code/dsh-pager-grok/compat/fixtures/dsh-0.1.1-rc.2/node_modules/@deepseek-ai/dsh/lib/bin.js
✓ DSH package  0.1.1-rc.2 /home/leo/code/dsh-pager-grok/compat/fixtures/dsh-0.1.1-rc.2/node_modules/@deepseek-ai/dsh/package.json
✓ support  candidate/npm · candidate: pre-release evidence only; not yet supported
✓ adapter  apiproxy-v1 runtime=@dsh-pager-grok/runtime-apiproxy-v1
✓ profile  /tmp/dsh-pager-doctor-rc2-fixture.mUKzHU/profiles/dsh-pager-grok-apiproxy-v1 family=apiproxy-v1 schema=1
✗ pager CLI ↔ runtime  missing @dsh-pager-grok/runtime-apiproxy-v1; install it or set DSH_PAGER_RUNTIME_ROOT in development
✗ capabilities  unavailable until @dsh-pager-grok/runtime-apiproxy-v1 is installed and asserts its adapter
```

alpha.1（exit 1 同样是 runtime/capability 未安装且非 TTY）：

```text
✓ DSH entry  DSH_BIN_JS /home/leo/code/deepseek-harness-latest/apps/cli/lib/bin.js
✓ DSH package  0.1.2-alpha.1 /home/leo/code/deepseek-harness-latest/apps/cli/package.json
✓ support  experimental/source-only · experimental: source-only and not an npm default
✓ adapter  controllers-v2 runtime=@dsh-pager-grok/runtime-controllers-v2
✓ profile  /tmp/dsh-pager-doctor-alpha1.IUIRJN/profiles/dsh-pager-grok-controllers-v2 family=controllers-v2 schema=2
✗ pager CLI ↔ runtime  missing @dsh-pager-grok/runtime-controllers-v2; install it or set DSH_PAGER_RUNTIME_ROOT in development
✗ capabilities  unavailable until @dsh-pager-grok/runtime-controllers-v2 is installed and asserts its adapter
```

## Git 提交

- 预计 commit message：`feat(cli): resolve exact DSH versions`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-30_16-21-20_DSH多版本Phase5bCLI解析与Doctor.md`
- 暂存区审计：仅包含本记录列出的 CLI source/test/package、
  registry-copy/support/common scripts、adapter startup assertion source/tests 与本记录；
  无 registry 状态、profile/runtime package、root override/lock、CI、Rust wire/UI、
  方案正文或构建产物。

## 未解决问题和下一步

- `runtime-apiproxy-v1` 与 source-only controllers runtime metadata 尚未存在，因此
  doctor 本批保守报 runtime/capability unavailable。Phase 5d 建立可发布 rc.2
  runtime 并写入 ownership/capability metadata 后，Phase 6 cold install doctor 必须全绿。
- Phase 5c 在 family profile 名之上实现 ownership metadata、备份迁移与
  sessions/credentials 不可变测试；本批没有擅自创建或改名任何真实 profile。
