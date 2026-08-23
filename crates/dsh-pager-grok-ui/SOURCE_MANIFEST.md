# Grok source manifest

Manifest updated: 2026-08-23 (+0800)
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
| `vendor/grok/xai-grok-pager/src/views/timeline.rs` | same | `f537f9df19ed02bda47e2bf147076464a14e8f9015e35d247cb9969e0304390e` | `f537f9df19ed02bda47e2bf147076464a14e8f9015e35d247cb9969e0304390e` | none |
| `vendor/grok/xai-grok-pager/src/input/line_editor.rs` | same | `0b6fa76994d6b637442a98bc90cd5b539d13ae97dc6602befeb008bb61683d87` | `0b6fa76994d6b637442a98bc90cd5b539d13ae97dc6602befeb008bb61683d87` | none |
| `vendor/grok/xai-grok-pager/src/input/key.rs` | same | `69d6f7446bf106c33e1dc894ef095c62782dae3d609f6661ef0291046bc754ee` | `2836bde45b5b3dae4bb7ca655c2f463bd1b5066fe8496cb74e32ca1ae96c2db7` | doctest crate path uses `dsh_pager_grok_ui` |
| `vendor/grok/xai-grok-pager-render/src/modal_window_state.rs` | same | `023d33dbbacb445a6772eeb687a0e10e79bb93e50dda20ba9f55f23ab1c642df` | `023d33dbbacb445a6772eeb687a0e10e79bb93e50dda20ba9f55f23ab1c642df` | none |

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
| Glyphs | `crates/codegen/xai-grok-pager-render/src/glyphs.rs` | `4662e6e4d2870dc22be3b7763b0f623969a80432a24dad37179c6eafc1e3c3e1` | `src/glyphs.rs` | planned |
| PromptWidget | `crates/codegen/xai-grok-pager/src/views/prompt_widget/mod.rs` | `f89c9dda0fe244dd4ab2123601cedd0fe9e2722eee2533d5a8c9c14cd4ae23cf` | `PromptStyleContract`/`PromptInfoContract` + `PromptEditor` + `UiIntent` | production draw core integrated; controller closure pending |
| PromptWidget tests | `crates/codegen/xai-grok-pager/src/views/prompt_widget/tests.rs` | `407f924051144bb685354df4940f09df717cc30d3a89144a324302f3168f1fdd` | geometry/height/cell/cursor/selection/mouse fixtures now; upstream controller interaction fixtures with controller tranche | partial |
| AgentView layout | `crates/codegen/xai-grok-pager/src/views/agent.rs` | `87ac96e07010893fd779ac8e27875410d0b87ba4d11501f1cfe9646689bcee20` | full `AgentViewLayoutParams`, pane solver, `PaneAreas`, scrollbar/timeline geometry | integrated pure solver; pane-specific renderers pending |
| AgentView render order | `crates/codegen/xai-grok-pager/src/app/agent_view/render.rs` | `87af0b2c939299e35c10a967ee5cb42ff2fba35cd1109969346ef557c0b3e643` | runtime/parity consume one `AgentViewLayout` snapshot; DSH DTO heights | partial snapshot wiring; full upstream render order pending |
| Scrollback render | `crates/codegen/xai-grok-pager/src/scrollback/render.rs` | `29fe2d148ec0feaed5acb08c590d7f4ad3cd852ec178832f4daaaf90c0e8a97f` | `RichTranscript` / block DTO | planned |
| Scrollback layout | `crates/codegen/xai-grok-pager/src/scrollback/layout.rs` | `863ad75266d7e991b299e41bc872c648ef54dba2bb023b2e7e2a01605c630c7c` | `DshRenderContent` | planned |
| Markdown blocks | `crates/codegen/xai-grok-pager/src/scrollback/blocks/markdown_content.rs` | `cc3d18620be6756344bf69f093bb5d05b825e27a47d737a4a8e915a03c9aa5ad` | `DshRenderBlock::Markdown` | planned |
| Diff blocks | `crates/codegen/xai-grok-pager/src/scrollback/blocks/tool/edit.rs` | `31f7f39a277a15151cf75d43258f6fb9967ea44025adb7db1e7dee73185f7371` | `DshRenderBlock::Diff` | planned |
| File Search | `crates/codegen/xai-grok-pager/src/views/file_search/mod.rs` | `55b60c8f7c943e93cf2c87bd56a04d6e3337b65b39053fd3fa9aea83635f2307` | typed search snapshot/effect | planned |
| Suggestion | `crates/codegen/xai-grok-pager/src/views/suggestion_controller/mod.rs` | `f18ea878e39605513c90fd65a783e7b0b69dd0c613354e81f02054257ff98e4c` | `SuggestionSnapshot` | planned |
| Prompt images | `crates/codegen/xai-grok-pager-render/src/prompt_images.rs` | source available in fixed mirror | `MediaSnapshot` + attachment effect | planned |
| Workspace | `crates/codegen/xai-grok-pager/src/views/dashboard/render.rs` | `58ee1ef74da687d774ca738c8e52f5569e9586af3e0ffa61288b057449f015db` | `WorkspaceSnapshot` | planned |
| Tasks | `crates/codegen/xai-grok-pager/src/views/tasks_pane.rs` | `f609d1ef9a73b3eeee5a4b208a49b1568d800fb4b3dfd7a14a0be229588802ac` | `AgentSnapshot` | planned |
| Subagents | `crates/codegen/xai-grok-pager/src/views/subagent_catalog_pane.rs` | `cfed1c7f772534cd97894057489195c0590a3b34a525fe045b7e4c1a5f3551d4` | `AgentSnapshot` | planned |
| Agent status | `crates/codegen/xai-grok-pager/src/views/agent_status.rs` | `0721f13b99bd3def23c92976daf674de689ef4630c70fa90ff13554f4727506d` | status/task DTO | planned |
| Turn status | `crates/codegen/xai-grok-pager/src/views/turn_status.rs` | `06f1826cf8455252901675aa838e91eb3d1ccf56fe608060117e14877bd4e818` | streaming/interrupt DTO | planned |

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
