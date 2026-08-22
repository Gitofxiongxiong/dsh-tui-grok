//! Small, real terminal runtime around the copied Grok view primitives.
//!
//! This is intentionally a thin shell.  It owns focus and viewport state,
//! turns key events into DSH effects, and leaves all visual chrome to the
//! imported Grok modules.
//!
//! Migration note (M0.8): this file is the frozen fallback shell. New parity
//! behavior belongs in the Grok AppView/AgentView migration and must not grow
//! a second layout or dispatch system here. Removal exits when M3 production
//! shell and its focus/dispatch fixtures replace this entry path.

use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use dsh_pager::{
    PagerError, PagerResult, RpcTransport, SessionState, drain_notifications, repair_tail,
};
use dsh_pager_protocol::PromptMode;
use dsh_pager_render::{TerminalCapabilities, TerminalSurface};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{AppShell, Overlay, ShellAction, ShellEvent};
use crate::effects::{DshEffectSink, UiContext, UiEffectSink, UiEffectStatus, UiIntent};
use crate::host_adapter::{GrokHostSnapshot, TranscriptRow};
use crate::input::line_editor::{LineEditOutcome, LineEditor};
use crate::modal_window_state::ModalWindowState;
use crate::render::line_utils::truncate_str;
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
    let mut ui = UiState {
        capabilities: terminal.capabilities(),
        ..UiState::default()
    };
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
        if let Some(area) = terminal.sync_size()? {
            ui.dispatch_event(
                ShellEvent::Resize {
                    width: area.width,
                    height: area.height,
                },
                transport,
                session,
            )?;
        }
        match drain_notifications(transport, session) {
            Ok(update) if update.gap_detected => {
                if let Err(error) = repair_tail(transport, session) {
                    ui.status = Some(format!("history repair error: {error}"));
                }
            }
            Ok(update) if update.changed => {
                ui.shell.invalidate_content();
                let _ = ui.dispatch_event(ShellEvent::Notification, transport, session)?;
            }
            Ok(_) => {}
            Err(error) => {
                ui.status = Some(format!("notification error: {error}"));
            }
        }
        terminal.draw_with_links(&[], |frame| ui.render(frame, session))?;
        if !event::poll(POLL_INTERVAL)? {
            continue;
        }
        let event = event::read()?;
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if ui.dispatch_event(ShellEvent::Key(key), transport, session)? {
                    break;
                }
            }
            Event::Mouse(mouse) => {
                if ui.dispatch_event(ShellEvent::Mouse(mouse), transport, session)? {
                    break;
                }
            }
            Event::Paste(text) => {
                if ui.dispatch_event(ShellEvent::Paste(text), transport, session)? {
                    break;
                }
            }
            Event::Resize(width, height) => {
                let _ =
                    ui.dispatch_event(ShellEvent::Resize { width, height }, transport, session)?;
            }
            _ => {
                let _ = ui.dispatch_event(ShellEvent::Tick, transport, session)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct UiState {
    shell: AppShell,
    capabilities: TerminalCapabilities,
    scroll: usize,
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

        let shell_layout = self.shell.layout(area);
        let header = shell_layout.header;
        let body = shell_layout.body;
        let input = shell_layout.prompt;
        let footer = shell_layout.footer;

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
        let capability_notice = if !self.capabilities.bracketed_paste {
            Some("Paste unavailable")
        } else if !self.capabilities.mouse {
            Some("Mouse unavailable")
        } else {
            None
        };
        let footer_text = self
            .status
            .as_deref()
            .or(snapshot.status.as_deref())
            .or(capability_notice)
            .unwrap_or("Enter send  p sessions  ↑/↓ scroll  Esc quit");
        frame.render_widget(
            Paragraph::new(footer_text)
                .style(Style::default().fg(theme.gray_dim).bg(theme.bg_base)),
            footer,
        );

        if self.shell.overlay() == Overlay::Picker {
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
        let available = area.width.saturating_sub(label.len() as u16 + 1) as usize;
        let viewport = self.prompt.viewport(available);
        let visible = if self.prompt.text().is_empty() {
            truncate_str("Ask DeepSeek anything…", available)
        } else {
            self.prompt.text()[viewport.visible_byte_range].to_string()
        };
        let text = format!("{label}{visible}");
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
        if !self.prompt.text().is_empty() && area.height > 0 && area.width > 0 {
            let cursor_x = area
                .x
                .saturating_add(label.len() as u16)
                .saturating_add(viewport.cursor_display_column as u16)
                .min(area.right().saturating_sub(1));
            frame.set_cursor_position(Position::new(cursor_x, area.y));
        }
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

    fn dispatch_event(
        &mut self,
        event: ShellEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<bool> {
        let prompt_empty = self.prompt.text().is_empty();
        let action = self.shell.dispatch(event, prompt_empty);
        match action {
            ShellAction::Quit => Ok(true),
            ShellAction::OpenPicker => {
                self.picker = PickerState::input_active();
                self.picker.mode = PickerMode::Floating;
                self.picker_entry_count = 1;
                self.status = Some("Session picker opened".into());
                Ok(false)
            }
            ShellAction::CloseOverlay => {
                self.status = Some("Session picker closed".into());
                Ok(false)
            }
            ShellAction::ClearPrompt => {
                self.prompt.reset();
                self.status = Some("Draft cleared".into());
                Ok(false)
            }
            ShellAction::ScrollUp(amount) => {
                self.scroll = self.scroll.saturating_add(amount as usize);
                Ok(false)
            }
            ShellAction::ScrollDown(amount) => {
                self.scroll = self.scroll.saturating_sub(amount as usize);
                Ok(false)
            }
            ShellAction::SubmitPrompt => {
                self.submit_prompt(transport, session)?;
                Ok(false)
            }
            ShellAction::PromptKey(key) => {
                match self.prompt.handle_key(&key) {
                    LineEditOutcome::Unhandled => {}
                    LineEditOutcome::HandledNoChange
                    | LineEditOutcome::CursorChanged
                    | LineEditOutcome::TextChanged => self.status = None,
                }
                Ok(false)
            }
            ShellAction::PickerKey(key) => {
                let _ = self.handle_picker_event(Event::Key(key));
                Ok(false)
            }
            ShellAction::PickerMouse(mouse) => {
                let _ = self.handle_picker_event(Event::Mouse(mouse));
                Ok(false)
            }
            ShellAction::PromptPaste(text) => {
                if !text.is_empty() {
                    let _ = self.prompt.insert_paste(&text);
                    self.status = None;
                }
                Ok(false)
            }
            ShellAction::PickerPaste(text) => {
                let _ = self.handle_picker_event(Event::Paste(text));
                Ok(false)
            }
            ShellAction::Resized(area) => {
                let _ = self.shell.layout(area);
                self.modal = ModalWindowState::default();
                self.status = Some(format!("Resized to {}x{}", area.width, area.height));
                Ok(false)
            }
            ShellAction::None | ShellAction::Redraw => Ok(false),
        }
    }

    fn submit_prompt(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        let text = self.prompt.text().to_string();
        if text.trim().is_empty() {
            self.status = Some("Prompt is empty".into());
            return Ok(());
        }
        let mut sink = DshEffectSink::new(transport);
        let context = UiContext::from_session(session);
        let receipt = sink.submit(
            UiIntent::SubmitPrompt {
                text,
                mode: PromptMode::Queue,
            },
            &context,
        )?;
        self.prompt.reset();
        self.status = Some(if matches!(receipt.status, UiEffectStatus::Accepted) {
            "Prompt queued".into()
        } else {
            receipt
                .diagnostic
                .unwrap_or_else(|| "Prompt rejected by host".into())
        });
        Ok(())
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
                self.shell.close_overlay();
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
