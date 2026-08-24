# Grok TUI：长时间后台任务与 Subagent 功能点

> 调研对象：`/home/leo/code/grok-build` 中的 `xai-grok-pager`。
> 对齐目标：为 `dsh-pager-grok` 提供可实现、可验收的 TUI 行为契约；本文不改变 Grok 或 DSH 的生产代码。
> 源码镜像：commit `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，`SOURCE_REV=7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
> 浏览器证据：`ttyd` 暴露终端，Playwright 驱动真实 `xterm.js`，固定 1200×800、DPR 1、DejaVu Sans Mono 16。

## 1. 结论先行

Grok 不是只有一条“任务条”。一个长时间操作同时有三层可见面：

1. **顶部 Tasks pane**：统一列出运行中的 subagent、后台命令、monitor、loop 和 workflow。它默认在出现新的实时工作时自动打开，位于 scrollback 上方；分组可以折叠。
2. **提示词上方的持久状态条**：turn 已经空闲但 watcher 仍在运行时，显示 `○/◎/◉ 1 command still running` 或 `1 subagent still running`。这条不会随 scrollback 滚走，点击可重新打开 Tasks pane。
3. **Scrollback 生命周期块与详情 viewer**：启动、进度和完成结果留在会话历史中；后台任务的双击打开输出 viewer，subagent 的双击进入子会话全屏 viewer。详情不是把顶部行原地变高，而是打开一个拥有独立键盘焦点的覆盖层。

因此，“顶部显示一个正在执行的任务，双击后展开”在 Grok 中应实现为：

```text
新任务事件
    │
    ├─ Tasks pane 顶部出现一行（spinner + 描述 + elapsed + 操作按钮）
    ├─ scrollback 写入 Started/Running block
    └─ turn 结束后，提示词上方仍保留 watcher cue

鼠标第一次点击       = 选择行、显示 hover/操作按钮
同一行第二次点击       = 打开详情（任务输出 viewer / subagent 全屏）
Enter 或 Ctrl+F       = 对选中行执行同样的打开动作
Esc / q              = 关闭详情，回到父会话和原焦点
```

顶部 group header 的双击/Enter 则是另一种语义：**折叠或展开分组**，不会打开任务详情。

## 2. 三种“正在运行”状态要分开

| 表面 | 所在位置 | 解决的问题 | 生命周期 | 是否可打开详情 |
| --- | --- | --- | --- | --- |
| Active turn status | scrollback 与 prompt 之间 | 模型当前正在 Thinking/Responding/Running/Waiting | 当前 turn | 不直接打开；`[stop]` 取消，`[↓]` 将 execute 放到后台 |
| Tasks pane | AgentView 顶部、scrollback 之前 | 多个并行或独立后台工作总览 | 任务存在时 | 是；任务进入 block viewer，subagent 进入 child fullscreen |
| Watcher cue | prompt 正上方 | turn 已空闲但仍有后台工作 | 至少一个 watcher 存活 | 点击打开 Tasks pane |
| Scrollback block | 会话历史 | 告诉用户何时启动、当前活动、何时完成/失败 | 事件历史 | 是；任务输出或子会话详情 |

AgentView 的实际堆叠顺序是：`status bar → tasks → catalog/todo → scrollback → queue → turn status → banner/CTA → prompt`。所以“顶部”指 Tasks pane，不是提示词上方的 cue；两者都要保留，否则用户滚动历史或输入新消息时会失去后台任务可见性。

## 3. 数据模型与状态机

### 3.1 后台命令、monitor、loop

用户指南 `docs/user-guide/20-background-tasks.md` 定义 `run_terminal_command` 的 `background: true` 返回稳定 task id；前台执行可以通过 `Ctrl+B` 转后台。运行时 stdout 由 `task_id/tool_call_id` 路由到同一状态对象。

```text
ExecuteForeground
      │ Ctrl+B / background:true
      ▼
Running ──stdout/monitor tick──▶ Running(updated output/activity)
   │
   ├─ exit 0 ─▶ Done
   ├─ non-zero / signal ─▶ Failed
   └─ x / kill ─▶ kill requested ─▶ Done/Failed (terminal result)
```

Grok 的 `BgTaskState`（`src/app/agent.rs`）至少维护：

- `task_id`、关联 `tool_call_id`、`command`、可选 `description`、`cwd`；
- `status`（`Running/Done/Failed`）、`start_time/finish_time`、elapsed、exit code/signal；
- stdout 缓冲、行数、是否截断（上限 10 MB）、`pending_kill` 及 kill 超时；
- `is_monitor`、scrollback entry id、`restored_from_replay`。

完成事件可能先于 Started 事件抵达；handler 会创建可见的 tombstone/terminal block，而不是丢失结果。恢复会话中的仍运行任务可以展示，但 `restored_from_replay` 不会触发“新任务自动打开”动画。

### 3.2 Subagent

用户指南 `docs/user-guide/16-subagents.md` 将 subagent 定义为拥有独立上下文的 child session。`SubagentInfo`（`src/app/subagent.rs`）通过三个事件更新：

```text
SubagentSpawned
      │ 创建 SubagentInfo + child AgentView + 父 scrollback block
      ▼
Running ──SubagentProgress──▶ Running(activity/tokens/context/duration)
      │
      ├─ status=completed ─▶ Completed
      ├─ status=failed ────▶ Failed
      └─ cancelled/kill ───▶ Cancelled
```

关键字段包括：`subagent_id`、`child_session_id`、description、persona/role/type、model、started/finished/duration、status/error、tool calls/turns/tokens/context usage、`activity_label`、`is_background`、`pending_kill`、scrollback entry 和 transcript location。UI 的稳定主键应使用 `subagent_id`（跨事件），打开子会话使用 `child_session_id`。

同一个 subagent 有两种历史呈现：

- **blocking**：父会话中的一个 block 从 `running` 变为完成/失败；父 turn 可能停在可发送的 wait 上。
- **background**：先写 `Subagent started: ...`，完成时另写 `Subagent completed/failed/cancelled in Xs`，父 turn 可继续输入。

workflow 的内部 child 会保留在 workflow detail，不重复塞进普通 Subagents group；这避免同一工作在顶部出现两次。

## 4. 顶部 Tasks pane

### 4.1 分组、过滤和排序

`src/views/tasks_pane.rs` 用统一 `TaskEntry` 列表渲染四组：

```text
▾ Workflows N
▾ Subagents N
▾ Tasks N
▾ Watchers N       # monitor 与 /loop/scheduled
```

默认只显示 live 项；按 `h` 切换 `show_done` 后，完成项也会显示并以 dim 色退到后面。排序规则是：分组顺序 → 运行中优先 → 组内新启动优先 → 稳定 id tie-break。这样新任务不会因每帧 elapsed 改变而跳行。

分组 header 自身是可选择项，显示 `▾`（展开）或 `▸`（折叠）和数量。空组不显示，且空组时忘记旧的折叠状态；新一批 subagent 出现时默认展开。

### 4.2 自动打开、自动关闭和尺寸

- `running_count` 从 0 变为正数时自动打开 overlay；恢复 replay 的后台任务被排除，避免启动 TUI 时突然抢焦点。
- 由自动打开的 overlay 在没有运行项、没有暂停 workflow、未被用户聚焦且未显示完成项时自动关闭。
- 用户手动打开/聚焦后，任务结束不会强制关闭。
- `desired_height` 最多 8 行，同时不超过终端高度的 15%；终端高度小于 12 行时隐藏。搜索/过滤输入会额外预留一行。
- pane 自身驱动 spinner tick；运行行必须持续重绘 elapsed 和 activity。

这使“顶部一行”在单任务场景下只占一行；多任务时增长为可控的列表，而不是遮住 prompt。

### 4.3 后台任务行

有 description 时显示 `Task <description>`；没有 description 才回退到 shell command，并做 bash 高亮和单行截断。monitor 使用蓝色 `Monitor <description>` 标签。运行行使用高亮文字，完成项 dim；右侧 overlay 保留：

```text
⠴/⠦  8.4s   [↗] [✗]   (N) / (N+)
```

- spinner/check/叉分别表达 running/success/failure；
- elapsed 使用 wall-clock；
- `[↗]` 打开 stdout viewer，`[✗]` 请求 kill；
- `(N)` 是 stdout 行数，`(N+)` 表示输出被截断；
- viewer 的 preamble 仍可看到完整 description、`$ command` 和 cwd，不能只依赖 pane 的短标签。

### 4.4 Subagent 行

标签优先级是 persona → role → `subagent_type` → `[tag]` → `General`；description 会去除 tag 前缀。运行中如果有 progress activity，在描述后追加 `— <activity>`，描述最多约 40 列以给右侧状态让位。model 不内嵌到主文案，而是右对齐：

```text
General Long subagent demo — Running cargo test…   grok-4.6  17s  [↗] [✗]
```

状态配色：运行/待取消为 vivid accent；completed 为绿色 dim；failed/cancelled 为红色 dim。右侧还可显示 context 使用量 badge；详情打开后才展示完整 prompt、thinking、tool calls 和 transcript。

## 5. Scrollback 生命周期块与详情

### 5.1 BgTaskBlock

`src/scrollback/blocks/bg_task.rs` 的 block 默认折叠、可选择但不走普通 fold。生命周期文案：

| 状态 | 文案/视觉 |
| --- | --- |
| Started/Running | `Task started: <description>`，动画运行 bullet；activity/elapsed 可更新 |
| Completed | `Task completed in X: <description>`，绿色终态 bullet |
| Failed/killed | failure/killed 文案，红色终态 bullet |

Enter 或同一 block 双击创建 `BlockViewerPane::for_bg_task`。viewer 是带边框的覆盖层，显示标题、`$ command`、cwd、stdout，底部提供 `Esc:close /:search f:filter v:select w:wrap`；关闭后回到父 AgentView 的 scrollback/prompt 焦点。

### 5.2 SubagentBlock 与 child fullscreen

`src/scrollback/blocks/subagent.rs` 对 blocking subagent 原地更新，对 background subagent 追加独立 completion block。运行 bullet 和 activity 与 Tasks pane 同步；已完成行停止动画。

Enter/Ctrl-F 或 block 双击进入 child session fullscreen。fullscreen 顶部显示状态 icon、persona/type、description、model、elapsed、可选 resumed/forked badge；正文是子会话自己的 prompt、thinking、工具输出和 live activity，底部显示：

```text
→:expand | Enter:open | Ctrl+e:expand thinking | q/Esc:back | Ctrl+c:cancel | Ctrl+x:shortcuts
```

`q`/`Esc` 只返回父会话，不取消 subagent；取消需要 `[✗]` 或 `Ctrl+C/x` 对应的 kill/cancel action。子会话内容不能直接混进父 scrollback，否则无法区分上下文和焦点。

## 6. 交互矩阵

| 表面 | 操作 | 结果 |
| --- | --- | --- |
| 任意 AgentView | `Ctrl+G` | 打开/关闭 Tasks pane；关闭时焦点回 scrollback |
| Tasks pane | ↑/↓、`j/k`、滚轮 | 移动选择；运行行继续 tick |
| Tasks pane header | Enter/Ctrl-F、点击、←/→ | toggle；← 强制折叠，→ 强制展开 |
| Tasks pane 任务行 | Enter/Ctrl-F | BgTask → stdout viewer；Agent → child fullscreen |
| Tasks pane 任务行 | `x` 或 `[✗]` | 请求 kill/cancel；保留 pending-kill 视觉，等待终态事件 |
| BgTask 行 | `y` | 复制 stdout；没有输出时仍复制空结果/提示，不打开 shell |
| Tasks pane | `h` | 显示/隐藏完成项 |
| Tasks pane | `/`、`f`、`v` | 搜索、过滤、选择模式；列表底部预留输入行 |
| Tasks pane | `Tab` | 将焦点交给 prompt（pane 仍可见） |
| overlay/viewer | `Esc` | 关闭 pane/viewer，回到父焦点 |
| prompt 上方 watcher cue | 鼠标点击 | 打开 Tasks pane；首次点击可显示一次 `Tip: Ctrl+G toggles the tasks pane` |
| active turn status | `[stop]` | 取消当前 turn；不等同于 kill 已后台化任务 |
| active execute | `[↓]` 或 `Ctrl+B` | 将前台命令降级为后台任务，立即释放父 prompt |
| scrollback BgTask block | 单击 | 选中 block |
| scrollback BgTask block | 双击 | 打开任务输出 viewer（不是普通折叠） |
| scrollback Subagent block | 单击 | 选中 block |
| scrollback Subagent block | 双击 | 打开对应 child fullscreen |
| child fullscreen | `q`/`Esc` | 返回父会话 |

### 6.1 鼠标双击判定

`src/app/mouse.rs` 和 `src/app/agent_view/selection.rs` 使用同一行/同一 block 的时间窗口 `MULTI_CLICK_TIMEOUT_MS`（源码常量，不要在 DSH 中另设一个窗口）。Tasks pane 的第一次点击只更新 selection；第二次点击在窗口内才打开详情。点击右侧 `[↗]` 是显式单击打开，不依赖双击；点击 `[✗]` 只杀任务。

要注意坐标命中区域：header 是独立一行，任务行在其下一行；浏览器自动化应先定位 `.xterm` 的行文本，再点击该行中心，不能假设 pane 顶边就是任务行。实际截图中，点击 `▾ Tasks 1` 只会折叠；双击 `Task Long background demo` 才会出现 viewer。

## 7. Watcher cue 与 turn 语义

`src/views/turn_status.rs::still_running_label` 只拼接非零计数，格式示例：

```text
1 command · 2 monitors · 1 loop · 1 subagent still running
```

它渲染在 prompt 上方，使用较慢的 `○ ◎ ◉ ◎` 脉冲，意图是“后台仍在观察”，不是新的 active turn。点击命中区域只在有 watcher 时生成；键盘宿主不画假的鼠标按钮。若父 turn 停在可发送 wait，后缀为 `· send a message to interrupt`；有排队消息时改成 `· N queued` 或 `· N queued — Enter to send now`。

active turn 则使用更快 spinner、activity label、阶段 elapsed、总 elapsed/token，并显示 `[stop]` 或可选的 `[↓]`。实现 DSH 时必须把“取消当前 turn”“中断 wait 并发送新消息”“杀后台 task”建成三个不同 Intent，不能因为视觉上都叫 stop 而复用一个效果。

## 8. DSH 对齐契约

### 8.1 中立 DTO

建议 DSH adapter 向 Grok-style view 提供以下稳定 DTO（view 不直接访问 RPC、进程或 ACP）：

```text
BackgroundTaskDto {
  task_id, tool_call_id, kind(command|monitor|loop), description, command, cwd,
  status(running|done|failed|cancelled), started_at, finished_at, elapsed,
  activity, exit_code, signal, stdout, stdout_lines, stdout_truncated,
  pending_kill, restored_from_replay, scrollback_entry_id
}

SubagentDto {
  subagent_id, child_session_id, description, type_label, persona, role, model,
  status(running|completed|failed|cancelled), is_background, started_at,
  finished_at, elapsed, activity, tokens, context_used, error,
  pending_kill, scrollback_entry_id, transcript_location
}
```

必须使用稳定 id 做 selection、双击和事件去重；不能用显示文本或数组下标作为主键。每次 progress 事件要能安全重放，完成事件应覆盖先前 activity 并停止动画。

### 8.2 Intent/Effect

| UI Intent | DSH Effect | 结束条件 |
| --- | --- | --- |
| ToggleTasksPane | 只改变 pane visibility/focus | 下一帧布局可见性稳定 |
| ToggleTaskGroup(group) | 改变本地 collapse state | 不触碰任务进程 |
| OpenBgTask(task_id) | 读取已缓存 stdout/scrollback | viewer 打开；任务可继续运行 |
| OpenSubagent(child_session_id) | 聚焦已有 child transcript/view | child fullscreen 打开 |
| KillBgTask(task_id) | 发送终止请求，置 `pending_kill` | 收到 terminal status 或超时失败 |
| KillSubagent(subagent_id) | cancel child session | `SubagentFinished(cancelled/failed)` |
| BackgroundForegroundExecute | 标记 demoted，接收后续 task events | 前台 turn 立即释放 |
| SendWhileWaiting | 取消可中断 wait，再排入新消息 | 新 turn 开始；后台 watcher 不受影响 |

adapter 负责把 ACP/DSH 事件归一化为 `Spawned/Progress/Finished` 或 `Started/Stdout/Completed`；view 只消费 DTO 和 Intent。用 session/generation 或事件序号拒绝迟到事件，尤其是 `Finished` 后的旧 progress。

### 8.3 验收条件

- 一个后台命令启动后，顶部自动出现 `Tasks 1` 和 running row；父 turn 结束后 prompt 上方仍显示 watcher cue。
- 一个 subagent 启动后，顶部出现 `Subagents 1`；activity、model、elapsed 与 block 同步刷新。
- 第一次点行只选中，时间窗口内第二次点行打开正确详情；header 双击只折叠。
- 任务完成/失败后，spinner 停止，状态 icon、duration 和历史 completion block 正确；默认 pane 可自动收起但用户手动打开/`h` 时仍能查看。
- Esc/q 从 task viewer 或 child fullscreen 返回父会话；不会误取消任务。
- kill 后行进入 pending-kill，直到终态事件到达；迟到 progress 不会让完成行重新变成 running。
- stdout 无输出、超长输出、恢复 replay、Started/Completed 乱序均有确定渲染。

## 9. 浏览器截图证据

以下图片均是通过 `ttyd + xterm.js + Playwright` 截取的 `.xterm` 内容，不是用 DOM mock 拼出来的。spinner、elapsed、光标和模型响应是动态的，截图用于确认布局和交互语义，不作为像素级基准。

| 截图 | 观察到的行为 |
| --- | --- |
| [01-task-pane-running.png](assets/grok-tui-tasks/01-task-pane-running.png) | 顶部 `▾ Tasks 1`、`Task Long background demo`、elapsed、`[↗][✗]`；scrollback 有 `Task started`；prompt 上方有 `1 command still running`。 |
| [02-task-viewer-expanded.png](assets/grok-tui-tasks/02-task-viewer-expanded.png) | 双击任务行后出现带边框的大型 stdout viewer，标题为任务 description，显示 `$ sleep 90` 和 `Esc:close` 等 footer。 |
| [03-subagent-pane-running.png](assets/grok-tui-tasks/03-subagent-pane-running.png) | 顶部 `▾ Subagents 1`；行内有 `General`、description/activity，右侧有 `grok-4.6`、elapsed、view/kill；cue 显示 `1 subagent still running`。 |
| [04-subagent-fullscreen.png](assets/grok-tui-tasks/04-subagent-fullscreen.png) | 双击/Enter 后进入 child fullscreen；顶部显示 persona、description、model、elapsed、`[x]`，正文独立显示 child prompt/thinking/tool，底部有 `q/Esc:back`。 |

### 9.1 可复现实验环境

使用的终端服务命令（工作目录为隔离的 `/tmp/grok-tui-taskshots.*`）：

```bash
~/.local/bin/ttyd -i 127.0.0.1 -p 7688 -W \
  -T xterm-256color \
  -t fontSize=16 -t 'fontFamily=DejaVu Sans Mono' \
  -t screenReaderMode=true \
  -t 'theme={"background":"#0a0a0a","foreground":"#e1e1e1"}' \
  env GROK_HOME=/home/leo/.grok /home/leo/.grok/bin/grok \
    --cwd /tmp/grok-tui-taskshots.dhhr5B --no-alt-screen \
    --always-approve --no-plan
```

Playwright 通过浏览器连接 `http://127.0.0.1:7688`，等待 `.xterm`，聚焦 `.xterm-helper-textarea`，按键和鼠标都发送到真实终端，再保存 `.xterm` 截图。截图中的长任务和 subagent 使用 `sleep 90`，目的是让 running 状态稳定停留，随后可以验证双击打开 viewer；不代表功能要求命令一定是 sleep。

## 10. Grok 源码与用户指南索引

行为实现的主要入口：

- `crates/codegen/xai-grok-pager/src/views/tasks_pane.rs`：`TaskEntry`、分组、排序、自动开关、尺寸、pane key/mouse。
- `crates/codegen/xai-grok-pager/src/views/turn_status.rs`：`Watchers`、`still_running_label`、prompt 上方 cue、active turn/parked wait。
- `crates/codegen/xai-grok-pager/src/views/agent.rs`：`AgentViewLayout::compute`，确认 tasks 位于 scrollback 之前、turn status 位于 prompt 之前。
- `crates/codegen/xai-grok-pager/src/app/agent_view/render.rs`：每帧 sync tasks 并按布局绘制。
- `crates/codegen/xai-grok-pager/src/scrollback/blocks/bg_task.rs`、`subagent.rs`：历史 block 的 Started/Progress/Completed/Failed/Cancelled 视觉生命周期。
- `crates/codegen/xai-grok-pager/src/app/agent.rs`、`app/subagent.rs`：中心状态结构与 elapsed/输出缓存。
- `crates/codegen/xai-grok-pager/src/app/acp_handler/background.rs`：后台启动、stdout、完成/乱序事件、kill/tombstone。
- `crates/codegen/xai-grok-pager/src/app/acp_handler/session_notification.rs`、`subagent_activity.rs`：subagent spawn/progress/finish 与 scrollback/pane fan-out。
- `crates/codegen/xai-grok-pager/src/app/agent_view/selection.rs`：scrollback 单击/双击路由。
- `crates/codegen/xai-grok-pager/src/app/agent_view/panes.rs`、`src/app/mouse.rs`：Tasks pane 的键盘、按钮、双击和焦点回退。
- `crates/codegen/xai-grok-pager/docs/user-guide/16-subagents.md`、`20-background-tasks.md`、`23-dashboard.md`：面向用户的快捷键、任务生命周期和 dashboard 边界。

Dashboard（`Ctrl+X`）是会话级工作区列表，和 `Ctrl+G` 的任务 pane 不同；subagent 不在 dashboard 中单独列行，但只要后台工作仍活跃，会话状态仍保持 Working。实现对齐时不要把两个入口合并成一个浮层。
