//! Grok rewind picker adapted to DSH's completed-turn fork seam.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use dsh_pager::DshSeq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewindPoint {
    /// Chronological prompt index. The picker itself is newest-first.
    pub prompt_index: usize,
    pub prompt_preview: String,
    /// Exact composer text restored after the history prefix is attached.
    pub prompt_text: String,
    /// DSH fork anchor for the completed turn before this prompt. `None`
    /// means rewinding the first prompt and therefore creating a blank session.
    pub fork_at_seq: Option<DshSeq>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewindPhase {
    Picker {
        points: Vec<RewindPoint>,
        selected: usize,
    },
    Confirm {
        target_prompt_index: usize,
        active_idx: usize,
        prompt_preview: String,
    },
    Executing {
        target_prompt_index: usize,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewindState {
    pub phase: RewindPhase,
}

impl RewindState {
    pub fn picker(points: Vec<RewindPoint>) -> Self {
        Self {
            phase: RewindPhase::Picker {
                points,
                selected: 0,
            },
        }
    }

    pub fn point(&self, prompt_index: usize) -> Option<&RewindPoint> {
        match &self.phase {
            RewindPhase::Picker { points, .. } => points
                .iter()
                .find(|point| point.prompt_index == prompt_index),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindInput {
    Dismissed,
    DismissError,
    Confirm(usize),
    ConfirmNeverAsk(usize),
    PickerSelect(usize),
    MoveUp,
    MoveDown,
    ConfirmCursor,
    Consumed,
}

const CONFIRM_OPTIONS: usize = 3;

pub fn handle_rewind_key(state: &RewindState, key: &KeyEvent) -> RewindInput {
    if key.kind == KeyEventKind::Release {
        return RewindInput::Consumed;
    }
    match &state.phase {
        RewindPhase::Picker { points, selected } => match key.code {
            KeyCode::Char('j') | KeyCode::Down => RewindInput::MoveDown,
            KeyCode::Char('k') | KeyCode::Up => RewindInput::MoveUp,
            KeyCode::Enter => points
                .get(*selected)
                .map(|point| RewindInput::PickerSelect(point.prompt_index))
                .unwrap_or(RewindInput::Consumed),
            KeyCode::Esc => RewindInput::Dismissed,
            _ => RewindInput::Consumed,
        },
        RewindPhase::Confirm {
            target_prompt_index,
            ..
        } => match key.code {
            KeyCode::Char('y') => RewindInput::Confirm(*target_prompt_index),
            KeyCode::Char('a') => RewindInput::ConfirmNeverAsk(*target_prompt_index),
            KeyCode::Char('n') | KeyCode::Esc => RewindInput::Dismissed,
            KeyCode::Char('j') | KeyCode::Down => RewindInput::MoveDown,
            KeyCode::Char('k') | KeyCode::Up => RewindInput::MoveUp,
            KeyCode::Enter => RewindInput::ConfirmCursor,
            _ => RewindInput::Consumed,
        },
        RewindPhase::Error { .. } => match key.code {
            KeyCode::Esc | KeyCode::Enter => RewindInput::DismissError,
            _ => RewindInput::Consumed,
        },
        RewindPhase::Executing { .. } => RewindInput::Consumed,
    }
}

pub fn move_cursor(phase: &mut RewindPhase, delta: i32) {
    match phase {
        RewindPhase::Picker { points, selected } => {
            if points.is_empty() {
                return;
            }
            *selected = (*selected as i32 + delta).clamp(0, points.len() as i32 - 1) as usize;
        }
        RewindPhase::Confirm { active_idx, .. } => {
            *active_idx =
                (*active_idx as i32 + delta).clamp(0, CONFIRM_OPTIONS as i32 - 1) as usize;
        }
        _ => {}
    }
}

pub fn confirm_cursor(phase: &RewindPhase) -> RewindInput {
    match phase {
        RewindPhase::Confirm {
            target_prompt_index,
            active_idx,
            ..
        } => match active_idx {
            0 => RewindInput::Confirm(*target_prompt_index),
            1 => RewindInput::ConfirmNeverAsk(*target_prompt_index),
            _ => RewindInput::Dismissed,
        },
        _ => RewindInput::Consumed,
    }
}

pub fn rewind_row_at(phase: &RewindPhase, area: Rect, col: u16, row: u16) -> Option<usize> {
    if area.height == 0
        || area.width < 10
        || col < area.x
        || col >= area.right()
        || row < area.y
        || row >= area.bottom()
    {
        return None;
    }
    match phase {
        RewindPhase::Picker { points, selected } => crate::views::overlay_list::ListOverlay {
            len: points.len(),
            selected: *selected,
        }
        .row_at(area, col, row),
        RewindPhase::Confirm { .. } => match row.checked_sub(area.y + 2) {
            Some(0) => Some(0),
            Some(1) => Some(1),
            Some(2) => Some(2),
            _ => None,
        },
        RewindPhase::Error { .. } => (row == area.y + 3).then_some(0),
        RewindPhase::Executing { .. } => None,
    }
}

pub fn set_rewind_cursor(phase: &mut RewindPhase, index: usize) -> bool {
    match phase {
        RewindPhase::Picker { points, selected } if !points.is_empty() => {
            let next = index.min(points.len() - 1);
            let changed = *selected != next;
            *selected = next;
            changed
        }
        RewindPhase::Confirm { active_idx, .. } => {
            let next = index.min(CONFIRM_OPTIONS - 1);
            let changed = *active_idx != next;
            *active_idx = next;
            changed
        }
        _ => false,
    }
}

pub fn rewind_activate(phase: &RewindPhase) -> RewindInput {
    match phase {
        RewindPhase::Picker { points, selected } => points
            .get(*selected)
            .map(|point| RewindInput::PickerSelect(point.prompt_index))
            .unwrap_or(RewindInput::Consumed),
        RewindPhase::Error { .. } => RewindInput::DismissError,
        other => confirm_cursor(other),
    }
}

pub fn rewind_overlay_height(phase: &RewindPhase, screen_height: u16) -> u16 {
    match phase {
        RewindPhase::Picker { points, selected } => crate::views::overlay_list::ListOverlay {
            len: points.len(),
            selected: *selected,
        }
        .height(screen_height),
        RewindPhase::Executing { .. } => 3,
        RewindPhase::Confirm { .. } => 6,
        RewindPhase::Error { .. } => 5,
    }
}

pub fn render_rewind_overlay(buf: &mut Buffer, area: Rect, phase: &RewindPhase, focused: bool) {
    if area.height == 0 || area.width < 10 {
        return;
    }
    let theme = Theme::current();
    let background = theme.bg_light;

    if let RewindPhase::Picker { points, selected } = phase {
        crate::views::overlay_list::ListOverlay {
            len: points.len(),
            selected: *selected,
        }
        .render(
            buf,
            area,
            "Rewind to which turn?",
            focused,
            |index, context| {
                let point = &points[index];
                let preview = crate::render::line_utils::truncate_str(
                    &point.prompt_preview,
                    context.content_width.saturating_sub(8) as usize,
                );
                let bold = if context.is_cursor {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                };
                Line::from(vec![
                    Span::styled(
                        "\u{00B7} ",
                        Style::default().fg(theme.gray).bg(context.row_bg),
                    ),
                    Span::styled(
                        preview,
                        Style::default()
                            .fg(theme.text_primary)
                            .bg(context.row_bg)
                            .add_modifier(bold),
                    ),
                ])
            },
        );
        return;
    }

    buf.set_style(area, Style::default().bg(background));
    for row in area.y..area.bottom() {
        if let Some(cell) = buf.cell_mut((area.x, row)) {
            cell.set_symbol(crate::glyphs::accent_bar());
            cell.set_style(Style::default().fg(theme.accent_user));
        }
    }
    let content_x = area.x + 3;
    let content_width = area.width.saturating_sub(5);
    let title_style = Style::default()
        .fg(theme.accent_user)
        .add_modifier(Modifier::BOLD);

    match phase {
        RewindPhase::Executing { .. } => {
            buf.set_line(
                content_x,
                area.y + 1,
                &Line::from(Span::styled(
                    "Rewinding...",
                    Style::default().fg(theme.gray),
                )),
                content_width,
            );
        }
        RewindPhase::Confirm {
            active_idx,
            prompt_preview,
            ..
        } => {
            let prefix = "Rewind conversation to \u{201C}";
            let suffix = "\u{201D}?";
            let chrome = prefix.chars().count() + suffix.chars().count();
            let max_preview = (content_width as usize).saturating_sub(chrome + 1);
            let preview = if prompt_preview.chars().count() > max_preview {
                format!(
                    "{}\u{2026}",
                    prompt_preview
                        .chars()
                        .take(max_preview.saturating_sub(1))
                        .collect::<String>()
                )
            } else {
                prompt_preview.clone()
            };
            buf.set_line(
                content_x,
                area.y + 1,
                &Line::from(Span::styled(
                    format!("{prefix}{preview}{suffix}"),
                    title_style,
                )),
                content_width,
            );
            for (offset, (key, label)) in
                [('y', "Yes"), ('a', "Yes, and don't ask again"), ('n', "No")]
                    .into_iter()
                    .enumerate()
            {
                render_radio_row(
                    buf,
                    content_x,
                    area.y + 2 + offset as u16,
                    content_width,
                    RadioRow {
                        key,
                        label,
                        is_cursor: *active_idx == offset,
                    },
                    focused,
                    &theme,
                );
            }
        }
        RewindPhase::Error { message } => {
            buf.set_line(
                content_x,
                area.y + 1,
                &Line::from(Span::styled(
                    "Rewind failed",
                    Style::default()
                        .fg(theme.accent_error)
                        .add_modifier(Modifier::BOLD),
                )),
                content_width,
            );
            buf.set_line(
                content_x,
                area.y + 2,
                &Line::from(Span::styled(
                    message
                        .chars()
                        .take(content_width as usize)
                        .collect::<String>(),
                    Style::default().fg(theme.text_primary),
                )),
                content_width,
            );
            render_radio_row(
                buf,
                content_x,
                area.y + 3,
                content_width,
                RadioRow {
                    key: '\x1b',
                    label: "Dismiss",
                    is_cursor: true,
                },
                focused,
                &theme,
            );
        }
        RewindPhase::Picker { .. } => unreachable!(),
    }
    if !focused {
        crate::views::overlay_list::dim_foreground(buf, area, background, 0.66);
    }
}

struct RadioRow<'a> {
    key: char,
    label: &'a str,
    is_cursor: bool,
}

fn render_radio_row(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    row: RadioRow<'_>,
    focused: bool,
    theme: &Theme,
) {
    let background = if row.is_cursor && focused {
        theme.bg_visual
    } else {
        theme.bg_light
    };
    buf.set_style(
        Rect::new(x.saturating_sub(1), y, width + 2, 1),
        Style::default().bg(background),
    );
    let marker = if row.is_cursor {
        crate::glyphs::filled_dot()
    } else {
        "\u{25CB}"
    };
    let key_label = if row.key == '\x1b' {
        "Esc".to_string()
    } else {
        row.key.to_string()
    };
    buf.set_line(
        x,
        y,
        &Line::from(vec![
            Span::styled(
                format!("{key_label:<4}"),
                Style::default().fg(theme.accent_user).bg(background),
            ),
            Span::styled(
                format!("({marker}) "),
                Style::default()
                    .fg(if row.is_cursor {
                        theme.accent_user
                    } else {
                        theme.gray
                    })
                    .bg(background),
            ),
            Span::styled(
                row.label.to_string(),
                Style::default()
                    .fg(theme.text_primary)
                    .bg(background)
                    .add_modifier(if row.is_cursor {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ]),
        width,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn point(index: usize) -> RewindPoint {
        RewindPoint {
            prompt_index: index,
            prompt_preview: format!("turn {index}"),
            prompt_text: format!("turn {index}"),
            fork_at_seq: index.checked_sub(1).map(|seq| DshSeq::new(seq as i64)),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn picker_and_confirm_keys_match_grok() {
        let mut state = RewindState::picker(vec![point(2), point(1), point(0)]);
        assert_eq!(
            handle_rewind_key(&state, &key(KeyCode::Enter)),
            RewindInput::PickerSelect(2)
        );
        move_cursor(&mut state.phase, 1);
        assert_eq!(
            handle_rewind_key(&state, &key(KeyCode::Enter)),
            RewindInput::PickerSelect(1)
        );
        state.phase = RewindPhase::Confirm {
            target_prompt_index: 1,
            active_idx: 0,
            prompt_preview: "turn 1".into(),
        };
        assert_eq!(
            handle_rewind_key(&state, &key(KeyCode::Char('a'))),
            RewindInput::ConfirmNeverAsk(1)
        );
        assert_eq!(
            handle_rewind_key(&state, &key(KeyCode::Esc)),
            RewindInput::Dismissed
        );
    }

    #[test]
    fn picker_hit_test_uses_shared_overlay_geometry() {
        let phase = RewindPhase::Picker {
            points: vec![point(2), point(1), point(0)],
            selected: 0,
        };
        let area = Rect::new(0, 0, 40, 10);
        assert_eq!(rewind_row_at(&phase, area, 5, 1), None);
        assert_eq!(rewind_row_at(&phase, area, 5, 2), Some(0));
        assert_eq!(rewind_row_at(&phase, area, 5, 4), Some(2));
    }
}
