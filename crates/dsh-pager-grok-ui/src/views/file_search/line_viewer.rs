use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

pub fn dim_area(buf: &mut Buffer, area: Rect, bg: Color, _alpha: f32) {
    buf.set_style(area, Style::default().bg(bg));
}
