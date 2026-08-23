//! Minimal appearance state seam used by the copied modal chrome.

use ratatui::layout::Rect;

/// Renderer-only projection of Grok's appearance configuration.
///
/// This is deliberately a value object: it contains no config store, file
/// watcher, or host/runtime reference. The host may replace it per frame after
/// resolving user settings and terminal capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrokAppearanceSnapshot {
    pub compact: bool,
    pub outer_hpad: u16,
    pub outer_vpad: u16,
    pub scrollback_min_rows: u16,
    pub prompt_gap: u16,
    pub prompt_vpad_top: u16,
    pub prompt_show_prefix: bool,
    pub prompt_show_borders: bool,
    pub prompt_show_accent_line: bool,
    pub scrollbar_enabled: bool,
}

impl GrokAppearanceSnapshot {
    pub const SCROLLBACK_MIN_ROWS: u16 = 5;
    pub const SHORT_TERMINAL_ROWS: u16 = 16;
    pub const AUTO_COMPACT_MAX_ROWS: u16 = 20;

    pub fn for_area(area: Rect, user_compact: bool) -> Self {
        let compact =
            user_compact || (area.height > 0 && area.height <= Self::AUTO_COMPACT_MAX_ROWS);
        Self {
            compact,
            outer_hpad: if compact { 1 } else { 2 },
            outer_vpad: if compact || area.height <= Self::SHORT_TERMINAL_ROWS {
                0
            } else {
                1
            },
            scrollback_min_rows: Self::SCROLLBACK_MIN_ROWS,
            prompt_gap: u16::from(!compact),
            prompt_vpad_top: 1,
            prompt_show_prefix: true,
            prompt_show_borders: true,
            prompt_show_accent_line: false,
            scrollbar_enabled: area.width >= 20 && area.height >= 6,
        }
    }
}

impl Default for GrokAppearanceSnapshot {
    fn default() -> Self {
        Self::for_area(Rect::new(0, 0, 80, 24), false)
    }
}

pub mod cache {
    use std::sync::atomic::{AtomicBool, Ordering};

    static VIM_MODE: AtomicBool = AtomicBool::new(false);

    pub fn load_vim_mode() -> bool {
        VIM_MODE.load(Ordering::Relaxed)
    }

    pub fn set_vim_mode(enabled: bool) {
        VIM_MODE.store(enabled, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_matches_grok_short_terminal_breakpoint() {
        let compact = GrokAppearanceSnapshot::for_area(Rect::new(0, 0, 80, 16), false);
        assert!(compact.compact);
        assert_eq!(compact.outer_hpad, 1);
        assert_eq!(compact.outer_vpad, 0);
        assert_eq!(compact.scrollback_min_rows, 5);
    }

    #[test]
    fn snapshot_preserves_wide_desktop_chrome() {
        let desktop = GrokAppearanceSnapshot::for_area(Rect::new(0, 0, 120, 40), false);
        assert!(!desktop.compact);
        assert_eq!(desktop.outer_hpad, 2);
        assert_eq!(desktop.outer_vpad, 1);
        assert!(desktop.prompt_show_borders);
        assert!(desktop.scrollbar_enabled);
    }
}
