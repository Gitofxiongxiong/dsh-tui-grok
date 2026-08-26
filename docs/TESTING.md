# 验证策略

## 快速检查

```bash
cargo check --workspace
cargo test --workspace
```

GitHub Actions `ci.yml` runs `cargo test --workspace --locked` on ubuntu-24.04,
macos-14, and windows-2022. Protocol tests use Node stdio mocks and are
OS-portable; `scripts/pty-smoke.py` remains Unix-only and is not in that job.

Native pager tarballs are built with `node scripts/pack-native.mjs` (and the
`release-native` workflow). Pack with `npm pack`, not `pnpm pack`.

M0/M1 的可重建基线使用仓库脚本：

```bash
scripts/baseline.sh
```

它会校验 DSH wire fixture、Grok source manifest，运行 fmt/check/test/all-features
clippy，并用 checked-in mock backend 做 PTY 终端恢复 smoke。`scripts/test-fast.sh`
仍用于不编译 all-features 的快速循环。

固定来源、协议和工具版本见 [M0_BASELINE.md](M0_BASELINE.md)。

当前 `fixtures/parity/fallback-screen-80x24.json` 与
`fallback-screen-40x12.json` 是 M0 的简化 UI semantic baseline，明确标记为
fallback；它们不是 Grok reference golden，M10 reference runner 会替换它们。

## 分层测试

- Grok vendor 模块：保留上游已有单元测试，确认复制时行为不漂移。
- host adapter：用固定 `SessionState`/协议 fixture 验证 rows、状态和 effect 映射。
- runtime：继续运行协议、loader、control-plane、queue、lifecycle smoke。`cargo test -p dsh-pager` / `dsh-pager-bin` 的协议与 hello 测试用 Node stdio JSON-RPC mock，不依赖 `/bin/sh`，在 Windows 上同样编译运行；`scripts/pty-smoke.py` 仍使用 Unix `pty`/`termios`/`fcntl`，保持 Unix-only。
- binary：`--hello`、`--load-only`、`--dashboard` 和各 smoke flag 不进入终端 UI。

## 终端检查

在支持 PTY 的环境运行：

```bash
cargo run -p dsh-pager-bin -- --backend <mock-server>
python3 scripts/pty-smoke.py --binary target/debug/dsh-pager
```

检查窗口 resize、折行、滚动、picker/modal 打开关闭以及退出后的终端恢复。UI 的快照/黄金测试只断言稳定文本和几何关系，不把 ANSI 控制序列硬编码到 runtime 测试。

## M8-M10 完整端到端门禁

scripts/e2e.sh 是当前完整入口：它校验协议/source manifest，验证
ReferenceRunner 的六档尺寸、十二种状态和九类输入矩阵，运行 workspace
测试，构建 binary，并通过 checked-in mock Harness 执行 PTY 主路径（含 resize、
mouse/picker、queue、Esc ladder 和 terminal restore）；prompt、selection/copy、
approval/question 和 queue authority 由同一 mock Harness 的 binary integration
tests 覆盖。真实 DeepSeek
Harness 可通过 DSH_TUI_SERVER 注入同一 binary；没有凭据或服务时脚本仍保留
mock 证据，并将真实后端状态记录为 unavailable，而不会伪造成功。

## 真实 DeepSeek Harness

本机源码 checkout 可用下面的入口执行真实主路径。加载检查默认会创建一个
隔离的空 session（不提交 prompt、不修改 queue、不 rename/fork 已有 session）；
指定 `REAL_E2E_SESSION` 后则改为只读 attach 到该已有 session。PTY 生命周期检查
始终使用另一个隔离空 session，避免长历史页面影响终端退出断言：

```bash
DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness \
  DSH_TUI_PROFILE=dsh-pager-grok-e2e scripts/real-e2e.sh
```

`real-e2e.sh` 在未设置 `DSH_TUI_INSTALL_LOCAL=1` 时只检查 profile，并在 profile
缺失或不包含本项目 bundle 时提前失败；不会把 Node 的 profile 堆栈错误留给用户。
开发期没有发布 npm 包时，使用 `DSH_TUI_INSTALL_LOCAL=1`，脚本会自动构建并一次性
link 本仓库的 protocol、server、embedded 和 session-projection-recovery 源码包。不要把
packed tarball 分次安装，因为 packed manifest 中的 `workspace:*` 会变成 registry semver，
最后一个包会触发
npm 404：

```bash
DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness \
  DSH_TUI_PROFILE=dsh-pager-grok-e2e \
  DSH_TUI_INSTALL_LOCAL=1 \
  scripts/real-e2e.sh
```

首次准备 profile 也可以单独运行：

```bash
DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness \
  DSH_TUI_PROFILE=dsh-pager-grok-dev \
  scripts/setup-dev-profile.sh
```

启动脚本和真实 E2E 都优先使用仓库旁的 `../deepseek-harness`，其次使用已知本机路径；
生产或 CI 不应依赖这些机器路径，应显式设置 `DSH_HARNESS_ROOT`。`DSH_HOME` 默认是
`$HOME/.dsh`，profile 属于本机运行环境，不提交到仓库。CI/干净环境测试应使用临时
`DSH_HOME`，先运行 `setup-dev-profile.sh`，再运行 `--check` 或 `real-e2e.sh`。

`scripts/real-e2e.sh` 默认只验证 hello/list/dashboard/load/PTY，不提交 prompt。
`--smoke-interactions`、`--smoke-queue` 和 `--smoke-lifecycle` 是 checked-in mock
的确定性 smoke；pager 默认拒绝把它们发送给真实 Harness。只有明确确认隔离
session 和副作用后，才设置 `DSH_ALLOW_REAL_SMOKE=1` 手动运行。

```bash
REAL_E2E_SESSION=session-71569f6b-4d1f-4f4f-a13b-7f1613897a1b \
  DSH_HARNESS_ROOT=/home/leo/code/deepseek-harness scripts/real-e2e.sh
```

默认 backend 由 pager 旗标注入（不设置 `DSH_TUI_SERVER`）：

```text
--backend <node>
--backend-arg <DSH_HARNESS_ROOT>/apps/cli/lib/bin.js
--backend-arg --profile
--backend-arg dsh-pager-grok-dev
```

配置从 Harness 自己的 `$DSH_HOME` credentials/settings 层读取；密钥不写入本仓库。
如果刚修改过 Harness 的 TypeScript 源码，先在该 checkout 重建 host bundle：

```bash
(cd /home/leo/code/deepseek-harness && pnpm exec tsdown --env.DSH_BUILD_FACE host)
```

`DSH_TUI_SERVER` 可覆盖完整 backend 命令字符串（空白拆分，路径不得含空格）；
设置后启动脚本不再注入默认 `--backend` 链。需要验证真实模型 prompt、queue
mutation 或 lifecycle 时，应在确认会话和费用边界后显式运行对应 binary smoke
flag，而不是让只读门禁隐式改变已有 session。
