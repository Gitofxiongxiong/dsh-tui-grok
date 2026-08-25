# S6C sticky production 接线

> 开始时间：2026-08-25 16:29:40 +0800
> 执行者：Codex
> 状态：完成（S6C；S7 parity/oracle 收尾仍开放）

## 背景

- `scrollback/sticky.rs` 已有来自固定 Grok mirror 的纯坐标 B 适配，并通过渐进收缩、
  next-prompt push/clip 与 bottom-line continuity 单测，但 production 仍完全不调用它。
- 提交 `ded7c4c` 已把 production host/window/painter 收敛到 `DshScrollbackHost`，现在可以
  在一个边界内组合 sticky header、body viewport、selection/hit-map，不再扩大神文件。
- 本批只接 sticky 与其 production 几何；legacy `RichTranscript`/parity 共用 painter 在
  sticky 稳定后另开记录，避免把视觉变化和 oracle 重写塞进同一提交。

## 本轮目标

1. 从 host canonical user entries 与 Fenwick layout 构造 `PromptDescriptor`，调用已适配的
   `compute_sticky_layout`；显式展开的 user prompt 不参与 pin，但仍参与 push 边界。
2. 用同一 `RichPaintLine`/`EntryRenderer::paint_buffer_line` 绘制 pushed/pinned header 和
   body；header 消耗 viewport rows，body scroll offset 用 sticky contract 补偿，保持底行连续。
3. sticky 行继续产生正确 stable entry/block hit target、copy geometry 与 timestamp hover；
   pushed `clip_top` 必须移除不可见行并重置 screen row。
4. 覆盖 zero scroll、渐进 collapse、next prompt push、tiny viewport、header/body 无重复和
   runtime Buffer/hit-map 组合。

## 计划修改范围

若发现新增目标，必须先更新本节再修改。

- `crates/dsh-pager-grok-ui/src/scrollback_adapter/host_pane.rs`
- `crates/dsh-pager-grok-ui/src/appearance.rs`
- `crates/dsh-pager-grok-ui/src/runtime.rs`
- `crates/dsh-pager-grok-ui/vendor/grok/xai-grok-pager/src/scrollback/sticky.rs`
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`
- `docs/GROK_SCROLLBACK_CLOSURE_PLAN.md`
- `docs/GROK_TUI_TRANSCRIPT_SURFACE.md`
- 本进度记录

## 合同

- `scroll_top == 0`、无 user entry、零高 viewport 或 sticky 占满 viewport 时必须安全退化。
- header/body 使用一个 screen-row 坐标系；同一可见 cell 不能被两条 entry 路径重复登记。
- pinned/pushed header 只改变视图组合，不改变 Scrollback canonical height、revision 或
  anchor identity。
- sticky header 的 copy/selection 只包含实际可见行；被 `clip_top` 裁掉的行不可命中。
- 本批不引入第二套 scratch-buffer painter；剪裁通过已物化行的确定性 row slice 完成。

## 风险与回滚

- 动态 header 高度会影响 page/scroll bottom math；必须用 bottom continuity 与 runtime
  viewport tests 锁住。
- user prompt 的 full height 包含 entry separator，而 sticky 只绘 semantic rows；descriptor
  必须排除 separator 并把 gap 交给 sticky layout。
- 小步接入 `DshScrollbackHost`，保留无 header 时的原 visible-body 路径；不执行 destructive git。

## 验证计划

- sticky pure tests + host composition tests + runtime transcript selection/hit-map tests。
- UI/all-target、workspace check、Clippy、fmt、diff、PTY full smoke。
- 尝试固定 1200×800 browser 截图；若 runner 仍不可用，记录部分验收且不宣称 parity。

## 实施记录

- 2026-08-25 16:29:40 +0800：建立 S6C 独立记录；尚未修改代码。
- `DshScrollbackHost` 新增排序的 canonical user-entry 索引；full sync、incremental in-place
  revision 与 clear 都维护同一索引，topology 变化仍走既有 full resync。
- host 从 Fenwick entry layout 选择最近的未显式展开 prompt 和下一 prompt，物化最多两个
  header entry，并把 semantic row height（不含 separator）投给 `compute_sticky_layout`。
- `prepare_viewport` 先物化 body，再按 header 高度重算 body top/height；最多三轮稳定，最终
  恢复离屏 header cache。header/body 共享 `RichPaintLine` dynamic paint、Buffer painter、
  selection/copy 与 HitMap；`clip_top` 直接 slice 已物化行，没有 scratch buffer。
- `appearance` 默认在 desktop 与 compact 都启用 sticky。最初曾按 compact 关闭；真实 Grok
  1200×400 浏览器对照证明上游仍保留固定 prompt，已在提交前修正并补断言。
- vendor B 文件只更新接线状态说明；`SOURCE_MANIFEST.md` 同步本地 hash 与 production 状态。

## 验证结果

- Rust/source：
  - `cargo test -p dsh-pager-grok-ui --all-targets --locked`：456/456 通过；新增 4 个 host
    composition 测试覆盖 bottom continuity、push/clip、tiny viewport、expanded prompt、
    Buffer/HitMap 共用可见行。
  - `cargo check --workspace --locked`、`cargo test --workspace --locked --quiet`：通过。
  - `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features --locked --no-deps --
    -D warnings`：通过。
  - `cargo fmt --all -- --check`、`git diff --check`、protocol fixtures、parity focused tests：
    通过。
  - `check-source-manifest.py --upstream /home/leo/aidreamschool/grok-build`：72 rows，
    local/upstream drift 0，missing license 0。
- 构建/PTY：`cargo build -p dsh-pager-bin --locked` 与
  `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full --timeout 30` 通过；
  覆盖 resize、queue、mouse 与 terminal restore。
- ttyd + xterm.js + Playwright：
  - ttyd 1.7.7、Playwright 1.61.1/headless Chromium；DPR 1、DejaVu Sans Mono 16px、dark、
    `TERM=xterm-256color`、`en-US`、Asia/Shanghai。浏览器 console/page errors 均为 0。
  - DSH 用工作区外确定性 `mock-sticky.mjs` 提供两轮 prompt 和各 48 个列表行；1200×800
    真实 `.xterm` 截图覆盖 follow、连续 PageUp 以及第一 prompt 被第二 prompt 推出的
    transition，正文继续滚动且无 header/body 重复行。
  - Grok 用 `/home/leo/.grok/bin/grok --continue --cwd
    /tmp/dsh-grok-s5-browser-20260825/grok-cwd --fullscreen` 读取隔离合成会话；附加
    1200×400 同环境对照把双方都驱动到“滚过 user prompt”状态。该门禁发现 compact
    sticky 缺陷；修复后双方顶部都保留 user prompt。业务数据不同，只比较 sticky 几何/
    视觉角色，不把整屏差异解释成 pixel parity。
  - 工件目录 `/tmp/dsh-grok-sticky-browser-20260825-1638/`。代表 SHA-256：DSH follow
    `fc221dcb...eb4d`、DSH push `89bb709e...0b4`、DSH compact fixed
    `bcced713...e82a`、Grok compact `99e8b5e7...7b7d`、并排图
    `04fb73d5...04e`。

## Git 提交

- 预期 trailer：
  `Progress-Record: docs/开发进度记录/2026-08-25_16-29-40_S6C-sticky-production接线.md`

## 未解决问题和下一步

- S6C production sticky 已闭包；S7 仍需让 production/parity 共用一个最终 painter/semantic
  surface，并删除 legacy `RichTranscript` production glue/oracle 重复实现。
- 本批没有相同业务 transcript 的逐像素 Grok 对照，不能声称整个 scrollback pixel parity。
