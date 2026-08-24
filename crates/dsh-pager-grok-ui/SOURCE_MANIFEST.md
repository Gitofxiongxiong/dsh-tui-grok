# Grok source manifest

Manifest updated: 2026-08-24 (+0800)
DSH protocol fixture revision: `TUI_PROTOCOL_VERSION=1` (checked by
`scripts/check-protocol-fixtures.py`).

The copied view/input modules below come from the local Grok Build mirror at
`19d42e35c07a9c9244f03f6df0c4c353f970d4f9` (`SOURCE_REV`
`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`). Hashes are SHA-256. The
upstream and vendored columns are separate so intentional adapter edits remain
auditable instead of being misreported as upstream drift.

| Local path | Upstream path | Upstream SHA-256 | Vendored SHA-256 | Local change |
|---|---|---|---|---|
| `vendor/grok/xai-grok-pager/src/views/picker.rs` | same | `02b0aaf8d48e0190a2d2ac730af8dd67407c1be3f7ee8fcb9a335602698064c0` | `02b0aaf8d48e0190a2d2ac730af8dd67407c1be3f7ee8fcb9a335602698064c0` | none |
| `vendor/grok/xai-grok-pager/src/views/modal_window.rs` | same | `474b2dbcd9fc2d21b47931a22aabe7919d47bba8e74d54e86a8d5229c5584886` | `474b2dbcd9fc2d21b47931a22aabe7919d47bba8e74d54e86a8d5229c5584886` | none |
| `vendor/grok/xai-grok-pager/src/views/shortcuts_bar.rs` | same | `4e31188e575757a964d207d858589dd207ca43bcdddd18ac3667445170adcab9` | `4e31188e575757a964d207d858589dd207ca43bcdddd18ac3667445170adcab9` | none |
| `vendor/grok/xai-grok-pager/src/views/status_bar.rs` | same | `b51fe18399b67498afe1a60b3caf715daba00b768bbe55ef1815fb4bd092a53c` | `b51fe18399b67498afe1a60b3caf715daba00b768bbe55ef1815fb4bd092a53c` | none |
| `vendor/grok/xai-grok-pager/src/views/agent_status.rs` | same | `0721f13b99bd3def23c92976daf674de689ef4630c70fa90ff13554f4727506d` | `c13ecda6d4a921d99594e6a2944af00d094fdaae64a9f1763869d6f1beb0fb06` | B adaptation: preserves the composable right-aligned item/separator layout and returned hit rectangles; excludes Grok task/goal runtime builders |
| `vendor/grok/xai-grok-pager/src/views/context_bar.rs` | same | `65bfc73f25a2cf4c1f81516bac5d769fdb56b9886b720d85ffa2105a33368841` | `9a9262af30303e1a496f2912c602c31e0219b2fc57562e2c926dc145117838de` | B adaptation: preserves compact token formatting, urgency gradient, fixed-width hover progress and absence semantics; uses the DSH theme and host-projected usage percentage without Grok token-estimation/quantization crates |
| `vendor/grok/xai-grok-pager/src/views/turn_status.rs` | same | `06f1826cf8455252901675aa838e91eb3d1ccf56fe608060117e14877bd4e818` | `0de12a6072f4fcfd3e749788f225bc4858a24c0f4b35e92cb0ac82937d6cadf4` | B adaptation: preserves the single-row spinner/pulsing-diamond cadence, activity/phase and turn timers, compact token count, right alignment, truncation and `[stop]` hit rectangle; consumes `TurnStatusSnapshot` and excludes Grok AgentState, MCP init, watchers, goals, background demotion and task/bash runtime dependencies |
| `vendor/grok/xai-grok-pager/src/views/progress_bar.rs` | same | `7c9d89af405d2a0094d81a235093f682de41fa53f8d4a95020901c40ce3b871f` | `881435f0768c3a2c9687543dd56299fabd8ea8ff3c0cc594cd4025e84d8da5be` | B adaptation: preserves 1/8-cell block geometry and styled spans; excludes the legacy ConHost shade-glyph branch from the modern-terminal DSH target |
| `vendor/grok/xai-grok-pager/src/views/timeline.rs` | same | `f537f9df19ed02bda47e2bf147076464a14e8f9015e35d247cb9969e0304390e` | `f537f9df19ed02bda47e2bf147076464a14e8f9015e35d247cb9969e0304390e` | none |
| `vendor/grok/xai-grok-pager/src/input/line_editor.rs` | same | `0b6fa76994d6b637442a98bc90cd5b539d13ae97dc6602befeb008bb61683d87` | `0b6fa76994d6b637442a98bc90cd5b539d13ae97dc6602befeb008bb61683d87` | none |
| `vendor/grok/xai-grok-pager/src/input/key.rs` | same | `69d6f7446bf106c33e1dc894ef095c62782dae3d609f6661ef0291046bc754ee` | `2836bde45b5b3dae4bb7ca655c2f463bd1b5066fe8496cb74e32ca1ae96c2db7` | doctest crate path uses `dsh_pager_grok_ui` |
| `vendor/grok/xai-grok-pager-render/src/modal_window_state.rs` | same | `023d33dbbacb445a6772eeb687a0e10e79bb93e50dda20ba9f55f23ab1c642df` | `023d33dbbacb445a6772eeb687a0e10e79bb93e50dda20ba9f55f23ab1c642df` | none |
| `vendor/grok/xai-grok-pager/src/views/file_search/mod.rs` | `crates/codegen/xai-grok-pager/src/views/file_search/mod.rs` | `55b60c8f7c943e93cf2c87bd56a04d6e3337b65b39053fd3fa9aea83635f2307` | `55b60c8f7c943e93cf2c87bd56a04d6e3337b65b39053fd3fa9aea83635f2307` | provenance-only source; DSH adapter in `src/views/file_search/controller.rs` |
| `vendor/grok/xai-grok-pager/src/views/suggestion_controller/mod.rs` | `crates/codegen/xai-grok-pager/src/views/suggestion_controller/mod.rs` | `f18ea878e39605513c90fd65a783e7b0b69dd0c613354e81f02054257ff98e4c` | `f18ea878e39605513c90fd65a783e7b0b69dd0c613354e81f02054257ff98e4c` | provenance-only source; DSH adapter in `src/views/suggestion_controller.rs` |
| `vendor/grok/xai-grok-pager/src/scrollback/blocks/tool/execute.rs` | `crates/codegen/xai-grok-pager/src/scrollback/blocks/tool/execute.rs` | `82f28d9e7e525331782f9287926da04a1f1e3fbc357960ccb47d397e700b6d15` | `536a359c38f2a68f4386d6435a2b6f11e15dd18b1f113fe42d1323536ce5c52a` | B adaptation: preserves ExecuteToolCallBlock/display modes/header/truncation/panel/accent/fold tests; projects out Grok AppearanceConfig, BlockContent/selection ranges, VTE and permission/syntect runtime; DSH mapping lives in `src/views/execute_tool_adapter.rs` |
| `vendor/grok/xai-grok-pager/src/views/permission_view.rs` | `crates/codegen/xai-grok-pager/src/views/permission_view.rs` | `9dbd4459126c0ece90030458b944b6b149f7a4563bdab11b314011edcb5a86f2` | `140a457e73de81575913f126525a658149f24480911eadd0cea5f3f159bbdcd1` | B adaptation: preserves composer placement, height cap, accent rail, chrome/options geometry, radio/hover/focus rows and hit rectangles; consumes host-neutral one-shot choices and excludes ACP, remembered grants, MCP/bash scope editors and syntect. DSH callId/tool projection lives in `src/views/interaction.rs` |

The textarea is reused through the Cargo alias `xai-ratatui-textarea` and is
implemented by the workspace's `dsh-grok-textarea` crate. DSH-specific shims
are outside this table and live in `src/`.

## Renderer closure inventory

The following files are the next vendor tranche. They are recorded before
copying so the source boundary is explicit and the pure renderer/interaction
closure can be reviewed separately from Grok runtime code. `planned` means the
file is intentionally not yet in the local vendor tree; it must not be treated
as a completed production capability.

| Capability | Upstream path | Upstream SHA-256 | DSH seam | Status |
|---|---|---|---|---|
| Theme | `crates/codegen/xai-grok-pager-render/src/theme/mod.rs` | `c474aafdfa8085b18e030baa2a73629cde6d26201f2493fa89a4a9c71c1f877f` | `dsh_pager_render::Theme` | integrated projection |
| Theme constructors | `crates/codegen/xai-grok-pager-render/src/theme/tokyonight.rs` | `ae006d433f652c9e6d6533e38ba07ac49ff07770a63a9510a7dc8b1ae3c09f28` | `Theme::current` | planned |
| Appearance | `crates/codegen/xai-grok-pager-render/src/appearance/config.rs` | `ef27fec032dda66aa0cf882c8bd005e5d9f368d08dfbdc4a08812af539cba744` | `LayoutConfig`/`ScrollbarConfig`/`GrokAppearanceSnapshot` | integrated DSH-neutral projection |
| Scrollbar | `crates/codegen/xai-grok-pager-render/src/render/scrollbar.rs` | `7e835584339413f9729a3dab23961946b13d65e3c8441eb8adfb56b545f64ff5` | `dsh_pager_primitives::scrollbar` + AgentView `ScrollInfo` | integrated smooth full-block renderer, follow dim and timeline-rail exclusion; pointer drag pending |
| Glyphs | `crates/codegen/xai-grok-pager-render/src/glyphs.rs` | `4662e6e4d2870dc22be3b7763b0f623969a80432a24dad37179c6eafc1e3c3e1` | `src/glyphs.rs` | turn-status braille spinner/token arrow/diamond subset integrated (`src/glyphs.rs` SHA-256 `13efa338891823d7403d72e6cfb9a7ccba81a903c76a2b4947a89b59ccc17280`); remaining registry planned |
| PromptWidget | `crates/codegen/xai-grok-pager/src/views/prompt_widget/mod.rs` | `f89c9dda0fe244dd4ab2123601cedd0fe9e2722eee2533d5a8c9c14cd4ae23cf` | `PromptStyleContract`/`PromptInfoContract` + `PromptEditor` + `UiIntent` | production draw core integrated; controller closure pending |
| PromptWidget tests | `crates/codegen/xai-grok-pager/src/views/prompt_widget/tests.rs` | `407f924051144bb685354df4940f09df717cc30d3a89144a324302f3168f1fdd` | geometry/height/cell/cursor/selection/mouse fixtures now; upstream controller interaction fixtures with controller tranche | partial |
| AgentView layout | `crates/codegen/xai-grok-pager/src/views/agent.rs` | `87ac96e07010893fd779ac8e27875410d0b87ba4d11501f1cfe9646689bcee20` | full `AgentViewLayoutParams`, pane solver, `PaneAreas`, scrollbar/timeline geometry | integrated pure solver; pane-specific renderers pending |
| AgentView render order | `crates/codegen/xai-grok-pager/src/app/agent_view/render.rs` | `87af0b2c939299e35c10a967ee5cb42ff2fba35cd1109969346ef557c0b3e643` | runtime/parity consume one `AgentViewLayout` snapshot; DSH DTO heights | partial snapshot wiring; full upstream render order pending |
| Scrollback render | `crates/codegen/xai-grok-pager/src/scrollback/render.rs` | `29fe2d148ec0feaed5acb08c590d7f4ad3cd852ec178832f4daaaf90c0e8a97f` | `RichTranscript` / block DTO | planned |
| Scrollback layout | `crates/codegen/xai-grok-pager/src/scrollback/layout.rs` | `863ad75266d7e991b299e41bc872c648ef54dba2bb023b2e7e2a01605c630c7c` | `DshRenderContent` | planned |
| Markdown blocks | `crates/codegen/xai-grok-pager/src/scrollback/blocks/markdown_content.rs` | `cc3d18620be6756344bf69f093bb5d05b825e27a47d737a4a8e915a03c9aa5ad` | `DshRenderBlock::Markdown` | planned |
| Diff blocks | `crates/codegen/xai-grok-pager/src/scrollback/blocks/tool/edit.rs` | `31f7f39a277a15151cf75d43258f6fb9967ea44025adb7db1e7dee73185f7371` | `DshRenderBlock::Diff` / `DshToolDiff` | basic DSH projection integrated; contextual hunk/gutter parity pending |
| Tool block family | `crates/codegen/xai-grok-pager/src/scrollback/blocks/tool/mod.rs` | `4ee3869c6f7f5f5485cbcf886b04ac3535b2abcef484e81031a54e0de9ac070b` | `DshToolCallView` / `DshToolResultView`; `execute_tool_adapter` | ExecuteToolCallBlock integrated as B vendor adaptation; remaining tool variants and advanced viewers pending |
| Tool verb grouping | `crates/codegen/xai-grok-pager/src/scrollback/state/verb_group.rs` | `182f5a9534b00ba263fd313611e15318a9d8318189e9bb4d1cd71ad634c9e387` | typed Harness tool kind + `ScrollbackPane` projection | integrated semantic read/search/web runs; thought/subagent buckets pending |
| Tool/group entry chrome | `crates/codegen/xai-grok-pager/src/scrollback/wrappers/entry_renderer.rs` | `184e7af597929559616c7baff365a3f447f6cbd0cbafeeaf6492fc6e6d61f72e` | transcript rail, group header, running/error accent | partial DSH-neutral projection; hover animation pending |
| File Search | `crates/codegen/xai-grok-pager/src/views/file_search/mod.rs` | `55b60c8f7c943e93cf2c87bd56a04d6e3337b65b39053fd3fa9aea83635f2307` | typed search snapshot/effect | planned |
| Suggestion | `crates/codegen/xai-grok-pager/src/views/suggestion_controller/mod.rs` | `f18ea878e39605513c90fd65a783e7b0b69dd0c613354e81f02054257ff98e4c` | `SuggestionSnapshot` | planned |
| Prompt images | `crates/codegen/xai-grok-pager-render/src/prompt_images.rs` | source available in fixed mirror | `MediaSnapshot` + attachment effect | planned |
| Workspace | `crates/codegen/xai-grok-pager/src/views/dashboard/render.rs` | `58ee1ef74da687d774ca738c8e52f5569e9586af3e0ffa61288b057449f015db` | `WorkspaceSnapshot` | planned |
| Tasks | `crates/codegen/xai-grok-pager/src/views/tasks_pane.rs` | `f609d1ef9a73b3eeee5a4b208a49b1568d800fb4b3dfd7a14a0be229588802ac` | `AgentSnapshot` | planned |
| Subagents | `crates/codegen/xai-grok-pager/src/views/subagent_catalog_pane.rs` | `cfed1c7f772534cd97894057489195c0590a3b34a525fe045b7e4c1a5f3551d4` | `AgentSnapshot` | planned |
| Agent status | `crates/codegen/xai-grok-pager/src/views/agent_status.rs` | `0721f13b99bd3def23c92976daf674de689ef4630c70fa90ff13554f4727506d` | `AgentStatusBar` + `ContextUsageSnapshot` | composable status/context core integrated; task/goal items pending |
| Context bar | `crates/codegen/xai-grok-pager/src/views/context_bar.rs` | `65bfc73f25a2cf4c1f81516bac5d769fdb56b9886b720d85ffa2105a33368841` | Harness `contextPressure` projection | integrated used/window display and fixed-width hover progress |
| Turn status | `crates/codegen/xai-grok-pager/src/views/turn_status.rs` | `06f1826cf8455252901675aa838e91eb3d1ccf56fe608060117e14877bd4e818` | `TurnStatusSnapshot` from DSH event history/context/interaction + `CancelSession` effect | integrated B subset: active turn/approval/question/timers/tokens/stop; Grok-only MCP/watchers/goals/background controls excluded |
| Permission card | `crates/codegen/xai-grok-pager/src/views/permission_view.rs` | `9dbd4459126c0ece90030458b944b6b149f7a4563bdab11b314011edcb5a86f2` | `DshInteraction::Approval` + callId-linked tool block + `RespondInteraction` | blocking approval card integrated; always-approve/grant persistence and full question view intentionally pending |

## PromptWidget dependency classification

The fixed upstream `PromptWidget` is not a single-file renderer. Its draw path
mixes pure chrome/geometry with TextArea state, file-reference completion,
history, slash/suggestion ghost text, paste chips, prompt images, terminal
overlay escapes, and Grok agent/session helpers. The production migration must
therefore preserve the following boundary instead of importing the Grok
runtime to make the original file compile:

| Upstream responsibility | Reuse class | DSH seam / disposition |
|---|---|---|
| `PromptStyle`, `PromptInfo`, height and chrome rect split | A1 | `src/views/prompt_contract.rs`; field-for-field owned projection, no drawing |
| TextArea wrap, selection, cursor, mouse, undo/redo | A0 | workspace `dsh-grok-textarea`; production draw and mouse paths call its stateful APIs |
| border/prefix/info/placeholder/cursor draw order | A1/B | `src/views/prompt_widget.rs`; production/parity shared core, old AgentView renderer removed |
| file search, history, slash and suggestion state | B | migrate controller tranches against existing typed host snapshots/effects |
| paste/image preview and terminal overlay escapes | B/C | media DTO + explicit terminal capability; no Grok attachment runtime |
| agent/session/shell/ACP/config/telemetry calls | D | excluded; project through `GrokRenderSnapshot` and `UiIntent` only |

`production draw core integrated` is deliberately not a whole-PromptWidget or
pixel-parity completion claim. File search, history/slash/suggestion, paste/image
preview and terminal-overlay controllers remain separate audited tranches; P6.1
stays open until those controller and full AgentView gates close.
