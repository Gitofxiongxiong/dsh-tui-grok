# Rust 后端 Windows 安全 spawn 与 stderr 隔离

> 记录时间：2026-08-26 22:20:59 +0800
> 操作者/agent：Grok Build subagent
> 状态：completed

## 目标与背景

完成本地平台兼容 **step 1**：冻结 Ubuntu / macOS / WSL / native Windows（以及后续 v2 CLI）共用的 Rust pager backend argv 契约；拒绝 Windows 不安全的 `.cmd`/`.bat` spawn；加上 T4/T5 嵌套守卫；把 backend stderr 从 inherit 改为 pipe + 有界 drain，避免日志打穿 alternate screen。

本批只改 Rust pager spawn 合同，不实现 Node CLI、npm 原生包、`start-new-chat.sh`、clipboard、CI 矩阵或协议变更。

## 设计契约和复用依据

- 对应长期计划章节：`docs/PRODUCT_PLUGIN_LAUNCHER.md` v2 spawn/TTY 规则 T1–T9、KD 14/17；运行时必须修正的 backend stderr inherit。
- Grok source path、commit、SOURCE_REV：
  - 上游 `/home/leo/code/grok-build` mirror `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，SOURCE_REV `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`
  - `crates/codegen/xai-grok-mcp/src/servers.rs` `plan_stdio_spawn`（约 4200）及 `servers_tests.rs`：Windows `CreateProcessW` 只补 `.exe`、忽略 `PATHEXT`，std 随后把 `.cmd`/`.bat` 交给 `cmd.exe`
  - `crates/codegen/xai-tty-utils` `restore_native_stderr`；pager 在 `xai-grok-pager/src/app/mod.rs`、`signal_handler.rs` 退出时恢复 stderr
- 复用等级 A0/A1/B/C/D：
  - **不**复制 `plan_stdio_spawn` 去解析/执行 `dsh.cmd`（那是 npm shim 配方）。把它当作 **拒绝 `.cmd`/`.bat` 的理由**（CVE-2024-24576）。DSH 产品合同是 PE `node`/`node.exe` + 绝对 `lib/bin.js`。复用等级 **C**（DSH spawn 策略自建）+ 对 Grok 注释的引用。
  - **不** vendor 整个 `xai-tty-utils`。SOURCE_POLICY：runtime shell 不是复制目标。backend child stderr 是 DSH transport 问题：pipe + draining reader。Grok「不要让日志打穿 alternate screen / TUI 后再恢复 stderr」的想法以最短等价实现落在 `dsh-pager` transport，短注释指向 Grok 源。复用等级 **C**（想法）/**D**（不复制 runtime crate）。
  - 未发现更接近的 Grok helper 可直接用于 pager backend spawn。

## 计划修改范围

- 文件：
  - `docs/开发进度记录/2026-08-26_22-20-59_Rust后端Windows安全spawn与stderr隔离.md`
  - `crates/dsh-pager-bin/src/main.rs`：默认 `dsh --profile dsh-pager-grok`；抽出可测 `parse_args`/`default_backend`；T4 ROLE 守卫；help 文本
  - `crates/dsh-pager-bin/tests/hello.rs`：嵌套 ROLE / ALLOW_NESTED 集成测试；失败 backend stderr 尾部；现有 mock 仍传 `--backend node`
  - `crates/dsh-pager/src/transport.rs`：T5/`.cmd`/`.bat` 拒绝；stderr pipe + drain + 有界 tail；Drop 在失败时打印
  - `crates/dsh-pager/src/lib.rs`：按需导出 spawn 校验
  - `crates/dsh-pager/tests/spawn_contract.rs`：basename 拒绝、stderr flood drain（Node mock，非 `/bin/sh`）
  - `docs/PRODUCT_PLUGIN_LAUNCHER.md`：同步当前 Rust 默认 profile 与 T3 stderr pipe 事实（不改 Hello `identity.profile`）
- 预期行为：
  1. 无 `--backend` 且无 `DSH_TUI_SERVER` → `program == "dsh"`，args `["--profile", "dsh-pager-grok"]`。产品/Windows 不依赖 PATH `dsh`。
  2. `DSH_TUI_SERVER` 保持 `split_whitespace()`；help 写明路径不得含空白。
  3. basename 为 `dsh-pager` / `dsh-pager.exe` / `dsh-pager.cmd` / `dsh-pager.js` 则拒绝，除非 `DSH_PAGER_ALLOW_NESTED=1`。
  4. basename 以 `.cmd`/`.bat` 结尾一律拒绝（ALLOW_NESTED 不放开）；不 `shell: true`，不包 `cmd.exe`，不 PATHEXT 搜 `.cmd`。
  5. `DSH_PAGER_ROLE=pager` 且 ALLOW_NESTED 不是 `1`：stderr `refusing nested dsh-pager`，exit 1；然后无论谁启动都 `set_var("DSH_PAGER_ROLE", "pager")`。
  6. backend stderr = pipe + reader thread 排空；有界 ring；失败后在 TUI 释放（或 `--hello`/`--load-only` 进程退出）打印 tail。不向 alternate screen 实时倾倒。stdout 读侧继续 strip `\r`。
  7. 不改 `identity.profile = "tui-embedded"` 与 hello fixtures。
- 不在范围内：
  - `scripts/start-new-chat.sh` / profile setup（step 2）
  - clipboard / `clip.exe` / WSL clipboard（step 3）
  - 整份 `#![cfg(unix)]` transport_contract 改写、GitHub Actions 矩阵、npm native packages、`@dsh-pager-grok/cli`
  - JSON-RPC methods、Grok UI、vendor keybindings、npm publish、musl、PowerShell launcher
  - 未跟踪文件 `docs/开发进度记录/2026-08-25_21-53-51_计划审批问答提交被拒.md`

## 风险、回滚和依赖

- 真实 DSH backend 若大量写 stderr，失败尾部可能截断（有界 ring）；成功路径不打印。
- Unix `cargo run` 默认仍找 PATH `dsh`；Windows 上无 `--backend` 会因拒绝 `.cmd` 或找不到 `dsh.exe` 失败——这是合同，不是回归。
- `std::env::set_var` 在 rustc 1.98 为 unsafe；入口尚单线程。
- 不 drain stderr 会使 Node 在管道填满后死锁；flood 测试覆盖。
- 回滚：还原本记录列出的文件。

## 实际修改

- 文件：
  - `docs/开发进度记录/2026-08-26_22-20-59_Rust后端Windows安全spawn与stderr隔离.md`
  - `crates/dsh-pager-bin/src/main.rs`
  - `crates/dsh-pager-bin/tests/hello.rs`
  - `crates/dsh-pager/src/lib.rs`
  - `crates/dsh-pager/src/transport.rs`
  - `crates/dsh-pager/tests/spawn_contract.rs`（新）
  - `docs/PRODUCT_PLUGIN_LAUNCHER.md`
- 摘要：
  - Unix cargo-run 默认 backend 从 `dsh --profile tui-embedded` 改为 `dsh --profile dsh-pager-grok`。产品路径仍须显式 `--backend <node> --backend-arg <absolute bin.js> --backend-arg --profile --backend-arg <profile>`。`DSH_TUI_SERVER` 仍 `split_whitespace()`；help 写明路径不得含空白。
  - 抽出 `default_backend` / `parse_args_from` / `resolve_backend`，单测覆盖无 `--backend`、`--backend-arg --profile`。
  - T4：入口在 parse/spawn 前若 `DSH_PAGER_ROLE=pager` 且 `DSH_PAGER_ALLOW_NESTED` 不是 `1`，stderr `refusing nested dsh-pager` 并 exit 1；随后 `set_var("DSH_PAGER_ROLE", "pager")`。
  - T5 + Windows 脚本拒绝：`validate_backend_program` 在 `Command::new` 前拒绝 nested basename；`.cmd`/`.bat` 一律拒绝（ALLOW_NESTED 不放开）。无 `shell: true`、无 PATHEXT `.cmd` 探测。Grok `plan_stdio_spawn` 只作为拒绝理由引用，未复制。
  - stderr 从 inherit 改为 pipe + drain thread + 32KiB ring。失败时 Drop 打印 tail（`run_interactive` 已 restore 终端；`--hello` 等非 TUI 路径在进程退出时打印）。未 vendor `xai-tty-utils`。
  - Hello `identity.profile = "tui-embedded"` 未改。
  - 设计文档 T3 / 当前实现位置与 Rust 事实同步。

## 验证结果

- 命令：
  - `cargo fmt -p dsh-pager -p dsh-pager-bin`
  - `cargo test -p dsh-pager-bin --locked`
  - `cargo test -p dsh-pager --locked`
  - `cargo check --workspace --locked`
- 结果：
  - `dsh-pager-bin`：5 个 argv 单测 + 14 个 `hello.rs`（含原 mock `--backend node`、T4 ROLE、T5/`.cmd`/`.bat`、失败 stderr tail）通过。
  - `dsh-pager`：既有测试保持绿色；新增 transport 校验单测与 `spawn_contract`（basename 拒绝 + 128KiB stderr flood 不死锁）通过。`transport_contract` unix 套件未改，仍通过。
  - `cargo check --workspace --locked` 通过。
  - 本批无用户可见 TUI 布局/主题变化，未跑浏览器像素对比。

## Git 提交

- commit message：`fix(tui): spawn backend as node+js and isolate stderr`
- Progress-Record trailer：`docs/开发进度记录/2026-08-26_22-20-59_Rust后端Windows安全spawn与stderr隔离.md`
- 暂存区审计：仅上述列出文件；不含 `docs/开发进度记录/2026-08-25_21-53-51_计划审批问答提交被拒.md`。

## 未解决问题和下一步

- Step 2：`scripts/start-new-chat.sh` / profile setup。
- Step 3：clipboard / `clip.exe` / WSL clipboard。
- Unix `#![cfg(unix)]` transport_contract 未改写为 Node mock（本步仅新增 Node flood 测试）。
- 产品仍不依赖 PATH `dsh`；Windows `cargo run -p dsh-pager-bin` 无 `--backend` 会找不到 `dsh.exe` 或拒绝 `.cmd`，需显式 node+bin.js。
- 未做 GitHub Actions 矩阵、npm native packages、`@dsh-pager-grok/cli`、musl、PowerShell launcher。
- 未复制 Grok `plan_stdio_spawn` / `xai-tty-utils`；若后续需要 pager 自身 fd2 redirect，另开记录评估最小 seam。
