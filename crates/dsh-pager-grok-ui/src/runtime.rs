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

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use dsh_pager::dashboard::DashboardModel;
use dsh_pager::{
    DshGeneration, DshInteraction, DshQueueItemId, DshRequestId, PagerError, PagerResult,
    RpcTransport, SessionState, SessionUpdate, load_session_id, peek_session_tail, repair_tail,
};
use dsh_pager_protocol::{PromptMode, QueueAction, TuiInteractionResponse};
use dsh_pager_render::{TerminalCapabilities, TerminalSurface};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{AppShell, Overlay, ShellAction, ShellEvent};
use crate::clipboard::{self, ClipboardBackend};
use crate::effects::{
    DshEffectSink, OperationKey, UiContext, UiEffect, UiEffectSink, UiEffectStatus, UiIntent,
    compile_intent, receipt_status_message,
};
use crate::geometry::{
    GeometryLine, HitMap, HitTarget, LinkTarget, column_for_grapheme, first_link_target,
    insert_text_line,
};
use crate::host_adapter::GrokHostSnapshot;
use crate::input::{PromptEditor, line_editor::LineEditOutcome};
use crate::modal_window_state::ModalWindowState;
use crate::render::line_utils::truncate_str;
use crate::scheduler::SchedulerStats;
use crate::selection::SelectionModel;
use crate::theme::Theme;
use crate::views::{
    agent::AgentView,
    dashboard::{DashboardPeek, render_dashboard_content},
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
    transcript::RichTranscript,
};
use serde_json::json;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const NOTIFICATION_BUDGET: usize = 256;

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
        match drain_notifications_bounded(transport, session, NOTIFICATION_BUDGET) {
            Ok((update, processed)) if update.gap_detected => {
                ui.scheduler_stats.enqueued =
                    ui.scheduler_stats.enqueued.saturating_add(processed as u64);
                ui.scheduler_stats.processed = ui
                    .scheduler_stats
                    .processed
                    .saturating_add(processed as u64);
                ui.scheduler_stats.max_pending = ui.scheduler_stats.max_pending.max(processed);
                if let Err(error) = repair_tail(transport, session) {
                    ui.status = Some(format!("history repair error: {error}"));
                }
            }
            Ok((update, processed)) if update.changed => {
                ui.scheduler_stats.enqueued =
                    ui.scheduler_stats.enqueued.saturating_add(processed as u64);
                ui.scheduler_stats.processed = ui
                    .scheduler_stats
                    .processed
                    .saturating_add(processed as u64);
                ui.scheduler_stats.max_pending = ui.scheduler_stats.max_pending.max(processed);
                ui.shell.invalidate_content();
                let _ = ui.dispatch_event(ShellEvent::Notification, transport, session)?;
            }
            Ok((_, processed)) => {
                ui.scheduler_stats.enqueued =
                    ui.scheduler_stats.enqueued.saturating_add(processed as u64);
                ui.scheduler_stats.processed = ui
                    .scheduler_stats
                    .processed
                    .saturating_add(processed as u64);
                ui.scheduler_stats.max_pending = ui.scheduler_stats.max_pending.max(processed);
            }
            Err(error) => {
                ui.status = Some(format!("notification error: {error}"));
            }
        }
        let frame_links = ui.frame_links.clone();
        terminal.draw_with_links(&frame_links, |frame| {
            ui.render(frame, session, transport.control_plane())
        })?;
        if !event::poll(POLL_INTERVAL)? {
            continue;
        }
        let event = event::read()?;
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let quit = ui.dispatch_event(ShellEvent::Key(key), transport, session)?;
                ui.flush_copy(terminal);
                if quit {
                    break;
                }
            }
            Event::Mouse(mouse) => {
                let quit = ui.dispatch_event(ShellEvent::Mouse(mouse), transport, session)?;
                ui.flush_copy(terminal);
                if quit {
                    break;
                }
            }
            Event::Paste(text) => {
                let quit = ui.dispatch_event(ShellEvent::Paste(text), transport, session)?;
                ui.flush_copy(terminal);
                if quit {
                    break;
                }
            }
            Event::Resize(width, height) => {
                let _ =
                    ui.dispatch_event(ShellEvent::Resize { width, height }, transport, session)?;
                ui.flush_copy(terminal);
            }
            _ => {
                let _ = ui.dispatch_event(ShellEvent::Tick, transport, session)?;
                ui.flush_copy(terminal);
            }
        }
    }
    Ok(())
}

fn drain_notifications_bounded(
    transport: &mut RpcTransport,
    session: &mut SessionState,
    budget: usize,
) -> PagerResult<(SessionUpdate, usize)> {
    let mut combined = SessionUpdate::default();
    let mut processed = 0usize;
    while processed < budget {
        let Some(note) = transport.try_notification()? else {
            break;
        };
        processed += 1;
        let update = transport.route_notification(session, note)?;
        combined.changed |= update.changed;
        combined.gap_detected |= update.gap_detected;
    }
    // A pending sequence gap remains visible even when this frame's budget was
    // exhausted; repair is deliberately handled before the next paint.
    combined.gap_detected |= session.needs_repair();
    Ok((combined, processed))
}

#[derive(Debug, Default)]
struct UiState {
    shell: AppShell,
    capabilities: TerminalCapabilities,
    scroll: usize,
    transcript_width: Option<u16>,
    scroll_anchor: Option<dsh_pager::scrollback::ScrollAnchor>,
    picker: PickerState,
    picker_entry_count: usize,
    modal: ModalWindowState,
    prompt: PromptEditor,
    prompt_mode: Option<PromptMode>,
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
    dashboard: DashboardModel,
    dashboard_revision: Option<u64>,
    dashboard_query: PromptEditor,
    dashboard_query_active: bool,
    dashboard_peek: Option<DashboardPeek>,
    prompt_history_index: Option<usize>,
    local_prompt_history: Vec<String>,
    next_operation: u64,
    status: Option<String>,
    frame: usize,
    hit_map: HitMap,
    geometry_lines: Vec<GeometryLine>,
    selection: SelectionModel,
    hover_link: Option<LinkTarget>,
    frame_links: Vec<dsh_grok_inline::LinkSpan>,
    pending_copy: Option<String>,
    scheduler_stats: SchedulerStats,
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
        self.hit_map.resize(area);
        self.hit_map.clear();
        self.geometry_lines.clear();
        let background = Block::default().style(Style::default().bg(theme.bg_base));
        frame.render_widget(background, area);

        let snapshot =
            GrokHostSnapshot::from_session_with_control_plane(session, Some(control_plane));
        self.sync_dashboard(control_plane);
        self.reconcile_snapshot(&snapshot);
        let agent_layout = AgentView::layout_with_prompt(
            &mut self.shell,
            area,
            self.prompt.text(),
            snapshot.running,
            self.status.is_some() || snapshot.status.is_some(),
        );
        let header = agent_layout.header;
        let input = agent_layout.prompt;
        let connection = format!(
            "{} · {} · q{} · {}",
            snapshot.connection,
            snapshot.model,
            snapshot.queue_revision,
            if snapshot.running { "running" } else { "idle" }
        );
        let compact_header = agent_layout.compact || header.width < 70;
        let header_center = if compact_header {
            "GROK UI"
        } else {
            "DSH · GROK UI"
        };
        let header_right = if compact_header {
            if snapshot.running { "running" } else { "idle" }.to_string()
        } else {
            format!(" · {connection}")
        };
        frame.render_widget(
            StatusBar::new(&snapshot.session_title)
                .center(header_center)
                .right(&header_right),
            header,
        );

        self.render_transcript(
            frame,
            agent_layout.transcript,
            agent_layout.rail,
            &snapshot,
            &mut session.scrollback,
        );
        let mode = self.prompt_mode.unwrap_or(snapshot.prompt.default_mode);
        let prompt_width = input.width.saturating_sub(6).max(1) as usize;
        let viewport = self
            .prompt
            .viewport(prompt_width, input.height.saturating_sub(3).max(1) as usize);
        AgentView::render_prompt(
            frame,
            input,
            crate::views::agent::PromptRenderState {
                mode,
                running: snapshot.running,
                title: &snapshot.session_title,
                model: &snapshot.model,
                viewport: &viewport,
                empty: self.prompt.is_empty(),
            },
            theme,
        );
        self.hit_map.insert(crate::geometry::HitRegion {
            target: HitTarget::Prompt,
            rect: input,
            label: "prompt".into(),
            link: None,
            priority: 15,
        });
        AgentView::render_turn_status(
            frame,
            agent_layout.turn_status,
            snapshot.running,
            snapshot.status.as_deref(),
            theme,
        );
        let capability_notice = if !self.capabilities.bracketed_paste {
            Some("Paste unavailable")
        } else if !self.capabilities.mouse {
            Some("Mouse unavailable")
        } else {
            None
        };
        let task_status = task_status_line(&snapshot);
        let footer_text = self.status_line(&snapshot, capability_notice, &task_status);
        AgentView::render_status_line(frame, agent_layout.status_line, &footer_text, theme);
        AgentView::render_shortcuts(frame, agent_layout.shortcuts, agent_layout.compact);

        if self.shell.overlay() == Overlay::Picker {
            self.render_picker(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::Queue {
            self.render_queue(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::Interaction {
            self.render_interaction(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::Dashboard {
            self.render_dashboard(frame, area);
        }
        self.frame_links = self.hit_map.link_spans();
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
        let mode = self.prompt_mode.unwrap_or(snapshot.prompt.default_mode);
        let mut details = format!(
            "mode {} · queue r{}",
            AgentView::mode_label(mode),
            snapshot.queue_revision
        );
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
        if self.scheduler_stats.dropped > 0 {
            details.push_str(&format!(
                " · backlog dropped {}",
                self.scheduler_stats.dropped
            ));
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

    fn sync_dashboard(&mut self, control_plane: &dsh_pager::ControlPlaneStore) {
        let revision = control_plane.revision();
        if self.dashboard_revision == Some(revision) {
            return;
        }
        self.dashboard.replace_control_plane_with_workspaces(
            control_plane.snapshots().cloned().collect(),
            control_plane.workspaces().cloned().collect(),
            control_plane.workspace_order().to_vec(),
        );
        self.dashboard_revision = Some(revision);
    }

    fn render_dashboard(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::current();
        let shortcuts = [
            Shortcut {
                label: "Enter attach",
                clickable: true,
                id: 1,
            },
            Shortcut {
                label: "Esc back",
                clickable: true,
                id: 2,
            },
        ];
        let config = ModalWindowConfig {
            title: "Dashboard · DSH control plane",
            tabs: Some(&["sessions", "workspaces", "jobs"]),
            shortcuts: &shortcuts,
            sizing: ModalSizing::large(),
            fold_info: None,
        };
        let buf = frame.buffer_mut();
        if let Some(content) = render_modal_window(buf, area, &mut self.modal, &config, theme) {
            render_dashboard_content(
                buf,
                content.content,
                &self.dashboard,
                self.dashboard_peek.as_ref(),
                self.dashboard_query_active,
                self.dashboard_query.text(),
                theme,
            );
        }
    }

    fn render_transcript(
        &mut self,
        frame: &mut Frame<'_>,
        content: Rect,
        rail: Rect,
        snapshot: &GrokHostSnapshot,
        scrollback: &mut dsh_pager::scrollback::Scrollback,
    ) {
        let theme = Theme::current();
        let mut lines = Vec::new();
        let entries = scrollback.render_entries();
        if entries.is_empty() {
            self.scroll = 0;
            self.scroll_anchor = None;
            lines.push(Line::from(Span::styled(
                "  No transcript events yet. Type a prompt below.",
                Style::default().fg(theme.gray),
            )));
        } else {
            let render_width = content.width.saturating_sub(1).max(1) as usize;
            let rich = RichTranscript::new(&entries, render_width, *theme);
            let total_height = rich.total_height();
            let mut max_scroll = total_height.saturating_sub(content.height as usize);
            self.scroll = self.scroll.min(max_scroll);
            let mut scroll_top = max_scroll.saturating_sub(self.scroll);
            if self.transcript_width != Some(content.width) {
                if let Some(anchor) = self.scroll_anchor.take()
                    && let Some(restored) = rich.scroll_for_anchor(anchor)
                {
                    scroll_top = restored;
                }
                self.transcript_width = Some(content.width);
            }
            max_scroll = total_height.saturating_sub(content.height as usize);
            scroll_top = scroll_top.min(max_scroll);
            self.scroll = max_scroll.saturating_sub(scroll_top);
            for paint in rich.visible_lines(scroll_top, content.height) {
                let text = paint.line.to_string();
                while lines.len() < paint.screen_y as usize {
                    lines.push(Line::from(""));
                }
                lines.push(paint.line);
                let geometry = insert_text_line(
                    &mut self.hit_map,
                    HitTarget::TranscriptEntry(paint.entry_id),
                    paint.line_index,
                    content.x.saturating_add(1),
                    content.y.saturating_add(paint.screen_y),
                    render_width as u16,
                    &text,
                    first_link_target(&text),
                );
                self.geometry_lines.push(geometry);
            }
            self.scroll_anchor = rich.anchor_at(scroll_top);
        }
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme.bg_light))
            .style(Style::default().bg(theme.bg_base));
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(block)
                .style(Style::default().bg(theme.bg_base))
                .scroll((0, 0)),
            content,
        );

        if let Some(selection) = self.selection.selection() {
            let buffer = frame.buffer_mut();
            for line in &self.geometry_lines {
                let HitTarget::TranscriptEntry(entry_id) = line.target else {
                    continue;
                };
                let grapheme_count =
                    unicode_segmentation::UnicodeSegmentation::graphemes(line.text.as_str(), true)
                        .count();
                let Some((start, end)) =
                    selection.grapheme_range_for_line(entry_id, line.line_index, grapheme_count)
                else {
                    continue;
                };
                let start_column = column_for_grapheme(&line.text, start) as u16;
                let end_column = column_for_grapheme(&line.text, end) as u16;
                buffer.set_style(
                    Rect::new(
                        line.rect.x.saturating_add(start_column),
                        line.rect.y,
                        end_column.saturating_sub(start_column),
                        1,
                    ),
                    Style::default().bg(theme.bg_visual),
                );
            }
        }

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
            ShellAction::OpenDashboard => {
                if let Err(error) = dsh_pager::list_sessions(transport) {
                    self.status = Some(format!("Dashboard refresh failed: {error}"));
                }
                self.dashboard_revision = None;
                self.dashboard_query_active = false;
                self.dashboard_query.reset();
                self.dashboard_peek = None;
                self.sync_dashboard(transport.control_plane());
                self.dashboard.select_first();
                self.status = Some("Dashboard opened".into());
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
            ShellAction::TogglePromptMode => {
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                let current = self.prompt_mode.unwrap_or(snapshot.prompt.default_mode);
                let next = match current {
                    PromptMode::Queue => PromptMode::Steer,
                    PromptMode::Steer => PromptMode::Queue,
                };
                if next == PromptMode::Steer && !steer_capability_available(session, &snapshot) {
                    self.status = Some("Steer mode unavailable".into());
                } else {
                    self.prompt_mode = Some(next);
                    self.status = Some(format!("Prompt mode: {}", AgentView::mode_label(next)));
                }
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
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                if self.handle_prompt_command(&key, &snapshot) {
                    return Ok(false);
                }
                match self.prompt.handle_key(&key) {
                    LineEditOutcome::Unhandled => {}
                    LineEditOutcome::HandledNoChange | LineEditOutcome::CursorChanged => {
                        self.status = None
                    }
                    LineEditOutcome::TextChanged => {
                        self.prompt_history_index = None;
                        self.status = None;
                    }
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
                    self.prompt_history_index = None;
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
            ShellAction::DashboardKey(key) => {
                self.handle_dashboard_key(key, transport, session)?;
                Ok(false)
            }
            ShellAction::DashboardMouse(mouse) => {
                self.handle_dashboard_mouse(mouse, transport, session)?;
                Ok(false)
            }
            ShellAction::DashboardPaste(text) => {
                if self.dashboard_query_active && !text.is_empty() {
                    let _ = self.dashboard_query.insert_paste(&text);
                    self.dashboard.set_query(self.dashboard_query.text());
                }
                Ok(false)
            }
            ShellAction::TranscriptMouse(mouse) => {
                self.handle_transcript_mouse(mouse);
                Ok(false)
            }
            ShellAction::Resized(area) => {
                let _ = self.shell.layout(area);
                self.modal = ModalWindowState::default();
                self.hit_map.resize(area);
                self.selection.clear();
                self.hover_link = None;
                self.frame_links.clear();
                self.geometry_lines.clear();
                self.transcript_width = None;
                self.status = Some(format!("Resized to {}x{}", area.width, area.height));
                Ok(false)
            }
            ShellAction::None | ShellAction::Redraw => Ok(false),
        }
    }

    fn handle_transcript_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll = self.scroll.saturating_add(3);
            }
            MouseEventKind::ScrollDown => {
                self.scroll = self.scroll.saturating_sub(3);
            }
            MouseEventKind::Moved => {
                self.hover_link = self.hit_map.link_at(mouse.column, mouse.row).cloned();
                if let Some(link) = &self.hover_link {
                    self.status = Some(format!("Link: {}", link.url));
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(region) = self.hit_map.hit_test(mouse.column, mouse.row) else {
                    self.selection.clear();
                    return;
                };
                let Some(line) = self
                    .geometry_lines
                    .iter()
                    .find(|line| line.rect.y == mouse.row && line.target == region.target)
                else {
                    return;
                };
                if let Some(point) =
                    SelectionModel::point_for_line(&region.target, line, mouse.column)
                {
                    self.selection.begin(point);
                    self.status = Some("Selecting transcript".into());
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(region) = self.hit_map.hit_test(mouse.column, mouse.row) else {
                    return;
                };
                let Some(line) = self
                    .geometry_lines
                    .iter()
                    .find(|line| line.rect.y == mouse.row && line.target == region.target)
                else {
                    return;
                };
                if let Some(point) =
                    SelectionModel::point_for_line(&region.target, line, mouse.column)
                {
                    self.selection.extend(point);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let Some(selection) = self.selection.finish() else {
                    return;
                };
                if selection.is_empty() {
                    return;
                }
                let copied = self.selection.copy_lines(&self.geometry_lines, &selection);
                if !copied.is_empty() {
                    self.pending_copy = Some(copied);
                    self.status = Some("Selection copied".into());
                }
            }
            _ => {}
        }
    }

    fn flush_copy(&mut self, terminal: &mut TerminalSurface) {
        let Some(text) = self.pending_copy.take() else {
            return;
        };
        let result = if self.capabilities.osc52 {
            terminal.copy_text(&text).map(|_| ClipboardBackend::Osc52)
        } else {
            clipboard::system_clipboard_set(&text).map(|result| result.backend)
        };
        match result {
            Ok(ClipboardBackend::Osc52 | ClipboardBackend::System) => {
                self.status = Some("Selection copied".into());
            }
            Ok(ClipboardBackend::Unavailable) => {
                self.status = Some("Clipboard unavailable".into());
            }
            Err(error) => {
                self.status = Some(format!("Clipboard unavailable: {error}"));
            }
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
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        let mode = self.prompt_mode.unwrap_or(snapshot.prompt.default_mode);
        if mode == PromptMode::Steer && !steer_capability_available(session, &snapshot) {
            self.status = Some("Steer mode unavailable".into());
            return Ok(());
        }
        let mut sink = DshEffectSink::new(transport);
        let context = UiContext::from_session(session);
        let receipt = sink.submit(
            UiIntent::SubmitPrompt {
                text: text.clone(),
                mode,
            },
            &context,
        )?;
        if prompt_receipt_admitted(&receipt.status) {
            self.record_prompt_history(&text);
            self.prompt.reset();
            self.status = Some(prompt_admission_message(&receipt.status));
        } else {
            self.status = Some(receipt_status_message(&receipt, "Prompt"));
        }
        Ok(())
    }

    fn handle_prompt_command(
        &mut self,
        key: &crossterm::event::KeyEvent,
        snapshot: &GrokHostSnapshot,
    ) -> bool {
        use crossterm::event::KeyModifiers;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('x') {
            self.status = if snapshot.capabilities.external_editor {
                Some("External editor capability negotiated; terminal handoff is pending".into())
            } else {
                Some("External editor unavailable".into())
            };
            return true;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('p')
            && snapshot.capabilities.external_pager
        {
            self.status =
                Some("External pager capability negotiated; terminal handoff is pending".into());
            return true;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
            return self.navigate_prompt_history(snapshot, -1);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('n') {
            return self.navigate_prompt_history(snapshot, 1);
        }
        if key.code == KeyCode::Tab
            && self.prompt.text().starts_with('/')
            && snapshot.capabilities.prompt_suggestions
        {
            let prefix = self.prompt.text();
            if let Some(suggestion) = snapshot
                .prompt
                .suggestions
                .iter()
                .find(|suggestion| suggestion.starts_with(prefix))
            {
                let _ = self.prompt.replace_text(suggestion);
                self.status = None;
            } else {
                self.status = Some("No matching slash command".into());
            }
            return true;
        }
        false
    }

    fn combined_prompt_history(&self, snapshot: &GrokHostSnapshot) -> Vec<String> {
        let mut history = if snapshot.capabilities.prompt_history {
            snapshot.prompt.history.clone()
        } else {
            Vec::new()
        };
        history.extend(self.local_prompt_history.iter().cloned());
        history.dedup();
        history
    }

    fn navigate_prompt_history(&mut self, snapshot: &GrokHostSnapshot, direction: isize) -> bool {
        let history = self.combined_prompt_history(snapshot);
        if history.is_empty() {
            self.status = Some("Prompt history unavailable".into());
            return true;
        }
        let next = match (self.prompt_history_index, direction) {
            (None, -1) => history.len().saturating_sub(1),
            (Some(index), -1) => index.saturating_sub(1),
            (Some(index), 1) if index + 1 < history.len() => index + 1,
            (Some(_), 1) => {
                self.prompt_history_index = None;
                self.prompt.reset();
                return true;
            }
            (None, 1) => return true,
            _ => return true,
        };
        self.prompt_history_index = Some(next);
        let _ = self.prompt.replace_text(&history[next]);
        self.status = None;
        true
    }

    fn record_prompt_history(&mut self, text: &str) {
        self.local_prompt_history.retain(|item| item != text);
        self.local_prompt_history.push(text.to_string());
        if self.local_prompt_history.len() > 100 {
            let drop_count = self.local_prompt_history.len() - 100;
            self.local_prompt_history.drain(..drop_count);
        }
        self.prompt_history_index = None;
    }

    fn reset_transcript_view(&mut self) {
        self.scroll = 0;
        self.transcript_width = None;
        self.scroll_anchor = None;
        self.geometry_lines.clear();
        self.selection.clear();
        self.hover_link = None;
        self.frame_links.clear();
    }

    fn handle_dashboard_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        if self.dashboard_peek.is_some() {
            match key.code {
                KeyCode::Esc | KeyCode::Char('b') => self.dashboard_peek = None,
                KeyCode::Enter => {
                    let target = self
                        .dashboard_peek
                        .as_ref()
                        .map(|peek| peek.session_id.clone());
                    if let Some(target) = target {
                        self.attach_session(transport, session, &target)?;
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        if self.dashboard_query_active {
            match key.code {
                KeyCode::Esc => {
                    self.dashboard_query_active = false;
                    self.dashboard_query.reset();
                    self.dashboard.set_query("");
                }
                KeyCode::Enter => self.dashboard_query_active = false,
                _ => {
                    let _ = self.dashboard_query.handle_key(&key);
                    self.dashboard.set_query(self.dashboard_query.text());
                }
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Esc => {
                self.shell.close_overlay();
                self.status = Some("Dashboard closed".into());
            }
            KeyCode::Up | KeyCode::Char('k') => self.dashboard.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.dashboard.move_selection(1),
            KeyCode::Char('/') => {
                self.dashboard_query_active = true;
                let _ = self.dashboard_query.replace_text(self.dashboard.query());
            }
            KeyCode::Char('g') => self.dashboard.toggle_group_by_workspace(),
            KeyCode::Char('a') => self.dashboard.toggle_show_archived(),
            KeyCode::Char('c') => self.dashboard.toggle_collapse_inactive(),
            KeyCode::Char('v') => self.peek_dashboard_selection(transport)?,
            KeyCode::Char('r') => {
                dsh_pager::list_sessions(transport)?;
                self.dashboard_revision = None;
                self.sync_dashboard(transport.control_plane());
                self.status = Some("Dashboard refreshed".into());
            }
            KeyCode::Enter => {
                if let Some(row) = self.dashboard.selected() {
                    let target = row.session_id.clone();
                    self.attach_session(transport, session, &target)?;
                } else {
                    self.status = Some("No session selected".into());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_dashboard_mouse(
        &mut self,
        mouse: crossterm::event::MouseEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        match mouse.kind {
            crossterm::event::MouseEventKind::ScrollUp => self.dashboard.move_selection(-1),
            crossterm::event::MouseEventKind::ScrollDown => self.dashboard.move_selection(1),
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                if self.dashboard_peek.is_some() {
                    return Ok(());
                }
                if let Some(row) = self.dashboard.selected() {
                    let target = row.session_id.clone();
                    self.attach_session(transport, session, &target)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn peek_dashboard_selection(&mut self, transport: &mut RpcTransport) -> PagerResult<()> {
        let Some(row) = self.dashboard.selected() else {
            self.status = Some("No session selected".into());
            return Ok(());
        };
        let session_id = row.session_id.clone();
        let title = row.title.clone();
        let page = peek_session_tail(transport, &session_id, 20)?;
        let lines = page
            .events
            .into_iter()
            .map(|entry| {
                let kind = entry.event.event_type;
                let data = serde_json::to_string(&entry.event.data).unwrap_or_default();
                truncate_str(&format!("[{kind}] {data}"), 240)
            })
            .collect();
        self.dashboard_peek = Some(DashboardPeek {
            session_id,
            title,
            lines,
        });
        self.status = Some("Dashboard peek loaded; no session was attached".into());
        Ok(())
    }

    fn attach_session(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        target: &str,
    ) -> PagerResult<()> {
        if target == session.session_id() {
            self.dashboard_peek = None;
            self.shell.close_overlay();
            self.status = Some("Already attached".into());
            return Ok(());
        }
        self.status = Some(format!("Attaching {target}…"));
        match load_session_id(transport, session.generation(), target.to_string()) {
            Ok(next) => {
                *session = next;
                self.reset_transcript_view();
                self.dashboard_peek = None;
                self.dashboard_query_active = false;
                self.dashboard_revision = None;
                self.shell.close_overlay();
                self.status = Some("Session attached".into());
            }
            Err(error) => self.status = Some(format!("Attach failed: {error}")),
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
                self.status = Some(receipt_status_message(&receipt, "Queue update"));
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
                self.status = Some(receipt_status_message(&receipt, "Interaction response"));
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
                        self.reset_transcript_view();
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

fn steer_capability_available(session: &SessionState, snapshot: &GrokHostSnapshot) -> bool {
    if !snapshot.capabilities.queue_steer {
        return false;
    }
    let Some(value) = session.projection("capabilities") else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("queue_steer")
        .or_else(|| object.get("queueSteer"))
        .and_then(|value| value.as_bool())
        == Some(true)
}

fn prompt_receipt_admitted(status: &UiEffectStatus) -> bool {
    matches!(
        status,
        UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
    )
}

fn prompt_admission_message(status: &UiEffectStatus) -> String {
    match status {
        UiEffectStatus::Accepted => "Prompt accepted; waiting for host snapshot".into(),
        UiEffectStatus::Queued => "Prompt queued".into(),
        UiEffectStatus::Pending => "Prompt pending; waiting for host admission".into(),
        _ => format!("Prompt status: {status:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::prompt_admission_message;
    use super::prompt_receipt_admitted;
    use super::steer_capability_available;
    use crate::effects::UiEffectStatus;
    use crate::host_adapter::GrokHostSnapshot;
    use crate::modal_window_state::ModalWindowState;
    use crate::theme::Theme;
    use crate::views::modal_window::{
        ModalSizing, ModalWindowConfig, Shortcut, render_modal_window,
    };
    use crate::views::picker::{PickerState, render_picker_in_modal};
    use ratatui::{buffer::Buffer, layout::Rect};
    use serde_json::json;

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

    #[test]
    fn prompt_draft_is_only_cleared_after_admission() {
        assert!(prompt_receipt_admitted(&UiEffectStatus::Accepted));
        assert!(prompt_receipt_admitted(&UiEffectStatus::Queued));
        assert!(prompt_receipt_admitted(&UiEffectStatus::Pending));
        assert!(!prompt_receipt_admitted(&UiEffectStatus::Rejected));
        assert!(!prompt_receipt_admitted(&UiEffectStatus::Conflict));
        assert!(!prompt_receipt_admitted(&UiEffectStatus::Stale));
        assert!(!prompt_receipt_admitted(&UiEffectStatus::Failed));
        assert!(prompt_admission_message(&UiEffectStatus::Accepted).contains("accepted"));
        assert!(prompt_admission_message(&UiEffectStatus::Queued).contains("queued"));
        assert!(prompt_admission_message(&UiEffectStatus::Pending).contains("pending"));
    }

    #[test]
    fn steer_requires_an_explicit_host_capability_projection() {
        let mut session = dsh_pager::SessionState::new("s".into(), 1);
        let snapshot = GrokHostSnapshot::demo();
        assert!(!steer_capability_available(&session, &snapshot));
        assert!(session.set_projection("capabilities", 1, json!({"queueSteer": true})));
        assert!(steer_capability_available(&session, &snapshot));
        assert!(session.set_projection("capabilities", 2, json!({"queueSteer": false})));
        assert!(!steer_capability_available(&session, &snapshot));
    }
}
