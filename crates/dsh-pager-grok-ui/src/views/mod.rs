#![allow(clippy::needless_borrow)]

pub mod file_search {
    pub mod line_viewer;
}
pub mod interaction;
#[path = "../../vendor/grok/xai-grok-pager/src/views/modal_window.rs"]
#[allow(dead_code)]
pub mod modal_window;
pub mod queue;
#[path = "../../vendor/grok/xai-grok-pager/src/views/shortcuts_bar.rs"]
pub mod shortcuts_bar;

#[path = "../../vendor/grok/xai-grok-pager/src/views/picker.rs"]
#[allow(dead_code)]
pub mod picker;
#[path = "../../vendor/grok/xai-grok-pager/src/views/status_bar.rs"]
pub mod status_bar;
#[path = "../../vendor/grok/xai-grok-pager/src/views/timeline.rs"]
pub mod timeline;
pub mod transcript;
