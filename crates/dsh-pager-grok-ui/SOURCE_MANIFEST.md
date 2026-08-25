# Grok source manifest

Manifest updated: 2026-08-25 (+0800)
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
| `vendor/grok/xai-grok-pager/src/views/session_picker.rs` | `crates/codegen/xai-grok-pager/src/views/session_picker.rs` | `66f29ddd0236b827c8871fc9ac06c0d5adfee51a95379ccec01045ddec9d8e84` | `228203973c867925e1c08850e8a1e252f6eed5c28f01db26bcb926355b62d371` | B native-only adaptation: preserves repo grouping/current-repo pinning, stable selection, search/expanded-card mapping, fixed-width recency, modal title/sizing/search-divider/footer and Grok picker input/render composition; also extracts the SessionPicker branches of `app/modals.rs` (`51e778f3e657e53b57b46cfa81a3c1a80751b393564dadda6a09987737e4722b`); replaces Grok AppView/search/runtime types with owned DTO + revision completion seam and excludes foreign sources, filter/delete/worktree/direct UUID behavior |
| `vendor/grok/xai-grok-pager/src/slash/commands/resume.rs` | `crates/codegen/xai-grok-pager/src/slash/commands/resume.rs` | `a710b4c80c3858ab6d853bc62eb7fd81e901fe2a48e679f4d88e68209ae32ba2` | `38fbff6801d153ec64e887bad0e6c3f55cceee23a3b4d8b19c5f5a60b5740583` | A1: keeps name/description/usage/ShowSessionPicker action; imports the local minimal slash seam and omits Grok CommandExecCtx |
| `vendor/grok/xai-grok-pager/src/views/shortcuts_bar.rs` | same | `4e31188e575757a964d207d858589dd207ca43bcdddd18ac3667445170adcab9` | `4e31188e575757a964d207d858589dd207ca43bcdddd18ac3667445170adcab9` | none |
| `vendor/grok/xai-grok-pager/src/actions/mod.rs` | `crates/codegen/xai-grok-pager/src/actions/mod.rs` | `32a0a4b3602133171a79351a364aebf72c4c050bd633496e73889998325cbed8` | `8a08a2944d6ecd789d5c26d691a00cbf63325fa84ca2c0e006cbf8301d2d8768` | B adaptation: keeps ActionId/When/ActionRegistry lookup/hints/tests; `log_shortcut_used` is a no-op because Grok telemetry runtime is excluded |
| `vendor/grok/xai-grok-pager/src/actions/defaults.rs` | `crates/codegen/xai-grok-pager/src/actions/defaults.rs` | `353a000b63872fdcb77cd663fff2d12cdb7dffb5f8d4704c7e350fa8052ee09b` | `353a000b63872fdcb77cd663fff2d12cdb7dffb5f8d4704c7e350fa8052ee09b` | none |
| `vendor/grok/xai-grok-pager/src/views/agent_hints.rs` | `crates/codegen/xai-grok-pager/src/views/agent.rs` | `87ac96e07010893fd779ac8e27875410d0b87ba4d11501f1cfe9646689bcee20` | `188e33bb9b2ae17257705b140cf22b04bc452ed61340d3e8023f53d66581ab75` | B extract: `ActivePane` / `prompt_focus_hint` / `build_hints` and grok hint tests; layout stays in `src/views/agent.rs`; `PromptWidget` is a same-method composer seam |
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
| `vendor/grok/xai-grok-pager/src/views/permission_view.rs` | `crates/codegen/xai-grok-pager/src/views/permission_view.rs` | `9dbd4459126c0ece90030458b944b6b149f7a4563bdab11b314011edcb5a86f2` | `e1353d0589fd3a8eb238f4acc0661ab0ad0ff70e56d55129e8689478902c2d89` | B adaptation: preserves composer placement, height cap, accent rail, chrome/options geometry, radio/hover/focus rows and hit rectangles; host-neutral choices include DSH `DontAskAgain` (maps to allowed-once + danger-full-access). DSH callId/tool projection lives in `src/views/interaction.rs` |
| `vendor/grok/xai-grok-markdown-core/Cargo.toml` | `xai-grok-markdown-core/Cargo.toml` | `67d0718e559d810afe744f7488c0ac7c5b8e197cfed9b3f4d764bd335b718495` | `cfb556e778fdc784ebeed12b0dd7736f8c43a3e2b338cf19f49309783395f901` | A1: explicit edition and local unexpected-cfg lint replace upstream workspace inheritance |
| `vendor/grok/xai-grok-markdown-core/src/lib.rs` | `xai-grok-markdown-core/src/lib.rs` | `88acd29b890dbd928865992f2109001e7a38b312f11bffda6088d51772bd1846` | `88acd29b890dbd928865992f2109001e7a38b312f11bffda6088d51772bd1846` | none |
| `vendor/grok/xai-grok-markdown/Cargo.toml` | `xai-grok-markdown/Cargo.toml` | `62466412e89f2b2f50576b84fc3cca9fb3b99e26b5cdd867e6147e401ef33937` | `9582637e02764602158310e5c5c4c55d66f58cc990096af21eba90c7ed3adf9a` | A1: explicit edition; excludes unvendored playground binaries/textarea feature and benchmark target while retaining production dependencies and upstream tests |
| `vendor/grok/xai-grok-markdown/assets/tokyo-night.tmTheme` | `xai-grok-markdown/assets/tokyo-night.tmTheme` | `6713e3ab9dab57033b806be71122a640e745e5c66fb1445b20909374f7e48c7f` | `6713e3ab9dab57033b806be71122a640e745e5c66fb1445b20909374f7e48c7f` | none |
| `vendor/grok/xai-grok-markdown/src/buffers.rs` | `xai-grok-markdown/src/buffers.rs` | `4bf44049a5bf5af1f8d93ecb60dc427ab6721c0b02a3d383c972e3e8c8b923a5` | `4bf44049a5bf5af1f8d93ecb60dc427ab6721c0b02a3d383c972e3e8c8b923a5` | none |
| `vendor/grok/xai-grok-markdown/src/checkpoint.rs` | `xai-grok-markdown/src/checkpoint.rs` | `33f25322a362068016cb3c4905a4c2f4f20ead81f60bddbad4897d5928ce4042` | `33f25322a362068016cb3c4905a4c2f4f20ead81f60bddbad4897d5928ce4042` | none |
| `vendor/grok/xai-grok-markdown/src/colors.rs` | `xai-grok-markdown/src/colors.rs` | `a36b881159908deff20efd04a24fee05ebac57acf07d2ca9f0a47008479c65ea` | `a36b881159908deff20efd04a24fee05ebac57acf07d2ca9f0a47008479c65ea` | none |
| `vendor/grok/xai-grok-markdown/src/hyperlinks.rs` | `xai-grok-markdown/src/hyperlinks.rs` | `953ce3bdc1b7a5fa30d391932112d8c9cf4eaa6ab6d9ab58b493e1c0becb841f` | `953ce3bdc1b7a5fa30d391932112d8c9cf4eaa6ab6d9ab58b493e1c0becb841f` | none |
| `vendor/grok/xai-grok-markdown/src/latex/commands.rs` | `xai-grok-markdown/src/latex/commands.rs` | `c9682c707d250c615dcc55637966ea39ecfcb27ba6ea5a6ef27d90012a6902de` | `c9682c707d250c615dcc55637966ea39ecfcb27ba6ea5a6ef27d90012a6902de` | none |
| `vendor/grok/xai-grok-markdown/src/latex/cursor.rs` | `xai-grok-markdown/src/latex/cursor.rs` | `7c842fe3651c232f17cd3de644db72a1fd7b84337f0e1b668902f6136be0b8ae` | `7c842fe3651c232f17cd3de644db72a1fd7b84337f0e1b668902f6136be0b8ae` | none |
| `vendor/grok/xai-grok-markdown/src/latex/environments.rs` | `xai-grok-markdown/src/latex/environments.rs` | `e1e1d3300b773880aff1f0b089c0be5c96e479773989f89b5a4b733fcd9ddd4d` | `e1e1d3300b773880aff1f0b089c0be5c96e479773989f89b5a4b733fcd9ddd4d` | none |
| `vendor/grok/xai-grok-markdown/src/latex/math_box.rs` | `xai-grok-markdown/src/latex/math_box.rs` | `8496ae705c4adfefa5724a46d7d643430dcab5d883f5199a4246f18c4828c80f` | `8496ae705c4adfefa5724a46d7d643430dcab5d883f5199a4246f18c4828c80f` | none |
| `vendor/grok/xai-grok-markdown/src/latex/mod.rs` | `xai-grok-markdown/src/latex/mod.rs` | `8d9e1c799222e7a140e16f248f21c63c8050bc751f11264d6711a9d2f1b195ec` | `8d9e1c799222e7a140e16f248f21c63c8050bc751f11264d6711a9d2f1b195ec` | none |
| `vendor/grok/xai-grok-markdown/src/latex/symbols.rs` | `xai-grok-markdown/src/latex/symbols.rs` | `4bd41b7184d0af850f3c9b7968cfe73d997e873a2eba4d2cee1364f37f2706bf` | `4bd41b7184d0af850f3c9b7968cfe73d997e873a2eba4d2cee1364f37f2706bf` | none |
| `vendor/grok/xai-grok-markdown/src/latex/tests.rs` | `xai-grok-markdown/src/latex/tests.rs` | `86cbef3fa1d0d62fc4206538e65acb091d36ae61b8f685a0f758660211a3bbf7` | `86cbef3fa1d0d62fc4206538e65acb091d36ae61b8f685a0f758660211a3bbf7` | none |
| `vendor/grok/xai-grok-markdown/src/latex_delimiters.rs` | `xai-grok-markdown/src/latex_delimiters.rs` | `c457ad50293b8d1f6b29910c60d3c4bfcb50df7d222494677a5f66bbe0a1deb0` | `c457ad50293b8d1f6b29910c60d3c4bfcb50df7d222494677a5f66bbe0a1deb0` | none |
| `vendor/grok/xai-grok-markdown/src/lib.rs` | `xai-grok-markdown/src/lib.rs` | `d44ad0fdbfd5c94d822ca13f58dbf75c04005d1c7ee32d4b920da486aba163ff` | `d44ad0fdbfd5c94d822ca13f58dbf75c04005d1c7ee32d4b920da486aba163ff` | none |
| `vendor/grok/xai-grok-markdown/src/mermaid.rs` | `xai-grok-markdown/src/mermaid.rs` | `1dc29bca611bedf023b9ebaca76a3a1b7ab8d1208f95c2b58b1a5edf854f9c15` | `1dc29bca611bedf023b9ebaca76a3a1b7ab8d1208f95c2b58b1a5edf854f9c15` | none |
| `vendor/grok/xai-grok-markdown/src/open_code_highlighter.rs` | `xai-grok-markdown/src/open_code_highlighter.rs` | `218cdcc1c366e593acc1d76d05de330e344c3f77e6bc362768a04ebbbbddf645` | `218cdcc1c366e593acc1d76d05de330e344c3f77e6bc362768a04ebbbbddf645` | none |
| `vendor/grok/xai-grok-markdown/src/output.rs` | `xai-grok-markdown/src/output.rs` | `d9a37de3c2a1ad7e7831b04283ffd110c48e146fbb5882b1f43d7bce29219efc` | `d9a37de3c2a1ad7e7831b04283ffd110c48e146fbb5882b1f43d7bce29219efc` | none |
| `vendor/grok/xai-grok-markdown/src/parse.rs` | `xai-grok-markdown/src/parse.rs` | `00b0afca9aea47a319df01dbb040a23e1556d0e664275067ce7eb71f8e3ff6a2` | `00b0afca9aea47a319df01dbb040a23e1556d0e664275067ce7eb71f8e3ff6a2` | none |
| `vendor/grok/xai-grok-markdown/src/render.rs` | `xai-grok-markdown/src/render.rs` | `e91474bb50be0d53e09dcba4efb369af1c7ebc7d7d5767528f7a38a46d8e4d46` | `e91474bb50be0d53e09dcba4efb369af1c7ebc7d7d5767528f7a38a46d8e4d46` | none |
| `vendor/grok/xai-grok-markdown/src/source_map.rs` | `xai-grok-markdown/src/source_map.rs` | `b75936609a16966dab21a2079bbfdc9a58363a4f4761db69b5a90a441dce6660` | `b75936609a16966dab21a2079bbfdc9a58363a4f4761db69b5a90a441dce6660` | none |
| `vendor/grok/xai-grok-markdown/src/streaming.rs` | `xai-grok-markdown/src/streaming.rs` | `1c8d56722a359999736b62e569598fccd45fb81fe1ae551f62a9886fed662c74` | `1c8d56722a359999736b62e569598fccd45fb81fe1ae551f62a9886fed662c74` | none |
| `vendor/grok/xai-grok-markdown/src/style.rs` | `xai-grok-markdown/src/style.rs` | `b5558973bc53372eb68739cd1261bb49b3c71e94edc4d28f1e5572b7fe91b62a` | `b5558973bc53372eb68739cd1261bb49b3c71e94edc4d28f1e5572b7fe91b62a` | none |
| `vendor/grok/xai-grok-markdown/src/syntax.rs` | `xai-grok-markdown/src/syntax.rs` | `efc03d1898be7bc069b2bbb5dcfeb040cfe89f66bf63a5659e53994ea22e47c1` | `efc03d1898be7bc069b2bbb5dcfeb040cfe89f66bf63a5659e53994ea22e47c1` | none |
| `vendor/grok/xai-grok-markdown/src/url_scan.rs` | `xai-grok-markdown/src/url_scan.rs` | `923bf6ef79c2e78d11490f0bdb07b6832a3de304db501033abf07ed2e38853fe` | `923bf6ef79c2e78d11490f0bdb07b6832a3de304db501033abf07ed2e38853fe` | none |
| `src/render/markdown.rs` | `xai-grok-pager-render/src/theme/md_style.rs` | `54c318aaa9946fe40f4a65bd0d2ca73c041c0303f36f09a402512bb32a36b80b` | `a31488098392c7bedcf6a229efb1ffa59d0a1828c34b16b69be18fe742bd3f1e` | B adaptation: retains upstream ratatui→anstyle theme mapping and tests; adds the DSH-neutral complete-document render seam, width-constrained tables, prefix projection and semantic spacing/table regression tests |

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
| Glyphs | `crates/codegen/xai-grok-pager-render/src/glyphs.rs` | `4662e6e4d2870dc22be3b7763b0f623969a80432a24dad37179c6eafc1e3c3e1` | `src/glyphs.rs` | turn-status braille spinner/token arrow/diamond and vertically centered `disclosure_open` subset integrated (`src/glyphs.rs` SHA-256 `7d670d3a4a667b15c8cdcbdf8de5018e01c28403a03cccf3dd84bf651a02358f`); remaining registry planned |
| PromptWidget | `crates/codegen/xai-grok-pager/src/views/prompt_widget/mod.rs` | `f89c9dda0fe244dd4ab2123601cedd0fe9e2722eee2533d5a8c9c14cd4ae23cf` | `PromptStyleContract`/`PromptInfoContract` + `PromptEditor` + `UiIntent` | production draw core integrated; controller closure pending |
| PromptWidget tests | `crates/codegen/xai-grok-pager/src/views/prompt_widget/tests.rs` | `407f924051144bb685354df4940f09df717cc30d3a89144a324302f3168f1fdd` | geometry/height/cell/cursor/selection/mouse fixtures now; upstream controller interaction fixtures with controller tranche | partial |
| AgentView layout | `crates/codegen/xai-grok-pager/src/views/agent.rs` | `87ac96e07010893fd779ac8e27875410d0b87ba4d11501f1cfe9646689bcee20` | full `AgentViewLayoutParams`, pane solver, `PaneAreas`, scrollbar/timeline geometry | integrated pure solver; pane-specific renderers pending |
| AgentView render order | `crates/codegen/xai-grok-pager/src/app/agent_view/render.rs` | `87af0b2c939299e35c10a967ee5cb42ff2fba35cd1109969346ef557c0b3e643` | runtime/parity consume one `AgentViewLayout` snapshot; DSH DTO heights | pane shortcuts bar now uses Grok `build_hints` + `compact(5, ShortcutsHelp)`; remaining overlay/modal render order pending |
| Scrollback render | `crates/codegen/xai-grok-pager/src/scrollback/render.rs` | `29fe2d148ec0feaed5acb08c590d7f4ad3cd852ec178832f4daaaf90c0e8a97f` | `RichTranscript` / block DTO | planned |
| Scrollback layout | `crates/codegen/xai-grok-pager/src/scrollback/layout.rs` | `863ad75266d7e991b299e41bc872c648ef54dba2bb023b2e7e2a01605c630c7c` | `DshRenderContent` | planned |
| Markdown blocks | `crates/codegen/xai-grok-pager/src/scrollback/blocks/markdown_content.rs` | `cc3d18620be6756344bf69f093bb5d05b825e27a47d737a4a8e915a03c9aa5ad` | `DshRenderBlock::Markdown` | pure `xai-grok-markdown` renderer and Grok theme projection integrated through `src/render/markdown.rs`; upstream `MarkdownContent` streaming cache/raw overlay remain planned |
| Diff blocks | `crates/codegen/xai-grok-pager/src/scrollback/blocks/tool/edit.rs` | `31f7f39a277a15151cf75d43258f6fb9967ea44025adb7db1e7dee73185f7371` | `DshRenderBlock::Diff` / `DshToolDiff` | basic DSH projection integrated; contextual hunk/gutter parity pending |
| Tool block family | `crates/codegen/xai-grok-pager/src/scrollback/blocks/tool/mod.rs` | `4ee3869c6f7f5f5485cbcf886b04ac3535b2abcef484e81031a54e0de9ac070b` | `DshToolCallView` / `DshToolResultView`; `execute_tool_adapter` | ExecuteToolCallBlock integrated as B vendor adaptation; remaining tool variants and advanced viewers pending |
| Tool verb grouping | `crates/codegen/xai-grok-pager/src/scrollback/state/verb_group.rs` | `182f5a9534b00ba263fd313611e15318a9d8318189e9bb4d1cd71ad634c9e387` | typed Harness tool kind + `ScrollbackPane` projection | integrated semantic read/search/web runs; thought/subagent buckets pending |
| Tool/group entry chrome | `crates/codegen/xai-grok-pager/src/scrollback/wrappers/entry_renderer.rs` | `184e7af597929559616c7baff365a3f447f6cbd0cbafeeaf6492fc6e6d61f72e` | transcript rail, group header, running/error accent | partial DSH-neutral projection; hover animation pending |
| File Search | `crates/codegen/xai-grok-pager/src/views/file_search/mod.rs` | `55b60c8f7c943e93cf2c87bd56a04d6e3337b65b39053fd3fa9aea83635f2307` | typed search snapshot/effect | planned |
| Suggestion | `crates/codegen/xai-grok-pager/src/views/suggestion_controller/mod.rs` | `f18ea878e39605513c90fd65a783e7b0b69dd0c613354e81f02054257ff98e4c` | `SuggestionSnapshot` | planned |
| Prompt images | `crates/codegen/xai-grok-pager-render/src/prompt_images.rs` | source available in fixed mirror | `MediaSnapshot` + attachment effect | planned |
| Resume session picker | `crates/codegen/xai-grok-pager/src/views/session_picker.rs` + SessionPicker branches in `app/modals.rs` | `66f29ddd0236b827c8871fc9ac06c0d5adfee51a95379ccec01045ddec9d8e84` / `51e778f3e657e53b57b46cfa81a3c1a80751b393564dadda6a09987737e4722b` | `SessionPickerEntry` / `SessionSearchHit` + list/search effects + DSH attach barrier | integrated native-only B closure; foreign/delete/worktree/direct UUID excluded |
| Workspace | `crates/codegen/xai-grok-pager/src/views/dashboard/render.rs` | `58ee1ef74da687d774ca738c8e52f5569e9586af3e0ffa61288b057449f015db` | `WorkspaceSnapshot` | planned |
| Tasks | `crates/codegen/xai-grok-pager/src/views/tasks_pane.rs` | `f609d1ef9a73b3eeee5a4b208a49b1568d800fb4b3dfd7a14a0be229588802ac` | `AgentSnapshot` | planned |
| Subagents | `crates/codegen/xai-grok-pager/src/views/subagent_catalog_pane.rs` | `cfed1c7f772534cd97894057489195c0590a3b34a525fe045b7e4c1a5f3551d4` | `AgentSnapshot` | planned |
| Agent status | `crates/codegen/xai-grok-pager/src/views/agent_status.rs` | `0721f13b99bd3def23c92976daf674de689ef4630c70fa90ff13554f4727506d` | `AgentStatusBar` + `ContextUsageSnapshot` | composable status/context core integrated; task/goal items pending |
| Context bar | `crates/codegen/xai-grok-pager/src/views/context_bar.rs` | `65bfc73f25a2cf4c1f81516bac5d769fdb56b9886b720d85ffa2105a33368841` | Harness `contextPressure` projection | integrated used/window display and fixed-width hover progress |
| Turn status | `crates/codegen/xai-grok-pager/src/views/turn_status.rs` | `06f1826cf8455252901675aa838e91eb3d1ccf56fe608060117e14877bd4e818` | `TurnStatusSnapshot` from DSH event history/context/interaction + `CancelSession` effect | integrated B subset: active turn/approval/question/timers/tokens/stop; Grok-only MCP/watchers/goals/background controls excluded |
| Permission card | `crates/codegen/xai-grok-pager/src/views/permission_view.rs` | `9dbd4459126c0ece90030458b944b6b149f7a4563bdab11b314011edcb5a86f2` | `DshInteraction::Approval` + callId-linked tool block + `RespondInteraction` + `SetSessionMode` | blocking approval card integrated; normal-mode `DontAskAgain` allows once and switches the session to `danger-full-access`; Grok grant persistence still excluded |

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
