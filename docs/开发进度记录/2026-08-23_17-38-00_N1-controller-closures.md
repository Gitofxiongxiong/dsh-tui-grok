# N1 File Search、Suggestion controller 闭包

> 记录时间：2026-08-23 17:38:00 +0800
> 操作者/agent：Codex
> 状态：completed

## 目标与背景

N1-0 已闭合 effect identity 和 pending RPC boundary；本批把当前 runtime 中的
`Vec`/`usize` suggestion 与散落 File Search selection 分支收敛到可测试 controller。
固定 Grok source 只提供视觉/交互规则，DSH 继续拥有候选、revision、stable id 和
capability truth。

## 设计契约和复用依据

- 固定 Grok mirror commit `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`，SOURCE_REV
  `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`。
- provenance vendor：Grok `file_search/mod.rs` 的 shared styled-ref seam，和
  `suggestion_controller/mod.rs` 的 generation/selection/accept semantics；不引入
  fuzzy daemon、ACP、Grok session/config/runtime。
- DSH adapter：File Search controller 只消费 `FileSearchSnapshot`，按 query
  revision 和 row stable id 丢弃 stale result；Suggestion controller 只消费
  `SuggestionSnapshot`，候选过滤/viewport/accept 不能发 RPC。
- View event -> controller outcome -> runtime `UiIntent`/local draft；Enter/Tab
  成功前不清理草稿，Esc 遵循 overlay/dropdown/draft 阶梯。

## 计划修改范围

- `vendor/grok/xai-grok-pager/src/views/file_search/mod.rs`：固定 source provenance。
- `vendor/grok/xai-grok-pager/src/views/suggestion_controller/mod.rs`：固定 source
  provenance（只 vendor pure controller source，不编译 Grok runtime）。
- `crates/dsh-pager-grok-ui/src/views/file_search/controller.rs`：DSH File Search
  revision/stable-id/selection controller 和 unit tests。
- `crates/dsh-pager-grok-ui/src/views/suggestion_controller.rs`：DSH slash/history
  candidate controller、viewport、accept/dismiss 和 unit tests。
- `crates/dsh-pager-grok-ui/src/views/mod.rs`：注册 controller modules。
- `crates/dsh-pager-grok-ui/src/runtime.rs`：生产路径改用两个 controller，移除
  suggestion 的散落 selection/dismiss 判断和 File Search 的重复 selection state。
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md`：新增 vendor hash 行。
- 本记录文件：收尾回写结果。

## 风险、回滚和依赖

- 风险：controller 接线可能改变旧 semantic snapshots；先保留 renderer chrome，
  用现有 runtime tests + controller tests 锁定行为。
- 风险：host rows 可能重排或 stale；所有 action target 只能使用 row id，不能使用
  数组 index。
- 回滚：回退本记录单一 commit，恢复上一批 runtime controller shim；不影响
  transport/effect commit。
- 依赖：`1a42574` File Search DTO contract、`290a182` pending executor。

## 实际修改

- exact-vendor 固定 Grok `file_search/mod.rs` 与
  `suggestion_controller/mod.rs`，manifest 新增可校验 upstream/vendor SHA-256；
  vendor source 不进入编译，不引入 fuzzy daemon/ACP/config/runtime。
- 新增 `FileSearchController`：query 开始即产生 typed `Pending` snapshot，
  completion 必须命中当前 revision；selection 以 row stable id 保存，重排不漂移、
  删除后清空。
- 新增 `SuggestionController`：只消费 host `SuggestionSnapshot`，统一 slash filter、
  selected viewport、Tab/Enter accept、Esc dismiss 和 text generation invalidation。
- runtime 删除 `file_search_selected_id/revision/result` 与
  `suggestion_selected/dismissed` 平行字段，production render/input 全部改走 controller。
- 修正 filtered suggestion rows 高度原来按未过滤总数计算的问题；异步 prompt 只在
  accepted completion 且 draft 仍匹配时清空并记录 history。

## 验证结果

- `cargo fmt --all -- --check`
- `cargo clippy -p dsh-pager-grok-ui --all-targets --all-features -- -D warnings`
- `cargo test -p dsh-pager-grok-ui --lib`
- `python3 scripts/check-source-manifest.py`
- `git diff --check`
- 结果：UI 226 tests 通过；source manifest 10 rows、0 drift；fmt/clippy/diff
  check 通过。

## Git 提交

- commit message：`feat: migrate N1 file and suggestion controllers`
- Progress-Record trailer：
  `Progress-Record: docs/开发进度记录/2026-08-23_17-38-00_N1-controller-closures.md`

## 未解决问题和下一步

- Image 的上游 prompt-image renderer 和 terminal image escape 仍待 N1 后续批次；
  当前 media preview 仍保持 typed metadata/explicit fallback。
- N2 仍需迁移 Grok ScrollbackPane/block layout；本批不删除 `RichTranscript`。
