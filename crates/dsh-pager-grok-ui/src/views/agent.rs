//! Grok-derived AgentView geometry and main-surface chrome.
//!
//! The upstream AgentView keeps its layout pure and gives the scrollback a
//! minimum before optional rows are admitted. This module carries that same
//! contract into the DSH-neutral UI seam; runtime state is only used to choose
//! requested row heights and to paint the already-computed rectangles.

use std::borrow::Cow;

use crossterm::event::KeyCode;
use dsh_pager_protocol::PromptMode;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::{AppShell, ShellLayout};
use crate::input::{PromptViewport, key::KeyShortcut};
use crate::theme::Theme;
use crate::views::shortcuts_bar::{HintItem, ShortcutsBar};

/// Terminals at or below this height suppress optional prompt-adjacent rows.
pub const SHORT_TERMINAL_ROWS: u16 = 16;
/// The scrollback floor. Prompt growth consumes surplus rows before this one.
pub const SCROLLBACK_MIN_ROWS: u16 = 5;
/// Compact spacing is forced on terminals at or below this height.
pub const AUTO_COMPACT_MAX_ROWS: u16 = 20;

pub fn effective_compact(user_compact: bool, terminal_rows: u16) -> bool {
    user_compact || (terminal_rows > 0 && terminal_rows <= AUTO_COMPACT_MAX_ROWS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentViewLayout {
    pub header: Rect,
    pub transcript: Rect,
    pub rail: Rect,
    pub turn_status: Rect,
    pub banner: Rect,
    pub prompt: Rect,
    pub status_line: Rect,
    pub shortcuts: Rect,
    /// Compatibility alias used by the semantic fallback runner.
    pub footer: Rect,
    pub compact: bool,
    pub outer_hpad: u16,
    pub outer_vpad: u16,
}

impl AgentViewLayout {
    pub fn shell_layout(self) -> ShellLayout {
        ShellLayout {
            header: self.header,
            body: self.transcript,
            prompt: self.prompt,
            footer: self.footer,
        }
    }

    /// Compute the row stack. Optional rows are admitted only after the
    /// scrollback floor has been reserved, mirroring Grok's layout solver.
    pub fn compute(params: AgentViewLayoutParams) -> Self {
        let AgentViewLayoutParams {
            area,
            prompt_height,
            turn_status_height,
            banner_height,
            status_line_height,
            shortcuts_height,
            compact,
        } = params;
        let outer_hpad = if compact { 1 } else { 2 };
        let outer_vpad = if area.height <= SHORT_TERMINAL_ROWS {
            0
        } else {
            1
        };
        let inner = inset(area, outer_hpad, outer_vpad);
        if inner.width == 0 || inner.height == 0 {
            return Self {
                header: Rect::default(),
                transcript: Rect::default(),
                rail: Rect::default(),
                turn_status: Rect::default(),
                banner: Rect::default(),
                prompt: Rect::default(),
                status_line: Rect::default(),
                shortcuts: Rect::default(),
                footer: Rect::default(),
                compact,
                outer_hpad,
                outer_vpad,
            };
        }

        let header_height = 1.min(inner.height);
        let top_gap = u16::from(!compact && inner.height > header_height);
        let turn_status_height = turn_status_height.min(1);
        let banner_height = banner_height.min(1);
        let shortcuts_height = shortcuts_height.min(1);
        let prompt_gap = u16::from(!compact && prompt_height > 0);

        let fixed_without_prompt = header_height
            .saturating_add(top_gap)
            .saturating_add(SCROLLBACK_MIN_ROWS)
            .saturating_add(turn_status_height)
            .saturating_add(u16::from(turn_status_height > 0 && !compact))
            .saturating_add(banner_height)
            .saturating_add(u16::from(banner_height > 0 && !compact))
            .saturating_add(prompt_gap)
            .saturating_add(status_line_height.min(1))
            .saturating_add(shortcuts_height);
        let prompt_height = prompt_height.min(inner.height.saturating_sub(fixed_without_prompt));

        let status_line_height = status_line_height.min(1).min(
            inner.height.saturating_sub(
                header_height
                    .saturating_add(top_gap)
                    .saturating_add(SCROLLBACK_MIN_ROWS)
                    .saturating_add(turn_status_height)
                    .saturating_add(u16::from(turn_status_height > 0 && !compact))
                    .saturating_add(banner_height)
                    .saturating_add(u16::from(banner_height > 0 && !compact))
                    .saturating_add(prompt_gap)
                    .saturating_add(prompt_height)
                    .saturating_add(shortcuts_height),
            ),
        );

        let header = Rect::new(inner.x, inner.y, inner.width, header_height);
        let transcript_y = header.bottom().saturating_add(top_gap);
        let shortcuts = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(shortcuts_height),
            inner.width,
            shortcuts_height,
        );
        let status_line = if status_line_height > 0 {
            Rect::new(
                inner.x,
                shortcuts.y.saturating_sub(status_line_height),
                inner.width,
                status_line_height,
            )
        } else {
            Rect::default()
        };
        let prompt_bottom = if status_line.height > 0 {
            status_line.y
        } else {
            shortcuts.y
        };
        let prompt = if prompt_height > 0 {
            Rect::new(
                inner.x,
                prompt_bottom
                    .saturating_sub(prompt_height)
                    .saturating_sub(prompt_gap),
                inner.width,
                prompt_height,
            )
        } else {
            Rect::default()
        };
        let banner = if banner_height > 0 {
            let bottom = prompt.y.saturating_sub(u16::from(!compact));
            Rect::new(
                inner.x,
                bottom.saturating_sub(banner_height),
                inner.width,
                banner_height,
            )
        } else {
            Rect::default()
        };
        let turn_status = if turn_status_height > 0 {
            let banner_anchor = if banner.height > 0 {
                banner.y.saturating_sub(u16::from(!compact))
            } else {
                prompt.y
            };
            Rect::new(
                inner.x,
                banner_anchor.saturating_sub(turn_status_height),
                inner.width,
                turn_status_height,
            )
        } else {
            Rect::default()
        };
        let transcript_bottom = if turn_status.height > 0 {
            turn_status.y.saturating_sub(u16::from(!compact))
        } else if banner.height > 0 {
            banner.y.saturating_sub(u16::from(!compact))
        } else {
            prompt.y
        };
        let transcript_height = transcript_bottom.saturating_sub(transcript_y);
        let (transcript, rail) = split_transcript(Rect::new(
            inner.x,
            transcript_y,
            inner.width,
            transcript_height,
        ));

        Self {
            header,
            transcript,
            rail,
            turn_status,
            banner,
            prompt,
            status_line,
            shortcuts,
            footer: if status_line.height > 0 {
                status_line
            } else {
                shortcuts
            },
            compact,
            outer_hpad,
            outer_vpad,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AgentViewLayoutParams {
    pub area: Rect,
    pub prompt_height: u16,
    pub turn_status_height: u16,
    pub banner_height: u16,
    pub status_line_height: u16,
    pub shortcuts_height: u16,
    pub compact: bool,
}

pub(crate) struct PromptRenderState<'a> {
    pub mode: PromptMode,
    pub running: bool,
    pub title: &'a str,
    pub model: &'a str,
    pub viewport: &'a PromptViewport,
    pub empty: bool,
}

pub struct AgentView;

impl AgentView {
    /// Keep AppShell's revision/invalidation accounting while making AgentView
    /// the owner of the production geometry.
    pub fn layout(shell: &mut AppShell, area: Rect) -> AgentViewLayout {
        Self::layout_with_prompt(shell, area, "", false, false)
    }

    pub fn layout_with_prompt(
        shell: &mut AppShell,
        area: Rect,
        prompt_text: &str,
        running: bool,
        status_visible: bool,
    ) -> AgentViewLayout {
        let _ = shell.layout(area);
        let compact = effective_compact(false, area.height);
        let inner_width = area.width.saturating_sub(if compact { 2 } else { 4 });
        let prompt_width = inner_width.saturating_sub(6).max(1) as usize;
        let wrapped = wrapped_prompt_lines(prompt_text, prompt_width);
        let prompt_floor: u16 = if compact { 3 } else { 4 };
        let prompt_cap: u16 = if compact { 6 } else { 8 };
        let prompt_height = prompt_floor
            .saturating_add(wrapped.saturating_sub(1) as u16)
            .min(prompt_cap);
        let short = area.height <= SHORT_TERMINAL_ROWS;
        AgentViewLayout::compute(AgentViewLayoutParams {
            area,
            prompt_height,
            turn_status_height: u16::from(running && !short),
            banner_height: 0,
            status_line_height: u16::from(status_visible && !short),
            shortcuts_height: 1,
            compact,
        })
    }

    pub fn prompt_label(mode: PromptMode, running: bool) -> &'static str {
        match (mode, running) {
            (PromptMode::Steer, true) => " ! ",
            (PromptMode::Steer, false) => " ~ ",
            (PromptMode::Queue, true) => " > ",
            (PromptMode::Queue, false) => " · ",
        }
    }

    pub fn mode_label(mode: PromptMode) -> &'static str {
        match mode {
            PromptMode::Queue => "queue",
            PromptMode::Steer => "steer",
        }
    }

    /// Render Grok's prompt chrome around the DSH editor viewport.
    pub(crate) fn render_prompt(
        frame: &mut Frame<'_>,
        area: Rect,
        state: PromptRenderState<'_>,
        theme: &Theme,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let accent = theme.accent_user;
        let border = if state.running {
            accent
        } else {
            theme.bg_light
        };
        let mode_name = Self::mode_label(state.mode);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border).bg(theme.bg_base))
            .style(Style::default().bg(theme.bg_base))
            .title(Span::styled(
                format!(" {mode_name} "),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ))
            .title_alignment(ratatui::layout::Alignment::Right);
        frame.render_widget(block, area);

        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let accent_x = inner.x;
        for y in inner.y..inner.bottom() {
            if let Some(cell) = frame.buffer_mut().cell_mut((accent_x, y)) {
                cell.set_char('┃');
                cell.set_style(Style::default().fg(accent).bg(theme.bg_base));
            }
        }
        let text_x = inner.x.saturating_add(2);
        let text_width = inner.width.saturating_sub(2).max(1);
        let info_height = u16::from(inner.height >= 3);
        let input_height = inner.height.saturating_sub(info_height).max(1);
        let input_area = Rect::new(text_x, inner.y, text_width, input_height);
        let prefix = Self::prompt_label(state.mode, state.running);
        let prefix_width = prefix.width() as u16;
        let placeholder = if state.empty {
            "Ask DeepSeek anything…"
        } else {
            ""
        };
        let lines = if state.empty {
            vec![Line::from(vec![
                Span::styled(prefix, Style::default().fg(accent)),
                Span::styled(
                    fit_text(
                        placeholder,
                        text_width.saturating_sub(prefix_width) as usize,
                    ),
                    Style::default().fg(theme.gray_dim),
                ),
            ])]
        } else {
            state
                .viewport
                .lines
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let lead = if index == 0 {
                        prefix.to_string()
                    } else {
                        " ".repeat(prefix_width as usize)
                    };
                    Line::from(Span::styled(
                        format!("{lead}{line}"),
                        Style::default().fg(theme.text_primary),
                    ))
                })
                .collect::<Vec<_>>()
        };
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .style(Style::default().bg(theme.bg_base))
                .wrap(Wrap { trim: false }),
            input_area,
        );

        if info_height > 0 {
            let info_y = inner.bottom().saturating_sub(1);
            let left = fit_text(
                &format!("{} · {}", state.model, state.title),
                text_width as usize,
            );
            let right = if state.running { "running" } else { "idle" };
            let right_width = right.width() as u16;
            let buf = frame.buffer_mut();
            buf.set_string(
                text_x,
                info_y,
                left,
                Style::default().fg(theme.gray_dim).bg(theme.bg_base),
            );
            if right_width < text_width {
                buf.set_string(
                    text_x + text_width.saturating_sub(right_width),
                    info_y,
                    right,
                    Style::default()
                        .fg(if state.running { accent } else { theme.gray })
                        .bg(theme.bg_base),
                );
            }
        }

        let cursor_x = text_x
            .saturating_add(prefix_width)
            .saturating_add(state.viewport.cursor_x as u16)
            .min(area.right().saturating_sub(2));
        let cursor_y = inner
            .y
            .saturating_add(state.viewport.cursor_y as u16)
            .min(input_area.bottom().saturating_sub(1));
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }

    pub fn render_turn_status(
        frame: &mut Frame<'_>,
        area: Rect,
        running: bool,
        status: Option<&str>,
        theme: &Theme,
    ) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let (glyph, label, color) = if running {
            (
                '●',
                status.unwrap_or("Generating response"),
                theme.accent_user,
            )
        } else {
            ('○', status.unwrap_or("Ready"), theme.gray)
        };
        let text = format!(" {glyph} {label}");
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                fit_text(&text, area.width as usize),
                Style::default().fg(color).bg(theme.bg_base),
            )))
            .style(Style::default().bg(theme.bg_base)),
            area,
        );
    }

    pub fn render_status_line(frame: &mut Frame<'_>, area: Rect, text: &str, theme: &Theme) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                fit_text(text, area.width as usize),
                Style::default().fg(theme.gray_dim).bg(theme.bg_base),
            )))
            .style(Style::default().bg(theme.bg_base)),
            area,
        );
    }

    pub fn render_shortcuts(frame: &mut Frame<'_>, area: Rect, compact: bool) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let hints = [
            HintItem::new(KeyShortcut::key(KeyCode::Enter), Cow::Borrowed("send")),
            HintItem::new(
                KeyShortcut::key(KeyCode::Char('p')),
                Cow::Borrowed("sessions"),
            ),
            HintItem::new(KeyShortcut::key(KeyCode::Char('q')), Cow::Borrowed("queue")),
            HintItem::new(KeyShortcut::key(KeyCode::Esc), Cow::Borrowed("clear/quit")),
        ];
        let widget = if compact {
            ShortcutsBar::new(&hints).compact(2, None)
        } else {
            ShortcutsBar::new(&hints)
        };
        frame.render_widget(widget, area);
    }
}

fn inset(area: Rect, hpad: u16, vpad: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(hpad),
        area.y.saturating_add(vpad),
        area.width.saturating_sub(hpad.saturating_mul(2)),
        area.height.saturating_sub(vpad.saturating_mul(2)),
    )
}

fn split_transcript(body: Rect) -> (Rect, Rect) {
    if body.width < 6 {
        return (body, Rect::new(body.right(), body.y, 0, body.height));
    }
    (
        Rect::new(body.x, body.y, body.width.saturating_sub(2), body.height),
        Rect::new(body.right().saturating_sub(2), body.y, 2, body.height),
    )
}

fn wrapped_prompt_lines(text: &str, width: usize) -> usize {
    let width = width.max(1);
    text.split('\n')
        .map(|line| {
            if line.is_empty() {
                1
            } else {
                line.width().saturating_add(width).saturating_sub(1) / width
            }
        })
        .sum::<usize>()
        .max(1)
}

fn fit_text(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut output = String::new();
    let mut used = 0usize;
    for character in text.chars() {
        let character_width = character.to_string().width();
        if used.saturating_add(character_width).saturating_add(1) > width {
            break;
        }
        output.push(character);
        used += character_width;
    }
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppShell;

    #[test]
    fn layout_keeps_grok_outer_chrome_and_transcript_floor() {
        let mut shell = AppShell::default();
        let layout =
            AgentView::layout_with_prompt(&mut shell, Rect::new(0, 0, 80, 24), "", true, true);
        assert_eq!(layout.outer_hpad, 2);
        assert_eq!(layout.outer_vpad, 1);
        assert!(layout.transcript.height >= SCROLLBACK_MIN_ROWS);
        assert_eq!(layout.transcript.right(), layout.rail.x);
        assert!(layout.prompt.y > layout.transcript.y);
        assert_eq!(layout.shortcuts.bottom(), 23);
    }

    #[test]
    fn short_terminal_suppresses_optional_rows_without_losing_prompt() {
        let mut shell = AppShell::default();
        let layout = AgentView::layout_with_prompt(
            &mut shell,
            Rect::new(0, 0, 40, 12),
            "a multiline prompt that wraps",
            true,
            true,
        );
        assert!(layout.compact);
        assert_eq!(layout.outer_vpad, 0);
        assert_eq!(layout.turn_status.height, 0);
        assert_eq!(layout.status_line.height, 0);
        assert!(layout.prompt.height >= 3);
        assert!(layout.transcript.height >= SCROLLBACK_MIN_ROWS);
        assert!(layout.shortcuts.bottom() <= 12);
    }

    #[test]
    fn prompt_budget_grows_for_multiline_text_but_is_capped() {
        let mut shell = AppShell::default();
        let short =
            AgentView::layout_with_prompt(&mut shell, Rect::new(0, 0, 120, 40), "one", false, true);
        let long = AgentView::layout_with_prompt(
            &mut shell,
            Rect::new(0, 0, 120, 40),
            &"x".repeat(800),
            false,
            true,
        );
        assert!(long.prompt.height > short.prompt.height);
        assert!(long.prompt.height <= 8);
        assert!(long.transcript.height >= SCROLLBACK_MIN_ROWS);
    }

    #[test]
    fn prompt_labels_expose_queue_and_steer_modes() {
        assert_eq!(AgentView::prompt_label(PromptMode::Queue, true), " > ");
        assert_eq!(AgentView::prompt_label(PromptMode::Steer, true), " ! ");
        assert_eq!(AgentView::mode_label(PromptMode::Steer), "steer");
    }
}
