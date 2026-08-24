#![allow(clippy::needless_borrow)]

pub mod file_search {
    pub mod controller;
    pub mod line_viewer;
}
pub mod agent;
pub mod agent_panes;
pub mod dashboard;
#[path = "../../vendor/grok/xai-grok-pager/src/scrollback/blocks/tool/execute.rs"]
pub mod execute_tool;
pub mod execute_tool_adapter;
pub mod interaction;
#[path = "../../vendor/grok/xai-grok-pager/src/views/modal_window.rs"]
#[allow(dead_code)]
pub mod modal_window;
pub mod prompt_contract;
pub mod prompt_widget;
pub mod queue;
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
pub mod transcript;
pub mod workspace;
