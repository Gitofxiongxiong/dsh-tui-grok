//! Unicode block progress bar at 1/8-cell resolution.
//!
//! B adaptation of Grok Build's `views/progress_bar.rs`. DSH currently targets
//! the modern terminal path, so the legacy ConHost shade-glyph capability
//! branch is excluded while the geometry and span rendering are unchanged.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

const BLOCKS: [&str; 9] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

fn cell_breakdown(width: u16, value: f32) -> (u16, usize) {
    let value = value.clamp(0.0, 1.0);
    let total_eighths = (value * width as f32 * 8.0).round() as u16;
    let full = (total_eighths / 8).min(width);
    let remainder = (total_eighths % 8) as usize;
    (full, remainder)
}

fn bar_cells(width: u16, value: f32) -> impl Iterator<Item = (&'static str, bool)> {
    let (full, remainder) = cell_breakdown(width, value);
    (0..width).map(move |index| {
        if index < full {
            (BLOCKS[8], true)
        } else if index == full && remainder > 0 {
            (BLOCKS[remainder], true)
        } else {
            (" ", false)
        }
    })
}

pub fn progress_bar_spans(
    width: u16,
    value: f32,
    foreground: Color,
    background: Color,
) -> Vec<Span<'static>> {
    let foreground_style = Style::default().fg(foreground).bg(background);
    let background_style = Style::default().bg(background);
    bar_cells(width, value)
        .map(|(symbol, filled)| {
            Span::styled(
                symbol.to_string(),
                if filled {
                    foreground_style
                } else {
                    background_style
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_breakdown_keeps_eighth_cell_precision() {
        assert_eq!(cell_breakdown(4, 0.5), (2, 0));
        assert_eq!(cell_breakdown(4, 0.125), (0, 4));
        assert_eq!(cell_breakdown(5, 0.03), (0, 1));
        assert_eq!(cell_breakdown(4, 2.0), (4, 0));
    }

    #[test]
    fn span_bar_uses_fractional_glyphs() {
        let spans = progress_bar_spans(4, 0.125, Color::White, Color::Black);
        assert_eq!(spans[0].content, "▌");
        assert_eq!(spans[1].content, " ");
        assert_eq!(spans.len(), 4);
    }
}
