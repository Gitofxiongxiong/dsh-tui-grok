# File Search effect and projection

时间：2026-08-23 14:30:18 +0800

## 目标

把 File Search 从仅有 overlay 的 UI 状态推进到可审计的查询 effect 和权威
snapshot projection，同时保留 Harness 尚未提供 filesystem discovery RPC 时的
明确降级。

## 本批范围

- 新增 `UiIntent::FileSearchQuery` / `UiEffect::FileSearchQuery`，携带 query
  revision 和稳定 operation identity。
- file-search 文本编辑、粘贴和 Ctrl-U 清空统一提交 effect；receipt 只表示
  admission/diagnostic，不把结果当成成功。
- host adapter 读取 `fileSearch` / `file_search` projection，支持 available、
  pending、unsupported、revision、stable row id、path/line/snippet。
- 当前 Harness TUI protocol 没有暴露 `fileReferences/list`，effect sink 明确
  返回 unsupported；没有调用未登记 RPC，也没有把 `session.search` 冒充文件搜索。

## 验证

- `cargo test -p dsh-pager-grok-ui --locked`：197 tests passed。

## 下一批

确认并接通 Harness 的 file-reference remote/TUI method，随后将其结果投影为
`fileSearch` snapshot；继续接入 media preview bytes/effect 和 workspace mutation
receipt/convergence。
