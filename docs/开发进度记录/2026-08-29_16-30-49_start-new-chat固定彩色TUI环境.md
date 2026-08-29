# start-new-chat 固定彩色 TUI 环境

> 记录时间：2026-08-29 16:30:49 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

真实 `ttyd + xterm.js + Playwright` 诊断确认：父 shell 的非空 `NO_COLOR` 会被
`start-new-chat.sh` 原样传给 crossterm，导致 reasoning rail 的运行态渐变仍在内部推进，
但最终终端只收到统一前景色，视觉上退化为静态线。用户确认通过该项目专用开发脚本
启动，并要求把彩色 TUI 环境固定在脚本中。

## 设计与范围

- 只修改 `scripts/start-new-chat.sh` 的两个交互式 `--new` exec 分支，在启动 pager
  子进程时使用 `env -u NO_COLOR`。
- 不修改 `--check` 握手，因为它不进入 TUI；不修改 Rust Turn Status、rail、滚动或
  crossterm 全局语义。
- 不伪造 `TERM`/`COLORTERM`，继续使用真实终端能力；其他启动方式继续遵守调用者的
  `NO_COLOR`。
- 对应长期计划：M9.1/M10.3-M10.6 的真实终端可见性与浏览器门禁；无 Grok/vendor
  源码变化，无 source manifest 变化。

## 风险与回滚

- 该脚本是项目专用彩色 Grok TUI launcher，因此清除 `NO_COLOR` 是有意的启动契约；
  用户若需要无颜色输出，仍可直接运行 pager 或使用其他 launcher。
- `env -u` 只作用于最终 pager 子进程，不污染父 shell，也不影响 setup/build。
- 回滚方式：恢复两个 `exec "$pager" --new ...` 调用。

## 计划验证

- `bash -n scripts/start-new-chat.sh`；
- `scripts/start-new-chat.sh --help`；
- `scripts/start-new-chat.sh --check --skip-setup` 后端握手；
- `cargo fmt --all -- --check`、`git diff --check`；
- 静态审计两个 `--new` 分支均使用 `env -u NO_COLOR`，且 `TERM/COLORTERM` 未改。

## 实际修改与验证

- `scripts/start-new-chat.sh`：两个交互式 `--new` 分支改为
  `exec env -u NO_COLOR "$pager" ...`，并写明仅清除父 shell 注入的无颜色偏好；
  `TERM`、`COLORTERM`、build/setup 和 `--check` 均未改变。
- 静态与握手门禁：
  - `bash -n scripts/start-new-chat.sh`：通过；
  - `scripts/start-new-chat.sh --help`：通过；
  - `scripts/start-new-chat.sh --check --skip-setup`：通过，真实 backend 返回
    `tui.hello ok`；
  - `cargo fmt --all -- --check`：通过；
  - `git diff --check`：通过；
  - 本机无 `shellcheck`，已记录为工具不可用，不冒充通过。
- `ttyd + xterm.js + Playwright` 浏览器终端门禁：
  - 外层 ttyd 明确设置 `NO_COLOR=1`，再由 ttyd 启动
    `./scripts/start-new-chat.sh --skip-setup`；
  - 真实 pager 诊断三次均记录 `no_color=false term_program=vscode`，证明最终子进程
    没有继承 `NO_COLOR`；
  - 1200×800、DPR1、DejaVu Sans Mono 16px 的 welcome TUI 正常彩色渲染，浏览器
    console/page errors 为空；证据目录
    `/tmp/dsh-launcher-color-20260829-Friv6P/`；
  - 未发送模型 prompt，不产生 API 调用费用。

## Git 提交

- commit message：`fix(dev): preserve colored Grok launcher surface`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-29_16-30-49_start-new-chat固定彩色TUI环境.md`
