# S5 Tool Family 与 Verb Group 闭包

> 记录时间：2026-08-25 14:31:20 +0800
> 操作者/agent：Codex
> 状态：completed（S5 DSH-presentable production closure；不包含 S6/S7）
> 归档说明（2026-08-25）：本文件是 `02a71be` 整合提交前的历史执行附件，不是独立
> commit authority；提交范围与 trailer 以
> `2026-08-25_15-26-51_S3R-S5上游闭包整合.md` 为准，以下阶段性原文保留。

## 目标与背景

- 按固定 Grok Build `blocks/tool/*` 与 `state/{verb_group,groups}.rs` 的文件布局、
  类型词汇、分派顺序和测试迁入工具渲染/分组闭包。
- DSH DTO 只在 `scrollback_adapter` 一次投影；vendor tool/group 模块不得引用
  `dsh_pager`。
- 删除 `transcript.rs` 的本地 `GroupKind` / `ToolVerb` / `VerbCounts` /
  `build_projection` / `group_header_label` / `render_tool_call` 生产实现。

## 上游与复用基线

- mirror commit：`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`
- SOURCE_REV：`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`
- 复用来源：`scrollback/blocks/tool/{mod,execute,read,search,edit,list_dir,
  web_search,web_fetch,other}.rs`、`scrollback/state/{verb_group,groups}.rs`。
- 复用方式：上游类型/算法优先的 renderer-neutral B 闭包；Grok tool runtime、
  syntect global、media/MCP process 和 `ScrollbackState` 存储在 adapter seam 切断。

## 计划修改范围

- `crates/dsh-pager-grok-ui/vendor/grok/xai-grok-pager/src/scrollback/blocks/tool/{mod,execute,read,search,edit,list_dir,web_search,web_fetch,other}.rs`
- `crates/dsh-pager-grok-ui/vendor/grok/xai-grok-pager/src/scrollback/state/{verb_group,groups}.rs`
- `crates/dsh-pager-grok-ui/src/scrollback.rs`
- `crates/dsh-pager-grok-ui/src/scrollback_adapter/{mod,project_entry,project_tool,project_groups}.rs`
- `crates/dsh-pager-grok-ui/src/views/{transcript,execute_tool,execute_tool_adapter}.rs`
- `crates/dsh-pager-grok-ui/src/views/mod.rs`（同一 vendored execute 文件只编译一次并重导出）
- `crates/dsh-pager-grok-ui/src/{glyphs,lib}.rs`
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`
- `docs/GROK_SCROLLBACK_CLOSURE_PLAN.md`
- `docs/GROK_TUI_TRANSCRIPT_SURFACE.md`
- 本进度记录

不在范围内：Grok 历史/session/tool 执行真源、MCP/Hook 运行时、S6 viewport/sticky、
S7 `RichTranscript` 删除、Git commit。

## 行为矩阵

- Read/Search/ListDir/WebSearch/WebFetch：collapsed/expanded、running/failed、typed
  result detail、copy/selectable row。
- Edit/Execute：保留独立详细卡片，但作为 verb-run breaker。
- Other：显式通用工具，不从 payload 猜不存在的领域语义。
- Verb group：读取/搜索/目录/Web/Subagent 类成员；finished collapsed thought 可被
  run claim 但不计数；opened/pending member 保持可见；breaker 结束 run；标签按首次
  出现 bucket 排序并按 running/failed 更新时态/颜色。

## 风险、回滚与验证

- stable entry/block ID、fold/hit/copy 不能因 synthetic group header 重编号。
- 单个 read 的详细 header、execute selection、context fold 不得回归。
- 先跑 package check/test/clippy/fmt/source/diff，再跑 workspace/PTY，最后以固定
  `ttyd+xterm.js+Playwright` 对比 DSH fixture 与真实 Grok。
- 回滚仅恢复当前本地 tool/group 路径；不执行 destructive Git。

## 实际修改

- 按固定上游目录建立并接入
  `scrollback/blocks/tool/{mod,read,search,edit,list_dir,web_search,web_fetch,other}.rs`；
  已有 `execute.rs` 归入同一个 `ToolCallBlock` sum type，不再由
  `views/mod.rs` 重复编译第二份实现。各 B 文件保留 Grok 的 variant 分派、
  collapsed/expanded/truncated 分支、header/bullet/accent、running/failed 和 typed
  result 结构；Grok tool execution、MCP/hook/media process、全文件 syntect worker
  仍在既定 D 类边界外。
- 新增 `scrollback_adapter/project_tool.rs`，只在该 DSH-neutral seam 把
  `DshToolCallView` / `DshToolResultView` 投成 Read/Search/Edit/ListDir/WebFetch/
  WebSearch/Other/Execute。浏览器联调补齐两个真实 adapter 缺口：result diff 无标题
  时依次保留 call title、argument path、single-diff path；runtime view 被折叠掉时，
  `DshEditDetail` 的 old/new/path 仍进入 Edit diff，而不是退化成 `Edit edit` 空卡。
- 迁入 renderer-neutral `state/verb_group.rs` 与 `state/groups.rs`，保留上游
  Member/ThoughtMember/Transparent/Break walk、单成员 eager fold、首次出现 bucket
  顺序、时态/复数、failed suffix、citation/subagent distinct source count、pending/action
  breaker 和 stable group anchor。`project_groups.rs` 只负责把稳定 DSH entry id、
  display mode、pending state 投给该状态机。
- `transcript.rs` 已删除本地 `GroupKind`、`ToolVerb`、`VerbCounts`、
  `build_projection`、`group_header_label` 和旧 `render_tool_call` 生产实现；所有
  DSH-presentable tool variant 走新 tool family，所有 verb-run 走新 state modules。
- 修正真实鼠标 fold 路由：group header 即使 hit target 带 anchor block index，也先
  切整组；standalone Tool 的 block-index hit 回落到 entry fold；只有 Assistant 内嵌
  block 使用 `expanded_blocks`。因此浏览器双击 Edit 能显示 diff，双击
  `Reading 1 file, Searching 1 pattern` 能显示两个稳定成员。
- `SOURCE_MANIFEST.md` 登记 10 个新增上游文件的 upstream/vendor hash、B 接缝和
  明确排除项；closure inventory 标记 S5 tool family/verb grouping production
  closure。

## 验证结果

- Rust/source/static：
  - `cargo test -p dsh-pager-grok-ui --all-targets --locked`：427/427 通过。
  - `cargo check --workspace --locked`、`cargo test --workspace --locked --quiet`：通过；
    workspace unit/integration/doc tests 无失败。
  - `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features --locked --no-deps
    -- -D warnings`：通过。
  - workspace 全量 Clippy（不带 `--no-deps`）仍只被未修改 A0
    `xai-grok-markdown` 的两条新编译器 lint 阻挡：`mermaid.rs:3185`
    `question_mark`、`parse.rs:1682` `unnecessary_sort_by`；本批没有篡改固定上游文件
    来隐藏基线问题。
  - `cargo fmt --all -- --check`、`git diff --check`：通过。
  - protocol fixtures：2 files in sync；M10 parity：972 cases / 8 fixtures。
  - `check-source-manifest.py --upstream /home/leo/aidreamschool/grok-build`：69 rows，
    local/upstream drift 0，missing license 0。
- 构建/PTY：`cargo build -p dsh-pager-bin --locked` 通过；完整 PTY smoke 覆盖
  resize/queue/mouse/restore。冷门禁曾在共享 12s 总 deadline 的 `/resume` 等待点
  连续超时两次；同一二进制以 `--timeout 30` 通过，随后默认 12s 原样复跑也通过，
  判定为启动时序抖动而非吞掉失败。
- ttyd + xterm.js + Playwright：
  - 双端均为 ttyd 1.7.7、Playwright 1.61.1/headless Chromium、viewport
    1200×800、DPR 1、DejaVu Sans Mono 16px、dark、`TERM=xterm-256color`、
    `en-US`、Asia/Shanghai；全部 screenshot 为真实 `.xterm`，browser console/page
    errors 均为 0。
  - DSH 默认 checked-in mock 发送 `structured smoke`，真实双击 Edit 后自动断言并
    截到 `◆ Edit src/mock.rs +1/-1`、`- old line`、`+ new line`。
  - 为覆盖 verb-run，工作区外复制 checked-in mock 到 `mock-s5.mjs`，仅追加相邻
    Read/Search call；collapsed 截到 `◆ Reading 1 file, Searching 1 pattern`，真实
    双击后截到 open marker、`Read src/a.rs` 与 `Search TODO (no matches)`。临时 fixture
    没有写入仓库。
  - Grok 1.0.5 在隔离 cwd 中把 `src/mock.rs` 从 `old line` 改成 `new line` 并读回，
    截到两个 `Read 1 file` header、`◆ Edit src/mock.rs` 和 diff，最后回复 `done`。
  - 工件目录 `/tmp/dsh-grok-s5-browser-20260825/`。代表 SHA-256：
    DSH edit-expanded `9e8053ef...1434f4`，DSH group collapsed
    `1ec3b89e...3239b4`，DSH group expanded `5f3c91cb...eb91e`，Grok edit/read
    `d4be3fde...af3bc`；`result.json` 含完整 hash、errors 和环境。
  - 并排图和放大绝对像素 diff 已生成；整屏 changed ratio 约 20%。它不是 parity
    分数：DSH fixture 额外含 Tasks/Subagents/Queue，Grok 含 Thought/timestamps/
    Worked-for，均属业务数据差异。Edit 的 Grok contextual hunk/行号/syntect 与 DSH
    typed `+/-` diff 是 manifest 已声明的 B 排除差异；这也是为什么本批只声明 S5
    DSH-presentable closure，不声明 S7 pixel parity。

## Git 提交

- 本批不主动提交。
- 预期 trailer：
  `Progress-Record: docs/开发进度记录/2026-08-25_14-31-20_S5-tool-family与verb-group闭包.md`

## 未解决问题和下一步

- S5 范围内无未接线的 DSH-presentable tool family 或本地平行 verb-group 生产路径。
- S6A/S6B 仍需 range/borrow + viewport/overscan、`render.rs`/sticky/Buffer-direct；S7
  仍需删除 `RichTranscript` production glue。完成 S7 前不得声称整个 scrollback
  pixel parity。
- Grok contextual edit hunk/syntect/source-map、MCP/use_tool、media/tool execution 等
  只有在 DSH 提供相应中性数据/能力后才能开启；不得从字符串猜测或把 Grok runtime
  引入 DSH production dependency。
