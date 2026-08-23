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
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::{AppShell, ShellLayout};
use crate::appearance::GrokAppearanceSnapshot;
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
        let appearance = GrokAppearanceSnapshot::for_area(area, compact);
        let outer_hpad = appearance.outer_hpad;
        let outer_vpad = appearance.outer_vpad;
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
        let top_gap = u16::from(outer_vpad > 0);
        let turn_status_height = turn_status_height.min(1);
        let banner_height = banner_height.min(1);
        let shortcuts_height = shortcuts_height.min(1);
        let prompt_gap = u16::from(!compact && prompt_height > 0);
        let status_line_height = status_line_height.min(1);
        let mut constraints = vec![Constraint::Length(header_height)];
        if top_gap > 0 {
            constraints.push(Constraint::Length(top_gap));
        }
        constraints.push(Constraint::Min(appearance.scrollback_min_rows));
        if turn_status_height > 0 {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(turn_status_height));
        }
        if banner_height > 0 {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(banner_height));
        }
        if prompt_gap > 0 {
            constraints.push(Constraint::Length(prompt_gap));
        }
        constraints.push(Constraint::Length(prompt_height));
        let reserved = constraints
            .iter()
            .map(|constraint| match constraint {
                Constraint::Length(rows) | Constraint::Min(rows) => *rows,
                _ => 0,
            })
            .fold(0u16, u16::saturating_add)
            .saturating_add(shortcuts_height);
        let status_line_height = status_line_height.min(inner.height.saturating_sub(reserved));
        if status_line_height > 0 {
            constraints.push(Constraint::Length(status_line_height));
        }
        constraints.push(Constraint::Length(shortcuts_height));
        let chunks = Layout::vertical(constraints).split(inner);
        let mut index = 0usize;
        let header = chunks[index];
        index += 1;
        if top_gap > 0 {
            index += 1;
        }
        let (transcript, rail) = split_transcript(chunks[index]);
        index += 1;
        let turn_status = if turn_status_height > 0 {
            index += 1;
            let rect = chunks[index];
            index += 1;
            rect
        } else {
            Rect::default()
        };
        let banner = if banner_height > 0 {
            index += 1;
            let rect = chunks[index];
            index += 1;
            rect
        } else {
            Rect::default()
        };
        if prompt_gap > 0 {
            index += 1;
        }
        let prompt = chunks[index];
        index += 1;
        let status_line = if status_line_height > 0 {
            let rect = chunks[index];
            index += 1;
            rect
        } else {
            Rect::default()
        };
        let shortcuts = chunks[index];

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
    pub focused: bool,
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
        // Grok's prompt is vpad_top + textarea rows + one info/divider row.
        // The one-line empty prompt is therefore three rows in every mode.
        let prompt_floor: u16 = 3;
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
        let _ = (mode, running);
        "❯ "
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
        if area.width < 4 || area.height == 0 {
            return;
        }
        let focused = state.focused;
        let cursor = Self::render_prompt_buffer(frame.buffer_mut(), area, state, theme);
        if focused && let Some((x, y)) = cursor {
            frame.set_cursor_position(Position::new(x, y));
        }
    }

    /// Buffer-level prompt renderer. Keeping this separate from `Frame` makes
    /// the exact cell contract testable without a terminal backend.
    pub(crate) fn render_prompt_buffer(
        buf: &mut Buffer,
        area: Rect,
        state: PromptRenderState<'_>,
        theme: &Theme,
    ) -> Option<(u16, u16)> {
        let bg = theme.bg_base;
        let border = if state.focused {
            theme.prompt_border_active
        } else {
            theme.prompt_border
        };
        buf.set_style(area, Style::default().fg(theme.text_primary).bg(bg));

        let content = Rect::new(
            area.x.saturating_add(2),
            area.y,
            area.width.saturating_sub(4),
            area.height,
        );
        let info_block = u16::from(area.height >= 3);
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(info_block),
        ])
        .split(content);
        let text_area = chunks[1];

        if area.height >= 3 {
            draw_horizontal_divider(buf, area, area.y, '╭', '╮', border, bg);
            let title = state.title.trim();
            if !title.is_empty() && area.width >= 12 {
                let label = format!(" {title} ");
                let max_width = area.width.saturating_sub(6) as usize;
                let label = fit_text(&label, max_width);
                let x = area.right().saturating_sub(3 + label.width() as u16);
                buf.set_string(
                    x,
                    area.y,
                    label,
                    Style::default().fg(theme.accent_model).bg(bg),
                );
            }
        }

        let prefix = Self::prompt_label(state.mode, state.running);
        let prefix_width = prefix.width() as u16;
        if text_area.width > prefix_width {
            buf.set_string(
                text_area.x,
                text_area.y,
                prefix,
                Style::default().fg(theme.accent_user).bg(bg),
            );
        }
        let input = Rect::new(
            text_area.x.saturating_add(prefix_width),
            text_area.y,
            text_area.width.saturating_sub(prefix_width),
            text_area.height,
        );
        let placeholder = "Build anything";
        for (row, line) in state.viewport.lines.iter().enumerate() {
            let y = input.y.saturating_add(row as u16);
            if y >= input.bottom() {
                break;
            }
            let text = fit_text(line, input.width as usize);
            if !text.is_empty() {
                buf.set_string(
                    input.x,
                    y,
                    text,
                    Style::default().fg(theme.text_primary).bg(bg),
                );
            }
        }
        if state.empty && !state.focused && input.height > 0 {
            buf.set_string(
                input.x,
                input.y,
                fit_text(placeholder, input.width as usize),
                Style::default().fg(theme.gray).bg(bg),
            );
        }

        if area.height >= 3 {
            let divider_y = area.bottom().saturating_sub(1);
            draw_horizontal_divider(buf, area, divider_y, '╰', '╯', border, bg);
            let left = fit_text(
                &format!(" {} · {} ", state.model, Self::mode_label(state.mode)),
                content.width as usize,
            );
            buf.set_string(
                content.x,
                divider_y,
                left,
                Style::default().fg(theme.accent_model).bg(bg),
            );
            if state.viewport.lines.len() > 1 {
                let right = " multiline ";
                let right_x = content.right().saturating_sub(right.width() as u16);
                buf.set_string(
                    right_x,
                    divider_y,
                    right,
                    Style::default().fg(theme.gray).bg(bg),
                );
            }
        }

        for y in text_area.y..text_area.bottom() {
            set_cell(buf, area.x, y, '│', border, bg);
            set_cell(buf, area.right().saturating_sub(1), y, '│', border, bg);
        }

        if !state.focused {
            // Grok dims unfocused content while retaining the chrome geometry.
            for y in input.y..input.bottom() {
                for x in input.x..input.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.fg = theme.gray_dim;
                    }
                }
            }
        }

        if state.focused {
            let cursor_x = input
                .x
                .saturating_add(state.viewport.cursor_x as u16)
                .min(input.right().saturating_sub(1));
            let cursor_y = input
                .y
                .saturating_add(state.viewport.cursor_y as u16)
                .min(input.bottom().saturating_sub(1));
            Some((cursor_x, cursor_y))
        } else {
            None
        }
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

fn set_cell(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    symbol: char,
    fg: ratatui::style::Color,
    bg: ratatui::style::Color,
) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(symbol);
        cell.set_style(Style::default().fg(fg).bg(bg));
    }
}

fn draw_horizontal_divider(
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    left: char,
    right: char,
    fg: ratatui::style::Color,
    bg: ratatui::style::Color,
) {
    if area.width == 0 {
        return;
    }
    for x in area.x..area.right() {
        let symbol = if x == area.x {
            left
        } else if x == area.right().saturating_sub(1) {
            right
        } else {
            '─'
        };
        set_cell(buf, x, y, symbol, fg, bg);
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
        assert_eq!(AgentView::prompt_label(PromptMode::Queue, true), "❯ ");
        assert_eq!(AgentView::prompt_label(PromptMode::Steer, true), "❯ ");
        assert_eq!(AgentView::mode_label(PromptMode::Steer), "steer");
    }

    #[test]
    fn prompt_uses_grok_box_geometry_and_arrow_prefix() {
        let area = Rect::new(0, 0, 32, 3);
        let mut buffer = Buffer::empty(area);
        let viewport = PromptViewport {
            lines: vec![String::new()],
            cursor_x: 0,
            cursor_y: 0,
        };
        AgentView::render_prompt_buffer(
            &mut buffer,
            area,
            PromptRenderState {
                mode: PromptMode::Queue,
                running: false,
                focused: true,
                title: "Session 1",
                model: "deepseek",
                viewport: &viewport,
                empty: true,
            },
            Theme::current(),
        );
        assert_eq!(buffer[(0, 0)].symbol(), "╭");
        assert_eq!(buffer[(31, 0)].symbol(), "╮");
        assert_eq!(buffer[(0, 1)].symbol(), "│");
        assert_eq!(buffer[(2, 1)].symbol(), "❯");
        assert_eq!(buffer[(0, 2)].symbol(), "╰");
        assert_eq!(buffer[(31, 2)].symbol(), "╯");
    }

    #[test]
    fn reference_sizes_keep_all_rows_inside_the_terminal() {
        let mut shell = AppShell::default();
        for (width, height) in [(40, 12), (80, 24), (120, 40)] {
            let layout = AgentView::layout_with_prompt(
                &mut shell,
                Rect::new(0, 0, width, height),
                "hello",
                true,
                true,
            );
            for rect in [
                layout.header,
                layout.transcript,
                layout.turn_status,
                layout.banner,
                layout.prompt,
                layout.status_line,
                layout.shortcuts,
            ] {
                assert!(rect.right() <= width);
                assert!(rect.bottom() <= height);
            }
            assert!(layout.transcript.y + layout.transcript.height <= layout.prompt.y);
        }
    }
}
