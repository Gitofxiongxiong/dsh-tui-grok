//! Shared prompt-area list overlay, structurally ported from Grok Build.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

const MAX_ROWS: usize = 15;

pub struct ListOverlay {
    pub len: usize,
    pub selected: usize,
}

pub struct RowCtx {
    pub is_cursor: bool,
    pub row_bg: Color,
    pub content_width: u16,
}

impl ListOverlay {
    pub fn height(&self, screen_h: u16) -> u16 {
        let rows = self.len.min(MAX_ROWS) as u16;
        let height = 2 + rows;
        let cap = (screen_h as u32 * 60 / 100).max(6) as u16;
        height.min(cap) + 1
    }

    fn visible_rows(area: Rect) -> usize {
        area.height.saturating_sub(3) as usize
    }

    fn scroll_offset(&self, visible_rows: usize) -> usize {
        if visible_rows > 0 && self.selected >= visible_rows {
            self.selected - visible_rows + 1
        } else {
            0
        }
    }

    pub fn row_at(&self, area: Rect, col: u16, row: u16) -> Option<usize> {
        if area.height == 0
            || area.width < 10
            || col < area.x
            || col >= area.right()
            || row < area.y
            || row >= area.bottom()
        {
            return None;
        }
        let first = area.y + 2;
        if row < first {
            return None;
        }
        let visible_rows = Self::visible_rows(area);
        let relative = (row - first) as usize;
        if relative >= visible_rows {
            return None;
        }
        let index = self.scroll_offset(visible_rows) + relative;
        (index < self.len).then_some(index)
    }

    pub fn render(
        &self,
        buf: &mut Buffer,
        area: Rect,
        title: &str,
        focused: bool,
        mut row_line: impl FnMut(usize, &RowCtx) -> Line<'static>,
    ) {
        if area.height == 0 || area.width < 10 {
            return;
        }
        let theme = Theme::current();
        let background = theme.bg_light;
        buf.set_style(area, Style::default().bg(background));
        for row in area.y..area.bottom() {
            if let Some(cell) = buf.cell_mut((area.x, row)) {
                cell.set_symbol(crate::glyphs::accent_bar());
                cell.set_style(Style::default().fg(theme.accent_user));
            }
        }
        let content_x = area.x + 3;
        let content_width = area.width.saturating_sub(5);
        buf.set_line(
            content_x,
            area.y + 1,
            &Line::from(Span::styled(
                title.to_string(),
                Style::default()
                    .fg(theme.accent_user)
                    .add_modifier(Modifier::BOLD),
            )),
            content_width,
        );
        let visible_rows = Self::visible_rows(area);
        let offset = self.scroll_offset(visible_rows);
        for (y, index) in (area.y + 2..).zip((offset..self.len).take(visible_rows)) {
            let is_cursor = index == self.selected;
            let row_bg = if is_cursor && focused {
                theme.bg_visual
            } else {
                background
            };
            buf.set_style(
                Rect::new(content_x.saturating_sub(1), y, content_width + 2, 1),
                Style::default().bg(row_bg),
            );
            let context = RowCtx {
                is_cursor,
                row_bg,
                content_width,
            };
            buf.set_line(content_x, y, &row_line(index, &context), content_width);
        }
        if !focused {
            dim_foreground(buf, area, background, 0.66);
        }
    }
}

pub(crate) fn dim_foreground(buf: &mut Buffer, area: Rect, target: Color, opacity: f32) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y))
                && let Some(color) = crate::render::color::blend_color(target, cell.fg, opacity)
            {
                cell.set_fg(color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 0, 40, 10)
    }

    #[test]
    fn row_geometry_and_scroll_window_match_grok() {
        let list = ListOverlay {
            len: 3,
            selected: 0,
        };
        assert_eq!(list.row_at(area(), 5, 1), None);
        assert_eq!(list.row_at(area(), 5, 2), Some(0));
        assert_eq!(list.row_at(area(), 5, 4), Some(2));
        let long = ListOverlay {
            len: 20,
            selected: 19,
        };
        assert_eq!(long.row_at(area(), 5, 2), Some(13));
        assert_eq!(long.row_at(area(), 5, 8), Some(19));
    }

    #[test]
    fn height_has_same_row_and_screen_caps() {
        assert_eq!(
            ListOverlay {
                len: 2,
                selected: 0
            }
            .height(40),
            5
        );
        assert_eq!(
            ListOverlay {
                len: 30,
                selected: 0
            }
            .height(12),
            8
        );
    }
}
