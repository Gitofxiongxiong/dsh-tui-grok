//! Grok thinking block display modes over an already styled markdown body.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{
    Theme,
    appearance::ScrollbackAppearance,
    scrollback::types::{AccentStyle, BlockLine, BlockOutput, DisplayMode, RenderedBlock},
};

#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    body: Vec<Line<'static>>,
    elapsed_time_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct ThinkingBlockContext<'a> {
    pub mode: DisplayMode,
    pub is_running: bool,
    pub appearance: &'a ScrollbackAppearance,
    pub theme: Theme,
}

impl ThinkingBlock {
    pub fn new(body: Vec<Line<'static>>, elapsed_time_ms: Option<u64>) -> Self {
        Self {
            body,
            elapsed_time_ms,
        }
    }

    pub fn render(&self, ctx: ThinkingBlockContext<'_>) -> RenderedBlock {
        let output = match ctx.mode {
            DisplayMode::Collapsed => BlockOutput {
                lines: vec![BlockLine::header(self.header_line(ctx))],
            },
            DisplayMode::Truncated => self.render_truncated(ctx),
            DisplayMode::Expanded => self.render_expanded(ctx),
        };
        let cfg = &ctx.appearance.scrollback.blocks.thinking;
        let accent = if !cfg.accent_enabled || ctx.mode == DisplayMode::Collapsed {
            None
        } else if cfg.animate && ctx.is_running {
            Some(AccentStyle::animated(cfg.accent))
        } else {
            Some(AccentStyle::static_color(cfg.accent))
        };
        let bullet = if ctx.is_running { accent } else { None };
        RenderedBlock {
            output: prepend_bullet(
                output,
                ctx.appearance
                    .scrollback
                    .blocks
                    .tool
                    .bullet
                    .char()
                    .unwrap_or(""),
                bullet,
                ctx.theme.gray,
            ),
            accent,
            bullet,
            background: None,
            accent_background: false,
            vpad: false,
        }
    }

    fn render_truncated(&self, ctx: ThinkingBlockContext<'_>) -> BlockOutput {
        if self.body.is_empty() {
            return BlockOutput {
                lines: vec![BlockLine::header(self.header_line(ctx))],
            };
        }
        let count = usize::from(
            ctx.appearance
                .scrollback
                .blocks
                .thinking
                .truncated_lines
                .max(1),
        );
        let mut lines = Vec::new();
        if ctx.appearance.scrollback.blocks.thinking.header {
            lines.push(BlockLine::header(self.header_line(ctx)));
            lines.push(BlockLine::spacer());
        }
        if self.body.len() > count {
            lines.push(BlockLine::content(Line::from(Span::styled(
                "…",
                Style::default().fg(ctx.theme.gray),
            ))));
        }
        lines.extend(
            self.body[self.body.len().saturating_sub(count)..]
                .iter()
                .cloned()
                .map(|line| faded_body(line, ctx)),
        );
        BlockOutput { lines }
    }

    fn render_expanded(&self, ctx: ThinkingBlockContext<'_>) -> BlockOutput {
        if self.body.is_empty() {
            return BlockOutput {
                lines: vec![BlockLine::header(self.header_line(ctx))],
            };
        }
        let mut lines = Vec::new();
        if ctx.appearance.scrollback.blocks.thinking.header {
            lines.push(BlockLine::header(self.header_line(ctx)));
            lines.push(BlockLine::spacer());
        }
        lines.extend(self.body.iter().cloned().map(|line| faded_body(line, ctx)));
        BlockOutput { lines }
    }

    fn header_line(&self, ctx: ThinkingBlockContext<'_>) -> Line<'static> {
        let label_style = Style::default()
            .fg(ctx.theme.gray)
            .add_modifier(Modifier::BOLD);
        if ctx.is_running {
            Line::from(Span::styled("Thinking…", label_style))
        } else if let Some(elapsed) = self.elapsed_time_ms {
            Line::from(vec![
                Span::styled("Thought", label_style),
                Span::styled(
                    format!(" for {}", format_elapsed_ms(elapsed)),
                    Style::default().fg(ctx.theme.gray),
                ),
            ])
        } else {
            Line::from(Span::styled("Thought", label_style))
        }
    }
}

fn prepend_bullet(
    mut output: BlockOutput,
    bullet: &str,
    style: Option<AccentStyle>,
    default_color: ratatui::style::Color,
) -> BlockOutput {
    if let Some(first) = output.lines.first_mut() {
        first.content.spans.insert(
            0,
            Span::styled(
                format!("{bullet} "),
                Style::default().fg(style.map_or(default_color, |style| style.color)),
            ),
        );
    }
    output
}

fn faded_body(mut line: Line<'static>, ctx: ThinkingBlockContext<'_>) -> BlockLine {
    let factor = ctx
        .appearance
        .scrollback
        .blocks
        .thinking
        .bg_blend
        .clamp(0.0, 1.0);
    for span in &mut line.spans {
        let foreground = span.style.fg.unwrap_or(ctx.theme.text_primary);
        span.style.fg = crate::render::color::blend_color(ctx.theme.bg_base, foreground, factor);
    }
    BlockLine::content(line)
}

fn format_elapsed_ms(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms as f64 / 1_000.0;
    if seconds < 60.0 {
        format!("{seconds:.1}s")
    } else {
        let minutes = (seconds / 60.0).floor() as u64;
        let remaining = (seconds - minutes as f64 * 60.0).round() as u64;
        format!("{minutes}m{remaining}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::GrokAppearanceSnapshot;

    #[test]
    fn running_truncated_thinking_owns_header_tail_and_animated_styles() {
        let theme = Theme::default();
        let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
        let block = ThinkingBlock::new(
            (0..5).map(|n| Line::from(format!("line {n}"))).collect(),
            None,
        );
        let rendered = block.render(ThinkingBlockContext {
            mode: DisplayMode::Truncated,
            is_running: true,
            appearance: &appearance,
            theme,
        });
        assert_eq!(rendered.output.lines[0].content.to_string(), "◆ Thinking…");
        assert!(rendered.accent.is_some_and(|accent| accent.animated));
        assert_eq!(rendered.output.lines[2].content.to_string(), "…");
        assert_eq!(rendered.output.lines.len(), 6);
    }

    #[test]
    fn finished_collapsed_thinking_formats_grok_duration() {
        let theme = Theme::default();
        let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
        let rendered = ThinkingBlock::new(Vec::new(), Some(65_000)).render(ThinkingBlockContext {
            mode: DisplayMode::Collapsed,
            is_running: false,
            appearance: &appearance,
            theme,
        });
        assert_eq!(
            rendered.output.lines[0].content.to_string(),
            "◆ Thought for 1m5s"
        );
        assert_eq!(rendered.accent, None);
    }

    #[test]
    fn expanded_thinking_keeps_body_while_truncated_keeps_only_tail() {
        let theme = Theme::default();
        let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
        let body = (0..8).map(|n| Line::from(format!("line {n}"))).collect();
        let block = ThinkingBlock::new(body, Some(500));
        let expanded = block.render(ThinkingBlockContext {
            mode: DisplayMode::Expanded,
            is_running: false,
            appearance: &appearance,
            theme,
        });
        assert!(
            expanded
                .output
                .lines
                .iter()
                .any(|line| line.content.to_string() == "line 0")
        );
        assert!(
            expanded
                .output
                .lines
                .iter()
                .any(|line| line.content.to_string() == "line 7")
        );

        let truncated = block.render(ThinkingBlockContext {
            mode: DisplayMode::Truncated,
            is_running: true,
            appearance: &appearance,
            theme,
        });
        assert!(
            !truncated
                .output
                .lines
                .iter()
                .any(|line| line.content.to_string() == "line 0")
        );
        assert!(
            truncated
                .output
                .lines
                .iter()
                .any(|line| line.content.to_string() == "line 7")
        );
        assert!(
            truncated
                .output
                .lines
                .iter()
                .any(|line| line.content.to_string() == "…")
        );
    }

    #[test]
    fn elapsed_time_uses_upstream_second_and_minute_buckets() {
        assert_eq!(format_elapsed_ms(50), "0.1s");
        assert_eq!(format_elapsed_ms(59_949), "59.9s");
        assert_eq!(format_elapsed_ms(65_000), "1m5s");
    }
}
