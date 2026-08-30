# DSH 版本支持清单

[`dsh-support.json`](dsh-support.json) 是本项目 DSH 精确版本支持状态的单一真源。
版本、tag、commit、adapter family、runtime package、profile schema、支持状态和
分发方式不得再从脚本路径、workspace override 或 package manifest 反向猜测。

## 字段语义

- `schemaVersion`：清单格式版本；消费者遇到未知版本必须 fail closed。
- `family`：DSH 架构族，目前为 `apiproxy-v1` 或 `controllers-v2`。
- `tag` / `commit`：经过验证的上游精确 tag 与提交。
- `packageManager`：该 DSH checkout 根 manifest 要求的精确包管理器。
- `runtimePackage`：该 family 对应的 runtime 发布单元。
- `profileSchema`：family profile 的组合 schema。
- `distribution`：`npm` 表示可从 registry 组合；`source-only` 表示仅从精确源码
  checkout 构建，不能进入正式 npm publish。新增其他分发阶段需同步更新 schema
  检查和发布门禁。

`status` 遵循总控方案 §18：

- `supported`：当前完整 CI、真实 E2E 与发布门禁覆盖的 npm DSH。
- `maintenance`：上一个仍保留 adapter 与基本 E2E 的 npm DSH，只接受高优先级修复。
- `experimental`：精确的下一代源码版本，不作为默认 npm 产物。
- `unsupported`：不再进入主线 CI；用户停留在最后一个已知可用 pager release。
- `candidate`：迁移期内部状态，只有完整门禁通过后才能改为 `supported`，不得向
  最终用户展示为已支持。

## 消费者

下列环节必须读取或验证此清单：

- CLI backend resolver；
- `doctor`；
- 本地 profile setup；
- adapter startup assertion；
- GitHub Actions version matrix；
- runtime pack verification；
- npm dependency availability gate；
- release notes/support table generation。

package manifest 中无法动态生效的 exact dependencies 仍保留，但必须由
[`scripts/check-dsh-support.mjs`](../scripts/check-dsh-support.mjs) 检查，避免形成
不受约束的第二真源。

## 隔离 fixture 与三版本矩阵

两个 npm ApiProxy 版本各有独立目录、lockfile、`node_modules`、profile 与临时
`DSH_HOME`，不能互换 lockfile：

```bash
corepack pnpm@11.7.0 --pm-on-fail=ignore \
  --dir compat/fixtures/dsh-0.1.0-rc.8 install --frozen-lockfile
corepack pnpm@11.7.0 --pm-on-fail=ignore \
  --dir compat/fixtures/dsh-0.1.0-rc.8 run build
corepack pnpm@11.7.0 --pm-on-fail=ignore \
  --dir compat/fixtures/dsh-0.1.0-rc.8 run e2e

corepack pnpm@11.7.0 --pm-on-fail=ignore \
  --dir compat/fixtures/dsh-0.1.1-rc.2 install --frozen-lockfile
corepack pnpm@11.7.0 --pm-on-fail=ignore \
  --dir compat/fixtures/dsh-0.1.1-rc.2 run e2e
```

CI 的三个 exact-version job 与本地使用同一个入口。每条命令要求对应环境变量指向
registry 中 tag/commit 完全一致的 checkout，并自行创建和清理 `/tmp` 沙箱：

```bash
DSH_CHECKOUT_0_1_0_RC_8_ROOT=/path/to/dsh-rc8 \
  bash scripts/run-dsh-compat-matrix.sh 0.1.0-rc.8
DSH_CHECKOUT_0_1_1_RC_2_ROOT=/path/to/dsh-rc2 \
  bash scripts/run-dsh-compat-matrix.sh 0.1.1-rc.2
DSH_CHECKOUT_0_1_2_ALPHA_1_ROOT=/path/to/dsh-alpha1 \
  bash scripts/run-dsh-compat-matrix.sh 0.1.2-alpha.1
```

发布候选的 read-only registry availability gate 为：

```bash
node scripts/check-registry-dependencies.mjs
```

它对 candidate manifest 的 `dependencies` 与非 optional peers 逐项执行等价于
`npm view <package>@<exact-version> version` 的查询；range、本地 spec、冲突版本和
registry 缺失都会在上传任何包前失败。CLI 的 `doctor --release` 复用同一实现。

## 本地检查

无 checkout 参数时，检查器仍校验 JSON 形状和当前 runtime manifest，并逐个打印
上游校验的 skip 原因：

```bash
node scripts/check-dsh-support.mjs
```

要核对本地上游 checkout，为每个版本注入按版本生成的环境变量：将版本转为大写，
所有非字母数字字符替换为下划线，并加上 `DSH_CHECKOUT_` 前缀和 `_ROOT` 后缀。
当前三个变量为：

```bash
DSH_CHECKOUT_0_1_0_RC_8_ROOT=/path/to/dsh-rc8 \
DSH_CHECKOUT_0_1_1_RC_2_ROOT=/path/to/dsh-rc2 \
DSH_CHECKOUT_0_1_2_ALPHA_1_ROOT=/path/to/dsh-alpha1 \
  node scripts/check-dsh-support.mjs
```

检查器会比较 checkout HEAD、tag 指向、根 `packageManager` 和
`apps/cli/package.json` 的精确版本。任一差异都会集中列出并以非零状态退出。

## 修改流程

1. 先按 `AGENTS.md` 创建独立进度记录，写明支持状态变化的证据和回滚方式。
2. 只添加经过精确 tag/commit 固定的版本；跨架构变化必须新建 adapter family，
   不能在 core 中堆版本分支。
3. 同步 family runtime manifest 中无法动态表达的 exact dependencies。
4. 使用所有相关 checkout 运行检查器，再运行 adapter conformance、真实 DSH E2E、
   PTY 和对应 distribution 的安装/发布门禁。
5. 只有方案规定的门禁全部通过且经维护者评审，才能提升 `status` 或
   `distribution`；提交中同时更新 release/support 文档消费者。
