# DSH 多版本 Phase 5d 依赖与 Runtime 清理

> 记录时间：2026-08-30 16:39:21 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

完成 Phase 5 的高风险依赖收口：建立可发布且只含 rc.2 registry 依赖的
`@dsh-pager-grok/runtime-apiproxy-v1`，让 CLI 公开依赖图脱离 alpha/source-only
runtime；缩小根 override，清除 protocol deprecated alias，并明确退役无源码的
projection-recovery 构建残留。

## 设计契约和审计结论

- 对应长期计划 §13.2、§14.1、§19 Phase 5 与锁定决策 D-R7/D-R10。
- runtime 拆分采用 D-R10 原方案，而非“单 runtime 条件解析”：rc.2 package 只
  组装 protocol/core/apiproxy-v1 adapter 与独立 plugin entry；现有
  `@dsh-pager-grok/runtime` 标记 private，继续作为 controllers-v2/alpha 的
  source-only 开发 runtime，不发布、不立即删除。
- `controllers-v2/history.ts` 的 `@deepseek-ai/dsh-session/chunk-rows` 是
  adapter-family runtime 依赖，改为从安装该 runtime 的模块图解析并做 export
  断言。server `index.ts` 的 Cordis import 只用于 Context 类型、编译后消失；
  Schemastery 是 Cordis plugin `Config` 的运行时 schema，属于组装入口必需依赖，
  两者不移入 adapter。
- `packages/dsh-tui-session-projection-recovery` 经 `git ls-files` 为零，目录只有
  `.gitignore` 覆盖的 `lib/` 与 `node_modules/`，没有 src/package manifest、无法
  重建、也没有 workspace/package/script 消费者。决定退役并用 `gio trash`
  移走整个精确目录，不伪造源码；fresh apiproxy runtime 使用 DSH 官方
  `dsh-session-projection-cache`，不恢复旧 pager recovery 插件。
- deprecated alias 代码消费者为零；剩余命中只在 protocol 双语 README、冻结
  baseline 与旧架构报告。更新现行 protocol 文档/fixture 术语并删除导出，不改
  method 字符串、Rust wire 或协议版本。历史 Progress-Record 不重写。
- Grok source/reuse：无 UI 变化，复用等级 C（产品 runtime/package 真源）。

## 计划修改范围

- 新增 `packages/dsh-pager-runtime-apiproxy-v1/**` 与 workspace/root scripts。
- `scripts/assemble-runtime.mjs`、`verify-runtime-pack.mjs`、
  `verify-cli-pack.mjs`、`check-dsh-support.mjs`。
- CLI/source runtime/package manifests、`pnpm-workspace.yaml`、lockfile。
- server controllers runtime resolver/history/backend/tests；protocol alias exports、
  双语 README/compat baseline/架构报告中的现行术语。
- 退役本机 ignored-only
  `/home/leo/code/dsh-pager-grok/packages/dsh-tui-session-projection-recovery`。
- 本进度记录。
- 不在范围：registry status、rc.8 新 fixture/CI、release/cold install、方案正文、
  npm/push/dist-tag/Release、Rust wire/UI。

## 风险、回滚和机器状态

- 风险：apiproxy tarball 间接 import controllers/alpha。assembler 白名单复制
  `core` + `adapters/apiproxy-v1`，pack verifier 扫 tarball manifest、文件和文本。
- 风险：缩 override 后 alpha source build 失去依赖。只移除已发布且无需 checkout
  identity 的 Cordis/DSH CLI override；每个保留 alpha link 逐项注释 patch/source
  runtime 用途，并以 frozen install + 全门禁验证。
- 风险：删除 alias 破坏未检出的消费方。先把非历史全仓命中清零，再 build/test/
  fixture/pack/real E2E；任何回归立即 revert 本批 commit。
- 机器状态变更：仅将上述 ignored-only projection-recovery 精确目录移入桌面
  Trash（可恢复）；执行前本记录已经写明绝对路径、`git ls-files=0` 与内容事实。
  不触碰任何 `$DSH_HOME` profile/session/credentials。
- alpha 回归已创建空沙箱 `/tmp/dsh-pager-phase5d-alpha.OKGTCp`，登记时为空；
  setup/E2E 只允许使用 profile
  `dsh-pager-grok-controllers-v2-phase5d-e2e`。验证证据登记后用 `gio trash`
  清理该精确临时路径。

## 预期验证

- frozen workspace install；typecheck/test:ts；两个 conformance 12/12；protocol
  fixtures、support/pack gates。
- 新 runtime tarball 无 `link:`/`workspace:`/alpha、无 controllers-v2 文件/文本，
  exact rc.2 package 图与 registry 一致；CLI tarball同样无 alpha/本地 spec。
- `rg` deprecated alias 与 server 静态 DSH runtime import 清零（仅组装层保留项
  有记录）。
- `cargo test --workspace --locked`、PTY smoke、alpha + rc.2 两条真实 E2E。
- `git diff --check`。

## 实际修改

- 新增 publishable `@dsh-pager-grok/runtime-apiproxy-v1@0.1.0`。manifest 有
  16 个 non-optional dependencies：Cordis `4.0.1` 与所有 DSH 包均 exact
  `0.1.1-rc.2`，metadata 标明 `apiproxy-v1/schema 1` 及 14 项 capability。
  patch 由真实 rc.2 fixture seed 派生，使用官方
  `dsh-session-projection-cache`，server row 指向 family runtime subpath。
- runtime assembler 支持 `controllers-v2` 与 `apiproxy-v1` 两种显式 target。
  publish target 只复制 protocol、server core 与 `adapters/apiproxy-v1`，并生成
  profile-local ApiProxy/Cordis entry；不复制 controllers adapter。现有
  `@dsh-pager-grok/runtime` 增加 `private: true` 和
  `controllers-v2/schema 2/source-only` metadata，继续作为 alpha 开发产物。
- CLI 公开 dependencies 从 alpha DSH + workspace runtime 改为 registry
  `@deepseek-ai/dsh@0.1.1-rc.2` + pnpm；family runtime 仍按 support registry
  延迟解析，不把任一 family runtime 放入 CLI dependency graph。
- runtime/CLI pack verifier 现在拒绝 `link:`、`workspace:`、alpha version；
  runtime verifier 还拒绝 controllers adapter/recovery 文件与静态 alpha import，
  并检查所有 `@deepseek-ai/dsh*` 均 exact rc.2。support checker 同时核对 private
  source runtime 与 publish runtime 的 family/distribution，并核对 CLI 默认 DSH
  是 registry 中的 npm 版本。
- root override 从 21 项减为 20 项：永久移除 `@deepseek-ai/dsh` CLI override；
  保留 Cordis + 19 个 controllers-v2 alpha source graph 项，每项在 YAML 中写明
  build/patch/adapter 用途。尝试完全取消 root override 时，先后在 frozen graph
  重建中遇到未发布 `dsh-invariants`、`dsh-commands`、
  `dsh-cordis-host-runner`；即使把 source link 限定到 private 包且关闭 peer
  auto-install 仍由 linked alpha 传递图请求 registry。连续两次同类尝试无进展
  后按硬约束停止并恢复最小可构建集合。publishable tarball/cold install 的隔离
  由独立门禁承担，不以 root node_modules 冒充发布依赖图。
- controllers-v2 chunk-row decoder 从静态
  `@deepseek-ai/dsh-session/chunk-rows` import 改为构造 backend 时从 family runtime
  模块图解析，并对 `decodeStorageRecord` export fail closed；新增 2 个定向测试。
  server 剩余 DSH 静态 import 只有 invariant companion 的 type-only
  `InvariantInstaller`，编译后消失。`index.ts` 的 Cordis type-only import 与
  Schemastery `Config` runtime import 是 Cordis 组装入口所需，明确保留。
- 删除 protocol deprecated alias `API_PROXY_METHOD_SET`/`ApiProxyMethod`/
  `isApiProxyMethod`，现行双语 README/baseline/架构术语统一为
  `TUI_UNARY_METHOD_SET`/`TuiUnaryMethod`/`isTuiUnaryMethod`；method 字符串、fixture、
  Rust wire 与 `TUI_PROTOCOL_VERSION` 未变。
- `packages/dsh-tui-session-projection-recovery` 的 ignored-only `lib/node_modules`
  目录已整体移入 Trash。没有受版本控制文件被删除；不恢复缺失源码。旧构建
  产物可从 Trash 恢复，但不再作为 workspace/runtime/profile 组成。

## 验证结果

- `pnpm install --frozen-lockfile`：通过，12 workspace projects；root lock 不再
  override DSH CLI 本体。
- `pnpm run build:ts`、`typecheck:ts`、`test:ts`：通过。protocol 17、server
  77（新增 controllers runtime 2）、embedded 3、source runtime 1、apiproxy
  runtime 1、CLI 18；两个 conformance suite 均 12/12。
- `python3 scripts/check-protocol-fixtures.py`：6 fixtures 同步；deprecated alias
  的非方案/非历史代码和现行文档 `rg` 命中为零；server runtime DSH 静态 import
  命中为零，仅 invariant type-only import 保留。
- 三 checkout 环境变量齐全运行 `node scripts/check-dsh-support.mjs`：rc.8/rc.2/
  alpha exact commit/tag/package manager 全部核对通过；source runtime 22 个
  `@deepseek-ai/*` 声明解析为 alpha/source-only，publish runtime 18 个（含 dev
  tooling）解析为 rc.2/npm，3 个 CLI/script registry consumer 通过。
- `node scripts/verify-runtime-pack.mjs`：
  `dsh-pager-grok-runtime-apiproxy-v1-0.1.0.tgz` 通过，tarball 无 local/alpha
  dependency、无 controllers adapter/recovery/alpha import，临时目录已清理。
- `node scripts/verify-cli-pack.mjs`：临时 tarball
  `/tmp/dsh-cli-pack-TXxtvI/dsh-pager-grok-cli-0.1.0.tgz` 通过，默认 DSH exact
  rc.2、无 family runtime/alpha/local public dependency，脚本已清理。
- alpha 真实 E2E：`DSH_HOME=/tmp/dsh-pager-phase5d-alpha.OKGTCp ...
  DSH_TUI_INSTALL_LOCAL=1 scripts/real-e2e.sh` 通过 hello/list/dashboard/load/PTY，
  新 session `session-bb8c278b-36fc-4acf-827d-2bcc20b5d618`；证据登记后该临时
  home 已用 `gio trash` 移入 Trash，并确认原路径不存在（可恢复）。
- rc.2 真实 E2E：`corepack pnpm@11.7.0 --dir
  compat/fixtures/dsh-0.1.1-rc.2 run e2e` 通过 hello/list/dashboard/load/PTY，
  新 session `session-5ed39fbc-7acd-489e-a5f7-de79e922410e`；fixture 临时 home
  `/tmp/dsh-pager-phase4b-rc2.dyULNf` 已由 runner 清理。
- `cargo test --workspace --locked`、独立 mock `pty-smoke`、`git diff --check`：
  全部通过。本批无 Rust/UI 变化，不触发浏览器像素门禁。

### §19 Phase 5 验收

- [x] family-specific patches/runtimes/profiles：rc.2 publish runtime + patch、
  alpha private/source runtime、5c family ownership/profile 均已落地。
- [x] support registry-driven CLI selection：5b 精确 resolver，5d public graph
  收口到 rc.2 registry 包。
- [x] `doctor` backend/adapter/capability 证据：5b 五类 status + 三 checkout，
  5c ownership 实值，runtime metadata/capabilities 已供 Phase 6 cold install。
- [x] 可恢复 profile migration：5c backup-first + pager-setting whitelist，自动
  字节断言 sessions/credentials 不变，projection cache 不迁移。
- [x] 同机 rc.2/alpha profile：5c 同一临时 `DSH_HOME` 两条真实 E2E 与插件图
  双向无污染；5d 清依赖后再次各自真实回归。
- [x] unsupported version 在 pager 前失败：5b unit/实际 doctor 证据。

Phase 5 的 §19 产出与验收全部满足；rc.2 仍按 D-R11 保持 candidate，只有
Phase 6 三版本矩阵、registry gate 与 cold install 全绿后才提升 supported。

## Git 提交

- 预计 commit message：`refactor(runtime): isolate publishable adapter dependencies`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-30_16-39-21_DSH多版本Phase5d依赖与Runtime清理.md`
- 暂存区审计要求：仅包含本记录列出的 package/runtime/server/protocol/build/
  pack/support/workspace/lock/docs 变更；不含 registry status、CI/release、方案
  正文、ignored build output、真实 profile、Rust/UI。

## 未解决问题和下一步

- root workspace 仍是 controllers-v2 source build graph，因此同名 rc.2 依赖在
  根 node_modules 中会受 alpha override 影响；这不是公开 tarball 图。Phase 6
  必须在无 workspace/override/sibling checkout 的独立目录 cold install 本地
  tarballs + registry rc.2，才可宣称发布候选可安装。
- Phase 6a 固化 rc.8 独立 fixture/lock、registry dependency gate 与三版本 CI；
  6b 完成全部候选 pack/cold dogfood 后再提升 rc.2。
