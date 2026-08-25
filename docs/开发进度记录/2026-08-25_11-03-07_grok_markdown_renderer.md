# 接入 Grok Markdown renderer 并修复周期性空行

> 记录时间：2026-08-25 11:03:07 +0800
> 操作者/agent：Codex `/root`
> 状态：completed

## 目标与背景

当前生产 transcript 在每四个 Markdown 可见行后主动插入一个空白终端行，
导致同一软换行内容（例如 `dsh-pager-grok-` / `ui`）被空行割裂；本地
`pulldown-cmark` fallback 还忽略 table row/cell 结构，使表格内容粘连。

本批次删除周期性 spacer，直接复用固定 Grok Build snapshot 的
`xai-grok-markdown` 与 `xai-grok-markdown-core` 源码，把 Markdown 的块分隔、
表格、列表、代码、链接和软换行语义交回 Grok renderer。DeepSeek Harness
继续只提供 `DshRenderBlock::Markdown` 文本和稳定 entry identity。

## 设计契约和复用依据

- 对应长期计划章节：2.1 视觉契约、2.3 富内容契约、4.2 Markdown blocks、
  M2.4、M4.1/M4.5、P02/P13。
- Grok source path：
  - `xai-grok-markdown/**`
  - `xai-grok-markdown-core/**`
  - `crates/codegen/xai-grok-pager/src/scrollback/blocks/markdown_content.rs`
  - `crates/codegen/xai-grok-pager-render/src/theme/md_style.rs`
- Grok mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`；
  SOURCE_REV：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- 复用等级：A0（两个 Markdown crate 的 Rust/asset 源码）+ A1（Cargo workspace
  接线）+ B（Grok `md_style` 到本地既有 `Theme` 的路径适配和
  `DshRenderBlock::Markdown` seam）。
- DSH-neutral seam：输入仍为 `DshRenderBlock::Markdown { text }`，输出仍为
  `SemanticLine` / `RichPaintLine`；view 不读取 SessionState 或调用 RPC。
- 新增 DTO/Intent/Effect：无。
- 稳定 identity/generation：保持现有 entry/block identity；renderer 输出的
  semantic blank row 参与同一 height/hit-map，周期性伪 spacer 不再存在。
- 替换的旧模块：`transcript.rs::render_markdown` 本地 AST fallback 和
  `MARKDOWN_SPACING_EVERY` 逻辑。

## 计划修改范围

- `Cargo.toml`、`Cargo.lock`
- `NOTICE`
- `crates/dsh-pager-grok-ui/Cargo.toml`
- `crates/dsh-pager-grok-ui/NOTICE`
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`
- `crates/dsh-pager-grok-ui/vendor/grok/xai-grok-markdown/**`
- `crates/dsh-pager-grok-ui/vendor/grok/xai-grok-markdown-core/**`
- `crates/dsh-pager-grok-ui/src/render/mod.rs`
- `crates/dsh-pager-grok-ui/src/render/markdown.rs`
- `crates/dsh-pager-grok-ui/src/views/transcript.rs`
- 本进度记录

预期行为：连续 Markdown 折行之间不再出现固定四行周期的空白；Markdown
源中的真实块分隔保留；GFM table 使用 Grok box/table renderer，不再把单元格
直接拼接；宽度变化只改变正常 wrap/table reflow，不改变语义空白位置。

不在范围内：替换整个 Grok scrollback/AppView；Markdown raw-mode UI、Mermaid
点击 overlay、syntax theme cache、OSC8 link-map 的完整产品接线；DSH host 协议。

## 风险、回滚和依赖

- 风险：完整 renderer 增加 syntect/anstyle/table/URL 等编译依赖和 lockfile
  变化；限制为 Grok Markdown 的独立纯渲染闭包，不引入 Grok runtime crate。
- 风险：Grok renderer 自己输出语义 blank row，必须确保本地 wrap/height/copy
  对空行只计算一次。
- 风险：table renderer 需要真实可用宽度；由现有 block width 减去 entry chrome
  和 indent 后传入，不建立第二套 layout。
- 回滚：移除两个 vendor workspace member/依赖和 `render::markdown` adapter，
  恢复本批次前 `render_markdown`；不得恢复周期性 spacer 作为长期方案。
- 许可证：两个上游 crate 均为 Apache-2.0；复用现有
  `vendor/grok/LICENSE`，补充 root/crate NOTICE 与逐文件 SHA-256 manifest。

## 预期验证

- `cargo fmt --all -- --check`
- `cargo check --workspace --locked`
- `cargo test -p xai-grok-markdown --locked`
- `cargo test -p xai-grok-markdown-core --locked`
- `cargo test -p dsh-pager-grok-ui --locked`
- `cargo test --workspace --locked`
- `cargo clippy -p dsh-pager-grok-ui --all-targets --locked -- -D warnings`
- `python3 scripts/check-source-manifest.py --upstream /home/leo/aidreamschool/grok-build`
- `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`
- 固定 ttyd/xterm.js/Playwright 环境下，对 DSH 与本机 Grok 的长 Markdown、
  table 和窄宽度状态截图或像素对照；产物放在工作区外。
- `git diff --check`

## 实际修改

- 从固定 Grok mirror commit
  `19d42e35c07a9c9244f03f6df0c4c353f970d4f9` 原样 vendored
  `xai-grok-markdown` / `xai-grok-markdown-core` 的生产 Rust 源码与 Tokyo Night
  syntax asset；只对 Cargo workspace 继承、未复制的 playground/benchmark target
  做 A1 接线适配。两个 crate 继续使用 Apache-2.0，root/crate NOTICE 和逐文件
  SHA-256 manifest 已补齐。
- 新增 `src/render/markdown.rs`：沿用 Grok `md_style.rs` 的 ratatui→anstyle
  theme mapping，通过 `render_markdown_ratatui_with_buffers_width` 输出完整文档，
  保留真实 Markdown block blank row，并按可用宽度绘制 GFM table。
- `transcript.rs` 的 `DshRenderBlock::Markdown` seam 改为调用 Grok renderer；删除
  本地不完整的 `pulldown-cmark` event fallback（其 table event 原本被忽略）。
- 删除 `MARKDOWN_SPACING_EVERY=4`、Markdown row 计数和不可选择伪空行；现在每个
  `RichPaintLine` 都只对应 renderer 的真实输出，copy/select/hit-map 不再多出
  周期性几何。
- 新增回归测试：宽度 24 的长单段落必须产生四行以上且每行可选择/可复制；
  Grok adapter 另测语义 block blank row、table 边框/单元格、theme 色彩和 code/link
  样式。

## 验证结果

- Rust/工作区：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --locked`：通过。
  - `cargo test -p xai-grok-markdown --locked --quiet`：479 passed；3 doctests
    ignored。
  - `cargo test -p xai-grok-markdown-core --locked`：45 passed。
  - `cargo test -p dsh-pager-grok-ui --locked`：387 passed；doctest 通过（1
    ignored）。
  - `cargo test --workspace --locked --quiet`：全工作区 unit/integration/doc tests
    通过。
  - `cargo clippy -p dsh-pager-grok-ui --all-targets --no-deps --locked -- -D warnings`：
    本地 UI 代码通过。未加 `--no-deps` 时，当前 Rust 1.98 会在原样 vendored 的
    Grok `mermaid.rs` / `parse.rs` 上触发两条新增 Clippy 建议
    (`question_mark`、`unnecessary_sort_by`)；为保持 A0 源码哈希一致未改写上游。
- 契约/来源/PTY：
  - `python3 scripts/check-protocol-fixtures.py`：2 files in sync。
  - `python3 scripts/check-source-manifest.py --upstream /home/leo/aidreamschool/grok-build`：
    49 rows，local/upstream drift 0，missing license 0。
  - `python3 scripts/parity-matrix.py`：972 cases / 8 fixtures 通过。
  - `cargo build -p dsh-pager-bin --locked`：通过。
  - `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full`：通过。
- ttyd + xterm.js + Playwright 浏览器终端：
  - 固定 viewport 1200×800、DPR 1、DejaVu Sans Mono 16px、
    `TERM=xterm-256color`、`COLORTERM=truecolor`，分别启动本批 debug DSH 和本机
    Grok Build 1.0.5；两端渲染同一份 heading、长段落、GFM table 和最终 marker。
  - 自动断言 DSH 长段落实际换成 6 个终端行，段内空行 0；`grok-ui` / `Harness`
    table cell 两端均存在；browser console/page errors 两端均为 0。人工核对两端
    截图，段落连续、table 结构一致，未再出现每四行断裂。
  - 临时证据：`/tmp/dsh-grok-markdown-visual.KYhmAJ/dsh-markdown.png`（SHA-256
    `463a1d433e73f59bbc7a19cddfc218c6eae6c150adceef9a27ed8a44dd19b271`）、
    `grok-markdown.png`（SHA-256
    `1ac61ea5a157c17c7353cbebc835b05c7bc6fb2ca447bdd05c2995b647ec271e`）和
    `result.json`；均在工作区外，不纳入提交。
- `git diff --check`：通过。

## Git 提交

- commit message：待用户要求；本批次默认不主动提交。
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-25_11-03-07_grok_markdown_renderer.md`
- 暂存区审计：未暂存、未提交；工作区变更只包含本记录“计划修改范围”列出的
  Cargo/NOTICE/manifest、Markdown vendor/adapter 和 transcript 文件。

## 未解决问题和下一步

- 本批次功能范围内无未解决问题。
- Grok `MarkdownContent` 的增量 streaming cache/raw-mode/overlay、syntax theme cache
  和 OSC8 link-map 仍按“不在范围内”保留为后续独立 tranche；当前完整文档 renderer
  已解决生产 transcript 的周期空行和 table 结构问题。
- 若将来把 vendored Grok crate 纳入 workspace-wide `clippy -D warnings`，需升级到
  包含对应 lint 修正的上游 revision，或单独审核补丁并在 manifest 标记 drift；本批
  不用机械 lint 改写破坏 A0 来源一致性。
