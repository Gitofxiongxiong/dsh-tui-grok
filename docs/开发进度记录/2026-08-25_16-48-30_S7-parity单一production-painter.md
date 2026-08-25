# S7 parity 单一 production painter

> 开始时间：2026-08-25 16:48:30 +0800
> 执行者：Codex
> 状态：完成（S7；N2 scrollback renderer closure 已接上）

## 背景

- S6C 后 runtime 已只使用 `DshScrollbackHost`，但 `parity.rs` 仍两次构造 legacy
  `RichTranscript`：一次生成 semantic rows，一次生成 HitMap；随后又用统一 transcript
  文本样式重画 Buffer，绕开 production entry painter、group、sticky 与真实 content offset。
- `views/transcript.rs` 还同时承载 DSH semantic entry materializer。该部分仍是必要的中性
  adapter，不应随 legacy oracle 一起删除；应迁到 `scrollback_adapter` 并由 production/
  parity 共同消费。

## 本轮目标

1. 让 parity 从 snapshot 的中性 `DshRenderEntry` 建立 canonical `Scrollback`，调用
   `DshScrollbackHost` 的 sync/viewport/visible/direct-Buffer 路径一次，同时生成 rows 与
   HitMap。
2. 删除 `RichTranscript`、`RichEntry` 及其独立 height/anchor/viewport/dynamic-paint 实现；
   相关测试直接验证 semantic materializer 或 production host。
3. 把 entry semantic materializer 从 view 层迁到 `scrollback_adapter/materialize_entry.rs`，
   删除 `views::transcript` 模块，避免重新出现第二条 production renderer。
4. 保留 DSH stable entry/block identity、copy joiner、timestamp、selection/hit geometry 与现有
   50k viewport-bound 性能合同。

## 计划修改范围

若发现新增目标，必须先更新本节再修改。

- `crates/dsh-pager/src/scrollback.rs`
- `crates/dsh-pager-grok-ui/src/scrollback_adapter/mod.rs`
- `crates/dsh-pager-grok-ui/src/scrollback_adapter/materialize_entry.rs`（由
  `src/views/transcript.rs` 移入）
- `crates/dsh-pager-grok-ui/src/scrollback_adapter/host_pane.rs`
- `crates/dsh-pager-grok-ui/src/parity.rs`
- `crates/dsh-pager-grok-ui/src/views/mod.rs`
- `crates/dsh-pager-grok-ui/src/views/transcript.rs`（删除/移动）
- `docs/GROK_SCROLLBACK_CLOSURE_PLAN.md`
- `docs/GROK_TUI_TRANSCRIPT_SURFACE.md`
- 本进度记录

## 合同

- parity transcript cells 必须来自 `DshScrollbackHost::paint_buffer_line`，不得先转纯文本再
  统一套色；rows、cells、HitMap 必须消费同一份 `RichPaintLine`。
- snapshot → canonical scrollback 的入口只接受已投影的中性 DTO，不解析协议 JSON，也不
  取得 runtime/session authority。
- 删除 legacy oracle 后仍须保留所有 semantic block、wrap、copy、fold/group、sticky、
  timestamp 与 50k 性能测试；测试重写不能靠减少断言掩盖差异。
- 不改变 production runtime 的滚动状态机或外部协议。

## 风险与回滚

- parity 的 transcript cell 颜色/缩进会从伪统一样式变为真实 production 样式；任何快照
  差异必须分类为旧 oracle 缺陷或真实回归。
- synthetic scrollback 构造必须维护 identity/positions/revision/layout 初始不变量；新增单测
  锁定顺序、hidden height 与 anchor。
- 回滚以本独立提交为单位，不执行 destructive Git；8 份既有未跟踪进度文档继续排除。

## 验证计划

- UI/all-target、dsh-pager、workspace check/test、Clippy、fmt、diff、source/protocol/parity。
- source grep 明确 `RichTranscript`、`views::transcript` 和第二套 viewport painter 为 0。
- full PTY；由于本批改变 parity/reference Buffer 且 production 可见路径预期不变，仍重跑
  ttyd+xterm.js+Playwright DSH/Grok 浏览器回归，若发现可见变化则按用户可见门禁处理。

## 实施记录

- 2026-08-25 16:48:30 +0800：建立 S7 独立记录；尚未修改代码。
- `dsh_pager::Scrollback` 新增 `from_render_entries` 中性构造边界：已投影 DTO 仍通过
  canonical upsert/index/revision/layout 初始化，不需要逆向伪造协议事件；新增单测锁住
  identity、顺序、revision、hidden height 与 visible anchor。
- 将必要的 entry semantic materializer 从 `views/transcript.rs` 迁到
  `scrollback_adapter/materialize_entry.rs`，相邻 host 直接消费；删除 `views::transcript`
  module、`RichTranscript`、`RichEntry` 以及独立 height/anchor/visible/dynamic-paint、纯文本
  `render_row`/copy 路径。迁移后的 materializer 只接受 canonical entry + projection/appearance，
  不拥有 history 或 viewport。
- `ProjectionInfo` 的 test-only owned helper 也先建立 canonical `Scrollback` 再走 production
  borrowed projection，避免测试重建另一套默认 display-mode 入口。原裸行测试改为验证真实
  EntryRenderer chrome；需要覆盖展开内容的 block/tool 测试显式使用 Expanded 合同，没有修改
  production 默认折叠语义。
- `parity::render_semantic` 从 snapshot DTO 建 canonical `Scrollback`，只执行一次
  `DshScrollbackHost::{sync_with_appearance,prepare_viewport,visible_lines}`，并直接调用
  `paint_buffer_line` 写最终 semantic Buffer；同一 `RichPaintLine` 同时产生 transcript rows、
  entry/block copy HitMap 与 timestamp hover。后续 generic row pass 跳过 transcript，防止把真实
  block 样式重新覆盖成统一文本色。
- 新增 parity 回归：user transcript 必须出现 production prompt glyph/背景和
  `TranscriptEntry` hit geometry；旧 uniform fake painter 无法满足该断言。
- 2026-08-25 17:01 +0800：更新 closure plan 与 transcript surface，关闭 TEMPORARY
  `RichTranscript` 挂账；结论限定为 N2 scrollback renderer closure，不外推为全 TUI 或任意
  数据的逐像素一致。

## 验证结果

- Rust/静态门禁：
  - `cargo test -p dsh-pager-grok-ui --all-targets --locked --quiet`：456/456 通过。
  - `cargo test -p dsh-pager --all-targets --locked --quiet`：各目标通过（主库 80/80）。
  - `cargo check --workspace --locked --quiet`、`cargo test --workspace --locked --quiet`：通过。
  - `cargo clippy -p dsh-pager -p dsh-pager-grok-ui --all-targets --all-features --locked
    --no-deps -- -D warnings`：通过。
  - `cargo fmt --all -- --check`、`git diff --check`、`scripts/check-protocol-fixtures.py`：通过。
  - `scripts/parity-matrix.py`：972 cases / 8 fixtures 通过。
  - 50k focused viewport test：通过；source grep 的 `RichTranscript`、`RichEntry`、
    `views::transcript`、旧 `render_row`/`style_for_paint` production 命中均为 0。
  - source checker 不带参数时因仓库脚本仍默认 `/home/leo/code/grok-build` 而失败；改用实际
    mirror `--upstream /home/leo/aidreamschool/grok-build` 后通过：revision
    `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，72 rows，local/upstream drift 0，missing
    license 0。本批未修改 vendor/manifest。
- 构建/PTY：`cargo build -p dsh-pager-bin --locked`、
  `python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full --timeout 30` 通过。
- ttyd + xterm.js + Playwright 浏览器回归：
  - ttyd 1.7.7、Playwright 1.61.1/headless Chromium；1200×800、DPR 1、DejaVu Sans Mono
    16px、dark theme、`TERM=xterm-256color`、`en-US`、Asia/Shanghai；双方 `.xterm` bounding
    box 均为 1200×800，console/page errors 均为 0。
  - DSH 启动：`target/debug/dsh-pager --backend
    /home/leo/.nvm/versions/node/v22.23.1/bin/node --backend-arg
    /tmp/dsh-grok-sticky-browser-20260825-1638/mock-sticky.mjs`；确定性两轮/长正文 fixture，截取
    follow 后通过真实 helper textarea 发送 `PageUp`。滚动后顶部 prompt、左边框、正文缩进、
    scrollbar 与 prompt chrome 连续，无重复/覆盖。
  - Grok 启动：`/home/leo/.grok/bin/grok --continue --cwd
    /tmp/dsh-grok-s5-browser-20260825/grok-cwd --fullscreen`；截取同尺寸 follow/PageUp，user、
    thinking、Read/Edit rail、diff、timestamp 与 prompt chrome 均正常。会话正文长度不足一页，
    因此 PageUp 前后 hash 相同，这是输入状态事实而非门禁失败。
  - 工件：`/tmp/dsh-grok-s7-browser-20260825-165957/`；DSH follow SHA-256
    `38786399...208b`、DSH PageUp `626461d9...f761`、Grok follow/PageUp
    `4d57a401...d1b6`、并排图 `7d8fc953...d67f`。双方业务数据不同，人工对照仅验 geometry/
    视觉角色/交互回归，不声称整屏同数据 pixel equality。

## Git 提交

- 预期 trailer：
  `Progress-Record: docs/开发进度记录/2026-08-25_16-48-30_S7-parity单一production-painter.md`

## 未解决问题和下一步

- 本记录范围内无阻塞；S7 与本文定义的 N2 scrollback renderer closure 已关闭。
- 全 TUI pixel parity、N1/N3/N4，以及同业务数据的跨产品 golden/浏览器逐像素比较仍属于
  上级计划，不能用本批不同 transcript 的截图代替。
