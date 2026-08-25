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

pub fn dot_spinner_frames() -> &'static [&'static str] {
    &["·", "•", "●", "•"]
}

pub fn braille_spinner_frames() -> &'static [&'static str] {
    &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"]
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
