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

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
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

use crate::actions::{ActionId, ActionRegistry};
use crate::app::{AppShell, KeyOwner, Overlay, ShellAction, ShellEvent};
use crate::appearance::{GrokAppearanceSnapshot, LayoutConfig, ScrollbarConfig};
use crate::clipboard::{self, ClipboardBackend};
use crate::effects::{
    AsyncEffectExecutor, EffectLedger, OperationKey, UiContext, UiEffect, UiEffectCompletion,
    UiEffectStatus, UiIntent, compile_intent, receipt_status_message,
};
use crate::geometry::{
    GeometryLine, HitMap, HitTarget, LinkTarget, column_for_grapheme, first_link_target,
    insert_text_line,
};
use crate::host_adapter::{
    AgentSnapshot, FeatureStatus, FileSearchRow, FileSearchSnapshot, GrokHostSnapshot,
    MediaSnapshot, SuggestionSnapshot, resume_picker_entries, resume_picker_search_hits,
};
use crate::input::{PromptEditor, key::KeyShortcut, line_editor::LineEditOutcome};
use crate::media::{
    MediaPreviewBuffer, MediaPreviewController, MediaPreviewDecision, render_image_preview_content,
};
use crate::modal_window_state::ModalWindowState;
use crate::render::line_utils::truncate_str;
use crate::scheduler::SchedulerStats;
use crate::selection::SelectionModel;
use crate::theme::Theme;
use crate::views::{
    agent::{
        AgentView, AgentViewLayout, AgentViewLayoutParams, ScrollInfo, effective_compact,
        render_scrollbar,
    },
    agent_hints::{self, ActivePane, build_hints, prompt_focus_hint},
    agent_panes::{AgentPaneController, render_agent_tasks_content, render_inline_agent_panes},
    agent_status::AgentStatusBar,
    context_bar::context_bar_line,
    dashboard::{DashboardPeek, DashboardRenderState, render_dashboard_content},
    file_search::{controller::FileSearchController, line_viewer::render_file_search_content},
    interaction::{approval_outcome, permission_state, render_question_content, response_for},
    modal_window::{
        ModalSizing, ModalWindowConfig, ModalWindowOutcome, Shortcut, handle_modal_mouse,
        render_modal_window,
    },
    permission_view::{
        PermissionChoice, PermissionViewState, permission_view_height, render_permission_view,
    },
    prompt_contract::{
        PromptFlagContract, PromptGeometry, PromptInfoContract, PromptStyleContract,
    },
    prompt_widget::GrokPromptRenderer,
    queue::{QueueRenderState, moved_selection, render_queue_content},
    session_picker::{ResumePickerOutcome, ResumePickerState},
    shortcuts_bar::{HintItem, ShortcutsBar},
    status_bar::StatusBar,
    suggestion_controller::{SuggestionController, SuggestionOutcome},
    timeline::{RailViewport, compute_rail, render_rail},
    transcript::ScrollbackPane,
    turn_status::{MouseButtons as TurnStatusMouseButtons, TurnStatusArgs, render_turn_status},
    workspace::WorkspaceTreeController,
};
use serde_json::json;
use xai_ratatui_textarea::MouseAction;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const ANIMATION_POLL_INTERVAL: Duration = Duration::from_millis(33);
const NOTIFICATION_BUDGET: usize = 256;
const TRANSCRIPT_DOUBLE_CLICK: Duration = Duration::from_millis(450);

fn contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.right() && row >= rect.y && row < rect.bottom()
}

fn elapsed_since_epoch_ms(started_at_ms: Option<u64>) -> Option<Duration> {
    let started_at_ms = u128::from(started_at_ms?);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    let elapsed_ms = now_ms.checked_sub(started_at_ms)?;
    Some(Duration::from_millis(
        u64::try_from(elapsed_ms).unwrap_or(u64::MAX),
    ))
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

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
        if let Err(error) = ui.poll_effects(transport, session) {
            ui.status = Some(format!("effect completion error: {error}"));
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
        let poll_interval = if session.running() {
            ANIMATION_POLL_INTERVAL
        } else {
            POLL_INTERVAL
        };
        if !event::poll(poll_interval)? {
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
        let note = match transport.try_notification() {
            Ok(Some(note)) => note,
            Ok(None) => break,
            Err(error) => {
                let update = session.accept_stream_eof(error.to_string());
                combined.changed |= update.changed;
                combined.gap_detected |= update.gap_detected;
                break;
            }
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
    scrollback_pane: ScrollbackPane,
    resume_picker: ResumePickerState,
    modal: ModalWindowState,
    prompt: PromptEditor,
    prompt_renderer: GrokPromptRenderer,
    prompt_mode: Option<PromptMode>,
    queue_selected_id: Option<String>,
    queue_editing: bool,
    queue_editor: PromptEditor,
    queue_pending: Option<PendingQueueMutation>,
    interaction_editor: PromptEditor,
    interaction_selected: usize,
    interaction_request_id: Option<DshRequestId>,
    interaction_generation: Option<DshGeneration>,
    interaction_pending: Option<DshRequestId>,
    cancel_pending: Option<OperationKey>,
    interaction_args_expanded: bool,
    permission_area: Rect,
    permission_option_rows: Vec<Rect>,
    hovered_permission_item: Option<usize>,
    last_permission_click: Option<(Instant, usize)>,
    file_search_editor: PromptEditor,
    file_search: FileSearchController,
    suggestions: SuggestionController,
    image_selected: usize,
    media_preview: Option<MediaPreviewBuffer>,
    media_preview_controller: MediaPreviewController,
    agent_pane: AgentPaneController,
    agent_subagents: Vec<crate::host_adapter::SubagentRow>,
    dashboard: DashboardModel,
    workspace_tree: WorkspaceTreeController,
    dashboard_revision: Option<u64>,
    dashboard_query: PromptEditor,
    dashboard_query_active: bool,
    dashboard_peek: Option<DashboardPeek>,
    prompt_history_index: Option<usize>,
    local_prompt_history: Vec<String>,
    effect_ledger: EffectLedger,
    effect_executor: AsyncEffectExecutor,
    next_operation: u64,
    status: Option<String>,
    frame: usize,
    hit_map: HitMap,
    geometry_lines: Vec<GeometryLine>,
    selection: SelectionModel,
    /// Transcript target whose entry/block chrome is currently selected by a
    /// click.  The text selection model remains responsible for copy ranges;
    /// this stable target drives Grok's outer selection bracket, including for
    /// a single click with an empty text range.
    selected_transcript: Option<HitTarget>,
    last_transcript_click: Option<(Instant, dsh_pager::DshRenderEntryId, Option<usize>)>,
    hover_link: Option<LinkTarget>,
    context_hovered: bool,
    turn_stop_hovered: bool,
    frame_links: Vec<dsh_grok_inline::LinkSpan>,
    pending_copy: Option<String>,
    scheduler_stats: SchedulerStats,
}

impl UiState {
    fn submit_effect(
        &mut self,
        transport: &mut RpcTransport,
        intent: UiIntent,
        context: &UiContext,
    ) -> PagerResult<crate::effects::UiEffectReceipt> {
        let (executor, ledger) = (&mut self.effect_executor, &mut self.effect_ledger);
        executor.submit(transport, ledger, intent, context)
    }

    fn poll_effects(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> PagerResult<()> {
        let completions = {
            let (executor, ledger) = (&mut self.effect_executor, &mut self.effect_ledger);
            executor.poll(transport, ledger)?
        };
        for completion in completions {
            self.apply_effect_completion(completion, session);
        }
        Ok(())
    }

    fn apply_effect_completion(&mut self, completion: UiEffectCompletion, session: &SessionState) {
        let subject = match &completion.effect {
            UiEffect::CancelSession { .. } => "Cancel session",
            UiEffect::SubmitPrompt { .. } => "Prompt",
            UiEffect::ListSessions { .. } => "Session list",
            UiEffect::SearchSessions { .. } => "Session search",
            UiEffect::QueueMutation { .. } => "Queue update",
            UiEffect::RespondInteraction { .. } => "Interaction response",
            UiEffect::RenameSession { .. } => "Rename session",
            UiEffect::ForkSession { .. } => "Fork session",
            UiEffect::ArchiveSession { .. } | UiEffect::ArchiveSessionTarget { .. } => {
                "Archive session"
            }
            UiEffect::FileSearchQuery { .. } => "File search",
            UiEffect::PreviewMedia { .. } => "Image preview",
            UiEffect::ReorderSession { .. } => "Reorder session",
            UiEffect::InterruptSubagent { .. } => "Subagent interrupt",
            UiEffect::AttachSession { .. } => "Attach session",
        };
        if completion.receipt.operation.session_id.as_str() != session.session_id()
            || completion.receipt.operation.generation != DshGeneration::new(session.generation())
        {
            self.status = Some(format!(
                "Ignored stale {subject} completion for {}",
                completion.receipt.operation.request_id
            ));
            return;
        }
        if let UiEffect::ListSessions { revision, .. } = &completion.effect {
            let applied = if completion.receipt.status == UiEffectStatus::Accepted {
                if let Some(list) = completion.session_list.as_ref() {
                    self.resume_picker
                        .apply_entries(*revision, resume_picker_entries(list, session))
                } else {
                    self.resume_picker
                        .fail_entries(*revision, "session.list returned no value")
                }
            } else {
                self.resume_picker.fail_entries(
                    *revision,
                    completion
                        .receipt
                        .diagnostic
                        .clone()
                        .unwrap_or_else(|| "host rejected session.list".to_string()),
                )
            };
            if applied {
                self.status = completion
                    .receipt
                    .diagnostic
                    .as_ref()
                    .map(|_| receipt_status_message(&completion.receipt, subject));
            }
            return;
        }
        if let UiEffect::SearchSessions { revision, .. } = &completion.effect {
            let applied = if completion.receipt.status == UiEffectStatus::Accepted {
                if let Some(value) = completion.session_search.as_ref() {
                    self.resume_picker
                        .apply_search(*revision, resume_picker_search_hits(value))
                } else {
                    self.resume_picker
                        .fail_search(*revision, "session.search returned no value")
                }
            } else {
                self.resume_picker.fail_search(
                    *revision,
                    completion
                        .receipt
                        .diagnostic
                        .clone()
                        .unwrap_or_else(|| "host rejected session.search".to_string()),
                )
            };
            if applied && completion.receipt.status != UiEffectStatus::Accepted {
                self.status = Some(receipt_status_message(&completion.receipt, subject));
            }
            return;
        }
        if let UiEffect::FileSearchQuery { revision, .. } = &completion.effect {
            if *revision != self.file_search.revision() {
                self.status = Some(format!(
                    "Ignored stale File search completion for revision {revision}"
                ));
                return;
            }
            if let Some(value) = completion.file_references.clone() {
                let result = file_search_snapshot_from_effect(
                    self.file_search_editor.text(),
                    *revision,
                    value,
                );
                let _ = self.file_search.apply_result(result);
            }
        }
        if let UiEffect::PreviewMedia { attachment_id, .. } = &completion.effect
            && let Some(preview) = completion.attachment_preview.clone()
            && self.media_preview_controller.accepts(attachment_id)
            && preview.attachment_id == *attachment_id
        {
            self.media_preview = Some(MediaPreviewBuffer {
                attachment_id: preview.attachment_id,
                media_type: preview.media_type,
                data: preview.data,
                bytes: preview.bytes,
                width: preview.width,
                height: preview.height,
            });
        }
        if matches!(completion.effect, UiEffect::RespondInteraction { .. })
            && !matches!(
                completion.receipt.status,
                UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
            )
        {
            self.interaction_pending = None;
        }
        if matches!(completion.effect, UiEffect::CancelSession { .. }) {
            if matches!(
                completion.receipt.status,
                UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
            ) {
                self.status = Some("Cancellation accepted; waiting for host snapshot".into());
            } else {
                self.cancel_pending = None;
                self.turn_stop_hovered = false;
            }
        }
        if let UiEffect::SubmitPrompt { text, .. } = &completion.effect
            && completion.receipt.status == UiEffectStatus::Accepted
            && self.prompt.text() == text
        {
            let text = text.clone();
            self.record_prompt_history(&text);
            self.prompt.reset();
            self.suggestions.reset();
            self.status = Some(prompt_admission_message(&completion.receipt.status));
        }
        if completion.receipt.status != UiEffectStatus::Accepted
            || matches!(
                completion.effect,
                UiEffect::FileSearchQuery { .. } | UiEffect::PreviewMedia { .. }
            )
        {
            self.status = Some(receipt_status_message(&completion.receipt, subject));
        }
    }

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

        let mut snapshot =
            GrokHostSnapshot::from_session_with_control_plane(session, Some(control_plane));
        self.apply_file_search_result(&mut snapshot);
        self.sync_dashboard(control_plane);
        // Subagent catalog responses are host-authoritative but arrive through
        // the effect reducer after the base session projection. Fold the
        // latest stable-ID result into this frame's single render snapshot.
        snapshot.agent.subagents = self.agent_subagents.clone();
        self.agent_pane.sync(&snapshot.agent);
        self.reconcile_snapshot(&snapshot);
        let mode = self.prompt_mode.unwrap_or(snapshot.prompt.default_mode);
        let focused = self.shell.owner() == KeyOwner::Prompt;
        let compact = effective_compact(false, area.height);
        let appearance = GrokAppearanceSnapshot::for_area(area, compact);
        let layout_cfg = LayoutConfig::default();
        let scrollbar_cfg = ScrollbarConfig::default();
        let inner_width = AgentViewLayout::inner_width(area, &layout_cfg, compact);
        let prompt_outer_width = inner_width;
        let mut permission = snapshot.interaction.as_ref().and_then(|interaction| {
            permission_state(
                interaction,
                &snapshot.transcript,
                self.interaction_selected,
                self.interaction_pending.is_some(),
            )
        });
        if let Some(permission) = permission.as_mut() {
            permission.args_expanded = self.interaction_args_expanded;
        }
        let suggestion_rows = if permission.is_some() {
            0
        } else {
            self.suggestion_items(&snapshot)
                .map_or(0, |items| items.len().clamp(1, 3) as u16)
        };
        let prompt_style = PromptStyleContract {
            focused,
            compact,
            title: Some(snapshot.session_title.clone()),
            show_prefix: appearance.prompt_show_prefix,
            show_borders: appearance.prompt_show_borders,
            show_accent_line: appearance.prompt_show_accent_line,
            ..PromptStyleContract::default()
        };
        let prompt_cap = if compact { 6 } else { 8 };
        let prompt_geometry = PromptGeometry::compute(
            Rect::new(0, 0, prompt_outer_width, prompt_cap),
            &prompt_style,
            true,
            2,
        );
        let textarea_rows = self
            .prompt
            .desired_height(prompt_geometry.textarea.width.max(1));
        let prompt_info = PromptInfoContract {
            model_name: snapshot.model.clone(),
            flags: vec![PromptFlagContract {
                text: AgentView::mode_label(mode).into(),
                color: None,
                bold: false,
            }],
            multiline: textarea_rows > 1,
            ..PromptInfoContract::default()
        };
        let prompt_height = permission.as_ref().map_or_else(
            || {
                GrokPromptRenderer::desired_height(
                    self.prompt.textarea(),
                    prompt_outer_width,
                    &prompt_style,
                    Some(&prompt_info),
                    prompt_cap,
                )
            },
            |permission| {
                permission_view_height(
                    permission,
                    area.height,
                    prompt_outer_width.saturating_sub(5) as usize,
                )
            },
        );
        let tasks_height = if snapshot.tasks.is_empty() {
            0
        } else {
            (snapshot.tasks.len() as u16)
                .min(8)
                .min((area.height as f32 * 0.15).floor() as u16)
                .max(1)
        };
        let catalog_height = if snapshot.agent.subagents.is_empty() {
            0
        } else {
            (snapshot.agent.subagents.len() as u16)
                .min(8)
                .min((area.height as f32 * 0.15).floor() as u16)
                .max(1)
        };
        let queue_height = if snapshot.queue.is_empty() {
            0
        } else {
            (snapshot.queue.len() as u16).clamp(1, 3)
        };
        let timeline_width = crate::views::timeline::rail_width(
            appearance.show_timeline,
            false,
            area.width,
            snapshot.transcript.len(),
        );
        let short = area.height <= crate::views::agent::SHORT_TERMINAL_ROWS;
        let mut layout_params = AgentViewLayoutParams {
            area,
            layout_cfg,
            scrollbar_cfg,
            timeline_width,
            prompt_height,
            tasks_height,
            catalog_height,
            todo_height: 0,
            queue_height,
            btw_height: 0,
            turn_status_height: u16::from(snapshot.turn_status.visible),
            banner_height: suggestion_rows,
            cta_height: 0,
            follow_ups_height: 0,
            prompt_gap: u16::from(!compact && !short && prompt_height > 0),
            voice_recording_height: 0,
            shortcuts_height: 1,
            status_line_height: u16::from(
                (self.status.is_some() || snapshot.status.is_some()) && !short,
            ),
            compact,
        };
        layout_params.prompt_height = prompt_height.min(
            AgentViewLayout::rows_available_for_prompt(layout_params).max(if compact {
                3
            } else {
                2
            }),
        );
        let agent_layout = AgentView::layout(&mut self.shell, layout_params);
        let header = agent_layout.status_bar;
        let input = agent_layout.prompt;
        let compact_header = compact || header.width < 70;
        let status_bar = StatusBar::new(&snapshot.session_title);
        // The upstream status row is left/right aligned. Keep the compact
        // branding chip only where it cannot collide with the right status.
        if compact_header {
            frame.render_widget(status_bar.center("GROK UI"), header);
        } else {
            frame.render_widget(status_bar, header);
        }

        // Grok composes the right side as named status items so context usage
        // can retain a stable hover rectangle while its contents change.
        let mut agent_status = AgentStatusBar::new(theme);
        if let Some(context_line) = context_bar_line(
            snapshot.context_usage.used_tokens,
            snapshot.context_usage.total_tokens,
            self.context_hovered,
            theme,
        ) {
            agent_status.push("context", context_line);
        }
        let status_areas = agent_status.render(frame.buffer_mut(), header);
        if let Some(context_rect) = status_areas.get("context").copied() {
            self.hit_map.insert(crate::geometry::HitRegion {
                target: HitTarget::Overlay("context-usage".into()),
                rect: context_rect,
                label: "context usage".into(),
                link: None,
                priority: 20,
            });
        }

        self.render_transcript(
            frame,
            &agent_layout,
            &scrollbar_cfg,
            appearance.show_timestamps,
            &snapshot,
            &mut session.scrollback,
        );
        if let Some(permission) = permission.as_ref() {
            self.permission_area = input;
            let result = render_permission_view(
                frame.buffer_mut(),
                input,
                permission,
                self.hovered_permission_item,
                theme,
                self.shell.owner() == KeyOwner::Interaction,
            );
            self.permission_option_rows = result.option_rows;
        } else {
            self.permission_area = Rect::default();
            self.permission_option_rows.clear();
            self.hovered_permission_item = None;
            let prompt_result = self.prompt_renderer.draw(
                frame.buffer_mut(),
                input,
                self.prompt.textarea_mut(),
                &prompt_style,
                Some(&prompt_info),
                theme,
            );
            if let Some((x, y)) = prompt_result.cursor_pos {
                frame.set_cursor_position((x, y));
            }
        }
        self.render_suggestions(frame, agent_layout.banner, &snapshot);
        self.hit_map.insert(crate::geometry::HitRegion {
            target: if permission.is_some() {
                HitTarget::Overlay("permission".into())
            } else {
                HitTarget::Prompt
            },
            rect: input,
            label: if permission.is_some() {
                "permission".into()
            } else {
                "prompt".into()
            },
            link: None,
            priority: 15,
        });
        self.hit_map.insert(crate::geometry::HitRegion {
            target: HitTarget::Overlay("scrollback".into()),
            rect: agent_layout.scrollback_content,
            label: "scrollback".into(),
            link: None,
            priority: 1,
        });
        if agent_layout.timeline_width > 0 {
            self.hit_map.insert(crate::geometry::HitRegion {
                target: HitTarget::Overlay("timeline".into()),
                rect: Rect::new(
                    agent_layout.timeline_x,
                    agent_layout.scrollback.y,
                    agent_layout.timeline_width,
                    agent_layout.scrollback.height,
                ),
                label: "timeline".into(),
                link: None,
                priority: 2,
            });
        }
        for (rect, label) in [
            (agent_layout.tasks, "tasks"),
            (agent_layout.catalog, "catalog"),
            (agent_layout.todo, "todo"),
            (agent_layout.queue, "queue-pane"),
            (agent_layout.btw, "btw"),
            (agent_layout.turn_status, "turn-status"),
            (agent_layout.banner, "banner"),
            (agent_layout.plugin_cta, "plugin-cta"),
            (agent_layout.follow_ups, "follow-ups"),
            (agent_layout.voice_recording, "voice-recording"),
        ] {
            self.hit_map.insert(crate::geometry::HitRegion {
                target: HitTarget::Overlay(label.into()),
                rect,
                label: label.into(),
                link: None,
                priority: 5,
            });
        }
        let turn_status_output = render_turn_status(
            frame.buffer_mut(),
            agent_layout.turn_status,
            TurnStatusArgs {
                activity: &snapshot.turn_status.activity,
                turn_elapsed: elapsed_since_epoch_ms(snapshot.turn_status.turn_started_at_ms),
                activity_elapsed: elapsed_since_epoch_ms(
                    snapshot.turn_status.activity_started_at_ms,
                ),
                tick: self.frame as u64,
                pending_user_input: snapshot.turn_status.pending_user_input,
                buttons: self.capabilities.mouse.then_some(TurnStatusMouseButtons {
                    cancel_hovered: self.turn_stop_hovered,
                }),
                total_tokens: snapshot.turn_status.total_tokens,
                cancelling: self.cancel_pending.is_some() && snapshot.running,
            },
            theme,
        );
        if let Some(cancel_button) = turn_status_output.cancel_button {
            self.hit_map.insert(crate::geometry::HitRegion {
                target: HitTarget::Overlay("turn-stop".into()),
                rect: cancel_button,
                label: "stop current turn".into(),
                link: None,
                priority: 30,
            });
        } else {
            self.turn_stop_hovered = false;
        }
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
        if let Some(permission) = permission.as_ref() {
            self.render_permission_shortcuts(frame, agent_layout.shortcuts, permission, compact);
        } else {
            let (hints, help_hint) = self.pane_shortcut_hints(&snapshot, textarea_rows > 1);
            AgentView::render_shortcuts(frame, agent_layout.shortcuts, &hints, help_hint);
        }

        self.render_inline_panes(frame, &agent_layout, &snapshot, theme);

        if self.shell.overlay() == Overlay::Picker {
            self.render_resume_picker(frame, area, compact);
        } else if self.shell.overlay() == Overlay::Queue {
            self.render_queue(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::Interaction {
            self.render_interaction(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::FileSearch {
            self.render_file_search(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::ImagePreview {
            self.render_image_preview(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::AgentTasks {
            self.render_agent_tasks(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::Dashboard {
            self.render_dashboard(frame, area, &snapshot.workspace);
        }
        self.frame_links = self.hit_map.link_spans();
        self.frame = self.frame.wrapping_add(1);
    }

    fn render_inline_panes(
        &mut self,
        frame: &mut Frame<'_>,
        layout: &AgentViewLayout,
        snapshot: &GrokHostSnapshot,
        theme: &Theme,
    ) {
        let buf = frame.buffer_mut();
        render_inline_agent_panes(buf, layout.tasks, layout.catalog, &snapshot.agent, theme);
        if layout.queue.height > 0 {
            render_queue_content(
                buf,
                layout.queue,
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
            .unwrap_or("");
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
        if self.cancel_pending.as_ref().is_some_and(|pending| {
            pending.session_id != snapshot.session_header.id
                || pending.generation != snapshot.session_header.generation
        }) {
            self.cancel_pending = None;
            self.turn_stop_hovered = false;
        } else if !snapshot.running && self.cancel_pending.take().is_some() {
            self.turn_stop_hovered = false;
            self.status = Some("Turn cancelled; host snapshot converged".into());
        }

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
                self.interaction_args_expanded = false;
                self.hovered_permission_item = None;
                self.last_permission_click = None;
            }
            let approval = matches!(interaction, DshInteraction::Approval { .. });
            let target = if approval {
                Overlay::Permission
            } else {
                Overlay::Interaction
            };
            if self.shell.overlay() != target {
                if approval {
                    self.shell.open_permission();
                } else {
                    self.shell.open_interaction();
                }
            }
        } else if matches!(
            self.shell.overlay(),
            Overlay::Permission | Overlay::Interaction
        ) {
            self.shell.close_overlay();
            self.interaction_request_id = None;
            self.interaction_generation = None;
            self.interaction_pending = None;
            self.interaction_args_expanded = false;
            self.permission_area = Rect::default();
            self.permission_option_rows.clear();
            self.hovered_permission_item = None;
            self.last_permission_click = None;
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

    fn apply_file_search_result(&self, snapshot: &mut GrokHostSnapshot) {
        let Some(result) = self.file_search.snapshot() else {
            return;
        };
        if result.revision >= snapshot.file_search.revision
            && result.query == self.file_search_editor.text()
        {
            snapshot.file_search = result.clone();
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
        self.workspace_tree.sync(&self.dashboard);
        self.dashboard_revision = Some(revision);
    }

    fn render_dashboard(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &crate::host_adapter::WorkspaceSnapshot,
    ) {
        self.workspace_tree.sync(&self.dashboard);
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
                DashboardRenderState {
                    model: &self.dashboard,
                    peek: self.dashboard_peek.as_ref(),
                    query_active: self.dashboard_query_active,
                    query: self.dashboard_query.text(),
                    workspace,
                    workspace_tree: &self.workspace_tree,
                    theme,
                },
            );
        }
    }

    fn render_transcript(
        &mut self,
        frame: &mut Frame<'_>,
        layout: &AgentViewLayout,
        scrollbar_config: &ScrollbarConfig,
        show_timestamps: bool,
        snapshot: &GrokHostSnapshot,
        scrollback: &mut dsh_pager::scrollback::Scrollback,
    ) {
        let theme = Theme::current();
        let content = layout.scrollback_content;
        let rail = Rect::new(
            layout.timeline_x,
            layout.scrollback.y,
            layout.timeline_width,
            layout.scrollback.height,
        );
        let mut lines = Vec::new();
        let mut timestamp_overlays = Vec::new();
        let mut total_height = 0;
        let mut scroll_top = 0;
        let render_width = content.width.saturating_sub(1).max(1) as usize;
        self.scrollback_pane
            .sync_with_options(scrollback, render_width, *theme, show_timestamps);
        if self.scrollback_pane.is_empty() {
            self.scroll = 0;
            self.scroll_anchor = None;
            lines.push(Line::from(Span::styled(
                "  No transcript events yet. Type a prompt below.",
                Style::default().fg(theme.gray),
            )));
        } else {
            total_height = self.scrollback_pane.total_height(scrollback);
            let mut max_scroll = total_height.saturating_sub(content.height as usize);
            self.scroll = self.scroll.min(max_scroll);
            scroll_top = max_scroll.saturating_sub(self.scroll);
            if self.transcript_width != Some(content.width) {
                if let Some(anchor) = self.scroll_anchor.take()
                    && let Some(restored) =
                        self.scrollback_pane.scroll_for_anchor(scrollback, anchor)
                {
                    scroll_top = restored;
                }
                self.transcript_width = Some(content.width);
            }
            max_scroll = total_height.saturating_sub(content.height as usize);
            scroll_top = scroll_top.min(max_scroll);
            self.scroll = max_scroll.saturating_sub(scroll_top);
            for paint in self
                .scrollback_pane
                .visible_lines(scrollback, scroll_top, content.height)
            {
                let text = paint.copy_text.clone();
                let timestamp = paint.timestamp.clone();
                let has_timestamp = timestamp.is_some();
                while lines.len() < paint.screen_y as usize {
                    lines.push(Line::from(""));
                }
                lines.push(paint.line);
                if let Some(timestamp) = timestamp {
                    timestamp_overlays.push((paint.screen_y, timestamp, paint.background));
                }
                if !paint.selectable {
                    continue;
                }
                let line_x = content
                    .x
                    .saturating_add(1)
                    .saturating_add(paint.content_offset);
                let line_width = render_width
                    .saturating_sub(usize::from(paint.content_offset))
                    .saturating_sub(usize::from(has_timestamp) * 10);
                let target = paint.block_index.map_or_else(
                    || HitTarget::TranscriptEntry(paint.entry_id),
                    |block_index| HitTarget::TranscriptBlock {
                        entry_id: paint.entry_id,
                        block_index,
                    },
                );
                let geometry = insert_text_line(
                    &mut self.hit_map,
                    target,
                    paint.line_index,
                    line_x,
                    content.y.saturating_add(paint.screen_y),
                    line_width as u16,
                    &text,
                    first_link_target(&text),
                );
                self.geometry_lines.push(geometry);
            }
            self.scroll_anchor = self.scrollback_pane.anchor_at(scrollback, scroll_top);
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

        // Timestamp is a right-side overlay, matching Grok's renderer: it is
        // visible but not part of the selectable/copyable text geometry.
        if show_timestamps {
            let buffer = frame.buffer_mut();
            let timestamp_x = content.x.saturating_add(content.width.saturating_sub(10));
            for (screen_y, timestamp, background) in timestamp_overlays {
                let y = content.y.saturating_add(screen_y);
                let style = Style::default()
                    .fg(theme.gray)
                    .bg(background.unwrap_or(theme.bg_base));
                buffer.set_string(timestamp_x, y, format!("{:>10}", timestamp.short), style);
                self.hit_map.insert(crate::geometry::HitRegion {
                    target: HitTarget::Overlay("transcript-timestamp".into()),
                    rect: Rect::new(timestamp_x, y, 10, 1),
                    label: timestamp.hover,
                    link: None,
                    priority: 12,
                });
            }
        }

        if let Some(selection) = self.selection.selection() {
            let buffer = frame.buffer_mut();
            for line in &self.geometry_lines {
                let entry_id = match line.target {
                    HitTarget::TranscriptEntry(entry_id)
                    | HitTarget::TranscriptBlock { entry_id, .. } => entry_id,
                    _ => continue,
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

        // Upstream Grok keeps the selected entry/block visibly bracketed even
        // when a click has not produced a non-empty copy range yet.  Paint it
        // after the paragraph and text highlight so the left `┌│└` chrome is
        // not swallowed by the transcript's own accent/border cells.
        if let Some(target) = self.selected_transcript.as_ref() {
            render_transcript_selection_box(
                frame.buffer_mut(),
                content,
                theme,
                &self.geometry_lines,
                target,
            );
        }

        let turn_count = snapshot.transcript.len();
        let rail_geometry = if layout.timeline_width > 0 {
            compute_rail(
                rail,
                rail.x,
                turn_count,
                RailViewport {
                    active: turn_count.checked_sub(1),
                    up_target: turn_count.checked_sub(2),
                    down_target: None,
                    at_bottom: self.scroll == 0,
                },
            )
        } else {
            None
        };
        if let Some(rail_geometry) = rail_geometry {
            let buf = frame.buffer_mut();
            render_rail(buf, &rail_geometry, None, theme);
        } else {
            render_scrollbar(
                frame.buffer_mut(),
                layout.scrollback,
                layout.scrollbar_x,
                scrollbar_config,
                Some(ScrollInfo {
                    total_height,
                    viewport_height: content.height,
                    scroll_offset: scroll_top,
                }),
                self.scroll == 0,
                theme,
            );
        }
    }

    fn render_resume_picker(&mut self, frame: &mut Frame<'_>, area: Rect, compact: bool) {
        self.resume_picker.render(
            frame.buffer_mut(),
            area,
            Theme::current(),
            compact,
            self.frame as u64,
            now_epoch_ms(),
        );
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
        let Some(interaction @ DshInteraction::Question { .. }) = snapshot.interaction.as_ref()
        else {
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
            render_question_content(
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

    fn active_pane(&self) -> ActivePane {
        match self.shell.overlay() {
            Overlay::Queue => ActivePane::Queue,
            Overlay::AgentTasks => ActivePane::Tasks,
            _ => match self.shell.owner() {
                KeyOwner::Queue => ActivePane::Queue,
                KeyOwner::AgentTasks => ActivePane::Tasks,
                KeyOwner::Transcript => ActivePane::Scrollback,
                _ => ActivePane::Prompt,
            },
        }
    }

    /// Grok `AgentView::normal_pane_hints`: `build_hints` plus the queue toggle.
    fn pane_shortcut_hints(
        &self,
        snapshot: &GrokHostSnapshot,
        multiline_mode: bool,
    ) -> (
        Vec<crate::views::shortcuts_bar::HintItem>,
        Option<crate::views::shortcuts_bar::HintItem>,
    ) {
        let registry = ActionRegistry::defaults();
        let active_pane = self.active_pane();
        let prompt = agent_hints::PromptWidget::from_composer(
            self.prompt.text(),
            self.prompt.cursor(),
            self.suggestion_items(snapshot).is_some(),
        );
        debug_assert_eq!(
            prompt.can_send(),
            self.prompt.can_send(),
            "hint-seam can_send must match the composer predicate"
        );
        let mut hints = build_hints(
            active_pane,
            prompt_focus_hint(),
            &prompt,
            &registry,
            self.queue_editing,
            None,
            None,
            "expand thinking",
            false,
            false,
            None,
            false,
            false,
            false,
            multiline_mode,
            false,
            false,
            snapshot.running,
            false,
            !snapshot.queue.is_empty(),
            false,
            false,
            false,
            crate::terminal::terminal_context().shift_enter_unavailable(),
            None,
        );
        if (self.shell.overlay() == Overlay::Queue || !snapshot.queue.is_empty())
            && active_pane != ActivePane::Queue
            && !self.queue_editing
            && let Some(def) = registry.find(ActionId::ToggleQueue)
        {
            hints.push(def.hint());
        }
        let help_hint = registry.find(ActionId::ShortcutsHelp).map(|def| def.hint());
        (hints, help_hint)
    }

    fn render_permission_shortcuts(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        permission: &PermissionViewState,
        compact: bool,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let focused = self.shell.owner() == KeyOwner::Interaction;
        let hints = if focused {
            let mut hints = Vec::new();
            if !permission.pending {
                let last = char::from(b'0' + permission.options.len().clamp(1, 9) as u8);
                hints.push(HintItem::paired(
                    KeyShortcut::key(KeyCode::Char('1')),
                    KeyShortcut::key(KeyCode::Char(last)),
                    "select",
                ));
                if permission.options.len() > 1 {
                    hints.push(HintItem::new(KeyShortcut::key(KeyCode::Tab), "next option"));
                }
                if permission.has_collapsible_display(area.width.saturating_sub(5) as usize) {
                    hints.push(HintItem::new(
                        KeyShortcut::ctrl(KeyCode::Char('f')),
                        if permission.args_expanded {
                            "collapse"
                        } else {
                            "expand"
                        },
                    ));
                }
            }
            hints.push(HintItem::new(KeyShortcut::key(KeyCode::Esc), "scrollback"));
            hints
        } else {
            vec![
                HintItem::new(KeyShortcut::key(KeyCode::Enter), "permission"),
                HintItem::paired(
                    KeyShortcut::key(KeyCode::Up),
                    KeyShortcut::key(KeyCode::Down),
                    "scroll",
                ),
            ]
        };
        let widget = if compact {
            ShortcutsBar::new(&hints).compact(2, None)
        } else {
            ShortcutsBar::new(&hints)
        };
        frame.render_widget(widget, area);
    }

    fn render_file_search(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        snapshot: &GrokHostSnapshot,
    ) {
        let theme = Theme::current();
        let shortcuts = [
            Shortcut {
                label: "Enter preview",
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
            title: "File Search · DeepSeek host",
            tabs: None,
            shortcuts: &shortcuts,
            sizing: ModalSizing::large(),
            fold_info: None,
        };
        let buf = frame.buffer_mut();
        if let Some(content) = render_modal_window(buf, area, &mut self.modal, &config, theme) {
            render_file_search_content(
                buf,
                content.content,
                &snapshot.file_search,
                self.file_search_editor.text(),
                self.file_search.selected_id(),
                theme,
            );
        }
    }

    fn render_image_preview(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        snapshot: &GrokHostSnapshot,
    ) {
        let theme = Theme::current();
        let shortcuts = [Shortcut {
            label: "Enter load · Esc close",
            clickable: true,
            id: 1,
        }];
        let config = ModalWindowConfig {
            title: "Image Preview · host media",
            tabs: None,
            shortcuts: &shortcuts,
            sizing: ModalSizing::large(),
            fold_info: None,
        };
        let buf = frame.buffer_mut();
        if let Some(content) = render_modal_window(buf, area, &mut self.modal, &config, theme) {
            render_image_preview_content(
                buf,
                content.content,
                &snapshot.media,
                self.image_selected,
                self.media_preview.as_ref(),
                theme,
            );
        }
    }

    fn render_agent_tasks(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        snapshot: &GrokHostSnapshot,
    ) {
        let theme = Theme::current();
        let shortcuts = [Shortcut {
            label: "Esc close",
            clickable: true,
            id: 1,
        }];
        let config = ModalWindowConfig {
            title: "Agent Tasks · DeepSeek host",
            tabs: Some(&["tasks", "subagents"]),
            shortcuts: &shortcuts,
            sizing: ModalSizing::large(),
            fold_info: None,
        };
        let buf = frame.buffer_mut();
        if let Some(content) = render_modal_window(buf, area, &mut self.modal, &config, theme) {
            let mut agent = snapshot.agent.clone();
            agent.subagents = self.agent_subagents.clone();
            render_agent_tasks_content(buf, content.content, &agent, &self.agent_pane, theme);
        }
    }

    fn render_suggestions(
        &mut self,
        frame: &mut Frame<'_>,
        prompt_area: Rect,
        snapshot: &GrokHostSnapshot,
    ) {
        let Some(items) = self.suggestion_items(snapshot) else {
            return;
        };
        let height = prompt_area.height.min(items.len().min(3) as u16);
        if height == 0 {
            return;
        }
        let buf = frame.buffer_mut();
        for (index, item) in items.iter().take(height as usize).enumerate() {
            let selected = index
                == self
                    .suggestions
                    .selected()
                    .min(items.len().saturating_sub(1));
            let marker = if selected { "▸" } else { " " };
            let line = format!("{marker} {item}");
            let style = if selected {
                Style::default()
                    .fg(Theme::current().text_primary)
                    .bg(Theme::current().bg_visual)
            } else {
                Style::default()
                    .fg(Theme::current().gray)
                    .bg(Theme::current().bg_base)
            };
            buf.set_string(
                prompt_area.x,
                prompt_area.y + index as u16,
                truncate_str(&line, prompt_area.width as usize),
                style,
            );
        }
    }

    fn suggestion_items(&self, snapshot: &GrokHostSnapshot) -> Option<Vec<String>> {
        let merged = Self::merged_suggestion_snapshot(snapshot);
        self.suggestions
            .visible_items(&merged, self.prompt.text())
            .map(|items| items.into_iter().map(str::to_string).collect())
    }

    fn merged_suggestion_snapshot(snapshot: &GrokHostSnapshot) -> SuggestionSnapshot {
        let mut merged = SuggestionSnapshot {
            status: FeatureStatus::Available,
            active: true,
            selected: None,
            items: if snapshot.capabilities.prompt_suggestions
                && snapshot.suggestions.status == FeatureStatus::Available
                && snapshot.suggestions.active
            {
                snapshot.suggestions.items.clone()
            } else {
                Vec::new()
            },
        };
        crate::slash::merge_builtin_suggestions(&mut merged.items);
        merged
    }

    fn dispatch_event(
        &mut self,
        event: ShellEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<bool> {
        if let ShellEvent::Mouse(mouse) = &event {
            self.context_hovered = self.shell.overlay() == Overlay::None
                && self
                    .hit_map
                    .hit_test(mouse.column, mouse.row)
                    .is_some_and(|region| {
                        matches!(
                            &region.target,
                            HitTarget::Overlay(name) if name == "context-usage"
                        )
                    });
        }
        if self.shell.overlay() == Overlay::None
            && let ShellEvent::Mouse(mouse) = &event
        {
            let prompt_hit = self
                .hit_map
                .hit_test(mouse.column, mouse.row)
                .is_some_and(|region| region.target == HitTarget::Prompt);
            let action = if prompt_hit
                || matches!(
                    mouse.kind,
                    MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
                ) {
                self.prompt_renderer
                    .handle_mouse(self.prompt.textarea_mut(), *mouse)
            } else {
                MouseAction::Nothing
            };
            if prompt_hit || action != MouseAction::Nothing {
                if prompt_hit && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                    self.shell.focus_prompt();
                }
                if let Some(text) = self.prompt.textarea_mut().take_clipboard() {
                    self.pending_copy = Some(text);
                }
                self.status = None;
                return Ok(false);
            }
        }
        let prompt_empty = self.prompt.is_empty();
        let action = self.shell.dispatch(event, prompt_empty);
        match action {
            ShellAction::Quit => Ok(true),
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
            ShellAction::OpenFileSearch => {
                self.file_search_editor.reset();
                self.file_search.reset();
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                self.status = Some(match snapshot.file_search.status {
                    FeatureStatus::Available => "File search opened".into(),
                    FeatureStatus::Pending => "File search waiting for host result".into(),
                    FeatureStatus::Unsupported => "File search unavailable".into(),
                });
                Ok(false)
            }
            ShellAction::OpenImagePreview => {
                self.image_selected = 0;
                self.media_preview = None;
                self.media_preview_controller.clear();
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                self.status = Some(media_status_message(&snapshot.media));
                Ok(false)
            }
            ShellAction::OpenAgentTasks => {
                self.agent_pane.clear();
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                self.agent_subagents.clear();
                let mut subagent_diagnostic = None;
                if snapshot.capabilities.subagents {
                    match dsh_pager::list_subagents(transport, session.session_id()) {
                        Ok(catalog) => {
                            self.agent_subagents = catalog
                                .entries
                                .into_iter()
                                .map(|entry| crate::host_adapter::SubagentRow {
                                    id: entry.id,
                                    parent_id: session.session_id().to_string(),
                                    label: entry.label.unwrap_or_else(|| entry.kind.clone()),
                                    mode: entry.mode.map(|mode| format!("{mode:?}").to_lowercase()),
                                    status: entry.activity.or(entry.reason),
                                })
                                .collect();
                        }
                        Err(error) => {
                            subagent_diagnostic = Some(format!("Subagent list failed: {error}"));
                        }
                    }
                }
                let mut agent_snapshot = snapshot.agent.clone();
                agent_snapshot.subagents = self.agent_subagents.clone();
                self.agent_pane.sync(&agent_snapshot);
                self.status = Some(subagent_diagnostic.unwrap_or_else(|| {
                    agent_status_message_with_subagents(&snapshot.agent, self.agent_subagents.len())
                }));
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
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                if self.suggestion_items(&snapshot).is_some() {
                    self.suggestions.dismiss();
                    self.status = None;
                    return Ok(false);
                }
                self.prompt.reset();
                self.suggestions.reset();
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
                if !self.dispatch_local_slash_command(transport, session) {
                    self.submit_prompt(transport, session)?;
                }
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
                        self.suggestions.text_changed();
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
                    self.suggestions.text_changed();
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
                if self.shell.overlay() == Overlay::Interaction
                    && !text.is_empty()
                    && self.interaction_pending.is_none()
                {
                    let _ = self.interaction_editor.insert_paste(&text);
                }
                Ok(false)
            }
            ShellAction::FileSearchKey(key) => {
                self.handle_file_search_key(key, transport, session)?;
                Ok(false)
            }
            ShellAction::FileSearchMouse(mouse) => {
                self.handle_file_search_mouse(mouse);
                Ok(false)
            }
            ShellAction::FileSearchPaste(text) => {
                if !text.is_empty()
                    && !matches!(
                        self.file_search_editor.insert_paste(&text),
                        LineEditOutcome::Unhandled
                    )
                {
                    self.request_file_search(transport, session)?;
                }
                Ok(false)
            }
            ShellAction::ImagePreviewKey(key) => {
                self.handle_image_preview_key(key, transport, session)?;
                Ok(false)
            }
            ShellAction::ImagePreviewMouse(_) => Ok(false),
            ShellAction::AgentTasksKey(key) => {
                self.handle_agent_tasks_key(key, transport, session);
                Ok(false)
            }
            ShellAction::AgentTasksMouse(_) => Ok(false),
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
                if !self.handle_turn_status_mouse(mouse, transport, session)? {
                    self.handle_transcript_mouse(mouse);
                }
                Ok(false)
            }
            ShellAction::Resized(area) => {
                let _ = self.shell.layout(area);
                self.modal = ModalWindowState::default();
                self.hit_map.resize(area);
                self.selection.clear();
                self.selected_transcript = None;
                self.hover_link = None;
                self.frame_links.clear();
                self.geometry_lines.clear();
                self.last_transcript_click = None;
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
                } else if let Some(region) = self.hit_map.hit_test(mouse.column, mouse.row)
                    && matches!(region.target, HitTarget::Overlay(ref name) if name == "transcript-timestamp")
                {
                    self.status = Some(format!("Timestamp: {}", region.label));
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(region) = self.hit_map.hit_test(mouse.column, mouse.row) else {
                    self.selection.clear();
                    self.selected_transcript = None;
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
                    let (entry_id, block_index) = match &region.target {
                        HitTarget::TranscriptEntry(entry_id) => (*entry_id, None),
                        HitTarget::TranscriptBlock {
                            entry_id,
                            block_index,
                        } => (*entry_id, Some(*block_index)),
                        _ => return,
                    };
                    let now = Instant::now();
                    let double_click = self.last_transcript_click.is_some_and(
                        |(previous, previous_id, previous_block)| {
                            previous_id == entry_id
                                && previous_block == block_index
                                && now.duration_since(previous) <= TRANSCRIPT_DOUBLE_CLICK
                        },
                    );
                    if double_click {
                        self.last_transcript_click = None;
                        self.selection.clear();
                        self.selected_transcript = None;
                        if self
                            .scrollback_pane
                            .toggle_fold_or_group_at(entry_id, block_index)
                        {
                            // Rebuild the width-specific projection on the next
                            // frame and restore the anchor captured by the last
                            // paint instead of jumping to the transcript tail.
                            self.transcript_width = None;
                            self.status = Some(if block_index.is_some() {
                                "Toggled transcript block".into()
                            } else if self.scrollback_pane.is_group_header(entry_id) {
                                "Toggled transcript group".into()
                            } else {
                                "Toggled transcript fold".into()
                            });
                            return;
                        }
                    }
                    self.last_transcript_click = Some((now, entry_id, block_index));
                    self.selected_transcript = Some(region.target.clone());
                    self.selection.begin(point);
                    self.status = Some("Selecting transcript".into());
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // A drag is a selection gesture, never the first half of a
                // fold double-click.
                self.last_transcript_click = None;
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

    fn handle_turn_status_mouse(
        &mut self,
        mouse: MouseEvent,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> PagerResult<bool> {
        let over_stop = self.hit_map.hit_test(mouse.column, mouse.row).is_some_and(
            |region| matches!(&region.target, HitTarget::Overlay(name) if name == "turn-stop"),
        );
        if mouse.kind == MouseEventKind::Moved {
            self.turn_stop_hovered = over_stop;
            return Ok(false);
        }
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) || !over_stop {
            return Ok(false);
        }
        if !session.running() {
            self.status = Some("Turn already finished".into());
            return Ok(true);
        }
        if self.cancel_pending.is_some() {
            self.status = Some("Cancellation already pending".into());
            return Ok(true);
        }

        let request_id = DshRequestId::new(format!("cancel-{}", self.next_operation));
        self.next_operation = self.next_operation.saturating_add(1);
        let context = UiContext::for_operation(session, request_id);
        let receipt = self.submit_effect(transport, UiIntent::CancelSession, &context)?;
        if matches!(
            receipt.status,
            UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
        ) {
            self.cancel_pending = Some(receipt.operation.clone());
            self.status = Some("Cancelling turn; waiting for host snapshot".into());
        } else {
            self.status = Some(receipt_status_message(&receipt, "Cancel session"));
        }
        Ok(true)
    }

    fn handle_file_search_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        match key.code {
            KeyCode::Esc => {
                self.shell.close_overlay();
                self.file_search_editor.reset();
                self.file_search.reset();
                self.status = Some("File search closed".into());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_file_search_selection(-1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_file_search_selection(1);
            }
            KeyCode::Enter => {
                let mut snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                self.apply_file_search_result(&mut snapshot);
                if let Some(row) = snapshot
                    .file_search
                    .rows
                    .iter()
                    .find(|row| Some(row.id.as_str()) == self.file_search.selected_id())
                {
                    let mention = format_file_reference(&row.path);
                    self.prompt.insert_paste(&mention);
                    self.shell.close_overlay();
                    self.status = Some(format!("Added file reference {mention}"));
                } else {
                    self.status = Some(file_search_status_message(&snapshot.file_search));
                }
            }
            _ if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == KeyCode::Char('u') =>
            {
                self.file_search_editor.reset();
                self.request_file_search(transport, session)?;
            }
            _ => {
                if !matches!(
                    self.file_search_editor.handle_key(&key),
                    LineEditOutcome::Unhandled
                ) {
                    self.request_file_search(transport, session)?;
                }
            }
        }
        Ok(())
    }

    fn request_file_search(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> PagerResult<()> {
        let query = self.file_search_editor.text().to_string();
        let revision = self.file_search.begin_query(&query);
        let context = UiContext::for_operation(
            session,
            DshRequestId::new(format!("file-search-{revision}")),
        );
        let receipt = self.submit_effect(
            transport,
            UiIntent::FileSearchQuery { query, revision },
            &context,
        )?;
        self.status = Some(receipt_status_message(&receipt, "File search"));
        Ok(())
    }

    fn move_file_search_selection(&mut self, delta: isize) {
        self.file_search.move_selection(delta);
    }

    fn handle_file_search_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_file_search_selection(-1),
            MouseEventKind::ScrollDown => self.move_file_search_selection(1),
            _ => {}
        }
    }

    fn handle_image_preview_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> PagerResult<()> {
        if key.code == KeyCode::Esc {
            self.shell.close_overlay();
            self.status = Some("Image preview closed".into());
            return Ok(());
        }
        let snapshot = GrokHostSnapshot::from_session(session);
        let count = snapshot.media.rows.len();
        if count == 0 {
            return Ok(());
        }
        match key.code {
            KeyCode::Up => self.image_selected = self.image_selected.saturating_sub(1),
            KeyCode::Down => {
                self.image_selected = self.image_selected.saturating_add(1).min(count - 1)
            }
            KeyCode::Enter => {
                let row = &snapshot.media.rows[self.image_selected.min(count - 1)];
                if let Some(attachment_id) = row.attachment_id.as_deref() {
                    self.request_media_preview(transport, session, attachment_id)?;
                } else {
                    self.status = Some("Image preview unavailable: attachment id missing".into());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn request_media_preview(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        attachment_id: &str,
    ) -> PagerResult<()> {
        let snapshot = GrokHostSnapshot::from_session(session);
        let decision = self.media_preview_controller.begin(
            attachment_id,
            self.capabilities,
            snapshot.capabilities,
        );
        if let MediaPreviewDecision::Unsupported(reason) = decision {
            self.status = Some(format!("Image preview unavailable: {reason}"));
            return Ok(());
        }
        let request_id = DshRequestId::new(format!("media-preview:{attachment_id}"));
        let context = UiContext::for_operation(session, request_id);
        let receipt = self.submit_effect(
            transport,
            UiIntent::PreviewMedia {
                attachment_id: attachment_id.to_string(),
            },
            &context,
        )?;
        self.status = Some(receipt_status_message(&receipt, "Image preview"));
        Ok(())
    }

    fn handle_agent_tasks_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) {
        if key.code == KeyCode::Esc {
            self.shell.close_overlay();
            self.status = Some("Agent tasks closed".into());
            return;
        }
        let snapshot = GrokHostSnapshot::from_session(session);
        let mut agent = snapshot.agent.clone();
        agent.subagents = self.agent_subagents.clone();
        self.agent_pane.sync(&agent);
        match key.code {
            KeyCode::Up => self.agent_pane.move_selection(-1),
            KeyCode::Down => self.agent_pane.move_selection(1),
            KeyCode::Char('x') => {
                if let Some(row) = self.agent_pane.selected_subagent(&agent)
                    && row.mode.as_deref() == Some("continuable")
                {
                    let address = dsh_pager_protocol::SubagentAddress {
                        parent_session_id: row.parent_id.clone(),
                        child_session_id: row.id.clone(),
                        mode: dsh_pager_protocol::SubagentMode::Continuable,
                    };
                    let child_id = row.id.clone();
                    self.status = match self.submit_effect(
                        transport,
                        UiIntent::InterruptSubagent { address },
                        &UiContext::from_session(session),
                    ) {
                        Ok(receipt) => Some(receipt_status_message(
                            &receipt,
                            &format!("Interrupt {child_id}"),
                        )),
                        Err(error) => Some(format!("Interrupt unavailable: {error}")),
                    };
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

    fn dispatch_local_slash_command(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> bool {
        match crate::slash::dispatch(self.prompt.text()) {
            crate::slash::DispatchResult::NotLocal => false,
            crate::slash::DispatchResult::InvalidUsage(usage) => {
                self.status = Some(format!("Usage: {usage}"));
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::ShowSessionPicker) => {
                self.open_resume_picker(transport, session);
                self.prompt.reset();
                self.suggestions.reset();
                self.prompt_history_index = None;
                true
            }
        }
    }

    fn open_resume_picker(&mut self, transport: &mut RpcTransport, session: &SessionState) {
        let cwd = transport
            .control_plane()
            .snapshot(session.session_id())
            .and_then(|snapshot| snapshot.cwd.clone())
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "unknown".to_string());
        self.shell.open_picker();
        let revision = self.resume_picker.open(session.session_id(), cwd.as_str());
        let context = UiContext::from_session(session);
        match self.submit_effect(transport, UiIntent::ListSessions { revision }, &context) {
            Ok(receipt)
                if matches!(
                    receipt.status,
                    UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
                ) =>
            {
                self.status = None;
            }
            Ok(receipt) => {
                let message = receipt_status_message(&receipt, "Session list");
                self.resume_picker.fail_entries(revision, message.clone());
                self.status = Some(message);
            }
            Err(error) => {
                let message = format!("Session list failed: {error}");
                self.resume_picker.fail_entries(revision, message.clone());
                self.status = Some(message);
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
        let context = UiContext::from_session(session);
        let receipt = self.submit_effect(
            transport,
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
        let merged = Self::merged_suggestion_snapshot(snapshot);
        match self
            .suggestions
            .handle_key(key.code, &merged, self.prompt.text())
        {
            SuggestionOutcome::Accepted => {
                let accepted = self
                    .suggestions
                    .accepted_item(&merged, self.prompt.text())
                    .map(str::to_string);
                if let Some(accepted) = accepted {
                    let _ = self.prompt.replace_text(&accepted);
                    self.suggestions.dismiss();
                    self.status = None;
                }
                return true;
            }
            SuggestionOutcome::Handled | SuggestionOutcome::Dismissed => return true,
            SuggestionOutcome::Unhandled => {}
        }
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
                self.suggestions.reset();
                return true;
            }
            (None, 1) => return true,
            _ => return true,
        };
        self.prompt_history_index = Some(next);
        let _ = self.prompt.replace_text(&history[next]);
        self.suggestions.text_changed();
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
        self.last_transcript_click = None;
        self.selected_transcript = None;
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
        let workspace_actions_supported = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        )
        .workspace
        .actions_supported;
        match key.code {
            KeyCode::Esc => {
                self.shell.close_overlay();
                self.status = Some("Dashboard closed".into());
            }
            KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                if workspace_actions_supported {
                    self.reorder_dashboard_selection(transport, session, -1)?;
                } else {
                    self.status = Some("Workspace reorder unavailable".into());
                }
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
                if workspace_actions_supported {
                    self.reorder_dashboard_selection(transport, session, 1)?;
                } else {
                    self.status = Some("Workspace reorder unavailable".into());
                }
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
            KeyCode::Char('x') => {
                if workspace_actions_supported {
                    self.archive_dashboard_selection(transport, session)?;
                } else {
                    self.status = Some("Workspace archive unavailable".into());
                }
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

    fn archive_dashboard_selection(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> PagerResult<()> {
        let Some(row) = self.dashboard.selected() else {
            self.status = Some("No session selected".into());
            return Ok(());
        };
        let session_id = dsh_pager::DshSessionId::new(row.session_id.clone());
        let context = UiContext::from_session(session);
        let receipt = self.submit_effect(
            transport,
            UiIntent::ArchiveSessionTarget {
                session_id: session_id.clone(),
            },
            &context,
        )?;
        self.status = Some(receipt_status_message(&receipt, "Archive session"));
        Ok(())
    }

    fn reorder_dashboard_selection(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        delta: isize,
    ) -> PagerResult<()> {
        let Some(row) = self.dashboard.selected().cloned() else {
            self.status = Some("No session selected".into());
            return Ok(());
        };
        let Some(workspace_id) = row.workspace_id.clone() else {
            self.status = Some("Selected session has no workspace order".into());
            return Ok(());
        };
        let ids = self.dashboard.session_ids_in_workspace(&workspace_id);
        let Some(index) = ids.iter().position(|id| id == &row.session_id) else {
            self.status = Some("Selected session is not in workspace order".into());
            return Ok(());
        };
        let next = if delta < 0 {
            index.checked_sub(1)
        } else {
            (index + 1 < ids.len()).then_some(index + 1)
        };
        let Some(next) = next else {
            self.status = Some("Session is already at the workspace boundary".into());
            return Ok(());
        };
        let before_session_id = if delta < 0 {
            Some(ids[next].clone())
        } else {
            ids.get(next + 1).cloned()
        };
        let context = UiContext::for_operation(
            session,
            DshRequestId::new(format!(
                "reorder-session:{}:{}",
                workspace_id, row.session_id
            )),
        );
        let receipt = self.submit_effect(
            transport,
            UiIntent::ReorderSession {
                workspace_id,
                session_id: dsh_pager::DshSessionId::new(row.session_id),
                before_session_id,
            },
            &context,
        )?;
        self.status = Some(receipt_status_message(&receipt, "Reorder session"));
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
        let receipt = self.submit_effect(
            transport,
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
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        let Some(interaction) = snapshot.interaction.as_ref() else {
            self.shell.close_overlay();
            self.status = Some("Interaction is no longer pending".into());
            return Ok(());
        };
        if matches!(interaction, DshInteraction::Approval { .. }) {
            return self.handle_approval_key(key, transport, session, interaction, &snapshot);
        }
        if self.interaction_pending.is_some() {
            if key.code == KeyCode::Esc {
                self.shell.close_overlay();
                self.status = Some("Interaction response pending".into());
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Esc => {
                self.shell.close_overlay();
                self.status = Some("Interaction deferred".into());
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

    fn handle_approval_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        interaction: &DshInteraction,
        snapshot: &GrokHostSnapshot,
    ) -> PagerResult<()> {
        if key.code == KeyCode::Esc {
            self.shell.park_permission();
            self.status = Some(if self.interaction_pending.is_some() {
                "Approval response pending; focus moved to scrollback".into()
            } else {
                "Approval parked; press Enter or i to return".into()
            });
            return Ok(());
        }
        if self.interaction_pending.is_some() {
            return Ok(());
        }
        let Some(mut permission) = permission_state(
            interaction,
            &snapshot.transcript,
            self.interaction_selected,
            false,
        ) else {
            return Ok(());
        };
        permission.args_expanded = self.interaction_args_expanded;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
            if permission
                .has_collapsible_display(self.permission_area.width.saturating_sub(5) as usize)
            {
                self.interaction_args_expanded = !self.interaction_args_expanded;
            }
            return Ok(());
        }
        let count = permission.options.len();
        if count == 0 {
            return Ok(());
        }
        match key.code {
            KeyCode::Tab => {
                self.interaction_selected = (self.interaction_selected + 1) % count;
            }
            KeyCode::BackTab => {
                self.interaction_selected = if self.interaction_selected == 0 {
                    count - 1
                } else {
                    self.interaction_selected - 1
                };
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.interaction_selected = self.interaction_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.interaction_selected = self
                    .interaction_selected
                    .saturating_add(1)
                    .min(count.saturating_sub(1));
            }
            KeyCode::Char(digit @ '1'..='9') => {
                let index = digit.to_digit(10).unwrap_or(1) as usize - 1;
                if let Some(choice) = permission.options.get(index).map(|option| option.choice) {
                    self.interaction_selected = index;
                    self.submit_approval(
                        transport,
                        session,
                        interaction,
                        approval_outcome(choice),
                    )?;
                }
            }
            KeyCode::Enter => {
                permission.active_idx = self.interaction_selected;
                permission.clamp_selection();
                if let Some(choice) = permission.selected() {
                    self.submit_approval(
                        transport,
                        session,
                        interaction,
                        approval_outcome(choice),
                    )?;
                }
            }
            _ => {}
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
        if matches!(interaction, DshInteraction::Approval { .. }) {
            return self.handle_approval_mouse(mouse, transport, session, interaction);
        }
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

    fn handle_approval_mouse(
        &mut self,
        mouse: crossterm::event::MouseEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        interaction: &DshInteraction,
    ) -> PagerResult<()> {
        let item = self
            .permission_option_rows
            .iter()
            .position(|row| contains(*row, mouse.column, mouse.row));
        match mouse.kind {
            MouseEventKind::Moved => {
                self.hovered_permission_item = item;
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                self.handle_transcript_mouse(mouse);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(index) = item {
                    self.shell.focus_permission();
                    self.interaction_selected = index;
                    if self.interaction_pending.is_some() {
                        return Ok(());
                    }
                    let now = Instant::now();
                    let double_click =
                        self.last_permission_click
                            .is_some_and(|(previous, previous_index)| {
                                previous_index == index
                                    && now.duration_since(previous) <= TRANSCRIPT_DOUBLE_CLICK
                            });
                    if double_click {
                        self.last_permission_click = None;
                        let choice = if index == 0 {
                            PermissionChoice::AllowOnce
                        } else {
                            PermissionChoice::Reject
                        };
                        self.submit_approval(
                            transport,
                            session,
                            interaction,
                            approval_outcome(choice),
                        )?;
                    } else {
                        self.last_permission_click = Some((now, index));
                    }
                } else if contains(self.permission_area, mouse.column, mouse.row) {
                    self.shell.focus_permission();
                    self.last_permission_click = None;
                } else {
                    self.shell.park_permission();
                    self.last_permission_click = None;
                    self.handle_transcript_mouse(mouse);
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
        let receipt = self.submit_effect(
            transport,
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
        match self.resume_picker.handle_event(event) {
            ResumePickerOutcome::Closed => {
                self.resume_picker.close();
                self.shell.close_overlay();
                self.status = Some("Session picker closed".into());
                false
            }
            ResumePickerOutcome::Selected(target) => {
                let effect = compile_intent(
                    UiIntent::AttachSession {
                        session_id: dsh_pager::DshSessionId::new(target.clone()),
                    },
                    &UiContext::from_session(session),
                );
                let UiEffect::AttachSession { session_id, .. } = effect else {
                    self.status = Some("Unable to compile attach operation".into());
                    return false;
                };
                if session_id.as_str() == session.session_id() {
                    self.resume_picker.close();
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
                        self.resume_picker.close();
                        self.shell.close_overlay();
                        self.status = Some("Session attached".into());
                    }
                    Err(error) => {
                        self.status = Some(format!("Attach failed: {error}"));
                    }
                }
                false
            }
            ResumePickerOutcome::QueryChanged { query, revision } => {
                self.status = None;
                if !query.trim().is_empty() {
                    let context = UiContext::from_session(session);
                    match self.submit_effect(
                        transport,
                        UiIntent::SearchSessions { query, revision },
                        &context,
                    ) {
                        Ok(receipt)
                            if matches!(
                                receipt.status,
                                UiEffectStatus::Accepted
                                    | UiEffectStatus::Queued
                                    | UiEffectStatus::Pending
                            ) => {}
                        Ok(receipt) => {
                            let message = receipt_status_message(&receipt, "Session search");
                            self.resume_picker.fail_search(revision, message.clone());
                            self.status = Some(message);
                        }
                        Err(error) => {
                            let message = format!("Session search failed: {error}");
                            self.resume_picker.fail_search(revision, message.clone());
                            self.status = Some(message);
                        }
                    }
                }
                false
            }
            ResumePickerOutcome::Changed => {
                self.status = None;
                false
            }
            ResumePickerOutcome::Unchanged => false,
        }
    }
}

/// Paint the entry-level selection chrome used by Grok's scrollback.  The
/// border lives outside the text hit rectangles, so a single click can show a
/// visible `┌│└` bracket before a non-empty copy selection exists.
fn render_transcript_selection_box(
    buf: &mut ratatui::buffer::Buffer,
    content: Rect,
    theme: &Theme,
    lines: &[GeometryLine],
    target: &HitTarget,
) {
    let mut top = None;
    let mut bottom = None;
    for line in lines.iter().filter(|line| &line.target == target) {
        top = Some(top.map_or(line.rect.y, |value: u16| value.min(line.rect.y)));
        bottom = Some(bottom.map_or(line.rect.y, |value: u16| value.max(line.rect.y)));
    }
    let (Some(top), Some(bottom)) = (top, bottom) else {
        return;
    };
    let top = top.max(content.y);
    let bottom = bottom.min(content.bottom().saturating_sub(1));
    if top > bottom || content.width == 0 || content.height == 0 {
        return;
    }

    let left = content.x;
    let right = content.right().saturating_sub(1);
    let style = Style::default().fg(theme.selection_border);
    for y in top..=bottom {
        let edge = if (y == top && top == content.y)
            || (y == bottom && bottom.saturating_add(1) >= content.bottom())
        {
            '┆'
        } else {
            '│'
        };
        if let Some(cell) = buf.cell_mut((left, y)) {
            cell.set_char(edge).set_style(style);
        }
        if let Some(cell) = buf.cell_mut((right, y)) {
            cell.set_char(edge).set_style(style);
        }
    }
    if top > content.y {
        if let Some(cell) = buf.cell_mut((left, top - 1)) {
            cell.set_char('┌').set_style(style);
        }
        if let Some(cell) = buf.cell_mut((right, top - 1)) {
            cell.set_char('┐').set_style(style);
        }
    }
    let bottom_corner_y = bottom.saturating_add(1);
    if bottom_corner_y < content.bottom() {
        if let Some(cell) = buf.cell_mut((left, bottom_corner_y)) {
            cell.set_char('└').set_style(style);
        }
        if let Some(cell) = buf.cell_mut((right, bottom_corner_y)) {
            cell.set_char('┘').set_style(style);
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

fn file_search_status_message(snapshot: &FileSearchSnapshot) -> String {
    match snapshot.status {
        FeatureStatus::Available => {
            if snapshot.rows.is_empty() {
                "No file matches".into()
            } else {
                format!("{} file match(es)", snapshot.rows.len())
            }
        }
        FeatureStatus::Pending => "File search pending host result".into(),
        FeatureStatus::Unsupported => snapshot
            .diagnostic
            .clone()
            .unwrap_or_else(|| "File search unavailable".into()),
    }
}

fn file_search_snapshot_from_effect(
    query: &str,
    revision: u64,
    value: dsh_pager_protocol::FileReferencesListValue,
) -> FileSearchSnapshot {
    FileSearchSnapshot {
        status: FeatureStatus::Available,
        query: query.to_string(),
        revision,
        preview_status: FeatureStatus::Unsupported,
        selected_id: None,
        rows: value
            .items
            .into_iter()
            .map(|item| FileSearchRow {
                id: format!("{}:{}", item.kind, item.path),
                path: item.path,
                kind: Some(item.kind),
                preview: None,
            })
            .collect(),
        diagnostic: None,
    }
}

fn media_status_message(snapshot: &MediaSnapshot) -> String {
    match snapshot.status {
        FeatureStatus::Available => format!("{} media attachment(s)", snapshot.rows.len()),
        FeatureStatus::Pending => "Media preview pending host snapshot".into(),
        FeatureStatus::Unsupported => "Image preview unavailable".into(),
    }
}

fn format_file_reference(path: &str) -> String {
    if path
        .chars()
        .any(|character| character.is_whitespace() || character == '"')
    {
        format!(
            "@{}",
            serde_json::to_string(path).unwrap_or_else(|_| format!("\"{path}\""))
        )
    } else {
        format!("@{path}")
    }
}

fn agent_status_message_with_subagents(snapshot: &AgentSnapshot, subagents: usize) -> String {
    match snapshot.status {
        FeatureStatus::Available => format!(
            "{} task(s), {} subagent(s)",
            snapshot.tasks.len(),
            subagents
        ),
        FeatureStatus::Pending => "Agent task state pending host snapshot".into(),
        FeatureStatus::Unsupported => "Agent task state unavailable".into(),
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
    matches!(status, UiEffectStatus::Accepted | UiEffectStatus::Queued)
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
    use super::format_file_reference;
    use super::prompt_admission_message;
    use super::prompt_receipt_admitted;
    use super::render_file_search_content;
    use super::render_transcript_selection_box;
    use super::steer_capability_available;
    use super::{
        MediaPreviewBuffer, UiState, render_agent_tasks_content, render_image_preview_content,
    };
    use crate::effects::UiEffectStatus;
    use crate::geometry::{GeometryLine, HitMap, HitRegion, HitTarget};
    use crate::host_adapter::{
        AgentSnapshot, CapabilityMatrix, FeatureStatus, FileSearchSnapshot, GrokHostSnapshot,
        MediaSnapshot, SuggestionSnapshot,
    };
    use crate::modal_window_state::ModalWindowState;
    use crate::theme::Theme;
    use crate::views::agent_panes::AgentPaneController;
    use crate::views::modal_window::{
        ModalSizing, ModalWindowConfig, Shortcut, render_modal_window,
    };
    use crate::views::picker::{PickerState, render_picker_in_modal};
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use dsh_pager::{ControlPlaneStore, DshRenderEntryId, SessionState, scrollback::Scrollback};
    use dsh_pager_protocol::{
        HistoryEntry, JsonRpcNotification, SessionEvent, SessionHistoryValue,
    };
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};
    use serde_json::json;

    #[test]
    fn demo_snapshot_keeps_host_data_out_of_grok_views() {
        let snapshot = GrokHostSnapshot::demo();
        assert_eq!(snapshot.model, "deepseek-reasoner");
        assert_eq!(snapshot.picker_entries().len(), 3);
    }

    #[test]
    fn cancel_receipt_stays_pending_until_host_running_state_converges() {
        let mut session = SessionState::new("cancel-session".into(), 4);
        let host_status = |running| JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.host".into(),
            params: Some(json!({
                "type": "host/session-status",
                "sessionId": "cancel-session",
                "generation": 4,
                "running": running
            })),
        };
        session
            .accept_notification(host_status(true))
            .expect("running host state");
        let operation = crate::effects::OperationKey {
            session_id: dsh_pager::DshSessionId::new("cancel-session"),
            generation: dsh_pager::DshGeneration::new(4),
            request_id: dsh_pager::DshRequestId::new("cancel-1"),
            action: "cancel-session".into(),
            dedupe_key: "cancel-session:cancel-1".into(),
        };
        let mut ui = UiState {
            cancel_pending: Some(operation.clone()),
            ..UiState::default()
        };
        ui.apply_effect_completion(
            crate::effects::UiEffectCompletion {
                effect: crate::effects::UiEffect::CancelSession {
                    operation: operation.clone(),
                },
                receipt: crate::effects::UiEffectReceipt {
                    status: UiEffectStatus::Accepted,
                    operation: operation.clone(),
                    diagnostic: None,
                    retryable: Some(false),
                },
                session_list: None,
                session_search: None,
                file_references: None,
                attachment_preview: None,
            },
            &session,
        );
        assert_eq!(ui.cancel_pending.as_ref(), Some(&operation));
        ui.reconcile_snapshot(&GrokHostSnapshot::from_session(&session));
        assert!(ui.cancel_pending.is_some());

        session
            .accept_notification(host_status(false))
            .expect("idle host state");
        ui.reconcile_snapshot(&GrokHostSnapshot::from_session(&session));
        assert!(ui.cancel_pending.is_none());
        assert_eq!(
            ui.status.as_deref(),
            Some("Turn cancelled; host snapshot converged")
        );
    }

    fn session_with_long_messages(count: usize) -> SessionState {
        let events = (0..count)
            .map(|index| HistoryEntry {
                event: SessionEvent {
                    event_type: "assistant/message".into(),
                    seq: index as i64 + 1,
                    time: index as f64,
                    data: json!({
                        "message": {
                            "content": [{
                                "type": "text",
                                "text": (0..240)
                                    .map(|line| format!("message {index} line {line}"))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            }]
                        }
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            })
            .collect();
        let mut session = SessionState::new("context-session".into(), 1);
        session
            .install_initial(SessionHistoryValue {
                events,
                has_more: false,
                projections: None,
            })
            .expect("valid session fixture");
        session
    }

    fn buffer_text(buffer: &Buffer, width: u16, height: u16) -> String {
        (0..height)
            .flat_map(|row| (0..width).map(move |column| buffer[(column, row)].symbol()))
            .collect()
    }

    fn buffer_row_text(buffer: &Buffer, width: u16, row: u16) -> String {
        (0..width)
            .map(|column| buffer[(column, row)].symbol())
            .collect()
    }

    #[test]
    fn production_render_shows_context_and_smooth_scrollbar() {
        let mut session = session_with_long_messages(1);
        assert!(session.set_projection(
            "contextPressure",
            2,
            json!({
                "contextWindow": 128_000,
                "pressureTokens": 40_000,
                "projectedTokens": 42_000
            })
        ));
        let control_plane = ControlPlaneStore::default();
        let mut ui = UiState::default();
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("test terminal");
        terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render frame");
        let screen = buffer_text(terminal.backend().buffer(), 100, 30);
        let transcript_height = ui.scrollback_pane.total_height(&mut session.scrollback);
        assert!(screen.contains("42K / 128K"));
        assert!(
            screen.contains('█'),
            "overflow renders Grok's full-block thumb (height={transcript_height})"
        );
        let context_rect = ui
            .hit_map
            .regions()
            .iter()
            .find_map(|region| {
                matches!(&region.target, HitTarget::Overlay(name) if name == "context-usage")
                    .then_some(region.rect)
            })
            .expect("context hit area");
        let header_text = buffer_row_text(terminal.backend().buffer(), 100, context_rect.y);
        assert!(!header_text.contains("connected"));
        assert!(!header_text.contains("deepseek"));
        assert!(!header_text.contains(" q0 "));
        assert!(!header_text.contains("idle"));

        ui.context_hovered = true;
        terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("hover render frame");
        let hovered = buffer_text(terminal.backend().buffer(), 100, 30);
        assert!(hovered.contains("32.8%"));
    }

    #[test]
    fn production_render_keeps_scrollbar_when_default_timeline_is_off() {
        let mut session = session_with_long_messages(2);
        let control_plane = ControlPlaneStore::default();
        let mut ui = UiState::default();
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("test terminal");
        terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render frame");
        let screen = buffer_text(terminal.backend().buffer(), 100, 30);
        assert!(
            screen.contains('█'),
            "multi-turn overflow keeps the scrollbar"
        );
        assert!(!ui.hit_map.regions().iter().any(|region| {
            matches!(&region.target, HitTarget::Overlay(name) if name == "timeline")
        }));
    }

    #[test]
    fn approval_replaces_composer_with_call_linked_grok_permission_card() {
        let mut session = SessionState::new("approval-session".into(), 3);
        session
            .install_initial(SessionHistoryValue {
                events: vec![HistoryEntry {
                    event: SessionEvent {
                        event_type: "tool/call".into(),
                        seq: 1,
                        time: 1.0,
                        data: json!({
                            "name": "bash",
                            "callId": "call-approval",
                            "arguments": "{\"command\":\"find /work -maxdepth 3\"}"
                        }),
                        source_event_seqs: None,
                        surface_op: None,
                        ignorable: None,
                    },
                    view: Some(json!({
                        "for": "call",
                        "view": {
                            "card": "terminal",
                            "title": "find /work -maxdepth 3",
                            "description": "List project files",
                            "cwd": "/work"
                        }
                    })),
                }],
                has_more: false,
                projections: None,
            })
            .expect("tool fixture");
        session
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "approval/requested",
                    "sessionId": "approval-session",
                    "requestId": "rpc-approval",
                    "approvalId": "approval-1",
                    "callId": "call-approval",
                    "toolName": "bash",
                    "reason": "sandbox escalation"
                })),
            })
            .expect("approval fixture");
        session
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.host".into(),
                params: Some(json!({
                    "type": "host/session-status",
                    "sessionId": "approval-session",
                    "generation": 3,
                    "running": true
                })),
            })
            .expect("running fixture");

        let control_plane = ControlPlaneStore::default();
        let mut ui = UiState::default();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render approval");
        let screen = buffer_text(terminal.backend().buffer(), 80, 24);

        assert_eq!(ui.shell.overlay(), crate::app::Overlay::Permission);
        assert_eq!(ui.shell.owner(), crate::app::KeyOwner::Interaction);
        assert_eq!(ui.permission_option_rows.len(), 2);
        assert!(screen.contains("List project files"));
        assert!(screen.contains("find /work -maxdepth 3"));
        assert!(screen.contains("1 (●) Yes, proceed"));
        assert!(screen.contains("2 (○) No, reject"));
        assert!(screen.contains('◆'));
        assert!(screen.contains("List project files…"));
        assert!(ui.hit_map.regions().iter().any(|region| {
            matches!(&region.target, HitTarget::Overlay(name) if name == "turn-stop")
        }));
        assert!(!screen.contains("Interaction · host request"));
    }

    #[test]
    fn transcript_selection_box_wraps_a_clicked_block_with_grok_corners() {
        let target = HitTarget::TranscriptBlock {
            entry_id: dsh_pager::DshRenderEntryId::Event { seq: 4 },
            block_index: 2,
        };
        let lines = vec![
            GeometryLine {
                target: target.clone(),
                line_index: 0,
                text: "final answer".into(),
                rect: Rect::new(4, 2, 12, 1),
            },
            GeometryLine {
                target: target.clone(),
                line_index: 1,
                text: "second line".into(),
                rect: Rect::new(4, 3, 12, 1),
            },
        ];
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 8));
        render_transcript_selection_box(
            &mut buffer,
            Rect::new(0, 0, 20, 8),
            Theme::current(),
            &lines,
            &target,
        );
        assert_eq!(buffer.cell((0, 1)).expect("top corner").symbol(), "┌");
        assert_eq!(buffer.cell((0, 2)).expect("left edge").symbol(), "│");
        assert_eq!(buffer.cell((0, 4)).expect("bottom corner").symbol(), "└");
        assert_eq!(buffer.cell((19, 1)).expect("top corner").symbol(), "┐");
        assert_eq!(buffer.cell((19, 4)).expect("bottom corner").symbol(), "┘");
    }

    #[test]
    fn transcript_execute_double_click_uses_grok_component_for_command_and_output() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&HistoryEntry {
            event: SessionEvent {
                event_type: "tool/call".into(),
                seq: 70,
                time: 1.0,
                data: json!({
                    "name": "bash",
                    "callId": "call-70",
                    "arguments": "{\"command\":\"pwd\"}"
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: Some(json!({
                "for": "call",
                "view": {
                    "card": "terminal",
                    "title": "pwd",
                    "description": "Query the current workspace",
                    "cwd": "/work"
                }
            })),
        });
        scrollback.apply_event(&HistoryEntry {
            event: SessionEvent {
                event_type: "tool/result".into(),
                seq: 71,
                time: 2.0,
                data: json!({
                    "message": {
                        "source": { "callId": "call-70" },
                        "content": [{ "type": "text", "text": "/work" }]
                    }
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: Some(json!({
                "for": "result",
                "view": {
                    "card": "terminal",
                    "output": "/work\n",
                    "exitCode": 0
                }
            })),
        });

        let id = DshRenderEntryId::Event { seq: 70 };
        let mut ui = UiState {
            hit_map: HitMap::new(Rect::new(0, 0, 80, 8)),
            ..UiState::default()
        };
        ui.scrollback_pane
            .sync(&mut scrollback, 80, *Theme::current());
        let theme = *Theme::current();
        let collapsed = ui.scrollback_pane.visible_lines(&mut scrollback, 0, 20);
        let summary = collapsed
            .iter()
            .find(|line| line.entry_id == id && line.selectable)
            .expect("collapsed completed execute summary");
        assert_eq!(summary.copy_text, "› Run Query the current workspace");
        assert!(summary.line.to_string().starts_with("❙  › Run "));
        assert!(summary.line.spans.iter().any(|span| {
            span.content == "❙  " && span.style.fg == Some(theme.accent_success)
        }));
        assert!(
            summary.line.spans.iter().any(|span| {
                span.content == "› " && span.style.fg == Some(theme.accent_success)
            })
        );
        let target = HitTarget::TranscriptEntry(id);
        let rect = Rect::new(3, 2, 60, 1);
        ui.hit_map.insert(HitRegion {
            target: target.clone(),
            rect,
            label: "› Run Query the current workspace".into(),
            link: None,
            priority: 10,
        });
        ui.geometry_lines.push(GeometryLine {
            target,
            line_index: 0,
            text: "› Run Query the current workspace".into(),
            rect,
        });
        let click = || MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 8,
            row: 2,
            modifiers: KeyModifiers::NONE,
        };

        ui.handle_transcript_mouse(click());
        assert_eq!(ui.status.as_deref(), Some("Selecting transcript"));
        ui.handle_transcript_mouse(click());
        assert_eq!(ui.status.as_deref(), Some("Toggled transcript fold"));
        assert!(ui.last_transcript_click.is_none());
        assert!(ui.selected_transcript.is_none());
        assert_eq!(ui.transcript_width, None);

        ui.scrollback_pane
            .sync(&mut scrollback, 80, *Theme::current());
        let expanded = ui
            .scrollback_pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .map(|line| line.copy_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(expanded.contains("⌄ Run Query the current workspace"));
        assert!(expanded.contains("/work"));
        assert!(expanded.contains("$ pwd"));
        assert!(!expanded.contains("exit 0"));
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
        assert!(!prompt_receipt_admitted(&UiEffectStatus::Pending));
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

    #[test]
    fn file_search_surface_keeps_pending_and_unsupported_distinct() {
        let theme = Theme::current();
        for (status, expected) in [
            (
                FeatureStatus::Pending,
                "Waiting for authoritative filesystem results",
            ),
            (
                FeatureStatus::Unsupported,
                "Filesystem search is unavailable",
            ),
        ] {
            let mut buffer = Buffer::empty(Rect::new(0, 0, 72, 12));
            render_file_search_content(
                &mut buffer,
                Rect::new(2, 1, 68, 10),
                &FileSearchSnapshot {
                    status,
                    diagnostic: (status == FeatureStatus::Unsupported)
                        .then(|| "Filesystem search is unavailable".into()),
                    ..Default::default()
                },
                "src",
                None,
                theme,
            );
            let rendered = buffer
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(
                rendered.contains(expected),
                "missing {expected}: {rendered}"
            );
        }
    }

    #[test]
    fn suggestion_controller_filters_selects_and_accepts_authoritative_items() {
        let mut ui = UiState::default();
        let mut snapshot = GrokHostSnapshot::demo();
        snapshot.capabilities = CapabilityMatrix {
            prompt_suggestions: true,
            ..CapabilityMatrix::default()
        };
        snapshot.suggestions = SuggestionSnapshot {
            status: FeatureStatus::Available,
            active: true,
            selected: None,
            items: vec!["/help".into(), "/history".into(), "/model".into()],
        };
        let _ = ui.prompt.replace_text("/h");
        assert_eq!(
            ui.suggestion_items(&snapshot),
            Some(vec!["/help".to_string(), "/history".to_string()])
        );
        let down =
            crossterm::event::KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE);
        assert!(ui.handle_prompt_command(&down, &snapshot));
        let tab =
            crossterm::event::KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::NONE);
        assert!(ui.handle_prompt_command(&tab, &snapshot));
        assert_eq!(ui.prompt.text(), "/history");
        ui.suggestions.text_changed();
        let mut unsupported = snapshot.clone();
        unsupported.capabilities.prompt_suggestions = false;
        assert!(ui.suggestion_items(&unsupported).is_none());
        let _ = ui.prompt.replace_text("/r");
        ui.suggestions.text_changed();
        assert_eq!(
            ui.suggestion_items(&unsupported),
            Some(vec!["/resume".to_string()])
        );
    }

    #[test]
    fn media_and_agent_surfaces_keep_fallback_states_explicit() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 12));
        render_image_preview_content(
            &mut buffer,
            Rect::new(1, 1, 78, 10),
            &MediaSnapshot {
                status: FeatureStatus::Unsupported,
                rows: Vec::new(),
            },
            0,
            None,
            Theme::current(),
        );
        let image_text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(image_text.contains("Image preview unavailable"));

        let mut task_buffer = Buffer::empty(Rect::new(0, 0, 80, 12));
        render_agent_tasks_content(
            &mut task_buffer,
            Rect::new(1, 1, 78, 10),
            &AgentSnapshot {
                status: FeatureStatus::Pending,
                ..Default::default()
            },
            &AgentPaneController::default(),
            Theme::current(),
        );
        let task_text = task_buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(task_text.contains("Waiting for authoritative agent task snapshot"));
    }

    #[test]
    fn media_preview_buffer_is_rendered_only_for_matching_attachment() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 12));
        let snapshot = MediaSnapshot {
            status: FeatureStatus::Available,
            rows: vec![crate::host_adapter::MediaRow {
                id: "row:1".into(),
                attachment_id: Some("img-1".into()),
                media_type: Some("image/png".into()),
                name: Some("plot".into()),
            }],
        };
        let preview = MediaPreviewBuffer {
            attachment_id: "img-1".into(),
            media_type: "image/png".into(),
            data: "aGVsbG8=".into(),
            bytes: Some(5),
            width: Some(4),
            height: Some(3),
        };
        render_image_preview_content(
            &mut buffer,
            Rect::new(1, 1, 78, 10),
            &snapshot,
            0,
            Some(&preview),
            Theme::current(),
        );
        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Loaded image/png"));
        assert!(text.contains("5 bytes"));
        assert!(text.contains("8 base64 chars"));
    }

    #[test]
    fn file_reference_mentions_quote_paths_with_spaces() {
        assert_eq!(format_file_reference("src/main.rs"), "@src/main.rs");
        assert_eq!(
            format_file_reference("docs/my file.md"),
            "@\"docs/my file.md\""
        );
    }
}
