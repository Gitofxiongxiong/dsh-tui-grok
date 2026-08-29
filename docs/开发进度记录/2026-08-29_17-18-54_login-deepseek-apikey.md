# `/login` DeepSeek API Key 交互

> 记录时间：2026-08-29 17:18:54 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

当前 TUI 没有设置或轮换 DeepSeek API Key 的用户交互，但 Host ApiProxy
已经提供 `credentials.describe` 和 `credentials.set`。本批次实现本地
`/login` 命令：当前只有 DeepSeek 一个 provider，命令直接打开遮罩的
API Key 输入弹窗，通过 Harness 凭据真源保存 `DEEPSEEK_API_KEY`。

成功只表示 Host 已保存，不额外发送模型请求验证密钥。不做启动时
自动弹窗，不增加 `/apikey` 或 `/logout`。

## 设计契约和复用依据

- 对应长期计划章节：2.2 焦点/Esc、2.3 DSH 真源、2.4 Intent→Effect→Receipt、M5 Prompt、M7 Status、M10 浏览器对照。
- Grok source path：
  - `/home/leo/aidreamschool/grok-build/crates/codegen/xai-grok-pager/src/slash/commands/login.rs`
  - `/home/leo/aidreamschool/grok-build/crates/codegen/xai-grok-pager/src/views/modal_window.rs`
  - `/home/leo/aidreamschool/grok-build/crates/codegen/xai-grok-pager/src/app/dispatch/auth.rs`
- Grok mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`
- SOURCE_REV：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`
- 复用等级：`/login` 命令名称和 action 为 A1；弹窗 chrome、focus/Esc 和
  input 状态机为 B；DSH credential DTO/effect 为 C；Grok OAuth/auth runtime 为 D，
  明确不复制。
- DSH-neutral seam：UI 只产生 describe/set credential intent，runtime adapter
  通过已有 ApiProxy 传输执行，view 不持有 `RpcTransport`。
- 新增 DTO/Intent/Effect：无值的 `CredentialInfo`、describe 参数/结果、
  `DescribeCredential` 和 `SetCredential`。
- 稳定 identity/generation：credential 是 Host-global 操作；operation 仍用请求 id
  去重，completion 不应因 session 切换被当成 session-scoped stale 结果。
- 密钥仅在输入状态和 `credentials.set` 出站请求中存在；不进入 prompt、
  history、Debug、dedupe key、status、snapshot 或日志，安全零化可控缓冲。

## 计划修改范围

- 文件：
  - `docs/开发进度记录/2026-08-29_17-18-54_login-deepseek-apikey.md`
  - `docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md`
  - `crates/dsh-pager-protocol/src/lib.rs`
  - `crates/dsh-pager/src/loader.rs`
  - `crates/dsh-pager/src/lib.rs`
  - `crates/dsh-pager-grok-ui/src/effects.rs`
  - `crates/dsh-pager-grok-ui/src/slash.rs`
  - `crates/dsh-pager-grok-ui/src/app.rs`
  - `crates/dsh-pager-grok-ui/src/runtime.rs`
  - `crates/dsh-pager-grok-ui/src/views/mod.rs`
  - `crates/dsh-pager-grok-ui/src/views/login.rs`
  - `crates/dsh-pager-bin/tests/mock-server.mjs`
  - `scripts/pty-smoke.py`
- 预期行为：`/login` 显示 DeepSeek 凭据状态；可写时接收遮罩输入并
  保存；进程环境提供的只读凭据显示可操作说明；Enter 防重复，Esc
  清理密钥并关闭。
- 不在范围内：OAuth/device-code、provider picker、密钥回显、隐式模型
  验证、启动自动弹窗、`/logout`、Harness 凭据存储实现变更。

## 风险、回滚和依赖

- 风险：密钥可能通过 derive `Debug`/异步 completion/测试失败输出泄露；必须
  使用脱敏容器，并在请求入队后清除 pending effect 的值。
- 风险：环境变量优先级高于可写凭据层；必须先 describe，对
  `writable=false` 拒绝伪成功。
- 风险：`credentials.set` 不校验 API Key 有效性；成功文案只说已保存且
  下次请求使用。
- 回滚：回退上述记录中的命令、view、effect、DTO 和 mock/PTY 改动；
  Host 线协议没有 breaking change。
- 依赖：本机 DeepSeek Harness ApiProxy 的 `credentials.describe/set` 合同；
  不增加新 Rust crate。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo test -p dsh-pager-protocol -p dsh-pager -p dsh-pager-grok-ui`
- `cargo test --workspace --locked`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `python3 scripts/check-protocol-fixtures.py`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`
- `ttyd + xterm.js + Playwright`：1200×800、DPR 1、DejaVu Sans Mono 16px、
  `TERM=xterm-256color`，对 DSH `/login` 密钥弹窗与 Grok `/login` 对应
  状态进行截图和差异分类，测试值必须是明确的假密钥。

## 实际修改

- 文件：与“计划修改范围”一致，共 13 个文件；没有修改计划外文件。
- 协议/Host adapter：增加 value-free `CredentialInfo`、
  `credentials.describe`/`credentials.set` DTO 和 loader；read 路径不存在密钥值，
  set 路径使用 one-way payload。
- Intent/effect：增加 Host-global credential describe/set effect；operation identity
  保留 request id，但 completion 不受当前 session/generation 切换影响。异步请求
  发出后立即清空 pending effect 中的 secret。
- UI：增加 `/login` 本地命令和 `LoginProvider`/`LoginMethod` provider-neutral
  目录；当前直接选择 DeepSeek。弹窗复用 Grok `modal_window` chrome 和 modal
  focus/Esc 路由，支持键盘、鼠标、paste、编辑和遮罩显示。
- 状态语义：先 describe 再开放输入；环境变量提供的只读凭据拒绝覆盖并给出
  删除 `DEEPSEEK_API_KEY` 后重启的说明；set accepted 后关闭弹窗，文案只确认
  Host 已保存且下次请求使用，不伪造密钥有效性验证。
- 安全：输入使用固定容量可清零缓冲；secret-bearing 类型手写脱敏 `Debug`，
  不实现 serde；paste action、dedupe key、pending effect、status 和 mock 日志均不
  包含密钥。立即 transport 错误留在弹窗内显示，不终止 TUI。
- 测试/文档：mock Harness 增加 credential seam；PTY 主路径在 `/resume` 前用
  固定假密钥验证打开、遮罩、保存和无输出泄露；长期适配计划补记 M5/H10 证据。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo check --workspace`：通过。
- `cargo test --workspace --locked`：通过（包含 protocol、effect、view、shell、
  runtime Host-global completion 和 doctest）。
- `cargo build -p dsh-pager-bin`：通过。
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`：通过；
  `/login` 只显示圆点，固定假密钥未出现在 screen/raw PTY output，保存后弹窗关闭。
- `python3 scripts/check-protocol-fixtures.py`：通过，2 个 fixture 同步。
- `git diff --check`：通过。
- 严格 Clippy：
  - `dsh-pager-protocol` 的 `--all-targets --all-features --no-deps -D warnings`
    通过；
  - `dsh-pager` 和 `dsh-pager-grok-ui` 在只 allow 已有基线 lint 后的同等严格
    检查通过；
  - 全工作区原命令仍被本批未修改的已有 lint 挡住：
    `dsh-pager-test-support/src/node_mock.rs` 的 `io_other_error`，以及 vendor
    markdown 的 `question_mark`/`unnecessary_sort_by`。本批不扩大范围修复。
- ttyd + xterm.js + Playwright：通过，固定 1200×800、DPR 1、
  DejaVu Sans Mono 16px、`TERM=xterm-256color`；证据位于
  `/tmp/dsh-login-playwright/`：
  - `dsh-login-empty.png`、`dsh-login-masked.png`、`dsh-login-saved.png`
    分别证明打开、遮罩和 accepted 后关闭；固定假密钥不存在于 terminal buffer，
    两端 browser console/page error 均为空；
  - `grok-login.png` 证明本机 Grok 1.0.13 的 `/login` 是全屏 device-code +
    browser approval。DSH 使用 API Key modal 是认证产品差异，不应复制 Grok OAuth
    runtime 或强求逐像素相同；复用边界仍是命令名、焦点/Esc 和 Grok chrome。
- 全部验证只使用 mock 假密钥；没有输入真实 API Key，也没有提交模型请求。

## Git 提交

- commit message：`feat(tui): add DeepSeek API key login`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-29_17-18-54_login-deepseek-apikey.md`
- 暂存区审计：仅暂存“计划修改范围”的 13 个文件；工作区中用户已有的
  `deepseek-harness-版本差异分析.html`、对应 16:13 进度记录，以及并行出现的
  17:33 `Git-PR与发布规范` 进度记录均未暂存、未修改。

## 未解决问题和下一步

- 后续新增厂商时，在 provider 目录增加条目并在 `/login` 首屏引入 provider
  picker；本批只有 DeepSeek，因此不增加多余选择层。
- OAuth/device-code、`/logout`、启动自动登录和真实模型请求校验仍不在本批范围。
- 全工作区 Clippy 的既有 lint 基线可另开记录清理。
