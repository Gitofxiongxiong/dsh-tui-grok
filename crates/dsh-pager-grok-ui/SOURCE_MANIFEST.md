# Grok source manifest

The copied view/input modules below come from the local Grok Build mirror at
`19d42e35c07a9c9244f03f6df0c4c353f970d4f9` (`SOURCE_REV`
`7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`). Hashes are SHA-256.

| Local path | Upstream path | SHA-256 | Local change |
|---|---|---|---|
| `vendor/grok/xai-grok-pager/src/views/picker.rs` | same | `02b0aaf8d48e0190a2d2ac730af8dd67407c1be3f7ee8fcb9a335602698064c0` | none |
| `vendor/grok/xai-grok-pager/src/views/modal_window.rs` | same | `474b2dbcd9fc2d21b47931a22aabe7919d47bba8e74d54e86a8d5229c5584886` | none |
| `vendor/grok/xai-grok-pager/src/views/shortcuts_bar.rs` | same | `4e31188e575757a964d207d858589dd207ca43bcdddd18ac3667445170adcab9` | none |
| `vendor/grok/xai-grok-pager/src/views/status_bar.rs` | same | `b51fe18399b67498afe1a60b3caf715daba00b768bbe55ef1815fb4bd092a53c` | none |
| `vendor/grok/xai-grok-pager/src/views/timeline.rs` | same | `f537f9df19ed02bda47e2bf147076464a14e8f9015e35d247cb9969e0304390e` | none |
| `vendor/grok/xai-grok-pager/src/input/line_editor.rs` | same | `0b6fa76994d6b637442a98bc90cd5b539d13ae97dc6602befeb008bb61683d87` | none |
| `vendor/grok/xai-grok-pager/src/input/key.rs` | same | `2836bde45b5b3dae4bb7ca655c2f463bd1b5066fe8496cb74e32ca1ae96c2db7` | doctest crate path uses `dsh_pager_grok_ui` |
| `vendor/grok/xai-grok-pager-render/src/modal_window_state.rs` | same | `023d33dbbacb445a6772eeb687a0e10e79bb93e50dda20ba9f55f23ab1c642df` | none |

The textarea is reused through the Cargo alias `xai-ratatui-textarea` and is
implemented by the workspace's `dsh-grok-textarea` crate. DSH-specific shims
are outside this table and live in `src/`.
