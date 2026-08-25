#![allow(clippy::needless_borrow)]

pub mod file_search {
    pub mod controller;
    pub mod line_viewer;
}
pub mod agent;
#[path = "../../vendor/grok/xai-grok-pager/src/views/agent_hints.rs"]
pub mod agent_hints;
pub mod agent_panes;
#[path = "../../vendor/grok/xai-grok-pager/src/views/agent_status.rs"]
pub mod agent_status;
#[path = "../../vendor/grok/xai-grok-pager/src/views/context_bar.rs"]
pub mod context_bar;
pub mod dashboard;
pub mod execute_tool {
    pub use crate::scrollback::tool::execute::*;
}
pub mod execute_tool_adapter;
pub mod interaction;
#[path = "../../vendor/grok/xai-grok-pager/src/views/modal_window.rs"]
#[allow(dead_code)]
pub mod modal_window;
#[path = "../../vendor/grok/xai-grok-pager/src/views/permission_view.rs"]
pub mod permission_view;
#[path = "../../vendor/grok/xai-grok-pager/src/views/progress_bar.rs"]
pub mod progress_bar;
pub mod prompt_contract;
pub mod prompt_widget;
pub mod queue;
#[path = "../../vendor/grok/xai-grok-pager/src/views/session_picker.rs"]
pub mod session_picker;
#[path = "../../vendor/grok/xai-grok-pager/src/views/shortcuts_bar.rs"]
pub mod shortcuts_bar;

#[path = "../../vendor/grok/xai-grok-pager/src/views/picker.rs"]
#[allow(dead_code)]
pub mod picker;
#[path = "../../vendor/grok/xai-grok-pager/src/views/status_bar.rs"]
pub mod status_bar;
pub mod suggestion_controller;
#[path = "../../vendor/grok/xai-grok-pager/src/views/timeline.rs"]
pub mod timeline;
#[path = "../../vendor/grok/xai-grok-pager/src/views/turn_status.rs"]
pub mod turn_status;
pub mod welcome;
pub mod workspace;
