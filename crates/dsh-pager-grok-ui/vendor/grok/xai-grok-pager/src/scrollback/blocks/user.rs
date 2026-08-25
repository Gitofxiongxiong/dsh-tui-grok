//! Grok user prompt block projection.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    Theme,
    appearance::{BlockBackground, ScrollbackAppearance},
    render::wrapping::word_wrap_line_with_joiners,
    scrollback::types::{BlockLine, BlockOutput, DisplayMode, RenderedBlock},
};

pub struct UserPromptBlock<'a> {
    text: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct UserPromptContext<'a> {
    pub mode: DisplayMode,
    pub width: usize,
    pub appearance: &'a ScrollbackAppearance,
    pub theme: Theme,
}

impl<'a> UserPromptBlock<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub fn render(&self, ctx: UserPromptContext<'_>) -> RenderedBlock {
        let config = &ctx.appearance.scrollback.blocks.prompt;
        let prefix = if config.show_prefix { "❯ " } else { "" };
        let prefix_width = prefix.chars().count();
        let body_width = ctx.width.saturating_sub(prefix_width).max(1);
        let mut lines = Vec::new();
        for logical in self.text.split('\n') {
            let (wrapped, joiners) =
                word_wrap_line_with_joiners(&Line::from(logical.to_string()), body_width);
            for (content, joiner) in wrapped.into_iter().zip(joiners) {
                let first = lines.is_empty();
                let mut spans = vec![Span::styled(
                    if first {
                        prefix.to_string()
                    } else {
                        " ".repeat(prefix_width)
                    },
                    Style::default()
                        .fg(ctx.theme.accent_user)
                        .add_modifier(Modifier::BOLD),
                )];
                spans.extend(content.spans.into_iter().map(|mut span| {
                    span.style = span.style.fg(ctx.theme.text_primary);
                    span
                }));
                lines.push(BlockLine::content(Line::from(spans)).with_joiner(joiner));
            }
        }
        if lines.is_empty() {
            lines.push(BlockLine::content(Line::from(Span::styled(
                prefix.trim().to_string(),
                Style::default().fg(ctx.theme.accent_user),
            ))));
        }
        if ctx.mode != DisplayMode::Expanded && lines.len() > 3 {
            lines.truncate(3);
            if let Some(last) = lines.last_mut() {
                let ellipsis = if ctx.width > 1 { " …" } else { "…" };
                last.content = fit_line(
                    &last.content,
                    ctx.width.saturating_sub(ellipsis.width()).max(1),
                );
                last.content.spans.push(Span::styled(
                    ellipsis,
                    Style::default().fg(ctx.theme.text_primary),
                ));
            }
        }
        RenderedBlock {
            output: BlockOutput { lines },
            accent: None,
            bullet: None,
            background: match config.bg {
                BlockBackground::None => None,
                BlockBackground::Light => Some(ctx.theme.bg_light),
                BlockBackground::Dark => Some(ctx.theme.bg_dark),
            },
            accent_background: config.accent_bg,
            vpad: config.vpad,
        }
    }
}

fn fit_line(line: &Line<'_>, width: usize) -> Line<'static> {
    let mut remaining = width;
    let mut spans = Vec::new();
    for span in &line.spans {
        let mut text = String::new();
        for ch in span.content.chars() {
            let char_width = ch.width().unwrap_or(0);
            if char_width > remaining {
                break;
            }
            text.push(ch);
            remaining = remaining.saturating_sub(char_width);
        }
        if !text.is_empty() {
            spans.push(Span::styled(text, span.style));
        }
        if remaining == 0 {
            break;
        }
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::GrokAppearanceSnapshot;

    #[test]
    fn user_prompt_owns_prefix_truncation_and_vpad_contract() {
        let theme = Theme::default();
        let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
        let rendered = UserPromptBlock::new("one\ntwo\nthree\nfour").render(UserPromptContext {
            mode: DisplayMode::Collapsed,
            width: 40,
            appearance: &appearance,
            theme,
        });
        assert_eq!(rendered.output.lines.len(), 3);
        assert!(
            rendered.output.lines[0]
                .content
                .to_string()
                .starts_with("❯ ")
        );
        assert!(rendered.output.lines[2].content.to_string().ends_with(" …"));
        assert!(rendered.vpad);
    }

    #[test]
    fn prompt_soft_wraps_keep_joiners_and_ellipsis_fits_width() {
        let theme = Theme::default();
        let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
        let expanded = UserPromptBlock::new("hello  world again").render(UserPromptContext {
            mode: DisplayMode::Expanded,
            width: 9,
            appearance: &appearance,
            theme,
        });
        assert!(
            expanded
                .output
                .lines
                .iter()
                .skip(1)
                .any(|line| line.joiner.is_some())
        );

        let collapsed = UserPromptBlock::new("one two three four five six seven eight").render(
            UserPromptContext {
                mode: DisplayMode::Collapsed,
                width: 10,
                appearance: &appearance,
                theme,
            },
        );
        assert_eq!(collapsed.output.lines.len(), 3);
        assert!(
            collapsed.output.lines[2]
                .content
                .to_string()
                .ends_with(" …")
        );
        assert!(collapsed.output.lines[2].content.width() <= 10);
    }
}
