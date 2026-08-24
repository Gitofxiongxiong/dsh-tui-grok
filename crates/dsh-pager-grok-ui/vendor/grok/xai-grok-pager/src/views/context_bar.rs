//! Context usage bar for Grok's upper-right agent status row.
//!
//! B adaptation of Grok Build's `views/context_bar.rs`. The DSH seam supplies
//! authoritative token values, while this component retains Grok's compact
//! formatting, urgency gradient and fixed-width hover progress treatment.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::progress_bar::progress_bar_spans;
use crate::theme::Theme;

pub fn fmt_pct5(percentage: f64) -> String {
    if percentage >= 100.0 {
        "MAX %".to_string()
    } else if percentage < 10.0 {
        format!("{percentage:.2}%")
    } else {
        format!("{percentage:.1}%")
    }
}

pub fn fmt_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 10_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else if tokens < 1_000_000 {
        format!("{}K", tokens / 1_000)
    } else if tokens < 10_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else {
        format!("{}M", tokens / 1_000_000)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColorBreakpoint {
    pub percentage: f64,
    pub color: Color,
}

pub fn default_breakpoints(theme: &Theme) -> Vec<ColorBreakpoint> {
    vec![
        ColorBreakpoint {
            percentage: 0.0,
            color: theme.text_primary,
        },
        ColorBreakpoint {
            percentage: 50.0,
            color: theme.accent_user,
        },
        ColorBreakpoint {
            percentage: 65.0,
            color: theme.accent_user,
        },
        ColorBreakpoint {
            percentage: 75.0,
            color: theme.warning,
        },
        ColorBreakpoint {
            percentage: 85.0,
            color: theme.warning,
        },
        ColorBreakpoint {
            percentage: 95.0,
            color: theme.accent_error,
        },
    ]
}

pub fn blend_color(percentage: f64, breakpoints: &[ColorBreakpoint]) -> Color {
    let Some(first) = breakpoints.first() else {
        return Color::Reset;
    };
    if percentage <= first.percentage {
        return first.color;
    }
    for pair in breakpoints.windows(2) {
        let from = pair[0];
        let to = pair[1];
        if percentage <= to.percentage {
            let distance = to.percentage - from.percentage;
            let amount = if distance == 0.0 {
                1.0
            } else {
                (percentage - from.percentage) / distance
            };
            return lerp_color(from.color, to.color, amount as f32);
        }
    }
    breakpoints
        .last()
        .map_or(Color::Reset, |breakpoint| breakpoint.color)
}

fn lerp_color(from: Color, to: Color, amount: f32) -> Color {
    let (from_r, from_g, from_b) = color_to_rgb(from);
    let (to_r, to_g, to_b) = color_to_rgb(to);
    let amount = amount.clamp(0.0, 1.0);
    let channel =
        |from: u8, to: u8| (from as f32 + (to as f32 - from as f32) * amount).round() as u8;
    Color::Rgb(
        channel(from_r, to_r),
        channel(from_g, to_g),
        channel(from_b, to_b),
    )
}

fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(red, green, blue) => (red, green, blue),
        Color::Black => (0, 0, 0),
        Color::Red | Color::LightRed => (255, 0, 0),
        Color::Green | Color::LightGreen => (0, 255, 0),
        Color::Yellow | Color::LightYellow => (255, 255, 0),
        Color::Blue | Color::LightBlue => (0, 0, 255),
        Color::Magenta | Color::LightMagenta => (255, 0, 255),
        Color::Cyan | Color::LightCyan => (0, 255, 255),
        Color::Gray | Color::White => (192, 192, 192),
        Color::DarkGray => (128, 128, 128),
        Color::Indexed(value) => (value, value, value),
        Color::Reset => (198, 198, 198),
    }
}

pub const SEPARATOR: &str = "│";

const PERCENTAGE_WIDTH: u16 = 5;
const BAR_PERCENTAGE_GAP: u16 = 1;

pub fn context_bar_line(
    used_tokens: Option<u64>,
    total_tokens: Option<u64>,
    hovered: bool,
    theme: &Theme,
) -> Option<Line<'static>> {
    let used = used_tokens?;
    let total = total_tokens.filter(|total| *total > 0)?;
    let percentage = ((used as f64) / (total as f64) * 100.0).min(100.0);
    let mut token_text = format!("{} / {}", fmt_tokens(used), fmt_tokens(total));
    let natural_width = token_text.chars().count() as u16;
    let minimum_width = BAR_PERCENTAGE_GAP + PERCENTAGE_WIDTH;
    if natural_width < minimum_width {
        token_text.push_str(&" ".repeat((minimum_width - natural_width) as usize));
    }
    let total_width = natural_width.max(minimum_width);
    let color = blend_color(percentage, &default_breakpoints(theme));

    if hovered {
        let bar_width = total_width - minimum_width;
        let mut spans = progress_bar_spans(
            bar_width,
            percentage as f32 / 100.0,
            color,
            theme.bg_highlight,
        );
        spans.push(Span::styled(" ", Style::default().bg(theme.bg_base)));
        spans.push(Span::styled(
            fmt_pct5(percentage),
            Style::default().fg(theme.text_secondary).bg(theme.bg_base),
        ));
        Some(Line::from(spans))
    } else {
        Some(Line::from(Span::styled(
            token_text,
            Style::default().fg(color).bg(theme.bg_base),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn compact_token_format_matches_grok() {
        assert_eq!(fmt_tokens(999), "999");
        assert_eq!(fmt_tokens(1_200), "1.2K");
        assert_eq!(fmt_tokens(12_000), "12K");
        assert_eq!(fmt_tokens(1_200_000), "1.2M");
        assert_eq!(fmt_tokens(12_000_000), "12M");
    }

    #[test]
    fn default_shows_used_and_window() {
        let line = context_bar_line(Some(8_500), Some(1_000_000), false, &Theme::default())
            .expect("context data");
        assert_eq!(line_text(&line), "8.5K / 1.0M");
    }

    #[test]
    fn hover_shows_percentage_without_layout_shift() {
        let theme = Theme::default();
        let normal =
            context_bar_line(Some(420_000), Some(1_000_000), false, &theme).expect("context data");
        let hovered =
            context_bar_line(Some(420_000), Some(1_000_000), true, &theme).expect("context data");
        assert_eq!(normal.width(), hovered.width());
        assert!(line_text(&hovered).ends_with("42.0%"));
    }

    #[test]
    fn missing_or_zero_window_is_not_fabricated() {
        let theme = Theme::default();
        assert!(context_bar_line(None, Some(1_000_000), false, &theme).is_none());
        assert!(context_bar_line(Some(1_000), None, false, &theme).is_none());
        assert!(context_bar_line(Some(1_000), Some(0), false, &theme).is_none());
    }

    #[test]
    fn urgency_color_reaches_error_at_high_pressure() {
        let theme = Theme::default();
        let line =
            context_bar_line(Some(950_000), Some(1_000_000), false, &theme).expect("context data");
        assert_eq!(line.spans[0].style.fg, Some(theme.accent_error));
    }
}
