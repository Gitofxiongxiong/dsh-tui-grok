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
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::AppShell;
use crate::appearance::{LayoutConfig, ScrollbarConfig};
use crate::input::key::KeyShortcut;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePane {
    #[default]
    Scrollback,
    Todo,
    Queue,
    Prompt,
    Tasks,
    Catalog,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaneAreas {
    pub scrollback: Rect,
    pub todo: Rect,
    pub queue: Rect,
    pub prompt: Rect,
    pub tasks: Rect,
    pub catalog: Rect,
}

impl PaneAreas {
    pub fn hit_test(&self, col: u16, row: u16) -> Option<ActivePane> {
        let pos = (col, row).into();
        if self.tasks.area() > 0 && self.tasks.contains(pos) {
            return Some(ActivePane::Tasks);
        }
        if self.catalog.area() > 0 && self.catalog.contains(pos) {
            return Some(ActivePane::Catalog);
        }
        if self.todo.area() > 0 && self.todo.contains(pos) {
            return Some(ActivePane::Todo);
        }
        if self.queue.area() > 0 && self.queue.contains(pos) {
            return Some(ActivePane::Queue);
        }
        if self.scrollback.contains(pos) {
            return Some(ActivePane::Scrollback);
        }
        if self.prompt.contains(pos) {
            return Some(ActivePane::Prompt);
        }
        None
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AgentViewLayoutParams {
    pub area: Rect,
    pub layout_cfg: LayoutConfig,
    pub scrollbar_cfg: ScrollbarConfig,
    pub timeline_width: u16,
    pub prompt_height: u16,
    pub tasks_height: u16,
    pub catalog_height: u16,
    pub todo_height: u16,
    pub queue_height: u16,
    pub btw_height: u16,
    pub turn_status_height: u16,
    pub banner_height: u16,
    pub cta_height: u16,
    pub follow_ups_height: u16,
    pub prompt_gap: u16,
    pub voice_recording_height: u16,
    pub shortcuts_height: u16,
    pub status_line_height: u16,
    pub compact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentViewLayout {
    pub status_bar: Rect,
    pub tasks: Rect,
    pub catalog: Rect,
    pub scrollback: Rect,
    pub todo: Rect,
    pub queue: Rect,
    pub btw: Rect,
    pub turn_status: Rect,
    pub banner: Rect,
    pub plugin_cta: Rect,
    pub follow_ups: Rect,
    pub voice_recording: Rect,
    pub prompt: Rect,
    pub shortcuts: Rect,
    pub status_line: Rect,
    pub scrollback_content: Rect,
    pub scrollbar_x: u16,
    pub timeline_x: u16,
    pub timeline_width: u16,
}

impl AgentViewLayout {
    pub fn compute(params: AgentViewLayoutParams) -> Self {
        let AgentViewLayoutParams {
            area,
            layout_cfg,
            scrollbar_cfg,
            timeline_width,
            prompt_height,
            tasks_height,
            catalog_height,
            todo_height,
            queue_height,
            btw_height,
            turn_status_height,
            banner_height,
            cta_height,
            follow_ups_height,
            prompt_gap,
            voice_recording_height,
            shortcuts_height,
            status_line_height,
            compact,
        } = params;
        let outer_vpad = layout_cfg.eff_outer_vpad(compact);
        let bottom_vpad = if area.height <= SHORT_TERMINAL_ROWS {
            0
        } else {
            outer_vpad
        };
        let cta_height = if area.height <= SHORT_TERMINAL_ROWS {
            0
        } else {
            cta_height
        };
        let follow_ups_height = if area.height <= SHORT_TERMINAL_ROWS {
            0
        } else {
            follow_ups_height
        };
        let outer_block = Block::default().padding(Padding::new(
            layout_cfg.eff_hpad_left(compact),
            layout_cfg.eff_hpad_right(compact),
            outer_vpad,
            bottom_vpad,
        ));
        let inner_area = outer_block.inner(area);
        let mut constraints = vec![Constraint::Length(1)];
        let pane_gap = u16::from(outer_vpad > 0);
        if tasks_height > 0 {
            constraints.push(Constraint::Length(pane_gap));
            constraints.push(Constraint::Length(tasks_height));
        }
        if catalog_height > 0 {
            constraints.push(Constraint::Length(pane_gap));
            constraints.push(Constraint::Length(catalog_height));
        }
        if todo_height > 0 {
            constraints.push(Constraint::Length(pane_gap));
            constraints.push(Constraint::Length(todo_height));
        }
        constraints.push(Constraint::Length(pane_gap));
        constraints.push(Constraint::Min(SCROLLBACK_MIN_ROWS));
        if btw_height > 0 {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(btw_height));
        }
        if queue_height > 0 {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(queue_height));
        }
        if turn_status_height > 0 {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(turn_status_height));
        }
        if banner_height > 0 {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(banner_height));
        }
        if cta_height > 0 {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(cta_height));
        }
        if follow_ups_height > 0 {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(follow_ups_height));
        }
        if prompt_gap > 0 {
            constraints.push(Constraint::Length(prompt_gap));
        }
        if voice_recording_height > 0 {
            constraints.push(Constraint::Length(voice_recording_height));
        }
        constraints.push(Constraint::Length(prompt_height));
        let pushed = constraints
            .iter()
            .map(|constraint| match constraint {
                Constraint::Length(rows) | Constraint::Min(rows) | Constraint::Max(rows) => *rows,
                Constraint::Percentage(_) | Constraint::Ratio(_, _) | Constraint::Fill(_) => 0,
            })
            .fold(0u16, u16::saturating_add);
        let reserved = pushed.saturating_add(shortcuts_height);
        let status_line_height = status_line_height.min(inner_area.height.saturating_sub(reserved));
        let shortcuts_gap = u16::from(bottom_vpad > 0 && status_line_height == 0);
        if shortcuts_gap > 0 {
            constraints.push(Constraint::Length(shortcuts_gap));
        }
        if status_line_height > 0 {
            constraints.push(Constraint::Length(status_line_height));
        }
        constraints.push(Constraint::Length(shortcuts_height));
        let chunks = Layout::vertical(constraints).split(inner_area);
        let mut index = 0;
        let status_bar = chunks[index];
        index += 1;
        let tasks = take_optional_chunk(&chunks, &mut index, tasks_height);
        let catalog = take_optional_chunk(&chunks, &mut index, catalog_height);
        let todo = take_optional_chunk(&chunks, &mut index, todo_height);
        index += 1;
        let scrollback = chunks[index];
        index += 1;
        let btw = take_optional_chunk(&chunks, &mut index, btw_height);
        let queue = take_optional_chunk(&chunks, &mut index, queue_height);
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
        let plugin_cta = take_optional_chunk(&chunks, &mut index, cta_height);
        let follow_ups = take_optional_chunk(&chunks, &mut index, follow_ups_height);
        if prompt_gap > 0 {
            index += 1;
        }
        let voice_recording = if voice_recording_height > 0 {
            let rect = chunks[index];
            index += 1;
            rect
        } else {
            Rect::default()
        };
        let prompt = chunks[index];
        index += 1;
        if shortcuts_gap > 0 {
            index += 1;
        }
        let status_line = if status_line_height > 0 {
            let rect = chunks[index];
            index += 1;
            rect
        } else {
            Rect::default()
        };
        let shortcuts = chunks[index];
        let scrollbar_x = area
            .right()
            .saturating_sub(scrollbar_cfg.gap_right.saturating_add(1));
        let timeline_width = if scrollbar_cfg.enabled {
            timeline_width
        } else {
            0
        };
        let timeline_x = scrollbar_x.saturating_add(1).saturating_sub(timeline_width);
        let content_end_x = if timeline_width > 0 {
            timeline_x.saturating_sub(scrollbar_cfg.gap_left)
        } else {
            scrollbar_x.saturating_sub(scrollbar_cfg.gap_left)
        };
        let scrollback_content = if !scrollbar_cfg.enabled || content_end_x >= scrollback.right() {
            scrollback
        } else {
            Rect {
                width: content_end_x.saturating_sub(scrollback.x),
                ..scrollback
            }
        };

        Self {
            status_bar,
            tasks,
            catalog,
            scrollback,
            todo,
            queue,
            btw,
            turn_status,
            banner,
            plugin_cta,
            follow_ups,
            voice_recording,
            prompt,
            shortcuts,
            status_line,
            scrollback_content,
            scrollbar_x,
            timeline_x,
            timeline_width,
        }
    }

    pub fn rows_available_for_prompt(params: AgentViewLayoutParams) -> u16 {
        let probe = Self::compute(AgentViewLayoutParams {
            prompt_height: 0,
            ..params
        });
        probe.scrollback.height.saturating_sub(SCROLLBACK_MIN_ROWS)
    }

    pub fn inner_width(area: Rect, layout_cfg: &LayoutConfig, compact: bool) -> u16 {
        let vpad = layout_cfg.eff_outer_vpad(compact);
        Block::default()
            .padding(Padding::new(
                layout_cfg.eff_hpad_left(compact),
                layout_cfg.eff_hpad_right(compact),
                vpad,
                vpad,
            ))
            .inner(area)
            .width
    }

    pub fn pane_areas(&self) -> PaneAreas {
        PaneAreas {
            scrollback: self.scrollback,
            todo: self.todo,
            queue: self.queue,
            prompt: self.prompt,
            tasks: self.tasks,
            catalog: self.catalog,
        }
    }
}

pub struct AgentView;

impl AgentView {
    pub fn layout(shell: &mut AppShell, params: AgentViewLayoutParams) -> AgentViewLayout {
        let _ = shell.layout(params.area);
        AgentViewLayout::compute(params)
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

fn take_optional_chunk(chunks: &[Rect], index: &mut usize, height: u16) -> Rect {
    if height == 0 {
        return Rect::default();
    }
    *index += 1;
    let rect = chunks[*index];
    *index += 1;
    rect
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
        let area = Rect::new(0, 0, 80, 24);
        let compact = effective_compact(false, area.height);
        let layout = AgentView::layout(
            &mut shell,
            AgentViewLayoutParams {
                area,
                prompt_height: 3,
                turn_status_height: 1,
                status_line_height: 1,
                shortcuts_height: 1,
                compact,
                ..Default::default()
            },
        );
        assert_eq!(LayoutConfig::default().eff_hpad_left(compact), 2);
        assert_eq!(LayoutConfig::default().eff_outer_vpad(compact), 1);
        assert!(layout.scrollback.height >= SCROLLBACK_MIN_ROWS);
        assert!(layout.scrollback_content.width <= layout.scrollback.width);
        assert!(layout.prompt.y > layout.scrollback.y);
        assert_eq!(layout.shortcuts.bottom(), 23);
    }

    #[test]
    fn short_terminal_suppresses_optional_rows_without_losing_prompt() {
        let mut shell = AppShell::default();
        let area = Rect::new(0, 0, 40, 12);
        let layout = AgentView::layout(
            &mut shell,
            AgentViewLayoutParams {
                area,
                prompt_height: 6,
                shortcuts_height: 1,
                compact: effective_compact(false, area.height),
                ..Default::default()
            },
        );
        assert!(effective_compact(false, area.height));
        assert_eq!(LayoutConfig::default().eff_outer_vpad(true), 0);
        assert_eq!(layout.turn_status.height, 0);
        assert_eq!(layout.status_line.height, 0);
        assert!(layout.prompt.height >= 3);
        assert!(layout.scrollback.height >= SCROLLBACK_MIN_ROWS);
        assert!(layout.shortcuts.bottom() <= 12);
    }

    #[test]
    fn prompt_budget_grows_for_multiline_text_but_is_capped() {
        let mut shell = AppShell::default();
        let area = Rect::new(0, 0, 120, 40);
        let params = |prompt_height| AgentViewLayoutParams {
            area,
            prompt_height,
            status_line_height: 1,
            shortcuts_height: 1,
            compact: false,
            ..Default::default()
        };
        let short = AgentView::layout(&mut shell, params(3));
        let long = AgentView::layout(&mut shell, params(8));
        assert!(long.prompt.height > short.prompt.height);
        assert!(long.prompt.height <= 8);
        assert!(long.scrollback.height >= SCROLLBACK_MIN_ROWS);
    }

    #[test]
    fn suggestion_banner_is_reserved_above_prompt() {
        let mut shell = AppShell::default();
        let area = Rect::new(0, 0, 80, 24);
        let layout = AgentView::layout(
            &mut shell,
            AgentViewLayoutParams {
                area,
                prompt_height: 3,
                banner_height: 3,
                shortcuts_height: 1,
                ..Default::default()
            },
        );
        assert_eq!(layout.banner.height, 3);
        assert!(layout.banner.bottom() <= layout.prompt.y);
        assert!(layout.scrollback.bottom() <= layout.banner.y);
    }

    #[test]
    fn prompt_labels_expose_queue_and_steer_modes() {
        assert_eq!(AgentView::prompt_label(PromptMode::Queue, true), "❯ ");
        assert_eq!(AgentView::prompt_label(PromptMode::Steer, true), "❯ ");
        assert_eq!(AgentView::mode_label(PromptMode::Steer), "steer");
    }

    #[test]
    fn reference_sizes_keep_all_rows_inside_the_terminal() {
        let mut shell = AppShell::default();
        for (width, height) in [(40, 12), (80, 24), (120, 40)] {
            let area = Rect::new(0, 0, width, height);
            let layout = AgentView::layout(
                &mut shell,
                AgentViewLayoutParams {
                    area,
                    prompt_height: 3,
                    turn_status_height: u16::from(height > SHORT_TERMINAL_ROWS),
                    status_line_height: u16::from(height > SHORT_TERMINAL_ROWS),
                    shortcuts_height: 1,
                    compact: effective_compact(false, height),
                    ..Default::default()
                },
            );
            for rect in [
                layout.status_bar,
                layout.scrollback,
                layout.turn_status,
                layout.banner,
                layout.prompt,
                layout.status_line,
                layout.shortcuts,
            ] {
                assert!(rect.right() <= width);
                assert!(rect.bottom() <= height);
            }
            assert!(layout.scrollback.y + layout.scrollback.height <= layout.prompt.y);
        }
    }

    #[test]
    fn optional_pane_gaps_are_omitted_when_height_is_zero() {
        let area = Rect::new(0, 0, 80, 40);
        let base = AgentViewLayout::compute(AgentViewLayoutParams {
            area,
            prompt_height: 3,
            shortcuts_height: 1,
            ..Default::default()
        });
        let with_queue = AgentViewLayout::compute(AgentViewLayoutParams {
            queue_height: 2,
            ..AgentViewLayoutParams {
                area,
                prompt_height: 3,
                shortcuts_height: 1,
                ..Default::default()
            }
        });
        assert_eq!(base.queue, Rect::default());
        assert_eq!(base.prompt.y, base.scrollback.bottom());
        assert!(with_queue.queue.y > with_queue.scrollback.bottom());
        assert!(with_queue.queue.bottom() <= with_queue.prompt.y);
    }

    #[test]
    fn prompt_budget_reserves_scrollback_floor_and_clamps_status_line() {
        let area = Rect::new(0, 0, 80, 21);
        let params = AgentViewLayoutParams {
            area,
            prompt_height: 99,
            status_line_height: 3,
            shortcuts_height: 1,
            ..Default::default()
        };
        let budget = AgentViewLayout::rows_available_for_prompt(params);
        let layout = AgentViewLayout::compute(AgentViewLayoutParams {
            prompt_height: budget,
            ..params
        });
        assert!(layout.scrollback.height >= SCROLLBACK_MIN_ROWS);
        assert_eq!(layout.scrollback.height, SCROLLBACK_MIN_ROWS);
        assert_eq!(layout.status_line.height, 3);
        let over = AgentViewLayout::compute(AgentViewLayoutParams {
            prompt_height: budget.saturating_add(1),
            ..params
        });
        assert!(over.scrollback.height >= SCROLLBACK_MIN_ROWS);
        assert!(over.status_line.height <= 3);
    }

    #[test]
    fn scrollbar_and_timeline_geometry_share_one_gutter() {
        let area = Rect::new(0, 0, 80, 40);
        let no_rail = AgentViewLayout::compute(AgentViewLayoutParams {
            area,
            prompt_height: 3,
            shortcuts_height: 1,
            ..Default::default()
        });
        let with_rail = AgentViewLayout::compute(AgentViewLayoutParams {
            area,
            prompt_height: 3,
            shortcuts_height: 1,
            timeline_width: 2,
            ..Default::default()
        });
        assert_eq!(with_rail.scrollbar_x, no_rail.scrollbar_x);
        assert_eq!(
            with_rail.timeline_x + with_rail.timeline_width,
            with_rail.scrollbar_x + 1
        );
        assert!(with_rail.scrollback_content.right() <= with_rail.timeline_x);
        let disabled = AgentViewLayout::compute(AgentViewLayoutParams {
            area,
            prompt_height: 3,
            shortcuts_height: 1,
            timeline_width: 2,
            scrollbar_cfg: ScrollbarConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(disabled.timeline_width, 0);
        assert_eq!(disabled.scrollback_content, disabled.scrollback);
    }

    #[test]
    fn pane_hit_test_priority_matches_upstream_order() {
        let areas = PaneAreas {
            scrollback: Rect::new(2, 5, 70, 20),
            queue: Rect::new(2, 26, 70, 2),
            prompt: Rect::new(2, 29, 70, 4),
            tasks: Rect::new(2, 1, 70, 2),
            catalog: Rect::new(2, 4, 70, 1),
            ..Default::default()
        };
        assert_eq!(areas.hit_test(3, 1), Some(ActivePane::Tasks));
        assert_eq!(areas.hit_test(3, 4), Some(ActivePane::Catalog));
        assert_eq!(areas.hit_test(3, 26), Some(ActivePane::Queue));
        assert_eq!(areas.hit_test(3, 29), Some(ActivePane::Prompt));
    }
}
