//! Grok-derived, renderer-neutral scrollback modules.

#[path = "../vendor/grok/xai-grok-pager/src/scrollback/blocks/agent.rs"]
pub mod agent;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/wrappers/block_renderer.rs"]
pub mod block_renderer;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/wrappers/entry_renderer.rs"]
pub mod entry_renderer;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/state/groups.rs"]
pub mod groups;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/layout.rs"]
pub mod layout;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/blocks/thinking.rs"]
pub mod thinking;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/blocks/tool/mod.rs"]
pub mod tool;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/types.rs"]
pub mod types;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/blocks/user.rs"]
pub mod user;
#[path = "../vendor/grok/xai-grok-pager/src/scrollback/state/verb_group.rs"]
pub mod verb_group;

pub mod state {
    pub use super::{groups, verb_group};
}
