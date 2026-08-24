//! Agent status bar — composable right-aligned status items with separators.
//!
//! B adaptation of Grok Build's `views/agent_status.rs`: this file preserves
//! the standalone `AgentStatusBar` component and excludes task/goal status
//! builders whose runtime types do not belong at the DSH host seam.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::context_bar::SEPARATOR;
use crate::theme::Theme;

struct StatusEntry {
    id: &'static str,
    line: Line<'static>,
    width: u16,
}

/// Builder for Grok's right-aligned agent status row.
pub struct AgentStatusBar<'a> {
    items: Vec<StatusEntry>,
    theme: &'a Theme,
    right_pad: u16,
}

impl<'a> AgentStatusBar<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            items: Vec::new(),
            theme,
            right_pad: 0,
        }
    }

    pub fn push(&mut self, id: &'static str, line: Line<'static>) {
        let width = line.width() as u16;
        self.items.push(StatusEntry { id, line, width });
    }

    fn separator(&self) -> Span<'static> {
        Span::styled(
            format!(" {SEPARATOR} "),
            Style::default()
                .fg(self.theme.gray_dim)
                .bg(self.theme.bg_base),
        )
    }

    /// Render items in push order as one right-aligned group and return each
    /// item's exact screen rectangle for hover/click routing.
    pub fn render(self, buf: &mut Buffer, area: Rect) -> HashMap<&'static str, Rect> {
        if area.height == 0 || area.width == 0 || self.items.is_empty() {
            return HashMap::new();
        }

        buf.set_style(area, Style::default().bg(self.theme.bg_base));
        let sep = self.separator();
        let sep_width = sep.width() as u16;
        let items_width: u16 = self.items.iter().map(|entry| entry.width).sum();
        let separator_count = (self.items.len() as u16).saturating_sub(1);
        let total_width = items_width.saturating_add(separator_count * sep_width);
        let start_x = area
            .x
            .saturating_add(area.width.saturating_sub(self.right_pad + total_width));

        let mut x = start_x;
        let mut areas = HashMap::new();
        for (index, entry) in self.items.iter().enumerate() {
            if index > 0 {
                buf.set_span(x, area.y, &sep, sep_width);
                x = x.saturating_add(sep_width);
            }
            buf.set_line(x, area.y, &entry.line, entry.width);
            areas.insert(entry.id, Rect::new(x, area.y, entry.width, 1));
            x = x.saturating_add(entry.width);
        }
        areas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn items_are_right_aligned_and_return_exact_hit_rects() {
        let theme = Theme::default();
        let area = Rect::new(2, 1, 28, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 32, 3));
        let mut status = AgentStatusBar::new(&theme);
        status.push("state", Line::from("idle"));
        status.push("context", Line::from("8.5K / 1.0M"));

        let areas = status.render(&mut buf, area);
        let context = areas["context"];
        assert_eq!(context.right(), area.right());
        assert_eq!(context.width, 11);
        assert_eq!(areas["state"].right() + 3, context.x);
        assert_eq!(buf[(context.x - 2, area.y)].symbol(), "│");
        assert_eq!(buf[(context.x - 2, area.y)].fg, theme.gray_dim);
        assert_eq!(buf[(context.x, area.y)].fg, Color::Reset);
    }
}
