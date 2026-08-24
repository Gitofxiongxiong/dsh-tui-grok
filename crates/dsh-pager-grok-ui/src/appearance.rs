//! Minimal appearance state seam used by the copied modal chrome.

use ratatui::{layout::Rect, style::Color};

/// Grok viewport padding and block spacing projected without its config store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutConfig {
    pub outer_vpad: u16,
    pub outer_hpad_left: u16,
    pub outer_hpad_right: u16,
    pub block_pad_left: u16,
    pub block_pad_right: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            outer_vpad: 1,
            outer_hpad_left: 2,
            outer_hpad_right: 2,
            block_pad_left: 2,
            block_pad_right: 2,
        }
    }
}

impl LayoutConfig {
    pub const MIN_HPAD: u16 = 1;

    pub fn eff_outer_vpad(&self, compact: bool) -> u16 {
        if compact { 0 } else { self.outer_vpad }
    }

    pub fn eff_hpad_left(&self, compact: bool) -> u16 {
        if compact {
            Self::MIN_HPAD
        } else {
            self.outer_hpad_left
        }
    }

    pub fn eff_hpad_right(&self, compact: bool) -> u16 {
        if compact {
            Self::MIN_HPAD
        } else {
            self.outer_hpad_right
        }
    }

    pub fn validated(self) -> Self {
        Self {
            outer_hpad_left: self.outer_hpad_left.max(Self::MIN_HPAD),
            outer_hpad_right: self.outer_hpad_right.max(Self::MIN_HPAD),
            ..self
        }
    }
}

/// Grok scrollbar gutter geometry projected without theme/config ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarConfig {
    pub enabled: bool,
    pub gap_left: u16,
    pub gap_right: u16,
    pub scrollbar_bg: Option<Color>,
    pub scrollbar_fg: Option<Color>,
}

impl Default for ScrollbarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            gap_left: 0,
            gap_right: 0,
            scrollbar_bg: None,
            scrollbar_fg: None,
        }
    }
}

impl ScrollbarConfig {
    pub fn total_width(&self) -> u16 {
        if self.enabled {
            self.gap_left
                .saturating_add(1)
                .saturating_add(self.gap_right)
        } else {
            0
        }
    }

    pub fn is_outside(&self, outer_hpad_right: u16) -> bool {
        self.gap_right < outer_hpad_right
    }
}

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
    /// Grok's per-turn timeline sidebar is opt-in and replaces the scrollbar.
    pub show_timeline: bool,
    /// User/assistant transcript clocks are a local renderer preference. They
    /// never change DSH session state or issue an RPC effect.
    pub show_timestamps: bool,
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
            show_timeline: false,
            show_timestamps: true,
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
        assert!(!desktop.show_timeline, "Grok timeline is opt-in by default");
    }

    #[test]
    fn layout_validation_preserves_block_padding_and_clamps_outer_padding() {
        let config = LayoutConfig {
            outer_hpad_left: 0,
            outer_hpad_right: 0,
            block_pad_left: 3,
            block_pad_right: 4,
            ..LayoutConfig::default()
        }
        .validated();
        assert_eq!(config.outer_hpad_left, LayoutConfig::MIN_HPAD);
        assert_eq!(config.outer_hpad_right, LayoutConfig::MIN_HPAD);
        assert_eq!(config.block_pad_left, 3);
        assert_eq!(config.block_pad_right, 4);
        assert_eq!(config.eff_outer_vpad(true), 0);
    }

    #[test]
    fn scrollbar_width_is_zero_only_when_disabled() {
        assert_eq!(ScrollbarConfig::default().total_width(), 1);
        assert_eq!(
            ScrollbarConfig {
                enabled: false,
                ..ScrollbarConfig::default()
            }
            .total_width(),
            0
        );
    }
}
