use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::{
    host_adapter::{FeatureStatus, FileSearchSnapshot},
    render::line_utils::truncate_str,
    theme::Theme,
};

pub fn dim_area(buf: &mut Buffer, area: Rect, bg: Color, _alpha: f32) {
    buf.set_style(area, Style::default().bg(bg));
}

/// Render the typed File Search result surface. The query/revision controller
/// lives beside this view; this function only paints stable row ids and the
/// host's optional, typed line preview.
pub fn render_file_search_content(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &FileSearchSnapshot,
    query: &str,
    selected_id: Option<&str>,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let heading = if query.is_empty() {
        "Query: _".to_string()
    } else {
        format!("Query: {query}")
    };
    buffer.set_string(
        area.x,
        area.y,
        truncate_str(&heading, area.width as usize),
        Style::default().fg(theme.text_primary).bg(theme.bg_base),
    );
    match snapshot.status {
        FeatureStatus::Unsupported => buffer.set_string(
            area.x,
            area.y.saturating_add(2),
            truncate_str(
                snapshot
                    .diagnostic
                    .as_deref()
                    .unwrap_or("Filesystem search is unavailable"),
                area.width as usize,
            ),
            Style::default().fg(theme.warning).bg(theme.bg_base),
        ),
        FeatureStatus::Pending => buffer.set_string(
            area.x,
            area.y.saturating_add(2),
            "Waiting for authoritative filesystem results...",
            Style::default().fg(theme.gray).bg(theme.bg_base),
        ),
        FeatureStatus::Available if snapshot.rows.is_empty() => buffer.set_string(
            area.x,
            area.y.saturating_add(2),
            "No file matches",
            Style::default().fg(theme.gray).bg(theme.bg_base),
        ),
        FeatureStatus::Available => {
            for (index, row) in snapshot.rows.iter().enumerate() {
                let y = area.y.saturating_add(2 + index as u16);
                if y >= area.bottom().saturating_sub(1) {
                    break;
                }
                let selected = selected_id == Some(row.id.as_str());
                let marker = if selected { "▸" } else { " " };
                let detail = if let Some(preview) = row.preview.as_ref() {
                    match preview.line {
                        Some(line) => format!("{}:{}  {}", row.path, line, preview.snippet),
                        None => format!("{}  {}", row.path, preview.snippet),
                    }
                } else if let Some(kind) = row.kind.as_deref() {
                    format!("{}  [{kind}]", row.path)
                } else {
                    row.path.clone()
                };
                let style = if selected {
                    Style::default().fg(theme.text_primary).bg(theme.bg_visual)
                } else {
                    Style::default().fg(theme.text_secondary).bg(theme.bg_base)
                };
                buffer.set_string(
                    area.x,
                    y,
                    truncate_str(&format!("{marker} {detail}"), area.width as usize),
                    style,
                );
            }
        }
    }
    buffer.set_string(
        area.x,
        area.bottom().saturating_sub(1),
        if snapshot.preview_status == FeatureStatus::Available {
            "Type query · ↑/↓ select · Enter preview · Esc close"
        } else {
            "Type query · ↑/↓ select · Enter select · Esc close"
        },
        Style::default().fg(theme.gray_dim).bg(theme.bg_base),
    );
}
