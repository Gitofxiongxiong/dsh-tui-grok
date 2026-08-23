//! DSH-neutral projection of Grok's prompt rendering inputs.
//!
//! The fixed upstream `PromptWidget` owns drawing and interaction. These
//! types isolate the pure visual inputs and geometry needed to attach that
//! widget without importing Grok's agent, session, shell, or RPC runtime.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;

/// Background surface occupied by the prompt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PromptSurface {
    /// Main application canvas.
    #[default]
    Default,
    /// Explicit canvas color for an inline prompt using main-surface chrome.
    Canvas(Color),
    /// Inline panel color for question and permission prompts.
    Panel(Color),
}

/// Visual configuration projected into Grok's `PromptStyle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptStyleContract {
    pub focused: bool,
    pub show_prefix: bool,
    pub vpad_top: u16,
    pub chrome: bool,
    pub chrome_pad_left: u16,
    pub chrome_pad_right: u16,
    pub surface: PromptSurface,
    pub accent_color_override: Option<Color>,
    pub border_color_override: Option<Color>,
    pub prefix_override: Option<String>,
    pub placeholder_override: Option<String>,
    pub placeholder_when_focused: bool,
    pub compact: bool,
    pub show_accent_line: bool,
    pub show_borders: bool,
    pub title: Option<String>,
    pub image_preview: bool,
}

impl Default for PromptStyleContract {
    fn default() -> Self {
        Self {
            focused: true,
            show_prefix: true,
            vpad_top: 1,
            chrome: true,
            chrome_pad_left: 2,
            chrome_pad_right: 1,
            surface: PromptSurface::Default,
            accent_color_override: None,
            border_color_override: None,
            prefix_override: None,
            placeholder_override: None,
            placeholder_when_focused: false,
            compact: false,
            show_accent_line: false,
            show_borders: true,
            title: None,
            image_preview: true,
        }
    }
}

impl PromptStyleContract {
    /// Upstream chromeless style used by overlays.
    pub fn overlay() -> Self {
        Self {
            chrome: false,
            vpad_top: 0,
            ..Self::default()
        }
    }

    /// Upstream inline-panel style used by permission and question prompts.
    pub fn inline(background: Color) -> Self {
        Self {
            show_prefix: false,
            vpad_top: 0,
            chrome: false,
            chrome_pad_left: 0,
            chrome_pad_right: 0,
            surface: PromptSurface::Panel(background),
            show_borders: false,
            ..Self::default()
        }
    }

    pub const fn info_block(&self, has_info: bool) -> u16 {
        if has_info { 1 } else { 0 }
    }

    pub const fn accent_width(&self) -> u16 {
        if self.chrome && self.show_accent_line {
            1
        } else {
            0
        }
    }

    pub fn placeholder(&self) -> &str {
        self.placeholder_override
            .as_deref()
            .unwrap_or("Build anything")
    }
}

/// One mode/capability label on the prompt's bottom divider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptFlagContract {
    pub text: String,
    pub color: Option<Color>,
    pub bold: bool,
}

/// Optional bottom-divider content projected into Grok's `PromptInfo`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptInfoContract {
    pub model_name: String,
    pub flags: Vec<PromptFlagContract>,
    pub multiline: bool,
    pub usage_warning: Option<String>,
    pub usage_warning_critical: bool,
}

impl PromptInfoContract {
    pub fn is_blank(&self) -> bool {
        self.model_name.is_empty()
            && self.flags.is_empty()
            && !self.multiline
            && self.usage_warning.is_none()
    }
}

/// Rectangles computed by the fixed upstream `PromptWidget::draw` split.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PromptGeometry {
    pub outer: Rect,
    pub content: Rect,
    pub top: Rect,
    pub text: Rect,
    pub textarea: Rect,
    pub info: Rect,
    pub dim: Rect,
}

impl PromptGeometry {
    /// Reproduce the upstream chrome/content/text split without drawing cells.
    pub fn compute(
        area: Rect,
        style: &PromptStyleContract,
        has_info: bool,
        prefix_width: u16,
    ) -> Self {
        if area.height == 0 || area.width < 4 {
            return Self {
                outer: area,
                ..Self::default()
            };
        }

        let content = if style.chrome {
            let inset = style.accent_width().saturating_add(style.chrome_pad_left);
            Rect::new(
                area.x.saturating_add(inset),
                area.y,
                area.width
                    .saturating_sub(inset.saturating_add(style.chrome_pad_right)),
                area.height,
            )
        } else {
            area
        };
        let chunks = Layout::vertical([
            Constraint::Length(style.vpad_top),
            Constraint::Min(1),
            Constraint::Length(style.info_block(has_info)),
        ])
        .split(content);
        let prefix_width = if style.show_prefix { prefix_width } else { 0 };
        let textarea = Rect::new(
            chunks[1].x.saturating_add(prefix_width),
            chunks[1].y,
            chunks[1].width.saturating_sub(prefix_width),
            chunks[1].height,
        );
        let info_block = style.info_block(has_info);
        let dim = Rect::new(
            area.x.saturating_add(1),
            content.y.saturating_add(style.vpad_top),
            area.width.saturating_sub(2),
            content
                .height
                .saturating_sub(style.vpad_top.saturating_add(info_block)),
        );

        Self {
            outer: area,
            content,
            top: chunks[0],
            text: chunks[1],
            textarea,
            info: chunks[2],
            dim,
        }
    }
}

/// Upstream prompt height rule after textarea wrapping has been computed.
pub fn desired_prompt_height(
    textarea_rows: u16,
    style: &PromptStyleContract,
    has_info: bool,
    history_browse: bool,
    max_height: u16,
) -> u16 {
    let text_height = if history_browse {
        1
    } else {
        textarea_rows.max(1)
    };
    let info_block = style.info_block(has_info);
    let total = style
        .vpad_top
        .saturating_add(text_height)
        .saturating_add(info_block);
    let minimum = style.vpad_top.saturating_add(1).saturating_add(info_block);
    total.clamp(minimum.min(max_height), max_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_contract_matches_fixed_upstream_prompt_style() {
        let style = PromptStyleContract::default();
        assert!(style.focused);
        assert!(style.show_prefix);
        assert_eq!(style.vpad_top, 1);
        assert_eq!(style.chrome_pad_left, 2);
        assert_eq!(style.chrome_pad_right, 1);
        assert!(style.chrome);
        assert!(style.show_borders);
        assert!(!style.show_accent_line);
        assert_eq!(style.placeholder(), "Build anything");
    }

    #[test]
    fn geometry_matches_upstream_chrome_split() {
        let area = Rect::new(4, 7, 40, 5);
        let geometry = PromptGeometry::compute(area, &PromptStyleContract::default(), true, 2);
        assert_eq!(geometry.content, Rect::new(6, 7, 37, 5));
        assert_eq!(geometry.top, Rect::new(6, 7, 37, 1));
        assert_eq!(geometry.text, Rect::new(6, 8, 37, 3));
        assert_eq!(geometry.textarea, Rect::new(8, 8, 35, 3));
        assert_eq!(geometry.info, Rect::new(6, 11, 37, 1));
        assert_eq!(geometry.dim, Rect::new(5, 8, 38, 3));
    }

    #[test]
    fn accent_and_inline_surfaces_reclaim_the_expected_columns() {
        let mut accented = PromptStyleContract::default();
        accented.show_accent_line = true;
        let geometry = PromptGeometry::compute(Rect::new(0, 0, 20, 3), &accented, true, 2);
        assert_eq!(geometry.content, Rect::new(3, 0, 16, 3));

        let inline = PromptStyleContract::inline(Color::Black);
        let geometry = PromptGeometry::compute(Rect::new(2, 3, 20, 2), &inline, false, 2);
        assert_eq!(geometry.content, Rect::new(2, 3, 20, 2));
        assert_eq!(geometry.textarea, Rect::new(2, 3, 20, 2));
    }

    #[test]
    fn desired_height_freezes_while_history_browse_is_active() {
        let style = PromptStyleContract::default();
        assert_eq!(desired_prompt_height(4, &style, true, false, 8), 6);
        assert_eq!(desired_prompt_height(4, &style, true, true, 8), 3);
        assert_eq!(desired_prompt_height(20, &style, true, false, 8), 8);
    }

    #[test]
    fn blank_info_follows_upstream_semantics() {
        assert!(PromptInfoContract::default().is_blank());
        assert!(
            !PromptInfoContract {
                multiline: true,
                ..PromptInfoContract::default()
            }
            .is_blank()
        );
    }
}
