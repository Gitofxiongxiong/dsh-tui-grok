//! Renderer-neutral scrollback types extracted from Grok Build.

use ratatui::{style::Color, text::Line};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccentStyle {
    pub color: Color,
    pub animated: bool,
}

impl AccentStyle {
    pub const fn static_color(color: Color) -> Self {
        Self {
            color,
            animated: false,
        }
    }

    pub const fn animated(color: Color) -> Self {
        Self {
            color,
            animated: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Collapsed,
    Truncated,
    Expanded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLine {
    pub content: Line<'static>,
    pub background: Option<Color>,
    /// Column within the content area where a semantic line background starts.
    pub bg_start_col: u16,
    /// Decorative panel backgrounds may be flattened in terminal-native mode;
    /// semantic code/diff backgrounds remain owned by the row.
    pub background_is_panel: bool,
    pub selectable: bool,
    pub header: bool,
    /// Exact source text consumed between this visual row and the previous
    /// row. `None` means a hard break; `Some("")` is a split long word.
    pub joiner: Option<String>,
}

impl BlockLine {
    pub fn content(content: Line<'static>) -> Self {
        Self {
            content,
            background: None,
            bg_start_col: 0,
            background_is_panel: false,
            selectable: true,
            header: false,
            joiner: None,
        }
    }

    pub fn header(content: Line<'static>) -> Self {
        Self {
            header: true,
            ..Self::content(content)
        }
    }

    pub fn spacer() -> Self {
        Self {
            content: Line::default(),
            background: None,
            bg_start_col: 0,
            background_is_panel: false,
            selectable: false,
            header: false,
            joiner: None,
        }
    }

    pub fn with_joiner(mut self, joiner: Option<String>) -> Self {
        self.joiner = joiner;
        self
    }

    pub fn with_background(mut self, background: Color, is_panel: bool) -> Self {
        self.background = Some(background);
        self.background_is_panel = is_panel;
        self
    }

    /// Decorative panel band used by tool result previews (Read/Search boxes).
    pub fn with_panel_background(self, background: Color) -> Self {
        self.with_background(background, true)
    }

    /// Non-selectable decoration such as a blank gap or metadata row.
    pub fn separator(content: Line<'static>) -> Self {
        Self {
            content,
            background: None,
            bg_start_col: 0,
            background_is_panel: false,
            selectable: false,
            header: false,
            joiner: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockOutput {
    pub lines: Vec<BlockLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedBlock {
    pub output: BlockOutput,
    pub accent: Option<AccentStyle>,
    pub bullet: Option<AccentStyle>,
    pub background: Option<Color>,
    pub accent_background: bool,
    pub vpad: bool,
}

impl RenderedBlock {
    pub fn plain(output: BlockOutput) -> Self {
        Self {
            output,
            accent: None,
            bullet: None,
            background: None,
            accent_background: false,
            vpad: false,
        }
    }
}
