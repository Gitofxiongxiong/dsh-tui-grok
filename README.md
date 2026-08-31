<div align="center">

# dsh-tui-grok

**DeepSeek Harness 的原生终端体验。**<br>Grok Build 风格交互 × Rust TUI × DSH 原生后端。

[简体中文](README.md) · [English](README.en.md)

[![CI](https://img.shields.io/github/actions/workflow/status/Gitofxiongxiong/dsh-tui-grok/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/Gitofxiongxiong/dsh-tui-grok/actions/workflows/ci.yml) [![npm](https://img.shields.io/npm/v/%40dsh-pager-grok%2Fcli?style=flat-square&logo=npm&label=npm)](https://www.npmjs.com/package/@dsh-pager-grok/cli) ![Platforms](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-4f6ef7?style=flat-square) [![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square)](#许可证与来源)

[快速开始](#快速开始) · [功能亮点](#功能亮点) · [常用操作](#常用操作) · [参与项目](#参与项目)

</div>

![dsh-tui-grok 欢迎页：DeepSeek Harness 原生终端 UI](docs/assets/readme/welcome.png)

<p align="center"><sub>当前仓库源码的欢迎页；界面与命令可能领先于 npm 0.1.0。</sub></p>

`dsh-tui-grok` 为 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)
提供一个原生终端前端：DeepSeek Harness 继续负责会话、模型和工具执行，Rust
pager 负责紧凑的终端呈现，并复用 Grok Build 的交互与视觉语言。

> [!NOTE]
> 这是非官方社区项目，与 DeepSeek 或 xAI 无隶属关系。项目不会把 Grok 的
> agent runtime 带入 DSH；两者只在明确的 adapter 边界连接。

## 为什么做这个项目

很多终端 AI 工具只能在“简陋日志”和“全屏聊天应用”之间二选一。这个项目希望
保留终端的速度、键盘操作和原生滚动，同时把思考过程、工具调用、后台任务、
会话恢复和交互式确认放进一套一致的 TUI。

| 能力 | 体验 |
|---|---|
| **原生终端界面** | Rust + ratatui 渲染，支持键盘、鼠标、选择、复制、滚动与窗口缩放。 |
| **Agent 工作流可见** | 结构化展示思考、工具调用、队列、交互问题、后台任务和 subagent。 |
| **Grok 风格交互** | 复用其 prompt、picker、modal、状态栏、滚动和快捷键设计。 |
| **DSH 是唯一真源** | 会话、事件、权限和副作用都来自 DeepSeek Harness，不在 UI 中伪造状态。 |
| **一条命令安装** | npm CLI 自带精确版本的 DSH、pnpm runtime 和当前平台预编译程序。 |
| **会话管理** | 原生会话选择器、搜索与稳定会话身份；当前源码还提供显式 `--resume`。 |

## 快速开始

前置要求：Node.js `^22.19.0` 或 `>=24.0.0`，以及一个 DeepSeek API Key。

```bash
npm install -g @dsh-pager-grok/cli
dsh-pager --new
```

首次启动会在独立的 family profile（例如 `dsh-pager-grok-apiproxy-v1`）中准备所需 runtime，不会要求你
手工复制 Harness 仓库。公开版 `0.2.0` 启动前需要由 DeepSeek Harness 的凭据层
提供 API Key，例如设置 `DEEPSEEK_API_KEY` 或使用已有的 `$DSH_HOME/.credentials.yaml`。

> [!IMPORTANT]
> npm `0.2.0` 仍保留“无参数恢复最近会话”的旧行为；上面的 `--new` 是该已发布
> 版本的确定性新会话入口。当前源码已修复为无参数默认新建，历史会话必须显式使用
> `--resume`/`--continue` 或 TUI 内 `/resume`，将在下一个 patch release 生效。

建议安装后先做一次不显示密钥值的环境检查：

```bash
dsh-pager doctor
```

## 功能亮点

- **结构化对话**：Markdown、思考过程、工具调用、结果、diff 与时间信息各自呈现。
- **会话管理**：新建、搜索和恢复历史会话；支持 `/resume` 会话选择器。
- **模型与模式（当前源码）**：在 TUI 内使用 `/model` 切换模型，使用 `Shift+Tab` 切换 preset。
- **后台工作**：Tasks pane 汇总后台命令、monitor、subagent 与运行状态。
- **安全交互**：approval/question 由带身份的 Host 请求驱动，避免 UI 假成功。
- **终端体验**：流式滚动、快捷键、鼠标命中、选择复制、OSC52 fallback 和窗口缩放。
- **产品化启动器**：提供 `doctor`、`update`、`repair`、`uninstall`，卸载不会删除会话。

## 常用操作

### npm 0.2.0

| 命令 | 作用 |
|---|---|
| `dsh-pager` | 打开最近的顶层会话；没有历史时创建会话 |
| `dsh-pager --new` | 显式创建新会话 |
| `dsh-pager --session <session-id>` | 打开指定会话 |
| `dsh-pager --session-search <query>` | 搜索并打开历史会话 |
| `dsh-pager doctor` | 检查 Node、平台包、DSH profile 与 runtime |
| `dsh-pager update` | 将 profile runtime 重新对齐到当前 CLI 版本 |
| `dsh-pager repair` | 把损坏的 profile 改名备份后重建 |
| `dsh-pager uninstall` | 移除产品 profile runtime，保留历史会话 |

公开版 TUI 还提供 `/resume` 会话选择器、`/timestamps` 和 `Ctrl+G` Tasks pane。

### 当前源码（下一次发布候选）

| 操作 | 作用 |
|---|---|
| `/login` | 保存或替换 DeepSeek API Key |
| `/new` | 开始空白会话并选择 agent preset |
| `dsh-pager` | 默认开始新对话 |
| `dsh-pager --resume [session-id]` | 恢复最近或指定会话 |
| `/resume` | 打开历史会话选择器 |
| `/model` | 选择当前模型与 effort |
| `Shift+Tab` | 切换 agent preset |
| `Ctrl+O` | 切换 YOLO 权限模式 |
| `Ctrl+G` | 打开或关闭 Tasks pane |
| `Ctrl+X` | 打开当前上下文的快捷键提示 |

## 支持的平台

| 系统 | 架构 | 状态 |
|---|---|---|
| Linux glibc | x64、arm64 | 支持 |
| macOS | Intel x64、Apple Silicon | 支持 |
| Windows | x64 | 支持 |
| Alpine / Linux musl | — | 暂不支持 |
| Windows on ARM | arm64 | 暂不支持 |

原生程序通过 npm `optionalDependencies` 按平台安装，不在首次运行时临时下载
GitHub Release。

## 它是如何工作的

```text
DeepSeek Harness backend / profile
              │
              │ JSON-RPC + DSH authoritative state
              ▼
@dsh-pager-grok/runtime-<family> (TypeScript adapter)
              │
              │ DSH-neutral protocol
              ▼
dsh-pager (Rust host) ──► Grok-derived views ──► terminal
```

项目是独立、树外的适配器：不要求修改 DeepSeek Harness checkout，也不会复制
Harness 整仓。详细边界见 [架构文档](docs/ARCHITECTURE.md) 和
[外置插件说明](docs/EXTERNAL_DSH_PLUGIN.md)。

## 当前状态

当前公开版本是 [`0.2.0`](https://github.com/Gitofxiongxiong/dsh-tui-grok/releases/tag/v0.2.0)，
发布于 2026-08-31；七个公开 npm package 均为 `latest=0.2.0`。该版本采用 registry
驱动的多版本架构：`0.1.1-rc.2` 是默认 supported npm family，`0.1.0-rc.8` 保持
maintenance，`0.1.2-alpha.1` 是 controllers-v2 的 experimental/source-only 开发
family。精确 tag、commit、distribution 与 profile schema 见自动生成的
[支持表](docs/DSH_SUPPORT.md)；未列版本会在启动 pager 前 fail closed。

从旧的 pager-managed profile 切换 family/schema 时，CLI 会先备份完整旧 profile，
只迁移白名单内的 pager 设置。sessions 和 credentials 不会被读取或修改；旧 projection
cache 不迁移，只保留在备份目录中。

## 本地开发

<details>
<summary>展开构建、测试与真实 Harness 接线说明</summary>

```bash
corepack enable
pnpm install --frozen-lockfile

cargo check --workspace
cargo test --workspace --locked
pnpm run verify:ts
```

连接 source-only controllers-v2 开发 family 时，checkout 必须与支持表中的 alpha
tag/commit 精确一致；npm ApiProxy 版本使用各自隔离 fixture。三版本可重复命令见
[`compat/README.md`](compat/README.md)，不要用 sibling checkout 猜测版本。

```bash
DSH_HARNESS_ROOT=/path/to/deepseek-harness \
  ./scripts/start-new-chat.sh
```

只检查 backend/profile 而不创建会话：

```bash
DSH_HARNESS_ROOT=/path/to/deepseek-harness \
  ./scripts/start-new-chat.sh --check
```

更多说明：

- [验证策略](docs/TESTING.md)
- [产品启动器设计](docs/PRODUCT_PLUGIN_LAUNCHER.md)
- [源码复用与许可证策略](docs/SOURCE_POLICY.md)
- [迁移计划](docs/MIGRATION_PLAN.md)

</details>

## 参与项目

欢迎提交 [Issue](https://github.com/Gitofxiongxiong/dsh-tui-grok/issues) 报告 bug、
讨论体验或提出功能建议。问题中最好附上操作系统、终端、Node.js 版本、复现步骤，
以及已经脱敏的 `dsh-pager doctor` 输出。

如果这个项目让 DeepSeek Harness 在终端里更好用，请给它一个
[⭐ Star](https://github.com/Gitofxiongxiong/dsh-tui-grok)。这会帮助更多终端和
AI 工具爱好者发现它。

## 许可证与来源

本项目自身采用 [MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE) 双许可证。
Grok-derived 文件保留各自来源、版权与许可证，详见
[`SOURCE_MANIFEST.md`](crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md) 和
[`vendor/grok/LICENSE`](crates/dsh-pager-grok-ui/vendor/grok/LICENSE)。
