pub fn ballot_x_button() -> &'static str {
    "×"
}

pub fn ballot_x() -> &'static str {
    ballot_x_button()
}

pub fn chevron() -> &'static str {
    "›"
}

/// Open disclosure indicator used by expanded, collapsible surfaces.
///
/// This mirrors Grok's `disclosure_open` glyph. Unlike U+2304 DOWN ARROWHEAD,
/// U+25BE stays vertically centered in the monospace cell used by xterm.js.
pub fn disclosure_open() -> &'static str {
    "▾"
}

pub fn diamond_filled() -> &'static str {
    "◆"
}

/// Grok's default collapsed accent glyph.
pub fn collapsed_accent() -> &'static str {
    "❙"
}

/// Grok's fullscreen scrollback accent rail.
pub fn accent_bar() -> &'static str {
    "┃"
}

/// Grok's synthetic group-header diamond.
pub fn diamond_dotted() -> &'static str {
    "◈"
}

/// Grok's filled status/selection dot for the modern-terminal DSH target.
pub fn filled_dot() -> &'static str {
    "●"
}

pub fn dot_spinner_frames() -> &'static [&'static str] {
    &["·", "•", "●", "•"]
}

pub fn braille_spinner_frames() -> &'static [&'static str] {
    &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"]
}

/// Idle watcher pulse copied from Grok's `monitor_icon_frames`.
///
/// `○ ◎ ◉ ◎` (U+25CB / U+25CE / U+25C9) is a one-column breath so the
/// following `N subagents still running` label never shifts.
pub fn monitor_icon_frames() -> &'static [&'static str] {
    &["\u{25CB}", "\u{25CE}", "\u{25C9}", "\u{25CE}"]
}

pub fn token_arrow() -> &'static str {
    "⇣"
}

pub fn timeline_chevron_up() -> &'static str {
    "▲"
}

pub fn timeline_chevron_down() -> &'static str {
    "▼"
}

pub fn timeline_tick_active() -> &'static str {
    " ●"
}

pub fn timeline_tick_hover() -> &'static str {
    " ○"
}
