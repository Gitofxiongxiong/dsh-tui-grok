use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

#[allow(clippy::too_many_arguments)]
pub fn render_scrollbar_styled(
    buf: &mut Buffer,
    area: Option<Rect>,
    total: u16,
    visible: u16,
    offset: u16,
    track_style: Style,
    thumb_style: Style,
) {
    let Some(area) = area else { return };
    if area.width == 0 || area.height == 0 {
        return;
    }
    buf.set_style(area, track_style);
    let thumb_height = if total == 0 {
        area.height
    } else {
        ((visible as u32 * area.height as u32) / total as u32)
            .max(1)
            .min(area.height as u32) as u16
    };
    let max_top = area.height.saturating_sub(thumb_height);
    let max_offset = total.saturating_sub(visible);
    let top = if max_offset == 0 {
        0
    } else {
        ((offset.min(max_offset) as u32 * max_top as u32) / max_offset as u32) as u16
    };
    for row in top..top.saturating_add(thumb_height).min(area.height) {
        if let Some(cell) = buf.cell_mut((area.x, area.y + row)) {
            cell.set_symbol("┃");
            cell.set_style(thumb_style);
        }
    }
}
