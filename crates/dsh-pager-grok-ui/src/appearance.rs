//! Minimal host seam over Grok's copied runtime appearance configuration.

use ratatui::layout::Rect;

#[path = "../vendor/grok/xai-grok-pager-render/src/appearance/scroll_mode.rs"]
mod scroll_mode;
#[path = "../vendor/grok/xai-grok-pager-render/src/appearance/scrollback_config.rs"]
mod scrollback_config;

pub use scroll_mode::ScrollMode;
pub use scrollback_config::*;

/// Value-only subset needed by the scrollback renderer.
///
/// The two fields retain their upstream positions (`animation` and
/// `scrollback`). DSH resolves them per frame without importing Grok's TOML
/// store, file watcher, or process runtime.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollbackAppearance {
    pub animation: AnimationConfig,
    pub scrollback: ScrollbackConfig,
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

    /// Resolve the color-bearing scrollback appearance from this frame's
    /// semantic palette. This mirrors Grok's resolved runtime config without
    /// importing its process-global config cache.
    pub fn scrollback(self, theme: crate::Theme) -> ScrollbackAppearance {
        let mut scrollback = ScrollbackConfig::default();
        scrollback.layout.outer_vpad = self.outer_vpad;
        scrollback.layout.outer_hpad_left = self.outer_hpad;
        scrollback.layout.outer_hpad_right = self.outer_hpad;
        scrollback.scrollbar.enabled = self.scrollbar_enabled;
        scrollback.display.sticky_headers = true;

        scrollback.blocks.thinking.accent = theme.gray_dim;
        scrollback.blocks.prompt.vpad = !self.compact;
        scrollback.blocks.prompt.bg = BlockBackground::Light;
        scrollback.blocks.prompt.accent_bg = true;
        scrollback.blocks.prompt.show_prefix = self.prompt_show_prefix && !self.compact;
        scrollback.blocks.tool.bullet = ToolBullet::Diamond;
        scrollback.blocks.execute.running_accent = theme.accent_running;

        ScrollbackAppearance {
            animation: AnimationConfig::default(),
            scrollback,
        }
    }
}

impl Default for GrokAppearanceSnapshot {
    fn default() -> Self {
        Self::for_area(Rect::new(0, 0, 80, 24), false)
    }
}

pub mod cache {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    static VIM_MODE: AtomicBool = AtomicBool::new(false);

    pub fn load_vim_mode() -> bool {
        VIM_MODE.load(Ordering::Relaxed)
    }

    pub fn set_vim_mode(enabled: bool) {
        VIM_MODE.store(enabled, Ordering::Relaxed);
    }

    const SCROLL_SPEED_DEFAULT: u8 = 50;
    const SCROLL_LINES_MIN: u8 = 1;
    const SCROLL_LINES_MAX: u8 = 10;
    const SCROLL_LINES_UNSET: u8 = 0;

    thread_local! {
        static SCROLL_SPEED_CURRENT: Cell<u8> = const { Cell::new(SCROLL_SPEED_DEFAULT) };
        static SCROLL_SPEED_LOADED: Cell<bool> = const { Cell::new(false) };
        static SCROLL_MODE_CURRENT: Cell<super::ScrollMode> = const { Cell::new(super::ScrollMode::Auto) };
        static SCROLL_MODE_LOADED: Cell<bool> = const { Cell::new(false) };
        static INVERT_SCROLL_CURRENT: Cell<bool> = const { Cell::new(false) };
        static INVERT_SCROLL_LOADED: Cell<bool> = const { Cell::new(false) };
        static SCROLL_LINES_CURRENT: Cell<u8> = const { Cell::new(SCROLL_LINES_UNSET) };
        static SCROLL_LINES_LOADED: Cell<bool> = const { Cell::new(false) };
    }

    pub fn load_scroll_speed() -> u8 {
        SCROLL_SPEED_LOADED.with(|loaded| {
            if !loaded.get() {
                let value = std::env::var("GROK_SCROLL_SPEED")
                    .ok()
                    .and_then(|value| value.trim().parse::<u8>().ok())
                    .unwrap_or(SCROLL_SPEED_DEFAULT)
                    .clamp(1, 100);
                SCROLL_SPEED_CURRENT.with(|current| current.set(value));
                loaded.set(true);
            }
        });
        SCROLL_SPEED_CURRENT.with(Cell::get)
    }

    pub fn set_scroll_speed(speed: u8) {
        SCROLL_SPEED_CURRENT.with(|current| current.set(speed.clamp(1, 100)));
        SCROLL_SPEED_LOADED.with(|loaded| loaded.set(true));
    }

    pub fn load_scroll_mode() -> super::ScrollMode {
        SCROLL_MODE_LOADED.with(|loaded| {
            if !loaded.get() {
                let value = std::env::var("GROK_SCROLL_MODE")
                    .ok()
                    .and_then(|value| super::ScrollMode::from_canonical(value.trim()))
                    .unwrap_or_default();
                SCROLL_MODE_CURRENT.with(|current| current.set(value));
                loaded.set(true);
            }
        });
        SCROLL_MODE_CURRENT.with(Cell::get)
    }

    pub fn set_scroll_mode(mode: super::ScrollMode) {
        SCROLL_MODE_CURRENT.with(|current| current.set(mode));
        SCROLL_MODE_LOADED.with(|loaded| loaded.set(true));
    }

    pub fn load_invert_scroll() -> bool {
        INVERT_SCROLL_LOADED.with(|loaded| {
            if !loaded.get() {
                let value = std::env::var("GROK_INVERT_SCROLL")
                    .ok()
                    .and_then(|value| match value.trim() {
                        "1" | "true" => Some(true),
                        "0" | "false" => Some(false),
                        _ => None,
                    })
                    .unwrap_or(false);
                INVERT_SCROLL_CURRENT.with(|current| current.set(value));
                loaded.set(true);
            }
        });
        INVERT_SCROLL_CURRENT.with(Cell::get)
    }

    pub fn set_invert_scroll(enabled: bool) {
        INVERT_SCROLL_CURRENT.with(|current| current.set(enabled));
        INVERT_SCROLL_LOADED.with(|loaded| loaded.set(true));
    }

    pub fn load_scroll_lines() -> Option<u8> {
        SCROLL_LINES_LOADED.with(|loaded| {
            if !loaded.get() {
                let value = std::env::var("GROK_SCROLL_LINES")
                    .ok()
                    .and_then(|value| value.trim().parse::<u8>().ok())
                    .map(|lines| lines.clamp(SCROLL_LINES_MIN, SCROLL_LINES_MAX))
                    .unwrap_or(SCROLL_LINES_UNSET);
                SCROLL_LINES_CURRENT.with(|current| current.set(value));
                loaded.set(true);
            }
        });
        SCROLL_LINES_CURRENT.with(|current| match current.get() {
            SCROLL_LINES_UNSET => None,
            lines => Some(lines),
        })
    }

    pub fn set_scroll_lines(lines: u8) {
        SCROLL_LINES_CURRENT.with(|current| {
            current.set(lines.clamp(SCROLL_LINES_MIN, SCROLL_LINES_MAX));
        });
        SCROLL_LINES_LOADED.with(|loaded| loaded.set(true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn snapshot_matches_grok_short_terminal_breakpoint() {
        let compact = GrokAppearanceSnapshot::for_area(Rect::new(0, 0, 80, 16), false);
        assert!(compact.compact);
        assert_eq!(compact.outer_hpad, 1);
        assert_eq!(compact.outer_vpad, 0);
        assert!(
            compact
                .scrollback(*Theme::current())
                .scrollback
                .display
                .sticky_headers
        );
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
        assert!(
            desktop
                .scrollback(*Theme::current())
                .scrollback
                .display
                .sticky_headers
        );
        assert!(!desktop.show_timeline, "Grok timeline is opt-in by default");
    }

    #[test]
    fn scrollback_snapshot_uses_grok_thinking_and_execute_accents() {
        let theme = crate::Theme::default();
        let scrollback = GrokAppearanceSnapshot::default().scrollback(theme);
        assert_eq!(scrollback.animation.fps, 30);
        assert_eq!(scrollback.animation.wave_rows, 32);
        assert_eq!(scrollback.scrollback.blocks.thinking.accent, theme.gray_dim);
        assert_eq!(scrollback.scrollback.blocks.thinking.truncated_lines, 3);
        assert_eq!(
            scrollback.scrollback.blocks.execute.running_accent,
            theme.accent_running
        );
        assert_eq!(
            scrollback.scrollback.blocks.tool.bullet.char(),
            Some(crate::glyphs::diamond_filled())
        );
        assert_eq!(scrollback.scrollback.layout.block_pad_right, 2);
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
