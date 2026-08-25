//! Viewport-window math adapted from Grok's scrollback renderer.
//!
//! DSH keeps storage and height indexing in the host crate. This module owns
//! only the renderer-side interpretation of that host window: the absolute
//! entry range, its virtual origin, and the clipped rows to skip.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderWindow {
    pub entries: Range<usize>,
    pub content_y0: usize,
    pub skip_rows: usize,
    pub total_height: usize,
}

impl RenderWindow {
    pub fn new(
        entries: Range<usize>,
        content_y0: usize,
        total_height: usize,
        scroll_top: usize,
    ) -> Self {
        Self {
            entries,
            content_y0,
            skip_rows: scroll_top.saturating_sub(content_y0),
            total_height,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_window_keeps_absolute_entry_base_and_skip_rows() {
        let window = RenderWindow::new(40..47, 120, 9_000, 125);
        assert_eq!(window.entries, 40..47);
        assert_eq!(window.content_y0, 120);
        assert_eq!(window.skip_rows, 5);
        assert_eq!(window.total_height, 9_000);
    }

    #[test]
    fn overscan_start_above_viewport_never_underflows() {
        let window = RenderWindow::new(0..3, 0, 12, 0);
        assert_eq!(window.skip_rows, 0);
        assert!(!window.is_empty());
    }
}
