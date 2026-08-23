# 真实联调、本地 profile 安装与 smoke 边界

> 记录时间：2026-08-24 00:54:27 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

真实联调首次运行暴露两类问题：开发期将三个 packed tarball 分三次安装时，
`workspace:*` 被转成 registry semver，最后安装 `tui-embedded` 会请求尚未发布
的 `@dsh-pager-grok/tui-server` 并返回 404；另外，`dsh-pager-bin` 的
`--smoke-interactions` 只适合确定性的 mock backend，却会在真实 Harness 上把
任意模型 prompt 当成固定 approval/question 流程，存在误用和审批副作用风险。

本批目标是让本地源码 profile 能由真实联调脚本一次性 link，并把非交互 smoke
的异步等待、动态 question 答案和真实 backend 的显式 opt-in 边界固定下来。

## 设计契约和复用依据

- 对应长期计划章节：M0.4/M7.3/M9.5/M10.6；`docs/TESTING.md` 的真实 backend 入口。
- Grok source path、commit、SOURCE_REV：不涉及 Grok vendor。
- 复用等级：C（DSH host/smoke 边界与外置 profile 安装，不新增 UI）。
- `deepseek-harness` 只作为外部真实 backend，保持 checkout 干净。

## 计划修改范围

- `scripts/real-e2e.sh`：增加 `DSH_TUI_INSTALL_LOCAL=1`，一次性 link 三个源码包。
- `docs/EXTERNAL_DSH_PLUGIN.md`：修正开发期安装方式并说明 packed tarball 陷阱。
- `docs/TESTING.md`：记录本地 profile 和真实 prompt/smoke 的安全边界。
- `crates/dsh-pager-bin/src/main.rs`：非交互 interaction smoke 的异步收敛、动态问题答案、真实 backend 显式 opt-in。
- 本进度记录：记录实际失败、修复和验证。

不在范围内：`/home/leo/code/deepseek-harness` 源码、已有 Rust/UI 脏文件、Grok vendor。

## 风险、回滚和依赖

- 真实 Harness profile 安装写入 `$DSH_HOME/profiles`，不写入 DSH Git checkout；测试使用隔离 profile 名称。
- 真实模型 prompt 可能触发工具调用和审批；默认禁止把非 mock backend 交给 smoke flag，显式设置环境变量才可越过。
- 回滚方式：恢复本记录列出的四个目标文件，并删除本进度记录对应的 profile（不操作用户已有 profile）。

## 实际修改

- `scripts/real-e2e.sh`：新增 `DSH_TUI_INSTALL_LOCAL=1`，在 profile 中一次性 link protocol/server/embedded 源码目录，并检查三个包已构建。
- `docs/EXTERNAL_DSH_PLUGIN.md`、`docs/TESTING.md`：删除会触发 npm 404 的分次 tarball 开发期流程，改为源码 link；说明真实 E2E 默认不提交 prompt，非交互 smoke 对真实 backend 需要显式 opt-in。
- `crates/dsh-pager-bin/src/main.rs`：interaction smoke 等待异步 approval/question 和最终清除；根据真实 question 的 id/第一选项生成答案；识别交互种类，避免把 approval 当 question 回复；非 mock smoke 默认拒绝，只有 `DSH_ALLOW_REAL_SMOKE=1` 才允许。
- `deepseek-harness` checkout：未修改，Git 状态保持干净。

## 验证结果

- 已通过：源码目录 profile `grok-tui-test2` 的 hello/list/dashboard/load/PTY 真实联调。
- 已通过：新增 `DSH_TUI_INSTALL_LOCAL=1` 的 `grok-tui-test3`、`grok-tui-test4` 真实联调，最终输出 `real DeepSeek Harness E2E checks passed (hello/list/dashboard/load/PTY)`。
- 已通过：`pnpm run verify:ts`，protocol 16、server 37、embedded 2 个测试通过。
- 已通过：`python3 scripts/check-protocol-fixtures.py`，2 个 fixture 同步。
- 已通过：`cargo fmt --all -- --check`。
- 已通过：`cargo test -p dsh-pager-bin --locked`，8 个测试通过。
- 已通过：`cargo test --workspace --locked`，workspace 全部单测和 doctest 通过。
- 已通过：非 mock smoke guard 返回预期错误，未启动 backend。
- 已知失败：packed tarball 分次安装在 `tui-embedded` 处触发 npm 404；已由源码 link 流程替代。
- 已知失败：直接对真实 profile 运行 `--smoke-interactions` 会触发模型自身工具/审批行为，不属于确定性 smoke；本批将其改成默认拒绝并显式 opt-in。

## Git 提交

- commit message：待提交。
- Progress-Record trailer：`Progress-Record: docs/开发进度记录/2026-08-24_00-54-27_real_e2e_local_profile_and_smoke_boundary.md`
- 暂存区审计：待提交。

## 未解决问题和下一步

- 真实模型 prompt 的完整 stream/queue/interaction/reconnect 矩阵仍需单独设计确定性的 Harness fixture；不能用 mock 专用 smoke 代替。
- 真实模型 prompt 的完整 stream/queue/interaction/reconnect/detach 矩阵仍需单独设计确定性的 Harness fixture；不能用 mock 专用 smoke 代替。
- 本批只提交本记录列出的文件；已有其它 Rust/UI 改动保持不动。
