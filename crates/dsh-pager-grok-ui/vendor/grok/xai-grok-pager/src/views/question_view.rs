//! Grok-derived blocking question card (`ask_user_question`).
//!
//! Upstream also owns ACP oneshot replies, `/fork` local kinds, plan-mode
//! bottom actions, markdown previews and syntect. This B adaptation keeps the
//! composer placement, 33% height cap, accent rail, chrome/options geometry,
//! radio-row focus/hover styling and hit rectangles. DSH mapping stays in
//! `src/views/interaction.rs`.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// Pure renderer state. Request identity stays outside the copied component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionViewState {
    pub header: Option<String>,
    pub title: String,
    pub detail: Vec<String>,
    pub options: Vec<QuestionOption>,
    pub active_idx: usize,
    pub args_expanded: bool,
    pub pending: bool,
}

impl QuestionViewState {
    pub fn selected_label(&self) -> Option<&str> {
        self.options
            .get(self.active_idx)
            .map(|option| option.label.as_str())
    }

    pub fn clamp_selection(&mut self) {
        self.active_idx = self.active_idx.min(self.options.len().saturating_sub(1));
    }

    pub fn has_collapsible_display(&self, content_w: usize) -> bool {
        let mut expanded = self.clone();
        expanded.args_expanded = true;
        visible_body_rows(&expanded, content_w).len() > QUESTION_COLLAPSED_ROWS
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestionViewRenderResult {
    pub option_rows: Vec<Rect>,
}

pub const QUESTION_VIEW_HPAD: u16 = 5;
pub const QUESTION_COLLAPSED_ROWS: usize = 5;

/// Grok question overlay cap: 33% of screen, min 8, max 80%.
pub fn question_view_height(state: &QuestionViewState, screen_h: u16, content_w: usize) -> u16 {
    let body_rows = visible_body_rows(state, content_w).len() as u16;
    let total = 1u16
        .saturating_add(1) // top padding + title
        .saturating_add(body_rows)
        .saturating_add(1) // gap before options
        .saturating_add(state.options.len().max(1) as u16)
        .saturating_add(1); // bottom padding
    if state.args_expanded {
        return total.min(screen_h);
    }
    let cap = (u32::from(screen_h) * 33 / 100)
        .max(8)
        .min(u32::from(screen_h) * 80 / 100) as u16;
    total.min(cap).min(screen_h)
}

/// Render the question card that replaces the normal composer.
pub fn render_question_view(
    buf: &mut Buffer,
    area: Rect,
    state: &QuestionViewState,
    hovered_item: Option<usize>,
    theme: &Theme,
    focused: bool,
) -> QuestionViewRenderResult {
    if area.width == 0 || area.height == 0 {
        return QuestionViewRenderResult::default();
    }

    buf.set_style(area, Style::default().bg(theme.bg_light));
    for row in area.y..area.bottom() {
        if let Some(cell) = buf.cell_mut((area.x, row)) {
            cell.set_symbol("┃");
            cell.set_style(Style::default().fg(theme.accent_user));
        }
    }

    let content_x = area.x.saturating_add(3);
    let content_width = area.width.saturating_sub(QUESTION_VIEW_HPAD);
    if content_width == 0 {
        return QuestionViewRenderResult::default();
    }

    let option_count = state.options.len().min(area.height as usize);
    let fixed_rows = 1u16
        .saturating_add(1)
        .saturating_add(1)
        .saturating_add(option_count.max(1) as u16)
        .saturating_add(1);
    let body_budget = area.height.saturating_sub(fixed_rows) as usize;
    let body = clipped_body_rows(
        &visible_body_rows(state, content_width as usize),
        body_budget,
    );

    let mut y = area.y.saturating_add(1);
    if y < area.bottom() {
        let title = state.header.as_deref().unwrap_or(state.title.as_str());
        buf.set_line(
            content_x,
            y,
            &Line::from(Span::styled(
                truncate_display(title, content_width as usize),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            content_width,
        );
    }
    y = y.saturating_add(1);

    for (text, role) in body {
        if y >= area.bottom() {
            break;
        }
        let color = match role {
            BodyRole::Title => theme.text_primary,
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
            &question_option_line(option, index, selected, row_bg, state.pending, theme),
            content_width,
        );
        option_rows.push(row);
        y = y.saturating_add(1);
    }

    QuestionViewRenderResult { option_rows }
}

fn question_option_line<'a>(
    option: &QuestionOption,
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
    let label_color = if pending {
        theme.gray
    } else {
        theme.text_primary
    };
    let label_modifier = if selected && !pending {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };
    let mut spans = vec![
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
    ];
    if let Some(description) = option
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        spans.push(Span::styled(
            format!("  {description}"),
            Style::default().fg(theme.gray).bg(row_bg),
        ));
    }
    Line::from(spans).style(Style::default().bg(row_bg))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyRole {
    Title,
    Description,
    Pending,
    Indicator,
}

fn visible_body_rows(state: &QuestionViewState, width: usize) -> Vec<(String, BodyRole)> {
    let mut rows = Vec::new();
    if state.header.is_some() && state.title != state.header.as_deref().unwrap_or_default() {
        rows.extend(
            wrap_text(&state.title, width)
                .into_iter()
                .map(|row| (row, BodyRole::Title)),
        );
    }
    for detail in &state.detail {
        rows.extend(
            wrap_text(detail, width)
                .into_iter()
                .map(|row| (row, BodyRole::Description)),
        );
    }
    if state.pending {
        rows.push(("Sending response…".to_string(), BodyRole::Pending));
    }
    if !state.args_expanded && rows.len() > QUESTION_COLLAPSED_ROWS {
        rows.truncate(QUESTION_COLLAPSED_ROWS - 1);
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

    fn state() -> QuestionViewState {
        QuestionViewState {
            header: Some("Plan review".into()),
            title: "Approve this plan and leave plan mode?".into(),
            detail: vec!["# Plan".into(), "- ship the HTML report".into()],
            options: vec![
                QuestionOption {
                    label: "Approve".into(),
                    description: Some("Leave plan mode".into()),
                },
                QuestionOption {
                    label: "Keep planning".into(),
                    description: Some("Stay in plan mode".into()),
                },
            ],
            active_idx: 0,
            args_expanded: false,
            pending: false,
        }
    }

    fn buffer_text(buffer: &Buffer, area: Rect) -> String {
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_grok_question_rail_and_radio_rows() {
        let state = state();
        let area = Rect::new(0, 0, 60, question_view_height(&state, 24, 55));
        let mut buffer = Buffer::empty(area);
        let result = render_question_view(&mut buffer, area, &state, None, Theme::current(), true);
        let text = buffer_text(&buffer, area);
        assert!(text.contains("┃"));
        assert!(text.contains("Plan review"));
        assert!(text.contains("1 (●) Approve"));
        assert!(text.contains("2 (○) Keep planning"));
        assert!(text.contains("Leave plan mode"));
        assert_eq!(result.option_rows.len(), 2);
    }

    #[test]
    fn tiny_area_keeps_options_inside_the_buffer() {
        let state = state();
        let area = Rect::new(0, 0, 28, 8);
        let mut buffer = Buffer::empty(area);
        let result = render_question_view(&mut buffer, area, &state, None, Theme::current(), true);
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
        assert_eq!(state.selected_label(), Some("Keep planning"));
    }
}
