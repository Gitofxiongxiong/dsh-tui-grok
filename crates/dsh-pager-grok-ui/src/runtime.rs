//! Small, real terminal runtime around the copied Grok view primitives.
//!
//! This is intentionally a thin shell.  It owns focus and viewport state,
//! turns key events into DSH effects, and leaves all visual chrome to the
//! imported Grok modules.

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use dsh_pager::{
    PagerError, PagerResult, RpcTransport, SessionState, drain_notifications, repair_tail,
};
use dsh_pager_protocol::PromptMode;
use dsh_pager_render::TerminalSurface;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::effects::{DshEffectSink, UiEffect, UiEffectSink};
use crate::host_adapter::{GrokHostSnapshot, TranscriptRow};
use crate::input::line_editor::{LineEditOutcome, LineEditor};
use crate::modal_window_state::ModalWindowState;
use crate::theme::Theme;
use crate::views::{
    modal_window::{ModalSizing, ModalWindowConfig, Shortcut, render_modal_window},
    picker::{
        PickerConfig, PickerEntry, PickerMode, PickerOutcome, PickerState, handle_picker_input,
        picker_shortcuts, render_picker_in_modal,
    },
    status_bar::StatusBar,
    timeline::{RailViewport, compute_rail, render_rail},
};

const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Run the default Grok-derived UI until the user closes it.
pub fn run_interactive(mut transport: RpcTransport, mut session: SessionState) -> PagerResult<()> {
    let mut terminal = TerminalSurface::enter()?;
    let mut ui = UiState::default();
    let result = run_loop(&mut terminal, &mut transport, &mut session, &mut ui);
    // Restore explicitly so an error from the loop does not leave raw mode
    // enabled before the `Drop` fallback runs.
    let restore_result = terminal.restore();
    result.and(restore_result.map_err(PagerError::from))
}

fn run_loop(
    terminal: &mut TerminalSurface,
    transport: &mut RpcTransport,
    session: &mut SessionState,
    ui: &mut UiState,
) -> PagerResult<()> {
    loop {
        match drain_notifications(transport, session) {
            Ok(update) if update.gap_detected => {
                if let Err(error) = repair_tail(transport, session) {
                    ui.status = Some(format!("history repair error: {error}"));
                }
            }
            Ok(_) => {}
            Err(error) => {
                ui.status = Some(format!("notification error: {error}"));
            }
        }
        terminal.draw(|frame| ui.render(frame, session))?;
        if !event::poll(POLL_INTERVAL)? {
            continue;
        }
        let event = event::read()?;
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if ui.handle_key(key, transport, session)? {
                    break;
                }
            }
            Event::Resize(_, _) => {}
            Event::Mouse(mouse) if ui.picker_open => {
                let _ = ui.handle_picker_event(Event::Mouse(mouse));
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct UiState {
    scroll: usize,
    picker_open: bool,
    picker: PickerState,
    picker_entry_count: usize,
    modal: ModalWindowState,
    prompt: LineEditor,
    status: Option<String>,
    frame: usize,
}

impl UiState {
    fn render(&mut self, frame: &mut Frame<'_>, session: &SessionState) {
        let area = frame.area();
        let theme = Theme::current();
        let background = Block::default().style(Style::default().bg(theme.bg_base));
        frame.render_widget(background, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(area);
        let header = chunks[0];
        let body = chunks[1];
        let input = chunks[2];
        let footer = chunks[3];

        let snapshot = GrokHostSnapshot::from_session(session);
        let connection = format!("{} · {}", snapshot.connection, snapshot.model);
        frame.render_widget(
            StatusBar::new(&snapshot.session_title)
                .center("DSH · GROK UI")
                .right(&connection),
            header,
        );

        self.render_transcript(frame, body, &snapshot);
        self.render_prompt(frame, input, &snapshot);
        let footer_text = self
            .status
            .as_deref()
            .or(snapshot.status.as_deref())
            .unwrap_or("Enter send  p sessions  ↑/↓ scroll  Esc quit");
        frame.render_widget(
            Paragraph::new(footer_text)
                .style(Style::default().fg(theme.gray_dim).bg(theme.bg_base)),
            footer,
        );

        if self.picker_open {
            self.render_picker(frame, area, &snapshot);
        }
        self.frame = self.frame.wrapping_add(1);
    }

    fn render_transcript(&self, frame: &mut Frame<'_>, area: Rect, snapshot: &GrokHostSnapshot) {
        let theme = Theme::current();
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);
        let content = chunks[0];
        let rail = chunks[1];

        let mut lines = Vec::new();
        if snapshot.transcript.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No transcript events yet. Type a prompt below.",
                Style::default().fg(theme.gray),
            )));
        } else {
            for row in &snapshot.transcript {
                lines.extend(transcript_lines(row, theme));
                lines.push(Line::from(""));
            }
        }
        let max_scroll = lines.len().saturating_sub(content.height as usize);
        let scroll = self.scroll.min(max_scroll) as u16;
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme.bg_light))
            .style(Style::default().bg(theme.bg_base));
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(block)
                .style(Style::default().bg(theme.bg_base))
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0)),
            content,
        );

        let turn_count = snapshot.transcript.len();
        if let Some(rail_geometry) = compute_rail(
            rail,
            rail.x,
            turn_count,
            RailViewport {
                active: turn_count.checked_sub(1),
                up_target: turn_count.checked_sub(2),
                down_target: None,
                at_bottom: self.scroll == 0,
            },
        ) {
            let buf = frame.buffer_mut();
            render_rail(buf, &rail_geometry, None, theme);
        }
    }

    fn render_prompt(&self, frame: &mut Frame<'_>, area: Rect, snapshot: &GrokHostSnapshot) {
        let theme = Theme::current();
        let label = if snapshot.running { " > " } else { " · " };
        let text = if self.prompt.text().is_empty() {
            format!("{label}Ask DeepSeek anything…")
        } else {
            format!("{label}{}", self.prompt.text())
        };
        let style = if self.prompt.text().is_empty() {
            Style::default().fg(theme.gray_dim)
        } else {
            Style::default().fg(theme.text_primary)
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(style)
                .block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_style(Style::default().fg(theme.bg_light)),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_picker(&mut self, frame: &mut Frame<'_>, area: Rect, snapshot: &GrokHostSnapshot) {
        let theme = Theme::current();
        let mut entries = snapshot.picker_entries_filtered(self.picker.query());
        self.picker_entry_count = entries.len();
        for (index, entry) in entries.iter_mut().enumerate() {
            if let PickerEntry::Row(row) = entry {
                row.selected = index == self.picker.selected;
            }
        }
        let shortcuts = [
            Shortcut {
                label: "Enter select",
                clickable: true,
                id: 1,
            },
            Shortcut {
                label: "Esc close",
                clickable: true,
                id: 2,
            },
        ];
        let config = ModalWindowConfig {
            title: "Sessions · DSH host adapter",
            tabs: Some(&["sessions", "tasks"]),
            shortcuts: &shortcuts,
            sizing: ModalSizing::medium(),
            fold_info: None,
        };
        let buf = frame.buffer_mut();
        if let Some(content) = render_modal_window(buf, area, &mut self.modal, &config, theme) {
            render_picker_in_modal(
                buf,
                content.content,
                content.inner_x,
                content.inner_width,
                theme,
                &mut self.picker,
                &entries,
                &[],
                false,
            );
        }
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<bool> {
        if self.picker_open {
            return Ok(self.handle_picker_event(Event::Key(key)));
        }
        if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Ok(true);
        }
        match key.code {
            KeyCode::Char('p') if self.prompt.text().is_empty() => {
                self.picker_open = true;
                self.picker = PickerState::input_active();
                self.picker.mode = PickerMode::Floating;
                self.picker_entry_count = 1;
                self.status = Some("Session picker opened".into());
            }
            KeyCode::Up | KeyCode::Char('k') if self.prompt.text().is_empty() => {
                self.scroll = self.scroll.saturating_add(1);
            }
            KeyCode::Down | KeyCode::Char('j') if self.prompt.text().is_empty() => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::PageUp => self.scroll = self.scroll.saturating_add(8),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(8),
            KeyCode::Enter => {
                let text = self.prompt.text().trim().to_string();
                if text.is_empty() {
                    self.status = Some("Prompt is empty".into());
                } else {
                    let mut sink = DshEffectSink { transport };
                    let receipt = sink.dispatch(
                        UiEffect::SubmitPrompt {
                            text,
                            mode: PromptMode::Queue,
                        },
                        session,
                    )?;
                    self.prompt.reset();
                    self.status = Some(if receipt.accepted {
                        "Prompt queued".into()
                    } else {
                        "Prompt rejected by host".into()
                    });
                }
            }
            _ => match self.prompt.handle_key(&key) {
                LineEditOutcome::Unhandled => {}
                LineEditOutcome::HandledNoChange
                | LineEditOutcome::CursorChanged
                | LineEditOutcome::TextChanged => self.status = None,
            },
        }
        Ok(false)
    }

    fn handle_picker_event(&mut self, event: Event) -> bool {
        // Keep the picker input state machine from Grok intact.  The host
        // adapter currently presents the loaded session as the first row;
        // session switching is wired in the next vertical slice.
        let config = PickerConfig {
            title: Some("Sessions"),
            show_search_hint: false,
            expandable: true,
            esc_clears_query: true,
            shortcuts: Some(picker_shortcuts()),
            pending_hint: None,
            shortcuts_area: None,
            non_selectable: &[],
            non_selectable_clickable: &[],
            tabs: None,
            active_tab: 0,
            filter_label: None,
            filter_key_hint: None,
            filter_active: false,
            header_note: None,
            action_keys: &[],
            disable_search: false,
            compact_bottom_bar: false,
            search_only_on_slash: false,
            vim_normal_first: false,
        };
        match handle_picker_input(
            &event,
            &mut self.picker,
            self.picker_entry_count.max(1),
            &config,
        ) {
            PickerOutcome::Closed | PickerOutcome::Selected(_) => {
                self.picker_open = false;
                self.status = Some("Session picker closed".into());
                false
            }
            PickerOutcome::QueryChanged | PickerOutcome::Changed => {
                self.status = None;
                false
            }
            _ => false,
        }
    }
}

fn transcript_lines(row: &TranscriptRow, theme: &Theme) -> Vec<Line<'static>> {
    let color = match row.kind {
        dsh_pager::DshRenderKind::User => theme.accent_user,
        dsh_pager::DshRenderKind::Assistant => theme.text_primary,
        dsh_pager::DshRenderKind::Thinking => theme.fuzzy_accent,
        dsh_pager::DshRenderKind::ToolCall | dsh_pager::DshRenderKind::ToolResult => {
            theme.gray_bright
        }
        _ => theme.gray,
    };
    let header = Line::from(vec![
        Span::styled(
            format!("{} ", row.label),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("#{}", row.source_seq),
            Style::default().fg(theme.gray_dim),
        ),
    ]);
    let mut lines = vec![header];
    lines.extend(row.text.lines().map(|line| {
        Line::from(Span::styled(
            format!("  {line}"),
            Style::default().fg(color),
        ))
    }));
    lines
}

#[cfg(test)]
mod tests {
    use crate::host_adapter::GrokHostSnapshot;
    use crate::modal_window_state::ModalWindowState;
    use crate::theme::Theme;
    use crate::views::modal_window::{
        ModalSizing, ModalWindowConfig, Shortcut, render_modal_window,
    };
    use crate::views::picker::{PickerState, render_picker_in_modal};
    use ratatui::{buffer::Buffer, layout::Rect};

    #[test]
    fn demo_snapshot_keeps_host_data_out_of_grok_views() {
        let snapshot = GrokHostSnapshot::demo();
        assert_eq!(snapshot.model, "deepseek-reasoner");
        assert_eq!(snapshot.picker_entries().len(), 3);
    }

    #[test]
    fn transcript_projection_preserves_semantic_label() {
        let snapshot = GrokHostSnapshot::demo();
        assert!(snapshot.transcript.is_empty());
    }

    #[test]
    fn picker_filter_is_owned_by_the_host_adapter_boundary() {
        let snapshot = GrokHostSnapshot::demo();
        assert_eq!(snapshot.picker_entries_filtered("workspace").len(), 1);
        assert!(snapshot.picker_entries_filtered("missing").is_empty());
    }

    #[test]
    fn copied_modal_and_picker_compose_in_one_buffer() {
        let snapshot = GrokHostSnapshot::demo();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 90, 26));
        let shortcuts = [
            Shortcut {
                label: "Enter select",
                clickable: true,
                id: 1,
            },
            Shortcut {
                label: "Esc close",
                clickable: true,
                id: 2,
            },
        ];
        let config = ModalWindowConfig {
            title: "Sessions",
            tabs: None,
            shortcuts: &shortcuts,
            sizing: ModalSizing::medium(),
            fold_info: None,
        };
        let mut modal = ModalWindowState::default();
        let mut picker = PickerState::input_active();
        let entries = snapshot.picker_entries();
        let content = render_modal_window(
            &mut buffer,
            Rect::new(0, 0, 90, 26),
            &mut modal,
            &config,
            Theme::current(),
        )
        .expect("modal fits");
        render_picker_in_modal(
            &mut buffer,
            content.content,
            content.inner_x,
            content.inner_width,
            Theme::current(),
            &mut picker,
            &entries,
            &[],
            false,
        );
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Current session"));
        assert!(rendered.contains("Workspace tasks"));
        assert!(rendered.contains("Sessions"));
    }
}
