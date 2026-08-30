# Git、Pull Request 与发布规范

> 状态：仓库规范
> 适用范围：日常功能、缺陷修复、文档、CI 和 npm/GitHub Release
> 关联规范：[AGENTS.md](../AGENTS.md)、[开发进度记录](开发进度记录/README.md)、[PRODUCT_PLUGIN_LAUNCHER.md](PRODUCT_PLUGIN_LAUNCHER.md)

本仓库使用以下边界：

~~~text
短期分支 -> push -> PR -> CI -> 合并 main
                                  └-> 不发布 npm

明确的发布决策 -> 版本 PR -> 合并 main -> v* Tag -> 发布工作流
~~~

**PR 合并不等于发版，main 上可以长期存在尚未发布的变更。**

## 1. 核心规则

1. <code>main</code> 是受保护的集成分支，普通变更必须通过 PR 进入。
2. 功能、修复和文档变更从短期分支 push；不直接 <code>git push origin main</code>。
3. 每个 commit 必须遵守“一份主进度记录对应一个 commit”，并带
   <code>Progress-Record</code> trailer。
4. PR 默认使用 **rebase merge**，保留每个 commit 与进度记录的一对一关系。
5. 普通 PR、合并 <code>main</code> 和推送普通分支都不是 npm 发布授权。
6. 只有明确决定发布某个版本时，才可以创建并 push <code>v*</code> Tag。
7. npm 正式包只从 GitHub Actions Trusted Publishing/OIDC 发布；日常开发不从本机
   <code>npm publish</code>，不使用长期 <code>NPM_TOKEN</code>。
8. Tag 与已发布的 npm version 视为不可变；不 force push Tag，不重发同版本。

## 2. 分支与 commit 规范

### 2.1 分支命名

| 类型 | 命名 | 示例 |
|---|---|---|
| 功能 | <code>feat/&lt;topic&gt;</code> | <code>feat/model-effort-picker</code> |
| 修复 | <code>fix/&lt;topic&gt;</code> | <code>fix/login-focus</code> |
| 文档 | <code>docs/&lt;topic&gt;</code> | <code>docs/pr-release-workflow</code> |
| 构建/CI | <code>build/&lt;topic&gt;</code> 或 <code>ci/&lt;topic&gt;</code> | <code>ci/tag-release-gate</code> |
| 发布准备 | <code>release/vX.Y.Z</code> | <code>release/v0.2.0</code> |

分支应该短期存在、只承载一个可评审主题。不使用长期 <code>develop</code> 分支；
<code>main</code> 就是日常集成线，Tag 才是发布边界。

### 2.2 从最新 main 开始新任务

~~~bash
git fetch origin --prune
git switch main
git pull --ff-only origin main
git switch -c fix/<topic>
~~~

如果是功能或文档，将 <code>fix/&lt;topic&gt;</code> 替换为对应前缀。

### 2.3 当本地 main 已经有开发内容

不要为了“恢复 main”盲目 reset。先把当前状态原样放到新分支：

~~~bash
git status --short
git log --oneline origin/main..main
git switch -c fix/<topic>
~~~

<code>git switch -c</code> 会保留当前未提交改动和本地 commit。确认新分支安全后，
再 push 该分支并创建 PR。不要先把这些 commit push 到 <code>origin/main</code>。

### 2.4 进度记录与 commit

修改目标文件前，必须先创建本批次的时间戳进度记录。提交时只暂存记录中声明的文件，
不使用 <code>git add .</code>：

~~~bash
git diff --check
git status --short
git add <progress-record> <file-1> <file-2>
git diff --cached --name-status
git diff --cached --check
git commit -m "fix: describe the change" \
  -m "Progress-Record: docs/开发进度记录/<record>.md"
~~~

一个 PR 可以有多个 commit，但每个 commit 必须有自己的主进度记录，且每份记录只对应
一个 commit。

## 3. Push 与 Pull Request

### 3.1 推送开发分支

~~~bash
git push -u origin fix/<topic>
~~~

后续更新同一分支时直接：

~~~bash
git push
~~~

除正式发布步骤外，不使用 <code>git push --tags</code>。普通开发不执行
<code>git push origin main</code>。

### 3.2 用 gh 创建 PR

每台机器首次使用前检查登录：

~~~bash
gh auth status
gh auth login
~~~

分支已 push 后创建 PR：

~~~bash
gh pr create \
  --base main \
  --head fix/<topic> \
  --fill
~~~

PR 描述至少包含：

- 修改目标和用户可见影响；
- 主进度记录路径；
- 实际运行的验证命令和结果；
- 风险、回滚方式和未完成项；
- 发布影响：<code>none</code>、<code>prerelease</code> 或预计的 semver 类型。

常用检查命令：

~~~bash
gh pr status
gh pr view <number>
gh pr diff <number>
gh pr checks <number> --watch
~~~

### 3.3 在 PR 中同步最新 main

个人短期分支默认使用 rebase：

~~~bash
git fetch origin
git rebase origin/main
git push --force-with-lease
~~~

只允许 <code>--force-with-lease</code>，不使用 <code>--force</code>。如果分支由多人共享，
重写前先协调；不要静默覆盖他人的远程 commit。

### 3.4 只用 git 查看 PR 代码

Git 可以获取 GitHub 暴露的 PR head ref：

~~~bash
git fetch origin pull/<number>/head:review/pr-<number>
git log --oneline main..review/pr-<number>
git diff main...review/pr-<number>
~~~

但 PR 的标题、评论、审批、检查和合并状态属于 GitHub API，不是 Git 对象。纯
<code>git</code> 不能完整替代 <code>gh pr view</code> 或 <code>gh pr merge</code>；
没有 <code>gh</code> 时使用 GitHub 网页。

## 4. PR 合并

### 4.1 合并前门禁

只有同时满足下列条件才合并：

- PR 范围单一，没有误带其他工作区变更；
- 每个 commit 的 <code>Progress-Record</code> trailer 与文件列表一致；
- GitHub Actions 要求的 CI 全部通过；
- 评审意见已解决，不存在未说明的高风险失败；
- PR 未创建任何发布 Tag，除非它本身是已批准的发布 PR。

### 4.2 默认：rebase merge

~~~bash
gh pr checks <number> --watch
gh pr merge <number> --rebase --delete-branch
~~~

如果开启了 GitHub auto-merge，可以：

~~~bash
gh pr merge <number> --rebase --delete-branch --auto
~~~

rebase merge 不产生额外的 merge commit，并保留 PR 内每个 commit 的进度记录
trailer，因此是本仓库的默认方式。

### 4.3 squash 的限制

只有 PR 最终对应 **一份** 主进度记录时，才可以：

~~~bash
gh pr merge <number> --squash --delete-branch
~~~

如果 PR 中有多个分别对应进度记录的 commit，禁止直接 squash。确有必要时，应在
合并前将范围和记录合法收敛为一份，而不是让 GitHub 丢弃对应关系。

### 4.4 合并后同步本地

~~~bash
git switch main
git pull --ff-only origin main
git fetch origin --prune
~~~

<code>main</code> 合并到此完成。**不创建 Tag，不运行 npm publish。**

## 5. GitHub main 保护建议

在 GitHub Settings 为 <code>main</code> 配置 branch ruleset：

- Require a pull request before merging；
- Require status checks to pass before merging；
- 要求当前 <code>ci.yml</code> 的三平台 <code>test</code>、
  <code>verify-ts + contracts (ubuntu)</code>；adapter/support/compat 路径变化时还要求
  三个精确版本的 <code>DSH ...</code> job；
- Require conversation resolution before merging；
- Block force pushes 和 branch deletion；
- 个人仓库可先不要求第二人 approval，但仍保留 PR + CI 门禁；
- 多人协作时再要求至少 1 个 approval，并开启 dismiss stale approvals。

管理员也应遵守该规则；不应将“可以 bypass”当作日常开发路径。

## 6. 当前 GitHub Actions 真实行为

以当前 <code>.github/workflows/</code> 为准：

| 操作 | <code>ci.yml</code> | <code>release.yml</code> | <code>publish.yml</code> | npm 外部副作用 |
|---|---|---|---|---|
| 创建/更新 PR | 运行 | 不运行 | 不运行 | 无 |
| push <code>main</code> | 运行 | 不运行 | 不运行 | 无 |
| push 普通开发分支 | 默认不运行 | 不运行 | 不运行 | 无 |
| push <code>ci/trust-smoke</code> | 默认不运行 | 不运行 | 不运行 | 无 |
| push <code>v*</code> Tag | 不运行 | 构建并上传五平台 native 与 runtime/CLI 候选 artifact | 不运行 | 无 npm publish |
| 每周 scheduled | core + 三版本完整矩阵 | 不运行 | 不运行 | 仅 registry/read-only npm view |
| 从 <code>v*</code> Tag 手动运行 <code>release</code> 并输入精确确认值 | 不变 | OIDC 发布 native → runtime，registry cold/PTY 后发布 CLI | 不运行 | 发布到 <code>release-candidate</code> |
| 手动运行 <code>publish-smoke</code> 并输入确认值 | 不变 | 不运行 | 运行 trust smoke | 发布 <code>trust-test</code> 预发布包 |

必须特别注意：

- <code>release.yml</code> 是两阶段正式链：Tag push 只打包 artifact；只有从同一 Tag
  手动 dispatch 且输入 <code>publish-vX.Y.Z</code>，才使用 <code>npm-release</code>
  environment 的 Trusted Publishing/OIDC 发布。
- 正式链先把包放在隔离的 <code>release-candidate</code> dist-tag，完成精确 version、
  provenance、registry cold/warm/offline 与 PTY 验收后才结束。OIDC 不承担最终
  <code>latest</code> 移动；该动作仍由已认证维护者在全绿后显式执行。
- <code>publish.yml</code> 只保留手动 <code>workflow_dispatch</code>；必须输入
  <code>publish-trust-test</code> 才会继续。它会发布
  <code>@dsh-pager-grok/tui-protocol@0.1.1-trust.&lt;run_id&gt;.&lt;run_attempt&gt;</code>
  并使用 <code>trust-test</code> dist-tag。
- <code>main</code>、开发分支和 <code>v*</code> Tag 的 push 都不会触发
  <code>publish.yml</code>。该手动烟测不是 native → runtime → CLI 的正式发布链；
  正式发布只使用 <code>release.yml</code>。
- <code>ci.yml</code> 的 compatibility path filter 只减少普通 PR/main 的真实 DSH
  重复构建；每周 scheduled 固定运行 rc.8、rc.2、alpha.1 全矩阵。每个矩阵 job
  调用同一等价本地入口 <code>scripts/run-dsh-compat-matrix.sh &lt;exact-version&gt;</code>，
  不执行 publish、dist-tag、Release 或 workflow dispatch。

手动烟测可从 GitHub Actions 页面运行，或使用：

~~~bash
gh workflow run publish.yml -f confirm=publish-trust-test
gh run list --workflow publish.yml --limit 5
~~~

## 7. 正式发布规范

### 7.1 什么时候发布

正式发布必须是一个单独、明确的决策。下列操作不自动触发发布：

- 修复一个问题；
- 合并一个或多个 PR；
- push <code>main</code>；
- CI 全绿；
- 更新 changelog 但未创建版本 Tag。

一次发布需要明确：版本号、stable/prerelease、npm dist-tag、待发布包、发布 commit
以及发布人/批准人。

### 7.2 版本号

- <code>PATCH</code>：向后兼容的缺陷修复，例如 <code>0.1.0</code> → <code>0.1.1</code>。
- <code>MINOR</code>：向后兼容的新功能，例如 <code>0.1.1</code> → <code>0.2.0</code>。
- <code>MAJOR</code>：不兼容的公开接口、安装或协议变更。
- 预发布：<code>v0.2.0-alpha.1</code>、<code>v0.2.0-beta.1</code> 或
  <code>v0.2.0-rc.1</code>，npm 使用 <code>next</code> 等非
  <code>latest</code> dist-tag。

同一产品版本的 Cargo workspace、根 <code>package.json</code> 以及计划发布的
<code>packages/**/package.json</code> 必须保持版本契约一致。

### 7.3 发布 PR

从最新 <code>main</code> 创建专用发布分支：

~~~bash
git fetch origin --prune
git switch main
git pull --ff-only origin main
git switch -c release/vX.Y.Z
~~~

在该分支中：

1. 先创建本次版本进度记录；
2. 同步所有发布单元的版本号和锁文件；
3. 更新 release notes/changelog 和已知问题；
4. 运行发布门禁和 pack 审计；
5. commit、push，创建标题为 <code>release: vX.Y.Z</code> 的 PR；
6. CI 全绿并明确批准后，按 rebase merge 进入 <code>main</code>。

最小验证集合：

~~~bash
cargo fmt --all -- --check
cargo test --workspace --locked
pnpm install --frozen-lockfile
pnpm run verify:ts
node scripts/verify-native-matrix.mjs
node scripts/verify-runtime-pack.mjs
node scripts/verify-cli-pack.mjs
~~~

实际发布还需按 [TESTING.md](TESTING.md) 和
[RELEASE_CANDIDATE_CHECKLIST.md](RELEASE_CANDIDATE_CHECKLIST.md) 运行当前版本适用的
clean-prefix、cold/warm/offline、PTY、真实 DSH 和平台门禁。

### 7.4 创建和 push Tag

发布 PR 已合并且发布 commit 已确认后，才执行：

~~~bash
git switch main
git pull --ff-only origin main
git status --short
git tag --list vX.Y.Z
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git show --no-patch --decorate vX.Y.Z
git push origin refs/tags/vX.Y.Z
~~~

要求：

- <code>git status --short</code> 必须为空；
- Tag 必须指向已合并且 CI 通过的 <code>main</code> commit；
- Tag 名必须与 package/Cargo 版本一致；
- 只 push 本次单个 Tag，不使用 <code>git push --tags</code>；
- Tag push 只触发 <code>release.yml</code> 构建 artifact，不触发 npm publish；必须等
  artifact run 全绿，再从该 Tag 手动 dispatch 并输入精确确认值。

### 7.5 发布顺序

正式 npm 发布链必须实现且遵守下列顺序：

1. 从同一 Tag/commit 构建、strip 并测试各平台原生程序；
2. 发布 <code>@dsh-pager-grok/native-*</code> 平台包；
3. 发布 <code>@dsh-pager-grok/runtime-apiproxy-v1</code>；
4. 在干净 npm prefix 中完成 cold/warm/offline 安装与运行验收；
5. 最后发布 <code>@dsh-pager-grok/cli</code>；
6. 验证 npm dist-tag、provenance 和实际安装；
7. 创建 GitHub Release 作为发布记录和人工镜像。

正式 dispatch 示例（必须通过 <code>--ref</code> 锁定已 push 的 Tag）：

~~~bash
gh workflow run release.yml --ref vX.Y.Z -f confirm=publish-vX.Y.Z
gh run list --workflow release.yml --limit 10
~~~

新 npm package 在首次创建前无法配置 Trusted Publishing。只允许该新包从最终 Tag
候选 tarball 进行一次已认证 bootstrap publish，并使用
<code>release-candidate</code>；随后立即把该 package 的 trusted publisher 绑定到
<code>release.yml</code>、准确仓库与 <code>npm-release</code> environment。正式 workflow
会把 registry tarball 的 SHA-512 integrity 与同 Tag 重建候选逐字节核对，不一致时
停止 CLI 发布。已有 package 和后续版本不得走此例外。

创建 GitHub Release 的命令示例：

~~~bash
gh release create vX.Y.Z --verify-tag --generate-notes
~~~

<code>release.yml</code> 实现版本/Tag 一致性、精确人工确认、OIDC、顺序依赖、
registry cold/warm/offline/PTY 和 provenance 门禁；任一步失败时，后续 job 不运行。

### 7.6 发布后检查

~~~bash
gh run list --workflow release.yml --limit 10
gh run list --workflow publish.yml --limit 10
gh release view vX.Y.Z
npm view @dsh-pager-grok/cli version dist-tags --json
npm view @dsh-pager-grok/runtime-apiproxy-v1 version dist-tags --json
~~~

发布记录必须回写：Tag、commit、Actions run、每个包的实际 version/dist-tag、
安装验证、失败/重试和已知问题。

## 8. 失败与回滚

- Tag 尚未 push：修正发布 PR/commit，必要时删除本地错误 Tag 后重建。
- Tag 已 push 但 npm 尚未发布：暂停 workflow，记录状态；默认不重定位该 Tag。
- npm 某个 version 已公开：不覆盖、不复用同版本号；通过新的 patch 版本修复。
- 有问题的包可在审批后使用 <code>npm deprecate</code> 或调整 dist-tag；不在普通
  修复流程自动 unpublish。
- 发布失败不回滚 <code>main</code> 上已审核的功能历史；单独修复发布链或发布新版本。

## 9. Agent 和维护者的权限边界

- “修复、测试、commit 并 push”允许 agent 推送开发分支和创建/更新 PR。
- “合并 PR”允许在合并门禁通过后执行 <code>gh pr merge</code>。
- 上述两种指令都**不包含**创建/push <code>v*</code> Tag、手动运行
  <code>publish</code> workflow、<code>npm publish</code>、更改 npm dist-tag
  或创建 GitHub Release 的授权。
- 只有“发布 <code>vX.Y.Z</code>”或同等明确指令才授权发布操作；发布前仍需报告版本、
  Tag commit、包列表、dist-tag 和 workflow 实际副作用。

## 10. 日常修复的最短可执行路径

~~~bash
# 1. 建分支
git fetch origin --prune
git switch main
git pull --ff-only origin main
git switch -c fix/<topic>

# 2. 先建进度记录，再修改和测试

# 3. 精确暂存并 commit
git add <progress-record> <changed-files>
git diff --cached --name-status
git commit -m "fix: describe the change" \
  -m "Progress-Record: docs/开发进度记录/<record>.md"

# 4. push 并创建 PR
git push -u origin fix/<topic>
gh pr create --base main --head fix/<topic> --fill

# 5. 检查通过后合并
gh pr checks <number> --watch
gh pr merge <number> --rebase --delete-branch

# 6. 同步 main；到此结束，不发包
git switch main
git pull --ff-only origin main
~~~
