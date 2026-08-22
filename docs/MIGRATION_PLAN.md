# 迁移计划

长期目标、设计契约、Grok 文件级复用地图和后续所有细化工作以
[GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md](GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md)
为总控；本文件只保留当前切片摘要。

迁移按可运行的垂直切片推进，每一步都保持后端协议和 smoke 可验证。

## 已完成

1. 从原 `dsh-pager` 创建独立 successor，原目录不再被新 UI 改写。
2. 将 Grok view/render/input 模块复制到独立 vendor 边界，并用最小 shim 通过 `cargo test` 验证。
3. 清除 successor 中旧的设计文档，建立本架构的入口文档。

## 当前切片

1. 新建 `dsh-pager-grok-ui` crate，固定 Grok 源码和 host adapter。
2. 让 binary 的默认交互入口只调用新 crate。
3. 用真实 `SessionState` 生成顶部状态、transcript、picker/modal 和 timeline 数据；输入动作先转换为 `UiEffect`。

## 后续切片

1. 把 prompt editor、approval/question、queue editor、session dashboard 接入同一 effect reducer。
2. 用 PTY golden/snapshot 覆盖 Grok 的尺寸、折行、滚动和鼠标命中行为。
3. 当新 UI 覆盖旧 app 的必要场景后，从 runtime crate 删除剩余旧 UI 文件和无用依赖。
4. 把 adapter 的 host trait 做成稳定边界，再评估接入 Codex CLI 等其他 harness。

## 完成条件

- `cargo test --workspace` 全绿；
- 默认 binary 不再链接旧 `run_interactive`；
- Grok 组件的源文件保持可追溯，适配改动集中且可审计；
- 真实后端至少能加载会话、显示 transcript、打开 picker/modal，并能退出终端恢复现场。
