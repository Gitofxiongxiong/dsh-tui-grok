//! Grok-derived blocking permission card.
//!
//! Upstream's view also owns ACP option metadata, remembered bash/MCP grants,
//! pattern editing, and syntax highlighting.  This B adaptation keeps the
//! composer placement, height cap, accent rail, chrome/options geometry,
//! radio-row focus/hover styling, and hit rectangles while accepting a small
//! host-neutral state.  DSH-specific projection remains in `src/views`.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

use crate::theme::Theme;

/// An action the host can actually resolve for this permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionChoice {
    AllowOnce,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOption {
    pub choice: PermissionChoice,
    pub label: String,
}

/// Pure renderer state. Request identity and effect state stay outside the
/// copied component so redraw cannot submit or resolve an approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionViewState {
    pub title: String,
    pub command: Option<String>,
    pub description: Vec<String>,
    pub options: Vec<PermissionOption>,
    pub active_idx: usize,
    pub args_expanded: bool,
    pub pending: bool,
}

impl PermissionViewState {
    pub fn selected(&self) -> Option<PermissionChoice> {
        self.options
            .get(self.active_idx)
            .map(|option| option.choice)
    }

    pub fn clamp_selection(&mut self) {
        self.active_idx = self.active_idx.min(self.options.len().saturating_sub(1));
    }

    pub fn has_collapsible_display(&self, content_w: usize) -> bool {
        let mut expanded = self.clone();
        expanded.args_expanded = true;
        visible_body_rows(&expanded, content_w).len() > PERMISSION_COLLAPSED_ROWS
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionRenderResult {
    pub option_rows: Vec<Rect>,
}

/// Collapsed command/description budget retained from Grok's permission view.
pub const PERMISSION_COLLAPSED_ROWS: usize = 5;

/// Compute the composer height using Grok's 50%-of-screen cap (minimum 10,
/// maximum 80%). Expanded content may use the full screen.
pub fn permission_view_height(state: &PermissionViewState, screen_h: u16, content_w: usize) -> u16 {
    let body_rows = visible_body_rows(state, content_w).len() as u16;
    let total = 1u16
        .saturating_add(1) // top padding + title
        .saturating_add(body_rows)
        .saturating_add(1) // gap before options
        .saturating_add(state.options.len() as u16)
        .saturating_add(1); // bottom padding
    if state.args_expanded {
        return total.min(screen_h);
    }
    let cap = (screen_h as u32 / 2)
        .max(10)
        .min(screen_h as u32 * 80 / 100) as u16;
    total.min(cap)
}

/// Render the permission card that replaces the normal composer.
pub fn render_permission_view(
    buf: &mut Buffer,
    area: Rect,
    state: &PermissionViewState,
    hovered_item: Option<usize>,
    theme: &Theme,
    focused: bool,
) -> PermissionRenderResult {
    if area.width == 0 || area.height == 0 {
        return PermissionRenderResult::default();
    }

    buf.set_style(area, Style::default().bg(theme.bg_light));
    for row in area.y..area.bottom() {
        if let Some(cell) = buf.cell_mut((area.x, row)) {
            cell.set_symbol("┃");
            cell.set_style(Style::default().fg(theme.accent_user));
        }
    }

    let content_x = area.x.saturating_add(3);
    let content_width = area.width.saturating_sub(5);
    if content_width == 0 {
        return PermissionRenderResult::default();
    }

    let option_count = state.options.len().min(area.height as usize);
    let fixed_rows = 1u16
        .saturating_add(1) // top + title
        .saturating_add(1) // option gap
        .saturating_add(option_count as u16)
        .saturating_add(1); // bottom
    let body_budget = area.height.saturating_sub(fixed_rows) as usize;
    let body = visible_body_rows(state, content_width as usize);
    let body = clipped_body_rows(&body, body_budget);

    let mut y = area.y.saturating_add(1);
    if y < area.bottom() {
        buf.set_line(
            content_x,
            y,
            &Line::from(Span::styled(
                truncate_display(&state.title, content_width as usize),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            content_width,
        );
    }
    y = y.saturating_add(1);

    for row in body {
        if y >= area.bottom() {
            break;
        }
        let (text, role) = row;
        let color = match role {
            BodyRole::Command => theme.command,
            BodyRole::Description => theme.gray,
            BodyRole::Pending => theme.fuzzy_accent,
            BodyRole::Indicator => theme.gray,
        };
        let modifier = if role == BodyRole::Indicator {
            Modifier::DIM
        } else {
            Modifier::empty()
        };
        buf.set_line(
            content_x,
            y,
            &Line::from(Span::styled(
                text,
                Style::default().fg(color).add_modifier(modifier),
            )),
            content_width,
        );
        y = y.saturating_add(1);
    }

    // Grok leaves one clean row between the command chrome and radio rows.
    y = y.saturating_add(1);
    let mut option_rows = Vec::with_capacity(option_count);
    for (index, option) in state.options.iter().take(option_count).enumerate() {
        if y >= area.bottom() {
            break;
        }
        let selected = index == state.active_idx;
        let hovered = hovered_item == Some(index);
        let row_bg = if selected && focused {
            theme.bg_visual
        } else if hovered {
            theme.bg_hover
        } else {
            theme.bg_light
        };
        let row = Rect::new(content_x, y, content_width, 1);
        buf.set_style(row, Style::default().bg(row_bg));
        buf.set_line(
            content_x,
            y,
            &permission_option_line(option, index, selected, row_bg, state.pending, theme),
            content_width,
        );
        option_rows.push(row);
        y = y.saturating_add(1);
    }

    PermissionRenderResult { option_rows }
}

fn permission_option_line<'a>(
    option: &PermissionOption,
    index: usize,
    selected: bool,
    row_bg: ratatui::style::Color,
    pending: bool,
    theme: &Theme,
) -> Line<'a> {
    let number = if index < 9 {
        char::from(b'1' + index as u8)
    } else {
        ' '
    };
    let marker = if selected { "(●)" } else { "(○)" };
    let marker_style = if selected {
        Style::default()
            .fg(theme.text_primary)
            .bg(row_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.gray).bg(row_bg)
    };
    let label_color = if pending || option.choice == PermissionChoice::Reject {
        theme.gray
    } else {
        theme.text_primary
    };
    let label_modifier = if selected && !pending {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };
    Line::from(vec![
        Span::styled(
            format!("{number} "),
            Style::default().fg(theme.accent_user).bg(row_bg),
        ),
        Span::styled(format!("{marker} "), marker_style),
        Span::styled(
            option.label.clone(),
            Style::default()
                .fg(label_color)
                .bg(row_bg)
                .add_modifier(label_modifier),
        ),
    ])
    .style(Style::default().bg(row_bg))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyRole {
    Command,
    Description,
    Pending,
    Indicator,
}

fn visible_body_rows(state: &PermissionViewState, width: usize) -> Vec<(String, BodyRole)> {
    let mut rows = Vec::new();
    if let Some(command) = state.command.as_deref().filter(|value| !value.is_empty()) {
        rows.extend(
            wrap_text(command, width)
                .into_iter()
                .map(|row| (row, BodyRole::Command)),
        );
    }
    for description in &state.description {
        rows.extend(
            wrap_text(description, width)
                .into_iter()
                .map(|row| (row, BodyRole::Description)),
        );
    }
    if state.pending {
        rows.push(("Sending response…".to_string(), BodyRole::Pending));
    }
    if !state.args_expanded && rows.len() > PERMISSION_COLLAPSED_ROWS {
        rows.truncate(PERMISSION_COLLAPSED_ROWS - 1);
        rows.push(("… Ctrl+F to expand".to_string(), BodyRole::Indicator));
    }
    rows
}

fn clipped_body_rows(rows: &[(String, BodyRole)], budget: usize) -> Vec<(String, BodyRole)> {
    if rows.len() <= budget {
        return rows.to_vec();
    }
    if budget == 0 {
        return Vec::new();
    }
    let mut visible = rows[..budget].to_vec();
    visible[budget - 1] = ("…".to_string(), BodyRole::Indicator);
    visible
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for logical in text.split('\n') {
        if logical.is_empty() {
            rows.push(String::new());
            continue;
        }
        let mut row = String::new();
        let mut row_width = 0usize;
        for character in logical.chars() {
            let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
            if row_width + character_width > width && !row.is_empty() {
                rows.push(std::mem::take(&mut row));
                row_width = 0;
            }
            row.push(character);
            row_width += character_width;
        }
        rows.push(row);
    }
    rows
}

fn truncate_display(text: &str, width: usize) -> String {
    let mut output = String::new();
    let mut used = 0usize;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width {
            break;
        }
        output.push(character);
        used += character_width;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> PermissionViewState {
        PermissionViewState {
            title: "Inspect the workspace".into(),
            command: Some("find /work -maxdepth 3".into()),
            description: Vec::new(),
            options: vec![
                PermissionOption {
                    choice: PermissionChoice::AllowOnce,
                    label: "Yes, proceed".into(),
                },
                PermissionOption {
                    choice: PermissionChoice::Reject,
                    label: "No, reject".into(),
                },
            ],
            active_idx: 0,
            args_expanded: false,
            pending: false,
        }
    }

    #[test]
    fn renders_grok_permission_rail_and_radio_rows() {
        let state = state();
        let area = Rect::new(0, 0, 60, permission_view_height(&state, 24, 55));
        let mut buffer = Buffer::empty(area);
        let result =
            render_permission_view(&mut buffer, area, &state, None, Theme::current(), true);
        let text = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("┃"));
        assert!(text.contains("1 (●) Yes, proceed"));
        assert!(text.contains("2 (○) No, reject"));
        assert_eq!(result.option_rows.len(), 2);
    }

    #[test]
    fn tiny_area_keeps_options_visible_without_writing_outside_buffer() {
        let mut state = state();
        state.command = Some("one two three four five six seven eight nine ten".into());
        let area = Rect::new(0, 0, 24, 6);
        let mut buffer = Buffer::empty(area);
        let result =
            render_permission_view(&mut buffer, area, &state, None, Theme::current(), true);
        assert_eq!(result.option_rows.len(), 2);
        assert!(
            result
                .option_rows
                .iter()
                .all(|row| row.bottom() <= area.bottom())
        );
    }

    #[test]
    fn selection_is_clamped_to_host_options() {
        let mut state = state();
        state.active_idx = 99;
        state.clamp_selection();
        assert_eq!(state.selected(), Some(PermissionChoice::Reject));
    }
}
