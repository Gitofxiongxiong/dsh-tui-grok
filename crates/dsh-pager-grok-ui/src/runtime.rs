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

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use dsh_pager::{
    DshGeneration, DshInteraction, DshQueueItemId, DshRequestId, PagerError, PagerResult,
    RpcTransport, SessionState, drain_notifications, load_session_id, repair_tail,
};
use dsh_pager_protocol::{PromptMode, QueueAction, TuiInteractionResponse};
use dsh_pager_render::{TerminalCapabilities, TerminalSurface};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{AppShell, Overlay, ShellAction, ShellEvent};
use crate::effects::{
    DshEffectSink, OperationKey, UiContext, UiEffect, UiEffectSink, UiEffectStatus, UiIntent,
    compile_intent,
};
use crate::host_adapter::GrokHostSnapshot;
use crate::input::{PromptEditor, line_editor::LineEditOutcome};
use crate::modal_window_state::ModalWindowState;
use crate::render::line_utils::truncate_str;
use crate::theme::Theme;
use crate::views::{
    interaction::{render_interaction_content, response_for},
    modal_window::{
        ModalSizing, ModalWindowConfig, ModalWindowOutcome, Shortcut, handle_modal_mouse,
        render_modal_window,
    },
    picker::{
        PickerConfig, PickerEntry, PickerMode, PickerOutcome, PickerState, handle_picker_input,
        picker_shortcuts, render_picker_in_modal,
    },
    queue::{QueueRenderState, moved_selection, render_queue_content},
    status_bar::StatusBar,
    timeline::{RailViewport, compute_rail, render_rail},
};
use serde_json::json;

const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
struct PendingQueueMutation {
    operation: OperationKey,
    item_id: DshQueueItemId,
    base_revision: u64,
}

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
        terminal.draw_with_links(&[], |frame| {
            ui.render(frame, session, transport.control_plane())
        })?;
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
    prompt: PromptEditor,
    picker_selected_id: Option<String>,
    queue_selected_id: Option<String>,
    queue_editing: bool,
    queue_editor: PromptEditor,
    queue_pending: Option<PendingQueueMutation>,
    interaction_editor: PromptEditor,
    interaction_selected: usize,
    interaction_request_id: Option<DshRequestId>,
    interaction_generation: Option<DshGeneration>,
    interaction_pending: Option<DshRequestId>,
    next_operation: u64,
    status: Option<String>,
    frame: usize,
}

impl UiState {
    fn render(
        &mut self,
        frame: &mut Frame<'_>,
        session: &mut SessionState,
        control_plane: &dsh_pager::ControlPlaneStore,
    ) {
        let area = frame.area();
        let theme = Theme::current();
        let background = Block::default().style(Style::default().bg(theme.bg_base));
        frame.render_widget(background, area);

        let shell_layout = self.shell.layout(area);
        let header = shell_layout.header;
        let body = shell_layout.body;
        let input = shell_layout.prompt;
        let footer = shell_layout.footer;

        let snapshot =
            GrokHostSnapshot::from_session_with_control_plane(session, Some(control_plane));
        self.reconcile_snapshot(&snapshot);
        let connection = format!(
            "{} · {} · q{} · {}",
            snapshot.connection,
            snapshot.model,
            snapshot.queue_revision,
            if snapshot.running { "running" } else { "idle" }
        );
        frame.render_widget(
            StatusBar::new(&snapshot.session_title)
                .center("DSH · GROK UI")
                .right(&connection),
            header,
        );

        self.render_transcript(frame, body, &snapshot, &mut session.scrollback);
        self.render_prompt(frame, input, &snapshot);
        let capability_notice = if !self.capabilities.bracketed_paste {
            Some("Paste unavailable")
        } else if !self.capabilities.mouse {
            Some("Mouse unavailable")
        } else {
            None
        };
        let task_status = task_status_line(&snapshot);
        let footer_text = self.status_line(&snapshot, capability_notice, &task_status);
        frame.render_widget(
            Paragraph::new(footer_text)
                .style(Style::default().fg(theme.gray_dim).bg(theme.bg_base)),
            footer,
        );

        if self.shell.overlay() == Overlay::Picker {
            self.render_picker(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::Queue {
            self.render_queue(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::Interaction {
            self.render_interaction(frame, area, &snapshot);
        }
        self.frame = self.frame.wrapping_add(1);
    }

    fn status_line(
        &self,
        snapshot: &GrokHostSnapshot,
        capability_notice: Option<&str>,
        task_status: &str,
    ) -> String {
        let base = self
            .status
            .as_deref()
            .or(snapshot.status.as_deref())
            .or(capability_notice)
            .unwrap_or("Enter send  p sessions  q queue  i interaction  Esc quit");
        let mut details = format!("queue r{}", snapshot.queue_revision);
        if let Some(pending) = &self.queue_pending {
            details.push_str(&format!(
                " · pending {} ({})",
                pending.item_id, pending.operation.request_id
            ));
        }
        if self.interaction_pending.is_some() {
            details.push_str(" · response pending");
        }
        if !task_status.is_empty() {
            details.push_str(" · ");
            details.push_str(task_status);
        }
        format!("{base} · {details}")
    }

    fn reconcile_snapshot(&mut self, snapshot: &GrokHostSnapshot) {
        if let Some(interaction) = snapshot.interaction.as_ref() {
            let request_id = DshRequestId::new(interaction.request_id());
            let generation = snapshot.session_header.generation;
            let changed = self.interaction_request_id.as_ref() != Some(&request_id)
                || self.interaction_generation != Some(generation);
            if changed {
                self.interaction_request_id = Some(request_id);
                self.interaction_generation = Some(generation);
                self.interaction_editor.reset();
                self.interaction_selected = 0;
                self.interaction_pending = None;
            }
            if self.shell.overlay() != Overlay::Interaction {
                self.shell.open_interaction();
            }
        } else if self.shell.overlay() == Overlay::Interaction {
            self.shell.close_overlay();
            self.interaction_request_id = None;
            self.interaction_generation = None;
            self.interaction_pending = None;
            self.interaction_editor.reset();
            self.status = Some("Interaction resolved".into());
        }

        if let Some(pending) = &self.queue_pending
            && snapshot.queue_revision != pending.base_revision
        {
            self.queue_pending = None;
            self.queue_editing = false;
            self.queue_editor.reset();
            self.status = Some(format!(
                "Queue updated at revision {}; local receipt converged",
                snapshot.queue_revision
            ));
        }
        self.queue_selected_id = self.queue_selected_id.as_deref().and_then(|selected| {
            snapshot
                .queue
                .iter()
                .any(|item| item.id == selected)
                .then(|| selected.to_string())
        });
        if self.queue_selected_id.is_none() {
            self.queue_selected_id = snapshot
                .queue
                .first()
                .map(|item| DshQueueItemId::new(item.id.clone()).to_string());
        }
    }

    fn render_transcript(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        snapshot: &GrokHostSnapshot,
        scrollback: &mut dsh_pager::scrollback::Scrollback,
    ) {
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
            let total_height = scrollback.total_height(content.width as usize);
            let max_scroll = total_height.saturating_sub(content.height as usize);
            self.scroll = self.scroll.min(max_scroll);
            let scroll_top = max_scroll.saturating_sub(self.scroll);
            for paint in
                scrollback.visible_lines(content.width as usize, scroll_top, content.height)
            {
                let style = if paint.header {
                    Style::default().fg(theme.gray_bright)
                } else {
                    Style::default().fg(theme.text_primary)
                };
                lines.push(Line::from(Span::styled(paint.text, style)));
            }
        }
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme.bg_light))
            .style(Style::default().bg(theme.bg_base));
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(block)
                .style(Style::default().bg(theme.bg_base))
                .wrap(Wrap { trim: false })
                .scroll((0, 0)),
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
        let viewport = self
            .prompt
            .viewport(available, area.height.saturating_sub(1) as usize);
        let placeholder = if self.prompt.is_empty() {
            truncate_str("Ask DeepSeek anything…", available)
        } else {
            String::new()
        };
        let style = if self.prompt.is_empty() {
            Style::default().fg(theme.gray_dim)
        } else {
            Style::default().fg(theme.text_primary)
        };
        frame.render_widget(
            Paragraph::new(if placeholder.is_empty() {
                Text::from(
                    viewport
                        .lines
                        .iter()
                        .map(|line| Line::from(format!("{label}{line}")))
                        .collect::<Vec<_>>(),
                )
            } else {
                Text::from(Line::from(format!("{label}{placeholder}")))
            })
            .style(style)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(theme.bg_light)),
            )
            .wrap(Wrap { trim: false }),
            area,
        );
        if !self.prompt.is_empty() && area.height > 0 && area.width > 0 {
            let cursor_x = area
                .x
                .saturating_add(label.len() as u16)
                .saturating_add(viewport.cursor_x as u16)
                .min(area.right().saturating_sub(1));
            let cursor_y = area
                .y
                .saturating_add(viewport.cursor_y as u16)
                .min(area.bottom().saturating_sub(1));
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
    }

    fn render_picker(&mut self, frame: &mut Frame<'_>, area: Rect, snapshot: &GrokHostSnapshot) {
        let theme = Theme::current();
        let mut entries = snapshot.picker_entries_filtered(self.picker.query());
        let row_ids = snapshot.picker_row_ids_filtered(self.picker.query());
        if let Some(selected_id) = self.picker_selected_id.as_deref()
            && let Some(index) = row_ids.iter().position(|id| *id == selected_id)
        {
            self.picker.selected = index;
        }
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

    fn render_queue(&mut self, frame: &mut Frame<'_>, area: Rect, snapshot: &GrokHostSnapshot) {
        let theme = Theme::current();
        let shortcuts = [
            Shortcut {
                label: "Enter save",
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
            title: "Queue · host authority",
            tabs: None,
            shortcuts: &shortcuts,
            sizing: ModalSizing::medium(),
            fold_info: None,
        };
        let buf = frame.buffer_mut();
        if let Some(content) = render_modal_window(buf, area, &mut self.modal, &config, theme) {
            render_queue_content(
                buf,
                content.content,
                &snapshot.queue,
                QueueRenderState {
                    selected_id: self.queue_selected_id.as_deref(),
                    editing: self.queue_editing,
                    editor_text: self.queue_editor.text(),
                    pending_id: self
                        .queue_pending
                        .as_ref()
                        .map(|pending| pending.item_id.as_str()),
                    revision: snapshot.queue_revision,
                },
                theme,
            );
        }
    }

    fn render_interaction(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        snapshot: &GrokHostSnapshot,
    ) {
        let Some(interaction) = snapshot.interaction.as_ref() else {
            return;
        };
        let theme = Theme::current();
        let shortcuts = [
            Shortcut {
                label: "Enter respond",
                clickable: true,
                id: 1,
            },
            Shortcut {
                label: "Esc defer",
                clickable: true,
                id: 2,
            },
        ];
        let config = ModalWindowConfig {
            title: "Interaction · host request",
            tabs: None,
            shortcuts: &shortcuts,
            sizing: ModalSizing::medium(),
            fold_info: None,
        };
        let buf = frame.buffer_mut();
        if let Some(content) = render_modal_window(buf, area, &mut self.modal, &config, theme) {
            render_interaction_content(
                buf,
                content.content,
                interaction,
                self.interaction_selected,
                self.interaction_editor.text(),
                self.interaction_pending.is_some(),
                theme,
            );
        }
    }

    fn dispatch_event(
        &mut self,
        event: ShellEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<bool> {
        let prompt_empty = self.prompt.is_empty();
        let action = self.shell.dispatch(event, prompt_empty);
        match action {
            ShellAction::Quit => Ok(true),
            ShellAction::OpenPicker => {
                if let Err(error) = dsh_pager::list_sessions(transport) {
                    self.status = Some(format!("Session refresh failed: {error}"));
                }
                self.picker = PickerState::input_active();
                self.picker.mode = PickerMode::Floating;
                self.picker_selected_id = Some(session.session_id().to_string());
                self.picker_entry_count = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                )
                .picker_row_ids_filtered("")
                .len();
                self.status = Some("Session picker opened".into());
                Ok(false)
            }
            ShellAction::OpenQueue => {
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                self.queue_selected_id = snapshot.queue.first().map(|item| item.id.clone());
                self.queue_editing = false;
                self.queue_editor.reset();
                self.status = if snapshot.queue.is_empty() {
                    Some("Queue is empty".into())
                } else {
                    Some(format!(
                        "Queue opened at revision {}",
                        snapshot.queue_revision
                    ))
                };
                Ok(false)
            }
            ShellAction::OpenInteraction => {
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                if snapshot.interaction.is_none() {
                    self.shell.close_overlay();
                    self.status = Some("No pending interaction".into());
                } else {
                    self.reconcile_snapshot(&snapshot);
                }
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
            ShellAction::PromptNewline => {
                let _ = self.prompt.insert_newline();
                self.status = None;
                Ok(false)
            }
            ShellAction::PickerKey(key) => {
                let _ = self.handle_picker_event(Event::Key(key), transport, session);
                Ok(false)
            }
            ShellAction::PickerMouse(mouse) => {
                let _ = self.handle_picker_event(Event::Mouse(mouse), transport, session);
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
                let _ = self.handle_picker_event(Event::Paste(text), transport, session);
                Ok(false)
            }
            ShellAction::QueueKey(key) => {
                self.handle_queue_key(key, transport, session)?;
                Ok(false)
            }
            ShellAction::QueueMouse(mouse) => {
                self.handle_queue_mouse(mouse, transport, session)?;
                Ok(false)
            }
            ShellAction::QueuePaste(text) => {
                if self.queue_editing && !text.is_empty() {
                    let _ = self.queue_editor.insert_paste(&text);
                }
                Ok(false)
            }
            ShellAction::InteractionKey(key) => {
                self.handle_interaction_key(key, transport, session)?;
                Ok(false)
            }
            ShellAction::InteractionMouse(mouse) => {
                self.handle_interaction_mouse(mouse, transport, session)?;
                Ok(false)
            }
            ShellAction::InteractionPaste(text) => {
                if !text.is_empty() && self.interaction_pending.is_none() {
                    let _ = self.interaction_editor.insert_paste(&text);
                }
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
        if matches!(
            receipt.status,
            UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
        ) {
            self.prompt.reset();
            self.status = Some("Prompt queued".into());
        } else {
            self.status = Some(
                receipt
                    .diagnostic
                    .unwrap_or_else(|| "Prompt rejected by host; draft retained".into()),
            );
        }
        Ok(())
    }

    fn handle_queue_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        if self.queue_editing {
            match key.code {
                KeyCode::Esc => {
                    self.queue_editing = false;
                    self.queue_editor.reset();
                    self.status = Some("Queue edit cancelled".into());
                }
                KeyCode::Enter => self.submit_queue_edit(transport, session, &snapshot)?,
                _ => {
                    let _ = self.queue_editor.handle_key(&key);
                }
            }
            return Ok(());
        }
        if self.queue_pending.is_some() {
            if key.code == KeyCode::Esc {
                self.shell.close_overlay();
                self.status = Some("Queue overlay closed; update still pending".into());
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Esc => {
                self.shell.close_overlay();
                self.status = Some("Queue closed".into());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.queue_selected_id =
                    moved_selection(&snapshot.queue, self.queue_selected_id.as_deref(), -1)
                        .map(|id| id.to_string());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.queue_selected_id =
                    moved_selection(&snapshot.queue, self.queue_selected_id.as_deref(), 1)
                        .map(|id| id.to_string());
            }
            KeyCode::Char('e') => {
                let Some(item) = self.selected_queue_item(&snapshot) else {
                    self.status = Some("No queue item selected".into());
                    return Ok(());
                };
                self.queue_editor.reset();
                let text = item
                    .content
                    .editable_text
                    .as_deref()
                    .or(item.content.summary.as_deref())
                    .unwrap_or_default();
                let _ = self.queue_editor.insert_paste(text);
                self.queue_editing = true;
                self.status = Some(format!("Editing queue item {}", item.id));
            }
            KeyCode::Char('d') => {
                self.submit_queue_action(transport, session, &snapshot, QueueAction::Remove)?;
            }
            KeyCode::Char('s') => {
                self.submit_queue_action(transport, session, &snapshot, QueueAction::Steer)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_queue_mouse(
        &mut self,
        mouse: crossterm::event::MouseEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        if self.queue_pending.is_some() {
            return Ok(());
        }
        match mouse.kind {
            crossterm::event::MouseEventKind::ScrollUp => {
                self.queue_selected_id =
                    moved_selection(&snapshot.queue, self.queue_selected_id.as_deref(), -1)
                        .map(|id| id.to_string());
                return Ok(());
            }
            crossterm::event::MouseEventKind::ScrollDown => {
                self.queue_selected_id =
                    moved_selection(&snapshot.queue, self.queue_selected_id.as_deref(), 1)
                        .map(|id| id.to_string());
                return Ok(());
            }
            _ => {}
        }
        match handle_modal_mouse(&mut self.modal, mouse.kind, mouse.column, mouse.row) {
            ModalWindowOutcome::CloseRequested => {
                self.shell.close_overlay();
                self.status = Some("Queue closed".into());
            }
            ModalWindowOutcome::ShortcutActivated(1) if self.queue_editing => {
                self.submit_queue_edit(transport, session, &snapshot)?;
            }
            ModalWindowOutcome::ShortcutActivated(2) => {
                if self.queue_editing {
                    self.queue_editing = false;
                    self.queue_editor.reset();
                } else {
                    self.shell.close_overlay();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn selected_queue_item<'a>(
        &self,
        snapshot: &'a GrokHostSnapshot,
    ) -> Option<&'a dsh_pager::DshQueueItem> {
        self.queue_selected_id
            .as_deref()
            .and_then(|id| snapshot.queue.iter().find(|item| item.id == id))
            .or_else(|| snapshot.queue.first())
    }

    fn submit_queue_edit(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        snapshot: &GrokHostSnapshot,
    ) -> PagerResult<()> {
        let text = self.queue_editor.text().to_string();
        if text.trim().is_empty() {
            self.status = Some("Queue edit is empty".into());
            return Ok(());
        }
        self.submit_queue_action(
            transport,
            session,
            snapshot,
            QueueAction::Edit {
                content: vec![json!({"type": "text", "text": text})],
            },
        )
    }

    fn submit_queue_action(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        snapshot: &GrokHostSnapshot,
        action: QueueAction,
    ) -> PagerResult<()> {
        if self.queue_pending.is_some() {
            self.status = Some("Queue update already pending".into());
            return Ok(());
        }
        let Some(item) = self.selected_queue_item(snapshot) else {
            self.status = Some("No queue item selected".into());
            return Ok(());
        };
        let item_id = DshQueueItemId::new(item.id.clone());
        let request_id = DshRequestId::new(format!("queue-{}", self.next_operation));
        self.next_operation = self.next_operation.saturating_add(1);
        let context = UiContext::for_operation(session, request_id);
        let mut sink = DshEffectSink::new(transport);
        let receipt = sink.submit(
            UiIntent::QueueMutation {
                item_id: item_id.clone(),
                action,
            },
            &context,
        )?;
        match receipt.status {
            UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending => {
                self.queue_pending = Some(PendingQueueMutation {
                    operation: receipt.operation.clone(),
                    item_id,
                    base_revision: snapshot.queue_revision,
                });
                self.queue_editing = false;
                self.queue_editor.reset();
                self.status = Some(format!(
                    "Queue update accepted at revision {}; waiting for host snapshot",
                    snapshot.queue_revision
                ));
            }
            _ => {
                self.status = Some(
                    receipt
                        .diagnostic
                        .unwrap_or_else(|| "Queue update rejected; local queue unchanged".into()),
                );
            }
        }
        Ok(())
    }

    fn handle_interaction_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        if self.interaction_pending.is_some() {
            if key.code == KeyCode::Esc {
                self.shell.close_overlay();
                self.status = Some("Interaction response pending".into());
            }
            return Ok(());
        }
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        let Some(interaction) = snapshot.interaction.as_ref() else {
            self.shell.close_overlay();
            self.status = Some("Interaction is no longer pending".into());
            return Ok(());
        };
        match key.code {
            KeyCode::Esc => {
                self.shell.close_overlay();
                self.status = Some("Interaction deferred".into());
            }
            KeyCode::Char('y') | KeyCode::Char('a') => {
                if matches!(interaction, DshInteraction::Approval { .. }) {
                    self.submit_approval(transport, session, interaction, "allowed-once")?;
                }
            }
            KeyCode::Char('n') | KeyCode::Char('d') => {
                if matches!(interaction, DshInteraction::Approval { .. }) {
                    self.submit_approval(transport, session, interaction, "denied")?;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.interaction_selected = self.interaction_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.interaction_selected = self.interaction_selected.saturating_add(1);
            }
            KeyCode::Char(digit @ '1'..='9') => {
                self.interaction_selected = digit.to_digit(10).unwrap_or(1) as usize - 1;
            }
            KeyCode::Enter => {
                let response = response_for(
                    interaction,
                    self.interaction_selected,
                    self.interaction_editor.text(),
                );
                if let Some(response) = response {
                    self.submit_interaction(transport, session, interaction, response)?;
                } else {
                    self.status = Some("Answer is empty".into());
                }
            }
            _ => {
                let _ = self.interaction_editor.handle_key(&key);
            }
        }
        Ok(())
    }

    fn handle_interaction_mouse(
        &mut self,
        mouse: crossterm::event::MouseEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        if self.interaction_pending.is_some() {
            return Ok(());
        }
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        let Some(interaction) = snapshot.interaction.as_ref() else {
            self.shell.close_overlay();
            return Ok(());
        };
        match handle_modal_mouse(&mut self.modal, mouse.kind, mouse.column, mouse.row) {
            ModalWindowOutcome::CloseRequested | ModalWindowOutcome::ShortcutActivated(2) => {
                self.shell.close_overlay();
                self.status = Some("Interaction deferred".into());
            }
            ModalWindowOutcome::ShortcutActivated(1) => {
                if let Some(response) = response_for(
                    interaction,
                    self.interaction_selected,
                    self.interaction_editor.text(),
                ) {
                    self.submit_interaction(transport, session, interaction, response)?;
                } else {
                    self.status = Some("Answer is empty".into());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn submit_approval(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        interaction: &DshInteraction,
        outcome: &str,
    ) -> PagerResult<()> {
        let DshInteraction::Approval { approval_id, .. } = interaction else {
            return Ok(());
        };
        self.submit_interaction(
            transport,
            session,
            interaction,
            TuiInteractionResponse::Approval {
                approval_id: approval_id.clone(),
                outcome: outcome.into(),
            },
        )
    }

    fn submit_interaction(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        interaction: &DshInteraction,
        response: TuiInteractionResponse,
    ) -> PagerResult<()> {
        let request_id = DshRequestId::new(interaction.request_id());
        let context = UiContext::for_operation(session, request_id.clone());
        let mut sink = DshEffectSink::new(transport);
        let receipt = sink.submit(
            UiIntent::RespondInteraction {
                request_id: request_id.clone(),
                interaction: response,
            },
            &context,
        )?;
        match receipt.status {
            UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending => {
                self.interaction_pending = Some(request_id);
                self.status =
                    Some("Interaction response accepted; waiting for host resolution".into());
            }
            _ => {
                self.status = Some(
                    receipt
                        .diagnostic
                        .unwrap_or_else(|| "Interaction response rejected".into()),
                );
            }
        }
        Ok(())
    }

    fn handle_picker_event(
        &mut self,
        event: Event,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> bool {
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
            PickerOutcome::Closed => {
                self.shell.close_overlay();
                self.status = Some("Session picker closed".into());
                false
            }
            PickerOutcome::Selected(index) => {
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                let ids = snapshot.picker_row_ids_filtered(self.picker.query());
                let Some(target) = ids.get(index).copied() else {
                    self.status = Some("Selected session is no longer available".into());
                    return false;
                };
                self.picker_selected_id = Some(target.to_string());
                if target.contains(':') {
                    self.status = Some("Selected row is not attachable".into());
                    return false;
                }
                let effect = compile_intent(
                    UiIntent::AttachSession {
                        session_id: dsh_pager::DshSessionId::new(target),
                    },
                    &UiContext::from_session(session),
                );
                let UiEffect::AttachSession { session_id, .. } = effect else {
                    self.status = Some("Unable to compile attach operation".into());
                    return false;
                };
                if session_id.as_str() == session.session_id() {
                    self.shell.close_overlay();
                    self.status = Some("Already attached".into());
                    return false;
                }
                self.status = Some(format!("Attaching {}…", session_id.as_str()));
                match load_session_id(
                    transport,
                    session.generation(),
                    session_id.as_str().to_string(),
                ) {
                    Ok(next) => {
                        *session = next;
                        self.scroll = 0;
                        self.picker.reset();
                        self.picker_selected_id = None;
                        self.shell.close_overlay();
                        self.status = Some("Session attached".into());
                    }
                    Err(error) => {
                        self.status = Some(format!("Attach failed: {error}"));
                    }
                }
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

fn task_status_line(snapshot: &GrokHostSnapshot) -> String {
    if snapshot.tasks.is_empty() {
        return String::new();
    }
    let mut running = 0usize;
    let mut errors = 0usize;
    let mut completed = 0usize;
    for task in &snapshot.tasks {
        match task.status.to_ascii_lowercase().as_str() {
            "running" | "pending" | "queued" => running += 1,
            "error" | "failed" | "cancelled" => errors += 1,
            "completed" | "complete" | "done" => completed += 1,
            _ => {}
        }
    }
    let mut parts = Vec::new();
    if running > 0 {
        parts.push(format!("{running} running"));
    }
    if errors > 0 {
        parts.push(format!("{errors} error"));
    }
    if completed > 0 {
        parts.push(format!("{completed} completed"));
    }
    if parts.is_empty() {
        format!("{} tasks", snapshot.tasks.len())
    } else {
        parts.join(", ")
    }
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
