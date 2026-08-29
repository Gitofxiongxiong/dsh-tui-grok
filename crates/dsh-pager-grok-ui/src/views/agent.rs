//! Grok-derived AgentView geometry and main-surface chrome.
//!
//! The upstream AgentView keeps its layout pure and gives the scrollback a
//! minimum before optional rows are admitted. This module carries that same
//! contract into the DSH-neutral UI seam; runtime state is only used to choose
//! requested row heights and to paint the already-computed rectangles.

use dsh_pager_protocol::PromptMode;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::app::AppShell;
use crate::appearance::{LayoutConfig, ScrollbarConfig};
use crate::render::SafeBuf;
use crate::render::scrollbar::render_scrollbar_styled;
use crate::theme::Theme;
use crate::views::shortcuts_bar::{HintItem, ShortcutsBar};

/// Terminals at or below this height suppress optional prompt-adjacent rows.
pub const SHORT_TERMINAL_ROWS: u16 = 16;
/// The scrollback floor. Prompt growth consumes surplus rows before this one.
pub const SCROLLBACK_MIN_ROWS: u16 = 5;
/// Compact spacing is forced on terminals at or below this height.
pub const AUTO_COMPACT_MAX_ROWS: u16 = 20;

/// Scroll geometry produced by the DSH scrollback adapter and consumed by
/// Grok's AgentView scrollbar renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollInfo {
    pub total_height: usize,
    pub viewport_height: u16,
    pub scroll_offset: usize,
}

/// Render the AgentView scrollbar with Grok Build's theme, scaling and
/// follow-mode rules.
pub fn render_scrollbar(
    buf: &mut Buffer,
    scrollback_area: Rect,
    scrollbar_x: u16,
    scrollbar_cfg: &ScrollbarConfig,
    scroll_info: Option<ScrollInfo>,
    is_following: bool,
    theme: &Theme,
) {
    if !scrollbar_cfg.enabled {
        return;
    }
    let Some(scroll_info) = scroll_info else {
        return;
    };
    let scrollbar_area = Rect {
        x: scrollbar_x,
        y: scrollback_area.y,
        width: 1,
        height: scrollback_area.height,
    };
    let scrollbar_bg = scrollbar_cfg.scrollbar_bg.unwrap_or(theme.scrollbar_bg);
    let scrollbar_fg = scrollbar_cfg.scrollbar_fg.unwrap_or(theme.scrollbar_fg);
    let thumb_fg = if is_following {
        blend_color(scrollbar_bg, scrollbar_fg, 0.4).unwrap_or(scrollbar_fg)
    } else {
        scrollbar_fg
    };
    let track_style = Style::default().bg(scrollbar_bg);
    let thumb_style = Style::default().fg(thumb_fg).bg(scrollbar_bg);
    let scale = if scroll_info.total_height > u16::MAX as usize {
        (scroll_info.total_height / u16::MAX as usize) + 1
    } else {
        1
    };
    let scaled_total = (scroll_info.total_height / scale) as u16;
    let scaled_offset = (scroll_info.scroll_offset / scale) as u16;
    render_scrollbar_styled(
        buf,
        Some(scrollbar_area),
        scaled_total,
        scroll_info.viewport_height,
        scaled_offset,
        track_style,
        thumb_style,
    );
}

fn blend_color(base: Color, foreground: Color, opacity: f32) -> Option<Color> {
    let Color::Rgb(base_r, base_g, base_b) = base else {
        return None;
    };
    let Color::Rgb(fg_r, fg_g, fg_b) = foreground else {
        return None;
    };
    let opacity = opacity.clamp(0.0, 1.0);
    let channel = |base: u8, foreground: u8| {
        (base as f32 + (foreground as f32 - base as f32) * opacity).round() as u8
    };
    Some(Color::Rgb(
        channel(base_r, fg_r),
        channel(base_g, fg_g),
        channel(base_b, fg_b),
    ))
}

/// Render Grok's dropdown chrome anchored to the prompt and return the item
/// rows. Shared by the copied slash dropdown and future completion dropdowns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_dropdown_chrome(
    buf: &mut Buffer,
    item_count: usize,
    item_rows: u16,
    inline_prompt_area: Option<Rect>,
    layout_prompt: Rect,
    area: Rect,
    layout_cfg: &LayoutConfig,
    compact: bool,
    below: bool,
    theme: &Theme,
) -> Option<DropdownChrome> {
    let mut panel_height = item_rows + 2;
    let (top_border_y, bottom_border_y) = if below {
        let anchor = inline_prompt_area.unwrap_or(layout_prompt);
        let top = anchor.y + anchor.height;
        (top, top + panel_height - 1)
    } else {
        let bottom = if let Some(inline) = inline_prompt_area {
            inline.y.saturating_sub(1)
        } else {
            layout_prompt.y.saturating_sub(1)
        };
        let available = bottom.saturating_sub(area.y).saturating_add(1);
        panel_height = panel_height.min(available);
        if panel_height < 3 {
            return None;
        }
        (bottom.saturating_sub(panel_height - 1), bottom)
    };
    let embedded = crate::views::modal_window::embedded();
    let (hpad_left, hpad_right) = if embedded {
        (0, 0)
    } else {
        (
            layout_cfg.eff_hpad_left(compact),
            layout_cfg.eff_hpad_right(compact),
        )
    };
    let panel_x = area.x + hpad_left;
    let panel_width = area.width.saturating_sub(hpad_left + hpad_right);
    if top_border_y >= bottom_border_y || panel_width <= 4 {
        return None;
    }
    if below && bottom_border_y > area.y + area.height.saturating_sub(1) {
        return None;
    }
    let panel_area = Rect {
        x: panel_x,
        y: top_border_y,
        width: panel_width,
        height: panel_height,
    };
    if panel_area.bottom() > buf.area.bottom()
        || panel_area.right() > buf.area.right()
        || panel_area.y < buf.area.y
    {
        return None;
    }
    ratatui::widgets::Clear.render(panel_area, buf);
    if embedded {
        let reset = Color::Reset;
        let divider_style = Style::default().fg(theme.gray_dim).bg(reset);
        let divider = Line::styled("─".repeat(panel_width as usize), divider_style);
        buf.set_line_safe(panel_x, top_border_y, &divider, panel_width);
        let footer = "↑/↓ navigate · enter confirm · esc cancel";
        let footer_line = Line::styled(
            footer.to_string(),
            Style::default().fg(theme.gray_dim).bg(reset),
        );
        buf.set_line_safe(
            panel_x + 1,
            bottom_border_y,
            &footer_line,
            panel_width.saturating_sub(1),
        );
    } else {
        buf.set_style(
            panel_area,
            Style::default().fg(theme.text_primary).bg(theme.bg_light),
        );
        let border_style = Style::default().fg(theme.bg_highlight).bg(theme.bg_base);
        let border_line = Line::styled("─".repeat(panel_width as usize), border_style);
        buf.set_line_safe(panel_x, top_border_y, &border_line, panel_width);
        buf.set_line_safe(panel_x, bottom_border_y, &border_line, panel_width);
        let hint = format!("{item_count}");
        let hint_width = hint.len() as u16;
        if hint_width + 2 <= panel_width {
            let hint_x = panel_x + panel_width - hint_width - 1;
            let hint_line = Line::styled(hint, Style::default().fg(theme.gray).bg(theme.bg_base));
            buf.set_line_safe(hint_x, top_border_y, &hint_line, hint_width);
        }
    }
    let content_inset = dropdown_content_inset(layout_cfg, compact);
    let items_x = layout_prompt.x + content_inset;
    let items_width = layout_prompt.width.saturating_sub(content_inset);
    Some(DropdownChrome {
        items: Rect {
            x: items_x,
            y: top_border_y + 1,
            height: panel_height - 2,
            width: items_width,
        },
        panel: panel_area,
    })
}

fn dropdown_content_inset(layout_cfg: &LayoutConfig, compact: bool) -> u16 {
    if crate::views::modal_window::embedded() {
        0
    } else {
        1 + layout_cfg.eff_hpad_left(compact)
    }
}

pub(crate) fn dropdown_items_width(
    layout_prompt: Rect,
    layout_cfg: &LayoutConfig,
    compact: bool,
) -> u16 {
    layout_prompt
        .width
        .saturating_sub(dropdown_content_inset(layout_cfg, compact))
}

pub(crate) struct DropdownChrome {
    pub(crate) items: Rect,
    pub(crate) panel: Rect,
}

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

    /// Paint the Grok pane shortcuts bar: `compact(5, ShortcutsHelp)`.
    pub fn render_shortcuts(
        frame: &mut Frame<'_>,
        area: Rect,
        hints: &[HintItem],
        help_hint: Option<HintItem>,
        pending_hint: Option<crate::views::shortcuts_bar::PendingHint>,
    ) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        frame.render_widget(
            ShortcutsBar::new(hints)
                .compact(5, help_hint)
                .with_pending(pending_hint),
            area,
        );
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

    #[test]
    fn grok_scrollbar_uses_full_block_and_tracks_top_to_bottom() {
        let theme = Theme::default();
        let area = Rect::new(0, 0, 12, 10);
        let track = Rect::new(10, 0, 1, 10);
        let info = |offset| ScrollInfo {
            total_height: 100,
            viewport_height: 10,
            scroll_offset: offset,
        };

        let mut top = Buffer::empty(area);
        render_scrollbar(
            &mut top,
            track,
            track.x,
            &ScrollbarConfig::default(),
            Some(info(0)),
            false,
            &theme,
        );
        assert_eq!(top[(track.x, track.y)].symbol(), "█");
        assert_eq!(top[(track.x, track.y)].bg, theme.scrollbar_fg);

        let mut bottom = Buffer::empty(area);
        render_scrollbar(
            &mut bottom,
            track,
            track.x,
            &ScrollbarConfig::default(),
            Some(info(90)),
            false,
            &theme,
        );
        assert_eq!(bottom[(track.x, track.bottom() - 1)].symbol(), "█");
    }

    #[test]
    fn grok_scrollbar_dims_follow_mode_and_skips_content_that_fits() {
        let theme = Theme::default();
        let area = Rect::new(0, 0, 12, 10);
        let track = Rect::new(10, 0, 1, 10);
        let mut following = Buffer::empty(area);
        render_scrollbar(
            &mut following,
            track,
            track.x,
            &ScrollbarConfig::default(),
            Some(ScrollInfo {
                total_height: 100,
                viewport_height: 10,
                scroll_offset: 90,
            }),
            true,
            &theme,
        );
        assert_ne!(
            following[(track.x, track.bottom() - 1)].bg,
            theme.scrollbar_fg
        );

        let mut fitting = Buffer::empty(area);
        render_scrollbar(
            &mut fitting,
            track,
            track.x,
            &ScrollbarConfig::default(),
            Some(ScrollInfo {
                total_height: 10,
                viewport_height: 10,
                scroll_offset: 0,
            }),
            true,
            &theme,
        );
        assert!((track.top()..track.bottom()).all(|row| fitting[(track.x, row)].symbol() == " "));
    }
}
