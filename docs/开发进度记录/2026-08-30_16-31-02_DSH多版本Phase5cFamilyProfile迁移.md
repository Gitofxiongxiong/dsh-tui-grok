# DSH 多版本 Phase 5c Family Profile 迁移

> 记录时间：2026-08-30 16:31:02 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

为 family profile 写入 `dshPagerGrok` ownership metadata，并在已由 pager
管理的旧 profile 与目标 family/schema 不一致时执行可恢复迁移：先把完整
旧目录改名为备份，再新建目标 family profile，只复制明确白名单内的 pager
外观设置。sessions、credentials 与 projection cache 均不迁移、不删除、
不改写。

## 设计契约和复用依据

- 对应长期计划：`DSH_MULTI_VERSION_COMPATIBILITY_PLAN.md` §12、§19
  Phase 5、§21.9，以及锁定决策 D-R7。
- profile 内部名继续使用 5b 已建立的
  `dsh-pager-grok-<adapterFamily>`；manifest ownership 包含
  `managed/adapterFamily/dshVersion/profileSchema/runtimeVersion`。
- 仅迁移 `dshPagerGrok.pagerSettings` 中明确声明的非敏感外观键；未知键、
  session/projection 数据与 credentials 一律留在备份目录或原全局位置。
- 未带 pager ownership、也没有 legacy `@dsh-pager-grok/*` bundle/dependency
  证据的 profile 不接管，fail closed。
- fresh apiproxy-v1 profile 不依赖已缺源码的 projection-recovery package；
  既有 projection cache 只随完整旧目录进入备份，不复制到新 profile，CLI
  明确提示该限制。
- Grok source/reuse：不涉及 UI，复用等级 C（产品 CLI/profile 真源）。

## 计划修改范围

- `packages/dsh-pager-cli/lib/launcher.js`、`lib/main.js`、profile migration
  单元测试。
- `scripts/dsh-tui-common.sh`、`scripts/setup-dev-profile.sh`：fresh source-only
  family profile 的 ownership 写入。
- 必要时参数化 rc.2 fixture 的临时 `DSH_HOME`，以便在同一个隔离 home 中
  验证两种 family profile 并存；不改 fixture 协议/依赖版本。
- 本进度记录。
- 不在范围：真实 `~/.dsh` 迁移、projection-recovery 包最终处置、依赖清理、
  registry 状态、CI/发布、方案正文、Rust wire 和 Grok UI。

## 风险、回滚和机器状态

- 风险：误判用户自建 profile。只有 ownership 或 legacy pager package 图
  能证明归属时才允许迁移；其他情况报错。
- 风险：迁移过程中覆盖旧数据。实现顺序固定为“原目录原子改名备份 → 新建
  family profile”，不会删除备份；测试逐字节比较 sessions/credentials。
- 风险：投影缓存被误带入新 family。迁移白名单只读取 manifest 中的 pager
  外观设置，并断言新目录无 cache；输出明确说明 projection cache 未迁移。
- 真实 E2E 只使用随后创建的 `/tmp` 隔离 `DSH_HOME`，在执行前记录其绝对
  路径和目录清单；不读取、改名或删除真实 `~/.dsh` 中任何 profile/session/
  credentials。清理只针对本批创建且路径已核对的临时目录。
- 真实并存验证已创建空沙箱
  `/tmp/dsh-pager-phase5c-coexist.ERAYKj`；登记时目录为空。后续 alpha.1 与
  rc.2 setup/E2E 只允许把 `DSH_HOME` 指向该绝对路径。完成目录对照和日志
  摘要登记前不清理。
- 回滚：revert 本批代码；迁移算法产生的 `.backup-*` 本身是可恢复原目录，
  不自动删除。

## 预期验证

- CLI unit：fresh、同 family/schema、legacy/mismatch 备份迁移、unowned 拒绝；
  自动断言 sessions/credentials 未删除或改写，projection cache 未复制。
- `pnpm run typecheck:ts`、`pnpm run test:ts`、fixture/support 门禁、
  `cargo test --workspace --locked`、PTY smoke。
- 在同一个临时 `DSH_HOME` 中创建 rc.2/apiproxy-v1 与
  alpha.1/controllers-v2 profile，分别跑真实 E2E，并对两个 manifest/plugin
  图做双向无污染断言；记录迁移前后目录对照。
- `git diff --check`。

## 实际修改

- CLI 新增 `prepareFamilyProfile`：目标 family profile 或 legacy
  `dsh-pager-grok` 只有在 ownership metadata 或 legacy
  `@dsh-pager-grok/*` package 图能证明归属时才迁移。family/schema 不符时
  先 `rename` 为 `.backup-<timestamp>`，再新建目标 manifest；碰到未归属
  profile fail closed。`repair` 同样拒绝改名未归属 profile。
- ownership 精确写入 `managed/adapterFamily/dshVersion/profileSchema/
  runtimeVersion`。明确的 pager setting 白名单为 `theme`、`defaultView`、
  `reducedMotion`，分别只接受 string/string/boolean；其他 metadata（包括
  `projectionCache`）不复制。
- CLI run/update/uninstall 在 family runtime 操作前检查/迁移 ownership；npm
  runtime 安装完成后再次写入 ownership。`doctor` 不再只展示预期 profile，
  而是读取 manifest 并核对五项 ownership；缺失/mismatch 明确失败。
- source-only 开发 profile 默认名改为
  `dsh-pager-grok-controllers-v2-dev`。setup 从 canonical registry 和真实
  alpha checkout manifest 解析 family/version/schema，复用 CLI 迁移器，
  安装后盖章；名称不包含 registry family 时拒绝。
- rc.2 fixture runner 支持调用者显式提供 `DSH_COMPAT_E2E_HOME`，不会清理
  caller-owned home，并给创建的 profile 写入同一 ownership metadata；README
  记录该并存验证接口。
- 迁移输出逐项列出 backup、实际迁移设置，并明确 projection cache 只留在
  backup、sessions/credentials 没有读取或修改。没有恢复或伪造
  projection-recovery 源码。

## 验证结果

- CLI unit：18 passed。新增 3 个 migration tests，覆盖 mismatch + 完整目录
  backup、legacy 产品 profile、aligned metadata refresh、unowned refusal；
  自动逐字节比较 `$DSH_HOME/sessions/session.json`、`.credentials.yaml` 和
  `credentials/token.json`，并断言 projection cache 只在 backup。
- `pnpm run typecheck:ts`、`pnpm run test:ts`：通过（protocol 17、server 75、
  embedded 3、source runtime 1、CLI 18）。两个 conformance suite 均 12/12。
- `python3 scripts/check-protocol-fixtures.py`：6 fixtures 一致；
  `node scripts/check-dsh-support.mjs`：3 registry versions/22 exact runtime
  declarations/3 CLI-script consumers 通过。
- `cargo test --workspace --locked`：全部 Rust unit/integration/doc tests 通过；
  `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager`：通过；
  `node scripts/verify-cli-pack.mjs`：临时 tarball
  `/tmp/dsh-cli-pack-Vmy7Ou/dsh-pager-grok-cli-0.1.0.tgz` 验证通过并由脚本清理。
- shell/Node syntax 与 `git diff --check`：通过。本批不含 Rust wire、protocol
  version/method 或 Grok UI 视觉变化，因此不触发浏览器像素门禁。

### 同机 family 并存真实验证

唯一共享沙箱为 `/tmp/dsh-pager-phase5c-coexist.ERAYKj`，登记为空后才执行：

1. `DSH_HOME=<sandbox> DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness-latest
   DSH_TUI_PROFILE=dsh-pager-grok-controllers-v2-alpha-e2e
   scripts/setup-dev-profile.sh`：创建 alpha family profile。
2. `DSH_COMPAT_E2E_HOME=<sandbox> corepack pnpm@11.7.0 --dir
   compat/fixtures/dsh-0.1.1-rc.2 run e2e`：rc.2 hello/list/dashboard/load/PTY
   全绿，新 session `session-2782f0ba-01a9-4c77-8c17-576c3baaacf9`。
3. `DSH_HOME=<sandbox> ... DSH_TUI_PROFILE=
   dsh-pager-grok-controllers-v2-alpha-e2e scripts/real-e2e.sh`：在 rc.2 完成后
   alpha hello/list/dashboard/load/PTY 仍全绿，新 session
   `session-62f26b12-e00f-45fe-adac-63fdc4352d00`。
4. manifest 断言：alpha ownership 为
   `controllers-v2/0.1.2-alpha.1/schema 2/runtime 0.1.0`，22 个直接依赖含
   session/settings/workspace controllers，不含 apiproxy/projection-cache；rc.2
   ownership 为 `apiproxy-v1/0.1.1-rc.2/schema 1/runtime 0.1.0`，profile 直接
   bundle/dependency 只有独立 rc.2 fixture，不含 controllers。双向无污染检查
   输出 `family profile manifest/plugin graph isolation: ok`。

### 迁移前后目录对照

在同一临时 home 内另建 pager-owned legacy `profiles/dsh-pager-grok`：

```text
before:
  dsh-pager-grok/package.json
  dsh-pager-grok/projection-cache/cache.json

after:
  dsh-pager-grok-apiproxy-v1/package.json
  dsh-pager-grok.backup-2026-08-30T16-38-00-0800/package.json
  dsh-pager-grok.backup-2026-08-30T16-38-00-0800/projection-cache/cache.json
```

新 manifest 只含迁移设置 `theme/defaultView/reducedMotion`，没有
`projectionCache` 字段或目录；backup cache 仍存在。迁移前后临时
`.credentials.yaml` SHA-256 均为
`83671c1f2f6894756d6d73e7c2273dcb3fe98064a70cb658b3cf967d209f9173`；
`sessions/` 目录无文件且未被迁移器操作。命令输出
`live sandbox migration invariants: ok`。上述证据登记完成后，仅将本批创建的
精确沙箱路径交给 `gio trash` 清理；不触碰真实 `~/.dsh`。
清理已执行且原路径不存在；该临时沙箱进入桌面环境 Trash，可恢复而非直接
递归删除。

## Git 提交

- 预计 commit message：`feat(cli): migrate family profiles safely`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-30_16-31-02_DSH多版本Phase5cFamilyProfile迁移.md`
- 暂存区审计要求：仅包含 CLI migration source/tests、family-aware dev scripts、
  rc.2 caller-owned E2E 参数/README 与本记录；不含真实 profile、构建产物、
  registry 状态、依赖清理、CI/发布、方案正文或 Rust/UI。

## 未解决问题和下一步

- Phase 5 验收（§19）本批满足“同机分别运行 rc.2/alpha profile、插件图不
  共享”和“迁移不删除 sessions/credentials”；CLI unsupported fail-before-
  pager 已由 5b 满足。Phase 5 仍需 5d 依赖清理后才能整体勾选。
- projection-recovery 的保留/移除决定按 D-R7 留给 5d；本批只落实“缓存不
  迁移且显式告知”。
