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

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use dsh_pager::dashboard::DashboardModel;
use dsh_pager::{
    DshGeneration, DshInteraction, DshQueueItemId, DshRenderKind, DshRequestId, DshSeq, PagerError,
    PagerResult, RpcTransport, SessionState, SessionUpdate, create_blank_session, load_session_id,
    peek_session_tail, repair_tail,
};
use dsh_pager_protocol::{
    CommandDescriptor, CommandResultKind, PromptMode, QueueAction, TuiInteractionResponse,
};
use dsh_pager_render::{TerminalCapabilities, TerminalSurface};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders},
};

use crate::actions::{ActionId, ActionRegistry};
use crate::agent_preset::current_agent_preset_label;
use crate::app::{AppShell, HomeKeyState, KeyOwner, Overlay, ShellAction, ShellEvent};
use crate::appearance::{GrokAppearanceSnapshot, LayoutConfig, ScrollbarConfig};
use crate::clipboard;
use crate::diag;
use crate::effects::{
    AsyncEffectExecutor, EffectLedger, OperationKey, SensitiveString, UiContext, UiEffect,
    UiEffectCompletion, UiEffectStatus, UiIntent, compile_intent, receipt_status_message,
};
use crate::esc::PendingEscAction;
use crate::geometry::{
    GeometryLine, HitMap, HitTarget, LinkTarget, column_for_grapheme, first_link_target,
    insert_text_line,
};
use crate::host_adapter::{
    AgentSnapshot, ChildTranscriptView, FeatureStatus, FileSearchRow, FileSearchSnapshot,
    GrokHostSnapshot, MediaSnapshot, TurnActivitySnapshot, child_scrollback_from_history,
    media_snapshot_from_scrollback, resume_picker_entries, resume_picker_search_hits,
};
use crate::input::{
    PromptEditor,
    key::KeyShortcut,
    line_editor::LineEditOutcome,
    mouse::{MouseScrollState, ScrollConfig as MouseScrollConfig, ScrollDirection},
};
use crate::media::{
    MediaPreviewBuffer, MediaPreviewController, MediaPreviewDecision, render_image_preview_content,
};
use crate::modal_window_state::ModalWindowState;
use crate::model_state::{ModelId, ModelState, compact_model_effort_label};
use crate::render::line_utils::truncate_str;
use crate::scheduler::SchedulerStats;
use crate::scrollback_adapter::{
    host_pane::DshScrollbackHost, state::GrokScrollbackState, tick::animation_tick,
};
use crate::selection::SelectionModel;
use crate::session_controls::{DEFAULT_PERMISSION_PRESET, YOLO_PRESET};
use crate::slash::SlashController;
use crate::theme::Theme;
use crate::views::{
    agent::{
        AgentView, AgentViewLayout, AgentViewLayoutParams, ScrollInfo, dropdown_items_width,
        effective_compact, render_dropdown_chrome, render_scrollbar,
    },
    agent_hints::{self, ActivePane, build_hints, prompt_focus_hint},
    agent_panes::{
        AgentItemId, AgentPaneController, inline_agent_pane_height, render_agent_detail_chrome,
        render_agent_tasks_content, render_inline_agent_panes, render_watcher_cue, watcher_label,
    },
    agent_status::AgentStatusBar,
    context_bar::context_bar_line,
    dashboard::{DashboardPeek, DashboardRenderState, render_dashboard_content},
    file_search::{controller::FileSearchController, line_viewer::render_file_search_content},
    interaction::{
        QuestionAnswerDraft, approval_outcome, permission_state, question_state, response_for,
    },
    login::{DEEPSEEK_LOGIN_PROVIDER, LoginModalState, LoginOutcome},
    modal_window::{
        ModalSizing, ModalWindowConfig, ModalWindowOutcome, Shortcut, handle_modal_mouse,
        render_modal_window,
    },
    model_picker::{ModelPickerOutcome, ModelPickerState},
    permission_view::{PermissionChoice, permission_view_height, render_permission_view},
    preset_picker::{PresetPickerOutcome, PresetPickerState},
    prompt_contract::{
        PromptFlagContract, PromptGeometry, PromptInfoContract, PromptStyleContract,
    },
    prompt_widget::GrokPromptRenderer,
    question_view::{question_view_height, render_question_view},
    queue::{
        QueueRenderState, moved_selection, queue_item_is_visible, render_queue_content,
        visible_queue_items, visible_queue_len,
    },
    rewind::{
        RewindInput, RewindPhase, RewindPoint, RewindState, confirm_cursor, handle_rewind_key,
        move_cursor, render_rewind_overlay, rewind_activate, rewind_overlay_height, rewind_row_at,
        set_rewind_cursor,
    },
    session_picker::{ResumePickerOutcome, ResumePickerState},
    shortcuts_bar::{HintItem, PendingHint, ShortcutsBar},
    slash_dropdown::{desired_item_rows, render_dropdown},
    status_bar::StatusBar,
    timeline::{RailViewport, compute_rail, render_rail},
    turn_status::{MouseButtons as TurnStatusMouseButtons, TurnStatusArgs, render_turn_status},
    welcome::{WelcomeAnimation, format_cwd, render_welcome},
    workspace::WorkspaceTreeController,
};
use serde_json::json;
use xai_ratatui_textarea::MouseAction;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const ANIMATION_POLL_INTERVAL: Duration = Duration::from_millis(33);
// Mirrors Grok's ACP_DRAIN_BATCH_MAX: token firehoses stay batched, but
// queued terminal input waits for at most one small notification batch.
const NOTIFICATION_BUDGET: usize = 32;
const INPUT_DRAIN_BATCH_MAX: usize = 256;
const TRANSCRIPT_DOUBLE_CLICK: Duration = Duration::from_millis(450);
const CHILD_HISTORY_REFRESH: Duration = Duration::from_millis(400);

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

fn register_model_label_click(last_click: &mut Option<Instant>, now: Instant) -> bool {
    let double_click =
        last_click.is_some_and(|previous| now.duration_since(previous) <= TRANSCRIPT_DOUBLE_CLICK);
    *last_click = (!double_click).then_some(now);
    double_click
}

fn agent_overlay_key_closes(key: crossterm::event::KeyEvent) -> bool {
    if key.code == KeyCode::Esc {
        return true;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('q' | 'Q') if !ctrl => true,
        KeyCode::Char('g' | 'G' | 't' | 'T') if ctrl => true,
        _ => false,
    }
}

fn agent_overlay_close_click(modal: &ModalWindowState, mouse: &MouseEvent) -> bool {
    let is_press = matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
    );
    if !is_press {
        return false;
    }
    let in_rect = |rect: Rect| {
        mouse.column >= rect.x
            && mouse.column < rect.x.saturating_add(rect.width)
            && mouse.row >= rect.y
            && mouse.row < rect.y.saturating_add(rect.height)
    };
    if modal.close_button_rect.is_some_and(in_rect) {
        return true;
    }
    let Some(popup) = modal.popup_area else {
        return false;
    };
    let corner_width = 12.min(popup.width);
    let corner = Rect::new(
        popup
            .x
            .saturating_add(popup.width.saturating_sub(corner_width)),
        popup.y,
        corner_width,
        1,
    );
    if in_rect(corner) {
        return true;
    }
    matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) && !in_rect(popup)
}

fn paint_child_scrollback(
    pane: &mut DshScrollbackHost,
    state: &mut GrokScrollbackState,
    wave_started_at: &mut Option<Instant>,
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    scrollback: &mut dsh_pager::scrollback::Scrollback,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let now = Instant::now();
    let started_at = *wave_started_at.get_or_insert(now);
    pane.set_wave_tick(animation_tick(now.saturating_duration_since(started_at)));
    let width = area.width.max(1) as usize;
    pane.sync_with_options(scrollback, width, *theme, false);
    state.prepare_layout(pane, scrollback, width, area.height);
    for paint in state.visible_lines(pane, scrollback) {
        let _ = pane.paint_buffer_line(buf, area, &paint, None);
    }
}

fn subagent_catalog_unsupported(error: &PagerError) -> bool {
    match error.code().map(str::to_ascii_lowercase) {
        Some(code)
            if matches!(
                code.as_str(),
                "method-not-found"
                    | "method_not_found"
                    | "not-found"
                    | "unsupported"
                    | "capability-denied"
            ) =>
        {
            true
        }
        _ => {
            let message = error.to_string().to_ascii_lowercase();
            message.contains("unknown method") || message.contains("method not found")
        }
    }
}

fn agent_item_key(id: &AgentItemId) -> String {
    match id {
        AgentItemId::Task(id) => format!("task:{id}"),
        AgentItemId::Subagent(id) => format!("subagent:{id}"),
    }
}

fn agent_item_from_key(key: &str) -> Option<AgentItemId> {
    if let Some(id) = key.strip_prefix("task:") {
        return Some(AgentItemId::Task(id.to_string()));
    }
    key.strip_prefix("subagent:")
        .map(|id| AgentItemId::Subagent(id.to_string()))
}

#[derive(Debug, Clone)]
struct PendingQueueMutation {
    operation: OperationKey,
    item_id: DshQueueItemId,
    base_revision: u64,
}

#[derive(Debug, Clone)]
struct PendingPermissionSwitch {
    operation: OperationKey,
    target: String,
}

#[derive(Debug, Clone)]
struct PendingHostCommand {
    operation: OperationKey,
    line: String,
}

#[derive(Debug, Clone)]
struct PendingAgentPresetSwitch {
    operation: OperationKey,
    previous: Option<String>,
}

/// Run the default Grok-derived UI until the user closes it.
pub fn run_interactive(mut transport: RpcTransport, mut session: SessionState) -> PagerResult<()> {
    diag::install_panic_hook();
    diag::log(
        "interactive",
        format!(
            "enter session={} generation={} history={} no_color={} term_program={}",
            session.session_id(),
            session.generation(),
            session.history().len(),
            std::env::var_os("NO_COLOR").is_some(),
            std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "unset".into()),
        ),
    );
    let mut terminal = TerminalSurface::enter()?;
    let mut ui = UiState {
        capabilities: terminal.capabilities(),
        ..UiState::default()
    };
    if let Some(cadence_ms) = std::env::var("GROK_SCROLL_CADENCE_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
    {
        let cadence = Duration::from_millis(cadence_ms);
        ui.mouse_scroll.set_redraw_cadence(cadence);
        ui.child_mouse_scroll.set_redraw_cadence(cadence);
    }
    ui.refresh_agent_subagents(&mut transport, &session, true);
    // Restore the terminal before resuming a panic so the payload is not
    // trapped on the alternate screen.
    let result = catch_unwind(AssertUnwindSafe(|| {
        run_loop(&mut terminal, &mut transport, &mut session, &mut ui)
    }));
    let restore_result = terminal.restore();
    match result {
        Ok(loop_result) => {
            diag::log("interactive", "exit restored");
            loop_result.and(restore_result.map_err(PagerError::from))
        }
        Err(payload) => {
            diag::log_always("interactive", "panic after restore");
            let _ = restore_result;
            resume_unwind(payload)
        }
    }
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

        // Grok drains the whole immediately-buffered input backlog in one loop
        // iteration. Doing this before notifications prevents a token firehose
        // from keeping wheel/key events parked behind repeated notification
        // batches, and lets MouseScrollState coalesce one physical gesture.
        let scroll_tick_changed = ui.tick_mouse_scroll();
        let input_events = read_buffered_input()?;
        let had_input = !input_events.is_empty();
        let mut quit = false;
        for input_event in input_events {
            quit |= dispatch_terminal_event(ui, input_event, transport, session)?;
            if quit {
                break;
            }
        }
        if had_input || scroll_tick_changed {
            ui.flush_copy(terminal);
        }
        if quit {
            break;
        }

        // Match Grok's input_rx.is_empty() ACP gate. If input was already
        // buffered, skip notifications for this iteration; while draining,
        // stop before the next notification as soon as terminal input arrives.
        if !had_input {
            match drain_notifications_bounded(
                transport,
                session,
                NOTIFICATION_BUDGET,
                terminal_input_pending_fail_closed,
            ) {
                Ok((update, processed)) if update.gap_detected => {
                    record_scheduler_batch(ui, processed);
                    if let Err(error) = repair_tail(transport, session) {
                        ui.status = Some(format!("history repair error: {error}"));
                    }
                }
                Ok((update, processed)) if update.changed => {
                    record_scheduler_batch(ui, processed);
                    ui.shell.invalidate_content();
                    ui.refresh_agent_subagents(transport, session, false);
                    let _ = ui.dispatch_event(ShellEvent::Notification, transport, session)?;
                }
                Ok((_, processed)) => record_scheduler_batch(ui, processed),
                Err(error) => {
                    diag::log("notify", format!("error {error}"));
                    ui.status = Some(format!("notification error: {error}"));
                }
            }
        }

        let frame_links = ui.frame_links.clone();
        if let Err(error) = terminal.draw_with_links(&frame_links, |frame| {
            ui.render(frame, session, transport.control_plane())
        }) {
            diag::log_always("draw", format!("{error}"));
            return Err(PagerError::from(error));
        }
        let welcome_animating =
            ui.scrollback_pane.is_empty() && ui.welcome_animation.is_animating(Instant::now());
        let mut poll_interval = if session.running()
            || ui.scrollback_pane.is_animating()
            || welcome_animating
            || ui.watchers_live
            || ui.child_scrollback_pane.is_animating()
        {
            ANIMATION_POLL_INTERVAL
        } else {
            POLL_INTERVAL
        };
        if let Some(scroll_deadline) = ui.mouse_scroll.scroll_clock_deadline(Instant::now()) {
            poll_interval = poll_interval.min(scroll_deadline);
        }
        if let Some(scroll_deadline) = ui.child_mouse_scroll.scroll_clock_deadline(Instant::now()) {
            poll_interval = poll_interval.min(scroll_deadline);
        }
        match event::poll(poll_interval) {
            Ok(false) => continue,
            // Leave the event in crossterm's queue. The loop-top batch drain
            // timestamps and handles all buffered events together.
            Ok(true) => continue,
            Err(error) => {
                diag::log_always("input", format!("poll {error}"));
                return Err(PagerError::from(error));
            }
        }
    }
    Ok(())
}

fn read_buffered_input() -> PagerResult<Vec<Event>> {
    let mut buffered = Vec::new();
    while buffered.len() < INPUT_DRAIN_BATCH_MAX {
        match event::poll(Duration::ZERO) {
            Ok(false) => break,
            Ok(true) => match event::read() {
                Ok(input_event) => buffered.push(input_event),
                Err(error) => {
                    diag::log_always("input", format!("read {error}"));
                    return Err(PagerError::from(error));
                }
            },
            Err(error) => {
                diag::log_always("input", format!("poll {error}"));
                return Err(PagerError::from(error));
            }
        }
    }
    Ok(buffered)
}

fn dispatch_terminal_event(
    ui: &mut UiState,
    input_event: Event,
    transport: &mut RpcTransport,
    session: &mut SessionState,
) -> PagerResult<bool> {
    match input_event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            ui.dispatch_event(ShellEvent::Key(key), transport, session)
        }
        Event::Mouse(mouse) if ui.handle_normalized_transcript_scroll(&mouse) => Ok(false),
        Event::Mouse(mouse) => ui.dispatch_event(ShellEvent::Mouse(mouse), transport, session),
        Event::Paste(text) => ui.dispatch_event(ShellEvent::Paste(text), transport, session),
        Event::Resize(width, height) => {
            ui.dispatch_event(ShellEvent::Resize { width, height }, transport, session)
        }
        _ => ui.dispatch_event(ShellEvent::Tick, transport, session),
    }
}

fn terminal_input_pending_fail_closed() -> bool {
    match event::poll(Duration::ZERO) {
        Ok(pending) => pending,
        Err(error) => {
            diag::log("input", format!("notification gate poll failed: {error}"));
            true
        }
    }
}

fn record_scheduler_batch(ui: &mut UiState, processed: usize) {
    ui.scheduler_stats.enqueued = ui.scheduler_stats.enqueued.saturating_add(processed as u64);
    ui.scheduler_stats.processed = ui
        .scheduler_stats
        .processed
        .saturating_add(processed as u64);
    ui.scheduler_stats.max_pending = ui.scheduler_stats.max_pending.max(processed);
}

fn drain_notifications_bounded(
    transport: &mut RpcTransport,
    session: &mut SessionState,
    budget: usize,
    mut input_pending: impl FnMut() -> bool,
) -> PagerResult<(SessionUpdate, usize)> {
    let mut combined = SessionUpdate::default();
    let mut processed = 0usize;
    while processed < budget && !input_pending() {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PickerKind {
    #[default]
    Resume,
    AgentPreset,
    Model,
}

#[derive(Debug, Clone)]
struct PendingRewind {
    operation: OperationKey,
    prompt_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingLoginKind {
    Describe,
    Set,
}

#[derive(Debug, Clone)]
struct PendingLogin {
    operation: OperationKey,
    kind: PendingLoginKind,
}

#[derive(Debug, Default)]
struct UiState {
    shell: AppShell,
    capabilities: TerminalCapabilities,
    scrollback_state: GrokScrollbackState,
    mouse_scroll: MouseScrollState,
    scrollback_pane: DshScrollbackHost,
    resume_picker: ResumePickerState,
    preset_picker: PresetPickerState,
    model_picker: ModelPickerState,
    login: LoginModalState,
    pending_login: Option<PendingLogin>,
    picker_kind: PickerKind,
    models: ModelState,
    models_for_session: Option<String>,
    pending_model: Option<ModelId>,
    command_catalog: Vec<CommandDescriptor>,
    commands_for_session: Option<String>,
    command_catalog_revision: u64,
    preset_session_key: Option<(String, u64)>,
    pending_agent_preset: Option<String>,
    pending_agent_preset_switch: Option<PendingAgentPresetSwitch>,
    pending_first_prompt: Option<OperationKey>,
    preset_locked_locally: bool,
    agent_preset_roster: Vec<dsh_pager_protocol::AgentPresetEntry>,
    roster_requested: bool,
    modal: ModalWindowState,
    prompt: PromptEditor,
    prompt_renderer: GrokPromptRenderer,
    pending_permission: Option<PendingPermissionSwitch>,
    pending_host_command: Option<PendingHostCommand>,
    queue_selected_id: Option<String>,
    queue_editing: bool,
    queue_editor: PromptEditor,
    queue_pending: Option<PendingQueueMutation>,
    interaction_editor: PromptEditor,
    interaction_selected: usize,
    interaction_question_index: usize,
    interaction_answer_drafts: Vec<QuestionAnswerDraft>,
    interaction_request_id: Option<DshRequestId>,
    interaction_generation: Option<DshGeneration>,
    interaction_pending: Option<DshRequestId>,
    cancel_pending: Option<OperationKey>,
    rewind: Option<RewindState>,
    rewind_target: Option<RewindPoint>,
    pending_rewind: Option<PendingRewind>,
    rewind_skip_confirmation: bool,
    rewind_area: Rect,
    interaction_args_expanded: bool,
    permission_area: Rect,
    permission_option_rows: Vec<Rect>,
    hovered_permission_item: Option<usize>,
    last_permission_click: Option<(Instant, usize)>,
    file_search_editor: PromptEditor,
    file_search: FileSearchController,
    slash: SlashController,
    image_selected: usize,
    media_preview: Option<MediaPreviewBuffer>,
    media_preview_controller: MediaPreviewController,
    transcript_media_revision: Option<u64>,
    transcript_media_enabled: Option<bool>,
    transcript_media: MediaSnapshot,
    agent_pane: AgentPaneController,
    agent_detail: Option<AgentItemId>,
    child_transcript: Option<ChildTranscriptView>,
    child_scrollback: Option<dsh_pager::scrollback::Scrollback>,
    child_scrollback_pane: DshScrollbackHost,
    child_scrollback_state: GrokScrollbackState,
    child_mouse_scroll: MouseScrollState,
    child_history_at: Option<Instant>,
    last_agent_item_click: Option<(Instant, AgentItemId)>,
    last_model_label_click: Option<Instant>,
    watchers_live: bool,
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
    /// Local transcript-clock preference; `None` keeps the appearance default.
    timestamps_enabled: Option<bool>,
    status: Option<String>,
    frame: usize,
    rail_wave_started_at: Option<Instant>,
    welcome_animation: WelcomeAnimation,
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
    transcript_mouse_pos: Option<(u16, u16)>,
    context_hovered: bool,
    turn_stop_hovered: bool,
    frame_links: Vec<dsh_grok_inline::LinkSpan>,
    pending_copy: Option<String>,
    scheduler_stats: SchedulerStats,
    /// Last privacy-safe diagnostic fingerprints written under
    /// `DSH_PAGER_DIAG`. They prevent a 30fps animation from becoming a
    /// 30fps disk writer while retaining every chrome state transition.
    last_turn_chrome_diag: Option<String>,
    last_transcript_chrome_diag: Option<String>,
}

fn log_state_change(previous: &mut Option<String>, stage: &str, current: String) {
    if previous.as_deref() == Some(current.as_str()) {
        return;
    }
    diag::log(stage, &current);
    *previous = Some(current);
}

fn turn_activity_diag_name(activity: &TurnActivitySnapshot) -> &'static str {
    match activity {
        TurnActivitySnapshot::Thinking => "thinking",
        TurnActivitySnapshot::Responding => "responding",
        TurnActivitySnapshot::ToolRunning { .. } => "tool-running",
        TurnActivitySnapshot::Compacting => "compacting",
        TurnActivitySnapshot::Retrying { .. } => "retrying",
        TurnActivitySnapshot::WritingToolCall => "writing-tool-call",
        TurnActivitySnapshot::Waiting => "waiting",
        TurnActivitySnapshot::WaitingForInput => "waiting-for-input",
    }
}

impl UiState {
    fn mouse_scroll_config(&self) -> MouseScrollConfig {
        MouseScrollConfig::from_settings()
            .with_viewport_height(self.scrollback_state.viewport_height())
    }

    /// Route the main transcript's wheel reports through Grok's production
    /// wheel/trackpad normalizer. Overlay owners retain their existing input
    /// behavior and never mutate the hidden transcript viewport.
    fn handle_normalized_transcript_scroll(&mut self, mouse: &MouseEvent) -> bool {
        let Some(direction) = ScrollDirection::from_mouse_event(mouse) else {
            return false;
        };
        if self.shell.overlay() == Overlay::AgentTasks && self.agent_detail.is_some() {
            if self.child_scrollback_pane.is_empty() {
                return true;
            }
            let config = MouseScrollConfig::from_settings()
                .with_viewport_height(self.child_scrollback_state.viewport_height());
            let update = self.child_mouse_scroll.on_scroll_event(direction, config);
            if update.lines < 0 {
                self.child_scrollback_state
                    .scroll_up(update.lines.unsigned_abs().min(u16::MAX as u32) as u16);
            } else if update.lines > 0 {
                self.child_scrollback_state
                    .scroll_down((update.lines as u32).min(u16::MAX as u32) as u16);
            }
            return true;
        }
        if self.shell.overlay() != Overlay::None {
            return false;
        }
        if self.scrollback_pane.is_empty() {
            return true;
        }
        let config = self.mouse_scroll_config();
        let update = self.mouse_scroll.on_scroll_event(direction, config);
        self.apply_transcript_scroll_lines(update.lines);
        true
    }

    fn tick_mouse_scroll(&mut self) -> bool {
        if self.shell.overlay() == Overlay::AgentTasks && self.agent_detail.is_some() {
            self.mouse_scroll.cancel_stream();
            let update = self.child_mouse_scroll.on_tick();
            if update.lines < 0 {
                self.child_scrollback_state
                    .scroll_up(update.lines.unsigned_abs().min(u16::MAX as u32) as u16);
                return true;
            }
            if update.lines > 0 {
                self.child_scrollback_state
                    .scroll_down((update.lines as u32).min(u16::MAX as u32) as u16);
                return true;
            }
            return false;
        }
        self.child_mouse_scroll.cancel_stream();
        if self.shell.overlay() != Overlay::None {
            self.mouse_scroll.cancel_stream();
            return false;
        }
        let update = self.mouse_scroll.on_tick();
        self.apply_transcript_scroll_lines(update.lines)
    }

    fn apply_transcript_scroll_lines(&mut self, lines: i32) -> bool {
        if lines == 0 {
            return false;
        }
        if lines < 0 {
            self.scrollback_state
                .scroll_up(lines.unsigned_abs().min(u16::MAX as u32) as u16);
        } else {
            self.scrollback_state
                .scroll_down((lines as u32).min(u16::MAX as u32) as u16);
        }
        true
    }

    /// Refresh the child catalog outside rendering. Views only consume the
    /// `AgentSnapshot` projection; RPC and protocol details stay here.
    fn refresh_agent_subagents(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        force: bool,
    ) {
        let parent = session.session_id();
        if !force
            && !transport
                .control_plane()
                .subagent_catalog_needs_refresh(parent)
        {
            return;
        }
        diag::log("catalog", format!("refresh parent={parent} force={force}"));
        match dsh_pager::list_subagents(transport, parent) {
            Ok(catalog) => {
                diag::log(
                    "catalog",
                    format!(
                        "ok parent={parent} entries={} available={}",
                        catalog.entries.len(),
                        catalog.parent_available
                    ),
                );
                transport
                    .control_plane_mut()
                    .store
                    .apply_subagent_list(parent, &catalog);
            }
            Err(error) if subagent_catalog_unsupported(&error) => {
                diag::log("catalog", format!("unsupported parent={parent} {error}"));
                transport
                    .control_plane_mut()
                    .store
                    .mark_subagent_catalog_unsupported(parent);
                self.status = Some(format!("Subagent catalog unavailable: {error}"));
            }
            Err(error) => {
                diag::log("catalog", format!("err parent={parent} {error}"));
                self.status = Some(format!("Subagent list failed: {error}"));
            }
        }
    }

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
        session: &mut SessionState,
    ) -> PagerResult<()> {
        if !self.roster_requested {
            self.roster_requested = true;
            let context = UiContext::from_session(session);
            let _ = self.submit_effect(
                transport,
                UiIntent::ListAgentPresets { revision: 0 },
                &context,
            );
        }
        if self.models_for_session.as_deref() != Some(session.session_id()) {
            self.request_session_models(transport, session, 0);
        }
        if self.commands_for_session.as_deref() != Some(session.session_id()) {
            self.request_commands(transport, session);
        }
        let completions = {
            let (executor, ledger) = (&mut self.effect_executor, &mut self.effect_ledger);
            executor.poll(transport, ledger)?
        };
        for completion in completions {
            let rewind_prompt = self.pending_rewind.as_ref().and_then(|pending| {
                (matches!(completion.effect, UiEffect::ForkSession { .. })
                    && pending.operation == completion.receipt.operation)
                    .then(|| pending.prompt_text.clone())
            });
            let forked_session_id = completion.forked_session_id.clone();
            let rewind_accepted = completion.receipt.status == UiEffectStatus::Accepted;
            self.apply_effect_completion(completion, session);
            if let Some(prompt_text) = rewind_prompt {
                self.pending_rewind = None;
                if rewind_accepted {
                    if let Some(session_id) = forked_session_id {
                        self.finish_rewind_attach(
                            transport,
                            session,
                            session_id.as_str(),
                            &prompt_text,
                        );
                    } else {
                        self.fail_rewind("session.fork returned no child session id");
                    }
                } else if !matches!(
                    self.rewind.as_ref().map(|rewind| &rewind.phase),
                    Some(RewindPhase::Error { .. })
                ) {
                    self.fail_rewind("host rejected session.fork");
                }
            }
        }
        Ok(())
    }

    fn apply_effect_completion(&mut self, completion: UiEffectCompletion, session: &SessionState) {
        let subject = match &completion.effect {
            UiEffect::CancelSession { .. } => "Cancel session",
            UiEffect::SubmitPrompt { .. } => "Prompt",
            UiEffect::ExecuteCommand { .. } => "Command",
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
            UiEffect::SetPermissionPreset { .. } => "Permission preset",
            UiEffect::ListCommands { .. } => "Command list",
            UiEffect::ListAgentPresets { .. } => "Agent preset list",
            UiEffect::SelectAgentPreset { .. } => "Agent preset",
            UiEffect::ListSessionModels { .. } => "Model list",
            UiEffect::SelectSessionModel { .. } => "Model",
            UiEffect::DescribeCredential { .. } => "Credential status",
            UiEffect::SetCredential { .. } => "DeepSeek API key",
        };
        let host_global = matches!(
            &completion.effect,
            UiEffect::DescribeCredential { .. } | UiEffect::SetCredential { .. }
        );
        if !host_global
            && (completion.receipt.operation.session_id.as_str() != session.session_id()
                || completion.receipt.operation.generation
                    != DshGeneration::new(session.generation()))
        {
            self.status = Some(format!(
                "Ignored stale {subject} completion for {}",
                completion.receipt.operation.request_id
            ));
            return;
        }
        if matches!(&completion.effect, UiEffect::DescribeCredential { .. }) {
            let matches_pending = self.pending_login.as_ref().is_some_and(|pending| {
                pending.kind == PendingLoginKind::Describe
                    && pending.operation == completion.receipt.operation
            });
            if !matches_pending {
                return;
            }
            self.pending_login = None;
            if completion.receipt.status == UiEffectStatus::Accepted {
                if let Some(info) = completion.credential_info {
                    self.login.apply_info(info);
                    self.status = None;
                } else {
                    self.login
                        .fail("Host returned no state for DEEPSEEK_API_KEY");
                }
            } else {
                let message = completion
                    .receipt
                    .diagnostic
                    .clone()
                    .unwrap_or_else(|| receipt_status_message(&completion.receipt, subject));
                self.login.fail(message.clone());
                self.status = Some(format!("Credential status failed: {message}"));
            }
            return;
        }
        if matches!(&completion.effect, UiEffect::SetCredential { .. }) {
            let matches_pending = self.pending_login.as_ref().is_some_and(|pending| {
                pending.kind == PendingLoginKind::Set
                    && pending.operation == completion.receipt.operation
            });
            if !matches_pending {
                return;
            }
            self.pending_login = None;
            if completion.receipt.status == UiEffectStatus::Accepted {
                self.login.clear_secret();
                if self.shell.overlay() == Overlay::Login {
                    self.shell.close_overlay();
                }
                self.status = Some("DeepSeek API key saved. The next request will use it.".into());
            } else {
                let message = completion
                    .receipt
                    .diagnostic
                    .clone()
                    .unwrap_or_else(|| receipt_status_message(&completion.receipt, subject));
                self.login.fail(message.clone());
                self.status = Some(format!("Couldn't save DeepSeek API key: {message}"));
            }
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
            let matches_latest = self
                .cancel_pending
                .as_ref()
                .is_some_and(|pending| *pending == completion.receipt.operation);
            if !matches_latest {
                return;
            }
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
        if let UiEffect::SubmitPrompt { text, .. } = &completion.effect {
            let matches_first = self.pending_first_prompt.as_ref().is_some_and(|pending| {
                pending.request_id == completion.receipt.operation.request_id
            });
            if matches_first {
                if completion.receipt.status == UiEffectStatus::Accepted {
                    // Bridge the small window before the control-plane
                    // projection publishes `blank=false`.
                    self.preset_locked_locally = true;
                }
                self.pending_first_prompt = None;
            }
            if completion.receipt.status == UiEffectStatus::Accepted {
                let prompt_unchanged = self.prompt.text() == text;
                if prompt_unchanged {
                    let text = text.clone();
                    self.record_prompt_history(&text);
                    self.prompt.reset();
                    self.slash.reset();
                }
                if prompt_unchanged {
                    self.status = Some(prompt_admission_message(&completion.receipt.status));
                }
            }
        }
        if let UiEffect::ExecuteCommand { line, .. } = &completion.effect {
            let matches_pending = self.pending_host_command.as_ref().is_some_and(|pending| {
                pending.operation.request_id == completion.receipt.operation.request_id
            });
            if !matches_pending {
                self.status = Some("Ignored stale Command completion".into());
                return;
            }
            self.pending_host_command = None;
            if completion.receipt.status == UiEffectStatus::Accepted {
                if self.prompt.text() == line {
                    self.record_prompt_history(line);
                    self.prompt.reset();
                    self.slash.reset();
                }
                self.status = Some(
                    completion
                        .command_execution
                        .as_ref()
                        .and_then(|execution| execution.result.text.clone())
                        .unwrap_or_else(|| format!("Command completed: {line}")),
                );
            } else {
                self.commands_for_session = None;
                self.status = Some(if completion.receipt.status == UiEffectStatus::Rejected {
                    format!("Unknown or unavailable slash command: {line}")
                } else {
                    receipt_status_message(&completion.receipt, subject)
                });
            }
            return;
        }
        if let UiEffect::SetPermissionPreset { preset, .. } = &completion.effect {
            let matches_pending = self.pending_permission.as_ref().is_some_and(|pending| {
                pending.operation.request_id == completion.receipt.operation.request_id
            });
            if matches_pending {
                match completion.receipt.status {
                    UiEffectStatus::Accepted => match completion.command_execution.as_ref() {
                        Some(execution) if execution.result.kind == CommandResultKind::Success => {
                            self.status = Some(format!(
                                "Permission → {preset}; waiting for host projection"
                            ));
                        }
                        Some(execution) => {
                            self.pending_permission = None;
                            self.status = Some(
                                execution
                                    .result
                                    .text
                                    .clone()
                                    .unwrap_or_else(|| "Permission command failed".into()),
                            );
                        }
                        None => {
                            self.pending_permission = None;
                            self.status = Some("Permission command returned no result".into());
                        }
                    },
                    _ => {
                        self.pending_permission = None;
                        self.commands_for_session = None;
                        self.status = Some(receipt_status_message(&completion.receipt, subject));
                    }
                }
            }
            return;
        }
        if let UiEffect::ListAgentPresets { revision, .. } = &completion.effect {
            if completion.receipt.status == UiEffectStatus::Accepted {
                if let Some(list) = completion.agent_preset_list.clone() {
                    self.agent_preset_roster = list.presets.clone();
                    let _ = self.preset_picker.apply_entries(*revision, list.presets);
                } else {
                    let _ = self
                        .preset_picker
                        .fail_entries(*revision, "agentPreset.list returned no value");
                }
            } else {
                let _ = self.preset_picker.fail_entries(
                    *revision,
                    completion
                        .receipt
                        .diagnostic
                        .clone()
                        .unwrap_or_else(|| "host rejected agentPreset.list".to_string()),
                );
            }
            return;
        }
        if let UiEffect::ListCommands { revision, .. } = &completion.effect {
            if *revision != self.command_catalog_revision {
                self.status = Some(format!(
                    "Ignored stale Command list completion for revision {revision}"
                ));
                return;
            }
            if completion.receipt.status == UiEffectStatus::Accepted {
                if let Some(commands) = completion.commands.clone() {
                    self.command_catalog = commands;
                } else {
                    self.status = Some("Command list returned no value".into());
                }
            } else {
                self.status = Some(receipt_status_message(&completion.receipt, subject));
            }
            return;
        }
        if let UiEffect::SelectAgentPreset { agent_preset, .. } = &completion.effect {
            let matches_pending =
                self.pending_agent_preset_switch
                    .as_ref()
                    .is_some_and(|pending| {
                        pending.operation.request_id == completion.receipt.operation.request_id
                    });
            if !matches_pending {
                self.status = Some("Ignored stale Agent preset completion".into());
                return;
            }
            let pending = self.pending_agent_preset_switch.take();
            if completion.receipt.status == UiEffectStatus::Accepted {
                let id = completion
                    .selected_agent_preset
                    .clone()
                    .unwrap_or_else(|| agent_preset.clone());
                self.pending_agent_preset = Some(id.clone());
                self.models_for_session = None;
                self.model_picker.close();
                self.command_catalog.clear();
                self.commands_for_session = None;
                self.command_catalog_revision = self.command_catalog_revision.saturating_add(1);
                let label = current_agent_preset_label(Some(&id), &self.agent_preset_roster);
                self.status = Some(format!("Preset → {label}"));
            } else {
                self.pending_agent_preset = pending.and_then(|pending| pending.previous);
                self.status = Some(receipt_status_message(&completion.receipt, subject));
            }
            return;
        }
        if let UiEffect::ListSessionModels { revision, .. } = &completion.effect {
            if completion.receipt.status == UiEffectStatus::Accepted {
                if let Some(value) = completion.session_models.clone() {
                    self.models.apply_session_models(value);
                    self.pending_model = None;
                    let _ = self.model_picker.apply_catalog(*revision, &self.models);
                } else {
                    let _ = self
                        .model_picker
                        .fail_entries(*revision, "session.models returned no value");
                }
            } else {
                let _ = self.model_picker.fail_entries(
                    *revision,
                    completion
                        .receipt
                        .diagnostic
                        .clone()
                        .unwrap_or_else(|| "host rejected session.models".to_string()),
                );
            }
            return;
        }
        if let UiEffect::SelectSessionModel {
            provider,
            model,
            reasoning_effort,
            ..
        } = &completion.effect
        {
            if completion.receipt.status == UiEffectStatus::Accepted {
                let selected = completion.selected_model.clone().unwrap_or(
                    dsh_pager_protocol::ModelSelection {
                        provider: provider.clone(),
                        model: model.clone(),
                        reasoning_effort: reasoning_effort.clone(),
                    },
                );
                let id = ModelId::from(&selected);
                self.models
                    .set_current(id.clone(), selected.reasoning_effort.clone());
                self.pending_model = Some(id);
                let display_name = self.models.display_name_for(&ModelId::from(&selected));
                let msg = if let Some(eff) = selected.reasoning_effort.as_deref() {
                    format!("Switched to {display_name} ({eff} effort)")
                } else {
                    format!("Switched to {display_name}")
                };
                self.status = Some(msg);
                self.model_picker.close();
                if self.picker_kind == PickerKind::Model {
                    self.shell.close_overlay();
                }
            } else {
                self.pending_model = None;
                let diagnostic = completion
                    .receipt
                    .diagnostic
                    .clone()
                    .unwrap_or_else(|| receipt_status_message(&completion.receipt, subject));
                self.status = Some(format!("Couldn't switch model: {diagnostic}"));
            }
            return;
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

        let mut snapshot = GrokHostSnapshot::for_render(session, Some(control_plane));
        let scrollback_revision = session.scrollback.revision();
        if self.transcript_media_revision != Some(scrollback_revision)
            || self.transcript_media_enabled != Some(snapshot.capabilities.image)
        {
            self.transcript_media =
                media_snapshot_from_scrollback(&session.scrollback, snapshot.capabilities.image);
            self.transcript_media_revision = Some(scrollback_revision);
            self.transcript_media_enabled = Some(snapshot.capabilities.image);
        }
        snapshot.media = self.transcript_media.clone();
        self.apply_file_search_result(&mut snapshot);
        self.sync_dashboard(control_plane);
        self.agent_pane.sync(&snapshot.agent);
        self.reconcile_preset_session(&snapshot);
        self.reconcile_snapshot(&snapshot);
        self.reconcile_permission(&snapshot);
        self.welcome_animation.observe_session(&snapshot.session_id);
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
        let question = snapshot.interaction.as_ref().and_then(|interaction| {
            question_state(
                interaction,
                self.interaction_question_index,
                self.interaction_selected,
                self.interaction_pending.is_some(),
                self.interaction_args_expanded,
            )
        });
        let blocking_card = permission.is_some() || question.is_some() || self.rewind.is_some();
        self.slash.refresh(
            self.prompt.text(),
            self.prompt.cursor(),
            &self.models,
            &self.command_catalog,
            &snapshot.controls.permission.options,
        );
        let prompt_style = PromptStyleContract {
            focused,
            compact,
            title: None,
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
        let model_name = self
            .models
            .current_model_name()
            .unwrap_or_else(|| snapshot.model.clone());
        let prompt_info = PromptInfoContract {
            model_name: compact_model_effort_label(
                &model_name,
                self.models.reasoning_effort.as_deref(),
            ),
            flags: self.prompt_flags(&snapshot, theme),
            multiline: textarea_rows > 1,
            ..PromptInfoContract::default()
        };
        let prompt_height = if let Some(permission) = permission.as_ref() {
            permission_view_height(
                permission,
                area.height,
                prompt_outer_width.saturating_sub(5) as usize,
            )
        } else if let Some(question) = question.as_ref() {
            question_view_height(
                question,
                area.height,
                prompt_outer_width.saturating_sub(5) as usize,
            )
        } else if let Some(rewind) = self.rewind.as_ref() {
            rewind_overlay_height(&rewind.phase, area.height)
        } else {
            GrokPromptRenderer::desired_height(
                self.prompt.textarea(),
                prompt_outer_width,
                &prompt_style,
                Some(&prompt_info),
                prompt_cap,
            )
        };
        let tasks_height = inline_agent_pane_height(&snapshot.agent, area.height);
        let catalog_height = 0;
        let watcher_visible =
            !snapshot.turn_status.visible && watcher_label(&snapshot.agent).is_some();
        let queue_height = (visible_queue_len(&snapshot.queue) as u16).clamp(0, 3);
        let timeline_width = crate::views::timeline::rail_width(
            appearance.show_timeline,
            false,
            area.width,
            snapshot.transcript_len,
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
            turn_status_height: u16::from(snapshot.turn_status.visible || watcher_visible),
            // Grok paints slash completion as an overlay above the prompt;
            // it never reserves a separate banner row in the layout.
            banner_height: 0,
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
        self.rewind_area = input;
        let home = std::env::var("HOME").ok();
        let cwd = format_cwd(
            &snapshot.cwd,
            home.as_deref(),
            header.width.saturating_sub(18) as usize,
        );
        frame.render_widget(StatusBar::new(&cwd), header);

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
            &appearance,
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
        } else if let Some(question) = question.as_ref() {
            self.permission_area = input;
            let result = render_question_view(
                frame.buffer_mut(),
                input,
                question,
                self.hovered_permission_item,
                theme,
                self.shell.owner() == KeyOwner::Interaction,
            );
            self.permission_option_rows = result.option_rows;
        } else if let Some(rewind) = self.rewind.as_ref() {
            self.permission_area = Rect::default();
            self.permission_option_rows.clear();
            self.hovered_permission_item = None;
            render_rewind_overlay(
                frame.buffer_mut(),
                input,
                &rewind.phase,
                self.shell.owner() == KeyOwner::Prompt,
            );
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
            if prompt_result.model_area.width > 0 && prompt_result.model_area.height > 0 {
                self.hit_map.insert(crate::geometry::HitRegion {
                    target: HitTarget::Overlay("model-label".into()),
                    rect: prompt_result.model_area,
                    label: "double-click to choose model".into(),
                    link: None,
                    priority: 24,
                });
            }
            if let Some(rect) = prompt_result
                .info_flag_areas
                .first()
                .copied()
                .filter(|rect| rect.width > 0 && rect.height > 0)
            {
                self.hit_map.insert(crate::geometry::HitRegion {
                    target: HitTarget::Overlay("agent-preset".into()),
                    rect,
                    label: if self.preset_editable(&snapshot) {
                        "choose agent preset"
                    } else {
                        "agent preset fixed for this conversation"
                    }
                    .into(),
                    link: None,
                    priority: 24,
                });
            }
            if let Some((x, y)) = prompt_result.cursor_pos {
                let slash = self.slash.snapshot();
                if slash.args_query_is_empty
                    && let Some(placeholder) = slash.args_placeholder.as_deref()
                {
                    let available = prompt_result.textarea_area.right().saturating_sub(x) as usize;
                    if available > 0 {
                        frame.buffer_mut().set_string(
                            x,
                            y,
                            truncate_str(placeholder, available),
                            Style::default().fg(theme.gray).bg(theme.bg_base),
                        );
                    }
                }
                if self.shell.overlay() != Overlay::Login {
                    frame.set_cursor_position((x, y));
                }
            }
        }
        let (prompt_target, prompt_label) = if permission.is_some() {
            (HitTarget::Overlay("permission".into()), "permission")
        } else if question.is_some() {
            (HitTarget::Overlay("question".into()), "question")
        } else if self.rewind.is_some() {
            (HitTarget::Overlay("rewind".into()), "rewind")
        } else {
            (HitTarget::Prompt, "prompt")
        };
        self.hit_map.insert(crate::geometry::HitRegion {
            target: prompt_target,
            rect: input,
            label: prompt_label.into(),
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
        self.watchers_live = watcher_label(&snapshot.agent).is_some();
        let animation_tick_now = {
            let now = Instant::now();
            let started_at = *self.rail_wave_started_at.get_or_insert(now);
            animation_tick(now.saturating_duration_since(started_at))
        };
        if diag::log_path().is_some() {
            log_state_change(
                &mut self.last_turn_chrome_diag,
                "turn-chrome",
                format!(
                    "session_running={} visible={} activity={} pending_input={} area={},{},{},{}",
                    snapshot.running,
                    snapshot.turn_status.visible,
                    turn_activity_diag_name(&snapshot.turn_status.activity),
                    snapshot.turn_status.pending_user_input,
                    agent_layout.turn_status.x,
                    agent_layout.turn_status.y,
                    agent_layout.turn_status.width,
                    agent_layout.turn_status.height,
                ),
            );
        }
        let turn_status_output = if snapshot.turn_status.visible {
            render_turn_status(
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
            )
        } else {
            if let Some(rect) = render_watcher_cue(
                frame.buffer_mut(),
                agent_layout.turn_status,
                &snapshot.agent,
                animation_tick_now,
                theme,
            ) {
                self.hit_map.insert(crate::geometry::HitRegion {
                    target: HitTarget::Overlay("watcher-cue".into()),
                    rect,
                    label: "background tasks and subagents".into(),
                    link: None,
                    priority: 25,
                });
            }
            Default::default()
        };
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
            self.render_blocking_card_shortcuts(
                frame,
                agent_layout.shortcuts,
                permission.options.len(),
                permission.pending,
                "select",
                (permission.options.len() > 1).then_some("next option"),
                permission
                    .has_collapsible_display(self.permission_area.width.saturating_sub(5) as usize),
                permission.args_expanded,
                compact,
                "permission",
            );
        } else if let Some(question) = question.as_ref() {
            let question_count =
                snapshot
                    .interaction
                    .as_ref()
                    .map_or(0, |interaction| match interaction {
                        DshInteraction::Question { questions, .. } => questions.len(),
                        DshInteraction::Approval { .. } => 0,
                    });
            self.render_blocking_card_shortcuts(
                frame,
                agent_layout.shortcuts,
                question.options.len(),
                question.pending,
                if question_count > 1 {
                    "answer"
                } else {
                    "select"
                },
                if question_count > 1 {
                    Some("next question")
                } else if question.options.len() > 1 {
                    Some("next option")
                } else {
                    None
                },
                question
                    .has_collapsible_display(self.permission_area.width.saturating_sub(5) as usize),
                question.args_expanded,
                compact,
                "question",
            );
        } else {
            let (hints, help_hint) = self.pane_shortcut_hints(&snapshot, textarea_rows > 1);
            let pending_hint = (self.shell.pending_esc_action()
                == Some(PendingEscAction::ClearPrompt))
            .then_some(PendingHint {
                shortcut: KeyShortcut::key(KeyCode::Esc),
                label: "clear",
            });
            AgentView::render_shortcuts(
                frame,
                agent_layout.shortcuts,
                &hints,
                help_hint,
                pending_hint,
            );
        }

        self.render_inline_panes(frame, &agent_layout, &snapshot, theme);
        // Grok's slash dropdown is a top-level prompt overlay. Paint it after
        // inline queue/task panes so those ordinary layout rows cannot cover
        // command names or descriptions.
        if !blocking_card {
            self.render_slash_dropdown(
                frame,
                area,
                agent_layout.prompt,
                &layout_cfg,
                compact,
                theme,
            );
        }

        if self.shell.overlay() == Overlay::Picker {
            match self.picker_kind {
                PickerKind::Resume => self.render_resume_picker(frame, area, compact),
                PickerKind::AgentPreset => self.render_preset_picker(frame, area, compact),
                PickerKind::Model => self.render_model_picker(frame, area, compact),
            }
        } else if self.shell.overlay() == Overlay::Queue {
            self.render_queue(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::FileSearch {
            self.render_file_search(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::ImagePreview {
            self.render_image_preview(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::AgentTasks {
            self.render_agent_tasks(frame, area, &snapshot);
        } else if self.shell.overlay() == Overlay::Login {
            if let Some(cursor) =
                self.login
                    .render(frame.buffer_mut(), area, Theme::current(), compact)
            {
                frame.set_cursor_position(cursor);
            }
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
        let rows =
            render_inline_agent_panes(buf, layout.tasks, &snapshot.agent, &self.agent_pane, theme);
        for row in rows {
            self.hit_map.insert(crate::geometry::HitRegion {
                target: HitTarget::Overlay(format!("agent-item:{}", agent_item_key(&row.id))),
                rect: row.rect,
                label: "agent task or subagent".into(),
                link: None,
                priority: 18,
            });
        }
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
        let plan = if snapshot.controls.plan.target_active() {
            "on"
        } else {
            "off"
        };
        let mut details = format!(
            "plan {plan} · permission {} · queue r{}",
            self.effective_permission_preset(snapshot),
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
                self.interaction_question_index = 0;
                self.interaction_answer_drafts = match interaction {
                    DshInteraction::Question { questions, .. } => {
                        vec![QuestionAnswerDraft::default(); questions.len()]
                    }
                    DshInteraction::Approval { .. } => Vec::new(),
                };
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
            if changed || self.shell.overlay() != target {
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
            self.interaction_question_index = 0;
            self.interaction_answer_drafts.clear();
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
                .any(|item| item.id == selected && queue_item_is_visible(item))
                .then(|| selected.to_string())
        });
        if self.queue_selected_id.is_none() {
            self.queue_selected_id = visible_queue_items(&snapshot.queue)
                .next()
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
        appearance: &GrokAppearanceSnapshot,
        snapshot: &GrokHostSnapshot,
        scrollback: &mut dsh_pager::scrollback::Scrollback,
    ) {
        let theme = Theme::current();
        let show_timestamps = self
            .timestamps_enabled
            .unwrap_or(appearance.show_timestamps);
        let content = layout.scrollback_content;
        let rail = Rect::new(
            layout.timeline_x,
            layout.scrollback.y,
            layout.timeline_width,
            layout.scrollback.height,
        );
        let mut total_height = 0;
        let mut scroll_top = 0;
        let render_width = content.width.saturating_sub(1).max(1) as usize;
        let now = Instant::now();
        let started_at = *self.rail_wave_started_at.get_or_insert(now);
        self.scrollback_pane
            .set_wave_tick(animation_tick(now.saturating_duration_since(started_at)));
        self.scrollback_pane
            .set_selected_target(self.selected_transcript.clone());
        self.scrollback_pane
            .set_pending_interaction(snapshot.interaction.as_ref());
        self.scrollback_pane.sync_with_appearance(
            scrollback,
            render_width,
            *theme,
            show_timestamps,
            (*appearance).scrollback(*theme),
        );
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme.bg_light))
            .style(Style::default().bg(theme.bg_base));
        let transcript_inner = block.inner(content);
        frame.render_widget(block, content);
        let empty = self.scrollback_pane.is_empty();
        if empty {
            self.scrollback_state.reset();
        } else {
            self.scrollback_state.prepare_layout(
                &mut self.scrollback_pane,
                scrollback,
                render_width,
                content.height,
            );
            total_height = self.scrollback_state.total_height();
            scroll_top = self.scrollback_state.scroll_offset();
            let paints = self
                .scrollback_state
                .visible_lines(&mut self.scrollback_pane, scrollback);
            if diag::log_path().is_some() {
                let mut running_entries = 0usize;
                let mut running_reasoning_entries = 0usize;
                for entry in scrollback.render_entry_refs() {
                    if entry.finish != dsh_pager::DshRenderFinish::Running {
                        continue;
                    }
                    running_entries = running_entries.saturating_add(1);
                    if entry
                        .content
                        .blocks
                        .iter()
                        .any(|block| matches!(block, dsh_pager::DshRenderBlock::Reasoning { .. }))
                    {
                        running_reasoning_entries = running_reasoning_entries.saturating_add(1);
                    }
                }
                let animated_visible_lines = paints
                    .iter()
                    .filter(|paint| paint.accent.is_some_and(|accent| accent.animated))
                    .count();
                log_state_change(
                    &mut self.last_transcript_chrome_diag,
                    "transcript-chrome",
                    format!(
                        "running_entries={running_entries} running_reasoning_entries={running_reasoning_entries} animated_visible_lines={animated_visible_lines} pane_animating={} visible_lines={} follow={} viewport_height={}",
                        self.scrollback_pane.is_animating(),
                        paints.len(),
                        self.scrollback_state.is_following(),
                        self.scrollback_state.viewport_height(),
                    ),
                );
            }
            for paint in paints {
                let text = paint.copy_text.clone();
                if let Some(timestamp) = self.scrollback_pane.paint_buffer_line(
                    frame.buffer_mut(),
                    transcript_inner,
                    &paint,
                    self.transcript_mouse_pos,
                ) {
                    self.hit_map.insert(crate::geometry::HitRegion {
                        target: HitTarget::Overlay("transcript-timestamp".into()),
                        rect: timestamp.rect,
                        label: timestamp.label,
                        link: None,
                        priority: 12,
                    });
                }
                if !paint.selectable {
                    continue;
                }
                let line_x = content
                    .x
                    .saturating_add(1)
                    .saturating_add(paint.content_offset);
                let line_width = usize::from(paint.content_width);
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
                    paint.joiner_to_previous.clone(),
                    first_link_target(&text),
                );
                self.geometry_lines.push(geometry);
            }
        }
        if empty {
            let elapsed = self.welcome_animation.elapsed(Instant::now());
            let model_name = self
                .models
                .current_model_name()
                .unwrap_or_else(|| snapshot.model.clone());
            let model_label =
                compact_model_effort_label(&model_name, self.models.reasoning_effort.as_deref());
            let welcome = render_welcome(
                frame.buffer_mut(),
                transcript_inner,
                elapsed,
                &model_label,
                &self.agent_preset_label(snapshot),
                theme,
            );
            if welcome.model_area.width > 0 && welcome.model_area.height > 0 {
                self.hit_map.insert(crate::geometry::HitRegion {
                    target: HitTarget::Overlay("model-label".into()),
                    rect: welcome.model_area,
                    label: "double-click to choose model".into(),
                    link: None,
                    priority: 24,
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

        let turn_count = snapshot.transcript_len;
        let rail_geometry = if layout.timeline_width > 0 {
            compute_rail(
                rail,
                rail.x,
                turn_count,
                RailViewport {
                    active: turn_count.checked_sub(1),
                    up_target: turn_count.checked_sub(2),
                    down_target: None,
                    at_bottom: self.scrollback_state.is_following(),
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
                self.scrollback_state.is_following(),
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

    fn render_preset_picker(&mut self, frame: &mut Frame<'_>, area: Rect, compact: bool) {
        self.preset_picker.render(
            frame.buffer_mut(),
            area,
            Theme::current(),
            compact,
            self.frame as u64,
        );
    }

    fn render_model_picker(&mut self, frame: &mut Frame<'_>, area: Rect, compact: bool) {
        self.model_picker
            .render(frame.buffer_mut(), area, Theme::current(), compact);
    }

    fn agent_preset_label(&self, snapshot: &GrokHostSnapshot) -> String {
        current_agent_preset_label(
            self.pending_agent_preset
                .as_deref()
                .or(snapshot.agent_preset.as_deref()),
            &self.agent_preset_roster,
        )
    }

    fn preset_session_key(snapshot: &GrokHostSnapshot) -> (String, u64) {
        (
            snapshot.session_id.clone(),
            snapshot.session_header.generation.get(),
        )
    }

    fn bind_preset_session(&mut self, session: &SessionState, preset: Option<String>) {
        self.preset_session_key = Some((session.session_id().to_string(), session.generation()));
        self.pending_agent_preset = preset;
        self.pending_agent_preset_switch = None;
        self.pending_first_prompt = None;
        self.pending_host_command = None;
        self.pending_permission = None;
        self.preset_locked_locally = false;
    }

    fn reconcile_preset_session(&mut self, snapshot: &GrokHostSnapshot) {
        let key = Self::preset_session_key(snapshot);
        if self.preset_session_key.as_ref() != Some(&key) {
            self.preset_session_key = Some(key);
            self.pending_agent_preset = snapshot.agent_preset.clone();
            self.pending_agent_preset_switch = None;
            self.pending_first_prompt = None;
            self.pending_host_command = None;
            self.pending_permission = None;
            self.preset_locked_locally = false;
            self.preset_picker.close();
        }
        if !snapshot.session_blank {
            self.preset_locked_locally = true;
            self.pending_first_prompt = None;
        }
        if self.pending_agent_preset_switch.is_none()
            && let Some(authoritative) = snapshot.agent_preset.as_ref()
        {
            self.pending_agent_preset = Some(authoritative.clone());
        }
    }

    fn preset_editable(&self, snapshot: &GrokHostSnapshot) -> bool {
        snapshot.session_blank
            && !self.preset_locked_locally
            && self.pending_agent_preset_switch.is_none()
            && self.pending_first_prompt.is_none()
            && self.pending_host_command.is_none()
            && self.pending_permission.is_none()
    }

    fn preset_fixed_message(&self) -> String {
        if self.pending_agent_preset_switch.is_some() {
            "Agent preset is switching; wait for the Host before sending".into()
        } else if self.pending_first_prompt.is_some() {
            "First prompt is pending; preset selection waits for the Host result".into()
        } else if self.pending_host_command.is_some() {
            "A slash command is pending; preset selection waits for the Host result".into()
        } else if self.pending_permission.is_some() {
            "Permission is switching; preset selection waits for the Host result".into()
        } else {
            "Agent preset is fixed for this conversation; use /new to choose another".into()
        }
    }

    fn prompt_flags(&self, snapshot: &GrokHostSnapshot, theme: &Theme) -> Vec<PromptFlagContract> {
        let mut preset = self.agent_preset_label(snapshot);
        if self.preset_editable(snapshot) {
            preset.push_str(" ▾");
        } else if self.pending_agent_preset_switch.is_some() {
            preset.push_str(" …");
        }
        let mut flags = vec![PromptFlagContract {
            text: preset,
            color: None,
            bold: true,
        }];
        if snapshot.controls.plan.target_active() {
            flags.push(PromptFlagContract {
                text: "plan".into(),
                color: Some(theme.accent_plan),
                bold: true,
            });
        }
        if self.effective_permission_preset(snapshot) == YOLO_PRESET {
            flags.push(PromptFlagContract {
                text: "YOLO".into(),
                color: Some(theme.warning),
                bold: true,
            });
        }
        flags
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
            self.slash.is_open(),
        );
        debug_assert_eq!(
            prompt.can_send(),
            self.prompt.can_send(),
            "hint-seam can_send must match the composer predicate"
        );
        let has_visible_queue = visible_queue_len(&snapshot.queue) > 0;
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
            snapshot.running && self.shell.overlay() == Overlay::None,
            has_visible_queue,
            false,
            false,
            false,
            crate::terminal::terminal_context().shift_enter_unavailable(),
            None,
        );
        // Reuse Grok's canonical Shift+Tab binding while the DSH conversation
        // is blank. Once the first real turn starts, that conditional entry
        // disappears and the persistent Ctrl+O YOLO hint occupies the slot.
        if self.preset_editable(snapshot) {
            if let Some(mode_hint) = hints.iter_mut().find(|hint| hint.label == "mode") {
                mode_hint.label = "preset".into();
                mode_hint.description = Some("Choose agent preset before the first turn".into());
            }
            if !hints.iter().any(|hint| hint.label == "yolo")
                && let Some(def) = registry.find(ActionId::ToggleYolo)
            {
                hints.push(def.hint());
            }
        } else if let Some(mode_hint) = hints.iter_mut().find(|hint| hint.label == "mode")
            && let Some(def) = registry.find(ActionId::ToggleYolo)
        {
            *mode_hint = def.hint();
        }
        if (self.shell.overlay() == Overlay::Queue || has_visible_queue)
            && active_pane != ActivePane::Queue
            && !self.queue_editing
            && let Some(def) = registry.find(ActionId::ToggleQueue)
        {
            hints.push(def.hint());
        }
        let help_hint = registry.find(ActionId::ShortcutsHelp).map(|def| def.hint());
        (hints, help_hint)
    }

    fn render_blocking_card_shortcuts(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        option_count: usize,
        pending: bool,
        selection_label: &'static str,
        tab_label: Option<&'static str>,
        collapsible: bool,
        expanded: bool,
        compact: bool,
        parked_label: &'static str,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let focused = self.shell.owner() == KeyOwner::Interaction;
        let hints = if focused {
            let mut hints = Vec::new();
            if !pending && option_count > 0 {
                let last = char::from(b'0' + option_count.clamp(1, 9) as u8);
                hints.push(HintItem::paired(
                    KeyShortcut::key(KeyCode::Char('1')),
                    KeyShortcut::key(KeyCode::Char(last)),
                    selection_label,
                ));
            }
            if !pending && let Some(tab_label) = tab_label {
                hints.push(HintItem::new(KeyShortcut::key(KeyCode::Tab), tab_label));
            }
            if !pending && collapsible {
                hints.push(HintItem::new(
                    KeyShortcut::ctrl(KeyCode::Char('f')),
                    if expanded { "collapse" } else { "expand" },
                ));
            }
            hints.push(HintItem::new(KeyShortcut::key(KeyCode::Esc), "scrollback"));
            hints
        } else {
            vec![
                HintItem::new(KeyShortcut::key(KeyCode::Enter), parked_label),
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
            label: if self.agent_detail.is_some() {
                "Esc/q close · ↑/↓ select"
            } else {
                "Enter open · x interrupt · Esc close"
            },
            clickable: true,
            id: 1,
        }];
        let config = ModalWindowConfig {
            title: if self.agent_detail.is_some() {
                "Agent Detail · DeepSeek host"
            } else {
                "Agent Tasks · DeepSeek host"
            },
            tabs: self
                .agent_detail
                .is_none()
                .then_some(&["tasks", "subagents"] as &[&str]),
            shortcuts: &shortcuts,
            sizing: ModalSizing::large(),
            fold_info: None,
        };
        let buf = frame.buffer_mut();
        if let Some(content) = render_modal_window(buf, area, &mut self.modal, &config, theme) {
            let agent = snapshot.agent.clone();
            if let Some(id) = self.agent_detail.clone() {
                let error = self
                    .child_transcript
                    .as_ref()
                    .and_then(|view| view.error.as_deref());
                let loading = self.child_scrollback.is_none() && error.is_none();
                let body = render_agent_detail_chrome(
                    buf,
                    content.content,
                    &agent,
                    &id,
                    error,
                    loading,
                    theme,
                );
                if let Some(mut scrollback) = self.child_scrollback.take() {
                    if body.height > 0 {
                        paint_child_scrollback(
                            &mut self.child_scrollback_pane,
                            &mut self.child_scrollback_state,
                            &mut self.rail_wave_started_at,
                            buf,
                            body,
                            &mut scrollback,
                            theme,
                        );
                    }
                    self.child_scrollback = Some(scrollback);
                }
            } else {
                render_agent_tasks_content(buf, content.content, &agent, &self.agent_pane, theme);
            }
        }
    }

    fn render_slash_dropdown(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        prompt_area: Rect,
        layout_cfg: &LayoutConfig,
        compact: bool,
        theme: &Theme,
    ) {
        let snapshot = self.slash.snapshot();
        if !snapshot.open {
            return;
        }
        let items_width = dropdown_items_width(prompt_area, layout_cfg, compact);
        let item_rows = desired_item_rows(&snapshot.matches, items_width);
        let panel = {
            let Some(chrome) = render_dropdown_chrome(
                frame.buffer_mut(),
                snapshot.matches.len(),
                item_rows,
                None,
                prompt_area,
                area,
                layout_cfg,
                compact,
                false,
                theme,
            ) else {
                return;
            };
            let _ = render_dropdown(frame.buffer_mut(), chrome.items, &snapshot, None, theme);
            chrome.panel
        };
        self.hit_map.insert(crate::geometry::HitRegion {
            target: HitTarget::Overlay("slash-dropdown".into()),
            rect: panel,
            label: "slash commands".into(),
            link: None,
            priority: 25,
        });
    }

    fn dispatch_event(
        &mut self,
        event: ShellEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<bool> {
        if let ShellEvent::Key(key) = &event
            && key.code == KeyCode::Esc
        {
            diag::log(
                "input",
                format!(
                    "esc owner={:?} overlay={:?} running={} cancel_pending={} rewind={} prompt_empty={}",
                    self.shell.owner(),
                    self.shell.overlay(),
                    session.running(),
                    self.cancel_pending.is_some(),
                    self.rewind.is_some(),
                    self.prompt.is_empty()
                ),
            );
        }
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
        if self.rewind.is_some() {
            match event {
                ShellEvent::Key(key) => {
                    self.handle_rewind_key_event(key, transport, session)?;
                    return Ok(false);
                }
                ShellEvent::Mouse(mouse) => {
                    self.handle_rewind_mouse_event(mouse, transport, session)?;
                    return Ok(false);
                }
                ShellEvent::Paste(_) => return Ok(false),
                ShellEvent::Resize { .. } | ShellEvent::Tick | ShellEvent::Notification => {}
            }
        }
        if self.shell.overlay() == Overlay::None
            && let ShellEvent::Mouse(mouse) = &event
        {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                && let Some(region) = self.hit_map.hit_test(mouse.column, mouse.row)
                && let HitTarget::Overlay(name) = &region.target
            {
                if name == "agent-preset" {
                    self.open_preset_picker(transport, session);
                    return Ok(false);
                }
                if name == "model-label" {
                    let now = Instant::now();
                    if register_model_label_click(&mut self.last_model_label_click, now) {
                        self.open_model_picker(transport, session);
                    } else {
                        self.status = Some("Model selected · double-click to choose".into());
                    }
                    return Ok(false);
                }
                if let Some(key) = name.strip_prefix("agent-item:")
                    && let Some(id) = agent_item_from_key(key)
                {
                    self.handle_agent_item_mouse(id, transport, session);
                    return Ok(false);
                }
                if name == "watcher-cue" {
                    self.clear_agent_detail();
                    self.shell.open_agent_tasks();
                    self.status = Some("Agent tasks opened".into());
                    return Ok(false);
                }
            }
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
        // Grok gives an open slash dropdown priority over the application
        // action registry. In particular, Enter must accept `/model ` before
        // the generic prompt path can submit it, and Esc dismisses the menu
        // before the homepage policy can clear the draft.
        if self.shell.overlay() == Overlay::None
            && let ShellEvent::Key(key) = &event
        {
            let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                session,
                Some(transport.control_plane()),
            );
            self.refresh_slash(&snapshot);
            if self.handle_slash_key(*key, &snapshot, transport, session)? {
                self.shell.observe_preempted_key(key);
                return Ok(false);
            }
        }
        let prompt_empty = self.prompt.is_empty();
        let action = self.shell.dispatch_home(
            event,
            HomeKeyState {
                prompt_empty,
                turn_running: session.running(),
                cancel_pending: self.cancel_pending.is_some(),
                has_rewindable_turns: session
                    .history()
                    .iter()
                    .any(|entry| entry.event.event_type == "user/message"),
                selection_active: self.selection.selection().is_some()
                    || self.selected_transcript.is_some(),
                blocking_input_pending: false,
                normal_prompt_mode: true,
                history_search_active: false,
            },
        );
        if matches!(
            action,
            ShellAction::CancelTurn | ShellAction::OpenRewindPicker
        ) {
            diag::log("input", format!("esc action={action:?}"));
        }
        match action {
            ShellAction::Quit => {
                diag::log(
                    "input",
                    format!(
                        "quit prompt_empty={prompt_empty} owner={:?}",
                        self.shell.owner()
                    ),
                );
                Ok(true)
            }
            ShellAction::CancelTurn => {
                self.request_cancel_session(transport, session)?;
                Ok(false)
            }
            ShellAction::OpenRewindPicker => {
                self.open_rewind_picker(session);
                Ok(false)
            }
            ShellAction::ClearSelection => {
                self.selection.clear();
                self.selected_transcript = None;
                self.status = None;
                Ok(false)
            }
            ShellAction::OpenQueue => {
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                self.queue_selected_id = visible_queue_items(&snapshot.queue)
                    .next()
                    .map(|item| item.id.clone());
                self.queue_editing = false;
                self.queue_editor.reset();
                self.status = if visible_queue_len(&snapshot.queue) == 0 {
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
                self.clear_agent_detail();
                self.refresh_agent_subagents(transport, session, true);
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                self.agent_pane.sync(&snapshot.agent);
                self.status = Some(agent_status_message_with_subagents(
                    &snapshot.agent,
                    snapshot.agent.subagents.len(),
                ));
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
                self.slash.reset();
                self.status = Some("Draft cleared".into());
                Ok(false)
            }
            ShellAction::ToggleYolo => {
                self.toggle_yolo(transport, session)?;
                Ok(false)
            }
            ShellAction::OpenPresetPicker => {
                self.open_preset_picker(transport, session);
                Ok(false)
            }
            ShellAction::OpenModelPicker => {
                self.open_model_picker(transport, session);
                Ok(false)
            }
            ShellAction::ScrollUp(amount) => {
                self.scrollback_state.scroll_up(amount);
                Ok(false)
            }
            ShellAction::ScrollDown(amount) => {
                self.scrollback_state.scroll_down(amount);
                Ok(false)
            }
            ShellAction::PageUp => {
                self.scrollback_state.page_up();
                Ok(false)
            }
            ShellAction::PageDown => {
                self.scrollback_state.page_down();
                Ok(false)
            }
            ShellAction::HalfPageUp => {
                self.scrollback_state.half_page_up();
                Ok(false)
            }
            ShellAction::HalfPageDown => {
                self.scrollback_state.half_page_down();
                Ok(false)
            }
            ShellAction::GotoTop => {
                self.scrollback_state.goto_top();
                Ok(false)
            }
            ShellAction::GotoBottom => {
                self.scrollback_state.goto_bottom();
                Ok(false)
            }
            ShellAction::SubmitPrompt => {
                self.dispatch_prompt_submission(transport, session)?;
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
                if self.shell.overlay() == Overlay::Interaction
                    && !text.is_empty()
                    && self.interaction_pending.is_none()
                {
                    let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                        session,
                        Some(transport.control_plane()),
                    );
                    let has_options = snapshot.interaction.as_ref().is_some_and(|interaction| {
                        question_state(
                            interaction,
                            self.interaction_question_index,
                            0,
                            false,
                            false,
                        )
                        .is_some_and(|state| !state.options.is_empty())
                    });
                    if !has_options {
                        let _ = self.interaction_editor.insert_paste(&text);
                    }
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
            ShellAction::AgentTasksMouse(mouse) => {
                self.handle_agent_tasks_mouse(mouse, transport, session);
                Ok(false)
            }
            ShellAction::LoginKey(key) => {
                self.handle_login_event(Event::Key(key), transport, session)?;
                Ok(false)
            }
            ShellAction::LoginMouse(mouse) => {
                self.handle_login_event(Event::Mouse(mouse), transport, session)?;
                Ok(false)
            }
            ShellAction::LoginPaste(text) => {
                self.handle_login_event(Event::Paste(text.into_inner()), transport, session)?;
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
                self.transcript_mouse_pos = None;
                self.frame_links.clear();
                self.geometry_lines.clear();
                self.last_transcript_click = None;
                self.last_model_label_click = None;
                self.status = Some(format!("Resized to {}x{}", area.width, area.height));
                Ok(false)
            }
            ShellAction::None | ShellAction::Redraw => Ok(false),
        }
    }

    fn handle_transcript_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let _ = self.handle_normalized_transcript_scroll(&mouse);
            }
            MouseEventKind::Moved => {
                self.transcript_mouse_pos = Some((mouse.column, mouse.row));
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
        self.request_cancel_session(transport, session)?;
        Ok(true)
    }

    fn open_rewind_picker(&mut self, session: &SessionState) {
        let snapshot = GrokHostSnapshot::from_session(session);
        let points = rewind_points(&snapshot);
        if points.is_empty() {
            self.status = Some("No undoable prompts".into());
            return;
        }
        self.rewind_target = None;
        self.rewind = Some(RewindState::picker(points));
        self.status = None;
    }

    fn handle_rewind_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        let Some(rewind) = self.rewind.as_ref() else {
            return Ok(());
        };
        let input = handle_rewind_key(rewind, &key);
        self.apply_rewind_input(input, transport, session)
    }

    fn handle_rewind_mouse_event(
        &mut self,
        mouse: MouseEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        let Some(index) = self.rewind.as_ref().and_then(|rewind| {
            rewind_row_at(&rewind.phase, self.rewind_area, mouse.column, mouse.row)
        }) else {
            return Ok(());
        };
        let input = match mouse.kind {
            MouseEventKind::Moved => {
                if let Some(rewind) = self.rewind.as_mut() {
                    set_rewind_cursor(&mut rewind.phase, index);
                }
                RewindInput::Consumed
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(rewind) = self.rewind.as_mut() else {
                    return Ok(());
                };
                set_rewind_cursor(&mut rewind.phase, index);
                rewind_activate(&rewind.phase)
            }
            _ => RewindInput::Consumed,
        };
        self.apply_rewind_input(input, transport, session)
    }

    fn apply_rewind_input(
        &mut self,
        input: RewindInput,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        match input {
            RewindInput::Dismissed | RewindInput::DismissError => {
                self.rewind = None;
                self.rewind_target = None;
                self.pending_rewind = None;
                self.status = None;
            }
            RewindInput::MoveUp => {
                if let Some(rewind) = self.rewind.as_mut() {
                    move_cursor(&mut rewind.phase, -1);
                }
            }
            RewindInput::MoveDown => {
                if let Some(rewind) = self.rewind.as_mut() {
                    move_cursor(&mut rewind.phase, 1);
                }
            }
            RewindInput::ConfirmCursor => {
                let resolved = self
                    .rewind
                    .as_ref()
                    .map(|rewind| confirm_cursor(&rewind.phase))
                    .unwrap_or(RewindInput::Consumed);
                return self.apply_rewind_input(resolved, transport, session);
            }
            RewindInput::PickerSelect(prompt_index) => {
                let point = self
                    .rewind
                    .as_ref()
                    .and_then(|rewind| rewind.point(prompt_index))
                    .cloned();
                let Some(point) = point else {
                    self.fail_rewind("selected rewind point disappeared");
                    return Ok(());
                };
                self.rewind_target = Some(point.clone());
                if self.rewind_skip_confirmation {
                    self.execute_rewind(point, transport, session)?;
                } else if let Some(rewind) = self.rewind.as_mut() {
                    rewind.phase = RewindPhase::Confirm {
                        target_prompt_index: point.prompt_index,
                        active_idx: 0,
                        prompt_preview: point.prompt_preview,
                    };
                }
            }
            RewindInput::Confirm(prompt_index) | RewindInput::ConfirmNeverAsk(prompt_index) => {
                if matches!(input, RewindInput::ConfirmNeverAsk(_)) {
                    self.rewind_skip_confirmation = true;
                }
                let point = self
                    .rewind_target
                    .clone()
                    .filter(|point| point.prompt_index == prompt_index);
                let Some(point) = point else {
                    self.fail_rewind("selected rewind point disappeared");
                    return Ok(());
                };
                self.execute_rewind(point, transport, session)?;
            }
            RewindInput::Consumed => {}
        }
        Ok(())
    }

    fn execute_rewind(
        &mut self,
        point: RewindPoint,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        if let Some(rewind) = self.rewind.as_mut() {
            rewind.phase = RewindPhase::Executing {
                target_prompt_index: point.prompt_index,
            };
        }
        if let Some(at_seq) = point.fork_at_seq {
            let request_id = DshRequestId::new(format!("rewind-{}", self.next_operation));
            self.next_operation = self.next_operation.saturating_add(1);
            let context = UiContext::for_operation(session, request_id);
            let receipt = self.submit_effect(
                transport,
                UiIntent::ForkSession {
                    at_seq: Some(at_seq),
                },
                &context,
            )?;
            if matches!(
                receipt.status,
                UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
            ) {
                self.pending_rewind = Some(PendingRewind {
                    operation: receipt.operation,
                    prompt_text: point.prompt_text,
                });
                self.status = Some("Rewinding conversation…".into());
            } else {
                self.fail_rewind(&receipt_status_message(&receipt, "Rewind"));
            }
            return Ok(());
        }

        let prompt_text = point.prompt_text;
        let current_preset = transport
            .control_plane()
            .snapshot(session.session_id())
            .and_then(|snapshot| snapshot.agent_preset.clone());
        match self.start_new_session_with_preset(transport, session, current_preset.as_deref()) {
            Ok(()) => {
                let _ = self.prompt.replace_text(&prompt_text);
                self.shell.focus_prompt();
                self.rewind = None;
                self.rewind_target = None;
                self.status = Some("Conversation rewound; prompt restored".into());
                Ok(())
            }
            Err(error) => {
                self.fail_rewind(&error.to_string());
                Ok(())
            }
        }
    }

    fn finish_rewind_attach(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        target: &str,
        prompt_text: &str,
    ) {
        if let Err(error) = self.attach_session(transport, session, target) {
            self.fail_rewind(&error.to_string());
            return;
        }
        if session.session_id() != target {
            let diagnostic = self
                .status
                .clone()
                .unwrap_or_else(|| "could not attach forked session".into());
            self.fail_rewind(&diagnostic);
            return;
        }
        let _ = self.prompt.replace_text(prompt_text);
        self.shell.focus_prompt();
        self.rewind = None;
        self.rewind_target = None;
        self.status = Some("Conversation rewound; prompt restored".into());
    }

    fn fail_rewind(&mut self, message: &str) {
        self.pending_rewind = None;
        self.rewind_target = None;
        self.rewind = Some(RewindState {
            phase: RewindPhase::Error {
                message: message.to_string(),
            },
        });
        self.status = Some(format!("Rewind failed: {message}"));
    }

    fn request_cancel_session(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> PagerResult<()> {
        let retrying = self.cancel_pending.is_some();
        if !session.running() && !retrying {
            self.status = Some("Turn already finished".into());
            return Ok(());
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
            self.status = Some(if retrying {
                "Cancellation retry sent; waiting for host snapshot".into()
            } else {
                "Cancelling turn; waiting for host snapshot".into()
            });
        } else {
            self.status = Some(receipt_status_message(&receipt, "Cancel session"));
        }
        Ok(())
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
        if agent_overlay_key_closes(key) {
            self.close_agent_overlay();
            self.status = Some("Agent detail closed".into());
            return;
        }
        if self.agent_detail.is_some() {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.scroll_child_transcript(-3),
                KeyCode::Down | KeyCode::Char('j') => self.scroll_child_transcript(3),
                KeyCode::PageUp => self.child_scrollback_state.page_up(),
                KeyCode::PageDown => self.child_scrollback_state.page_down(),
                KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                    self.child_scrollback_state.half_page_up();
                }
                KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                    self.child_scrollback_state.half_page_down();
                }
                KeyCode::Char('g') if key.modifiers.is_empty() => {
                    self.child_scrollback_state.goto_top();
                }
                KeyCode::Char('G') => self.child_scrollback_state.goto_bottom(),
                _ => {}
            }
            return;
        }
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        let agent = snapshot.agent.clone();
        self.agent_pane.sync(&agent);
        match key.code {
            KeyCode::Up => self.agent_pane.move_selection(-1),
            KeyCode::Down => self.agent_pane.move_selection(1),
            KeyCode::Enter => {
                if let Some(id) = self.agent_pane.selected().cloned() {
                    self.open_agent_detail(transport, session, &agent, id);
                }
            }
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

    fn handle_agent_item_mouse(
        &mut self,
        id: AgentItemId,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) {
        self.agent_pane.select(id.clone());
        let now = Instant::now();
        let double_click = self
            .last_agent_item_click
            .as_ref()
            .is_some_and(|(at, previous)| {
                *previous == id && now.duration_since(*at) <= TRANSCRIPT_DOUBLE_CLICK
            });
        self.last_agent_item_click = Some((now, id.clone()));
        if double_click {
            let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                session,
                Some(transport.control_plane()),
            );
            self.open_agent_detail(transport, session, &snapshot.agent, id);
            self.shell.open_agent_tasks();
        } else {
            self.status = Some("Agent item selected · double-click to open".into());
        }
    }

    fn open_agent_detail(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        agent: &AgentSnapshot,
        id: AgentItemId,
    ) {
        self.agent_detail = Some(id.clone());
        self.child_transcript = None;
        self.child_scrollback = None;
        self.child_scrollback_pane.clear();
        self.child_scrollback_state.reset();
        self.child_mouse_scroll.cancel_stream();
        self.child_history_at = None;
        if matches!(id, AgentItemId::Task(_)) {
            self.status = Some("Agent detail opened".into());
            return;
        }
        self.load_child_transcript(transport, session, agent, true);
    }

    #[allow(dead_code)]
    fn refresh_open_child_transcript(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) {
        let Some(AgentItemId::Subagent(_)) = &self.agent_detail else {
            return;
        };
        if self
            .child_history_at
            .is_some_and(|at| at.elapsed() < CHILD_HISTORY_REFRESH)
        {
            return;
        }
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        self.load_child_transcript(transport, session, &snapshot.agent, false);
    }

    fn load_child_transcript(
        &mut self,
        transport: &mut RpcTransport,
        _session: &SessionState,
        agent: &AgentSnapshot,
        announce: bool,
    ) {
        let Some(AgentItemId::Subagent(child_id)) = self.agent_detail.clone() else {
            return;
        };
        let Some(row) = agent.subagents.iter().find(|row| row.id == child_id) else {
            self.child_transcript = Some(ChildTranscriptView {
                child_id,
                error: Some("Subagent no longer exists".into()),
            });
            self.child_scrollback = None;
            return;
        };
        let Some(address) = row.address() else {
            self.child_transcript = Some(ChildTranscriptView {
                child_id: child_id.clone(),
                error: Some("no durable child session for this job yet".into()),
            });
            self.child_scrollback = None;
            if announce {
                self.status = Some("Subagent job has no child session yet".into());
            }
            return;
        };
        self.child_history_at = Some(Instant::now());
        match dsh_pager::peek_subagent_history(transport, &address, 100) {
            Ok(history) => {
                let scrollback = child_scrollback_from_history(&history.events);
                let count = scrollback.entries().len();
                self.child_transcript = Some(ChildTranscriptView {
                    child_id: child_id.clone(),
                    error: None,
                });
                self.child_scrollback = Some(scrollback);
                self.child_scrollback_pane.clear();
                if announce {
                    self.status = Some(format!("Opened child transcript ({count} entries)"));
                }
            }
            Err(error) => {
                self.child_transcript = Some(ChildTranscriptView {
                    child_id: child_id.clone(),
                    error: Some(error.to_string()),
                });
                self.child_scrollback = None;
                if announce {
                    self.status = Some(format!("Child history unavailable: {error}"));
                }
            }
        }
    }

    fn handle_agent_tasks_mouse(
        &mut self,
        mouse: MouseEvent,
        _transport: &mut RpcTransport,
        _session: &SessionState,
    ) {
        if agent_overlay_close_click(&self.modal, &mouse) {
            self.close_agent_overlay();
            self.status = Some("Agent detail closed".into());
            return;
        }
        match handle_modal_mouse(&mut self.modal, mouse.kind, mouse.column, mouse.row) {
            ModalWindowOutcome::CloseRequested | ModalWindowOutcome::ShortcutActivated(_) => {
                self.close_agent_overlay();
                self.status = Some("Agent detail closed".into());
                return;
            }
            _ => {}
        }
        if self.agent_detail.is_some() {
            match mouse.kind {
                MouseEventKind::ScrollUp => self.scroll_child_transcript(-3),
                MouseEventKind::ScrollDown => self.scroll_child_transcript(3),
                _ => {}
            }
        }
    }

    fn clear_agent_detail(&mut self) {
        self.agent_detail = None;
        self.child_transcript = None;
        self.child_scrollback = None;
        self.child_scrollback_pane.clear();
        self.child_scrollback_state.reset();
        self.child_mouse_scroll.cancel_stream();
        self.child_history_at = None;
    }

    fn close_agent_overlay(&mut self) {
        self.clear_agent_detail();
        self.shell.close_overlay();
    }

    fn scroll_child_transcript(&mut self, delta: isize) {
        if delta.is_negative() {
            self.child_scrollback_state
                .scroll_up(delta.unsigned_abs().min(u16::MAX as usize) as u16);
        } else {
            self.child_scrollback_state
                .scroll_down((delta as usize).min(u16::MAX as usize) as u16);
        }
    }

    fn flush_copy(&mut self, terminal: &mut TerminalSurface) {
        let Some(text) = self.pending_copy.take() else {
            return;
        };
        // Grok `clipboard_write_with_route`: native and OSC 52 are both fired.
        let native = clipboard::system_clipboard_set(&text);
        let osc52 = if self.capabilities.osc52 {
            Some(terminal.copy_text(&text))
        } else {
            None
        };
        let result = clipboard::merge_copy_legs(native, osc52);
        self.status = Some(result.message.into());
    }

    fn dispatch_local_slash_command(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> bool {
        match crate::slash::dispatch_with_models(self.prompt.text(), &self.models) {
            crate::slash::DispatchResult::NotLocal => false,
            crate::slash::DispatchResult::InvalidUsage(usage) => {
                self.status = Some(format!("Usage: {usage}"));
                true
            }
            crate::slash::DispatchResult::Error(message) => {
                self.status = Some(message);
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::ShowSessionPicker) => {
                self.open_resume_picker(transport, session);
                self.prompt.reset();
                self.slash.reset();
                self.prompt_history_index = None;
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::Login) => {
                self.open_login(transport, session);
                self.prompt.reset();
                self.slash.reset();
                self.prompt_history_index = None;
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::ToggleTimestamps) => {
                let enabled = !self
                    .timestamps_enabled
                    .unwrap_or(GrokAppearanceSnapshot::default().show_timestamps);
                self.set_timestamps_enabled(enabled);
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::SetTimestamps(enabled)) => {
                self.set_timestamps_enabled(enabled);
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::NewSession) => {
                match self.start_new_session(transport, session) {
                    Ok(()) => {}
                    Err(error) => self.status = Some(format!("New session failed: {error}")),
                }
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::ShowModelPicker) => {
                self.open_model_picker(transport, session);
                self.prompt.reset();
                self.slash.reset();
                self.prompt_history_index = None;
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::SetDefaultModel(id)) => {
                self.select_session_model(transport, session, id, None);
                true
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::SwitchModel {
                model_id,
                effort,
            }) => {
                self.select_session_model(transport, session, model_id, effort);
                true
            }
        }
    }

    fn dispatch_prompt_submission(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        if self.dispatch_local_slash_command(transport, session) {
            return Ok(());
        }
        let line = self.prompt.text().to_string();
        if !line.starts_with('/') {
            return self.submit_prompt(transport, session);
        }
        if crate::slash::is_host_command(&line, &self.command_catalog) {
            return self.submit_host_command(transport, session, line);
        }

        let command = crate::slash::parse_invocation(&line)
            .map(|invocation| format!("/{}", invocation.token))
            .unwrap_or_else(|| line.clone());
        self.status = Some(format!(
            "Unknown or unavailable slash command: {command}; refreshing command list"
        ));
        self.request_commands(transport, session);
        Ok(())
    }

    fn submit_host_command(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        line: String,
    ) -> PagerResult<()> {
        if self.pending_agent_preset_switch.is_some() {
            self.status =
                Some("Agent preset is switching; wait for the Host before sending".into());
            return Ok(());
        }
        if self.pending_first_prompt.is_some() {
            self.status = Some("First prompt is still waiting for Host admission".into());
            return Ok(());
        }
        if let Some(pending) = self.pending_host_command.as_ref() {
            self.status = Some(format!(
                "Command is still running: {} ({})",
                pending.line, pending.operation.request_id
            ));
            return Ok(());
        }
        if let Some(pending) = self.pending_permission.as_ref() {
            self.status = Some(format!(
                "Permission switch to {} is still running ({})",
                pending.target, pending.operation.request_id
            ));
            return Ok(());
        }
        let receipt = self.submit_effect(
            transport,
            UiIntent::ExecuteCommand { line: line.clone() },
            &UiContext::from_session(session),
        )?;
        if matches!(
            receipt.status,
            UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
        ) {
            self.pending_host_command = Some(PendingHostCommand {
                operation: receipt.operation,
                line: line.clone(),
            });
            self.status = Some(format!("Running command: {line}"));
        } else {
            self.status = Some(receipt_status_message(&receipt, "Command"));
        }
        Ok(())
    }

    fn set_timestamps_enabled(&mut self, enabled: bool) {
        self.timestamps_enabled = Some(enabled);
        self.prompt.reset();
        self.slash.reset();
        self.prompt_history_index = None;
        self.status = Some(format!("Timestamps {}", if enabled { "on" } else { "off" }));
    }

    fn request_session_models(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        revision: u64,
    ) {
        self.models_for_session = Some(session.session_id().to_string());
        let context = UiContext::from_session(session);
        let _ = self.submit_effect(
            transport,
            UiIntent::ListSessionModels { revision },
            &context,
        );
    }

    fn request_commands(&mut self, transport: &mut RpcTransport, session: &SessionState) {
        self.command_catalog.clear();
        self.commands_for_session = Some(session.session_id().to_string());
        self.command_catalog_revision = self.command_catalog_revision.saturating_add(1);
        let revision = self.command_catalog_revision;
        if let Err(error) = self.submit_effect(
            transport,
            UiIntent::ListCommands { revision },
            &UiContext::from_session(session),
        ) {
            self.status = Some(format!("Command list failed: {error}"));
        }
    }

    fn open_model_picker(&mut self, transport: &mut RpcTransport, session: &SessionState) {
        self.picker_kind = PickerKind::Model;
        self.shell.open_picker();
        let revision = self.model_picker.open(&self.models);
        self.request_session_models(transport, session, revision);
        self.status = None;
    }

    fn open_login(&mut self, transport: &mut RpcTransport, session: &SessionState) {
        if self
            .pending_login
            .as_ref()
            .is_some_and(|pending| pending.kind == PendingLoginKind::Set)
        {
            self.status = Some("DeepSeek API key save is still pending".into());
            return;
        }
        self.shell.open_login();
        self.login.open(DEEPSEEK_LOGIN_PROVIDER);
        let provider = self.login.provider();
        match self.submit_effect(
            transport,
            UiIntent::DescribeCredential {
                provider_id: provider.id.to_string(),
                credential_ref: provider.credential_ref.to_string(),
            },
            &UiContext::from_session(session),
        ) {
            Ok(receipt) if receipt.status == UiEffectStatus::Pending => {
                self.pending_login = Some(PendingLogin {
                    operation: receipt.operation,
                    kind: PendingLoginKind::Describe,
                });
                self.status = None;
            }
            Ok(receipt) => {
                let message = receipt_status_message(&receipt, "Credential status");
                self.login.fail(message.clone());
                self.status = Some(message);
            }
            Err(error) => {
                let message = error.to_string();
                self.login.fail(message.clone());
                self.status = Some(format!("Credential status failed: {message}"));
            }
        }
    }

    fn handle_login_event(
        &mut self,
        event: Event,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> PagerResult<()> {
        match self.login.handle_event(event) {
            LoginOutcome::Close => {
                let saving = self
                    .pending_login
                    .as_ref()
                    .is_some_and(|pending| pending.kind == PendingLoginKind::Set);
                if !saving {
                    self.pending_login = None;
                }
                self.shell.close_overlay();
                self.status = Some(if saving {
                    "DeepSeek API key save is still pending".into()
                } else {
                    "Login canceled".into()
                });
            }
            LoginOutcome::Submit(value) => {
                if self.pending_login.is_some() {
                    return Ok(());
                }
                let provider = self.login.provider();
                let submission = self.submit_effect(
                    transport,
                    UiIntent::SetCredential {
                        provider_id: provider.id.to_string(),
                        credential_ref: provider.credential_ref.to_string(),
                        value: SensitiveString::new(value),
                    },
                    &UiContext::from_session(session),
                );
                match submission {
                    Ok(receipt) if receipt.status == UiEffectStatus::Pending => {
                        self.login.mark_saving();
                        self.pending_login = Some(PendingLogin {
                            operation: receipt.operation,
                            kind: PendingLoginKind::Set,
                        });
                        self.status = Some("Saving DeepSeek API key…".into());
                    }
                    Ok(receipt) => {
                        let message = receipt_status_message(&receipt, "DeepSeek API key");
                        self.login.fail(message.clone());
                        self.status = Some(message);
                    }
                    Err(error) => {
                        let message = error.to_string();
                        self.login.fail(message.clone());
                        self.status = Some(format!("Couldn't save DeepSeek API key: {message}"));
                    }
                }
            }
            LoginOutcome::Changed => self.status = None,
            LoginOutcome::Unchanged => {}
        }
        Ok(())
    }

    fn apply_model_slash(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        command: &str,
    ) {
        match crate::slash::dispatch_with_models(command, &self.models) {
            crate::slash::DispatchResult::Action(crate::slash::Action::SetDefaultModel(id)) => {
                self.select_session_model(transport, session, id, None);
            }
            crate::slash::DispatchResult::Action(crate::slash::Action::SwitchModel {
                model_id,
                effort,
            }) => {
                self.select_session_model(transport, session, model_id, effort);
            }
            crate::slash::DispatchResult::Error(message) => {
                self.status = Some(message);
            }
            crate::slash::DispatchResult::InvalidUsage(usage) => {
                self.status = Some(format!("Usage: {usage}"));
            }
            _ => {
                self.status = Some(format!("Unknown model: {command}"));
            }
        }
    }

    fn select_session_model(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        model_id: ModelId,
        effort: Option<String>,
    ) {
        self.prompt.reset();
        self.slash.reset();
        self.prompt_history_index = None;
        self.pending_model = Some(model_id.clone());
        // Grok SetDefaultModel is optimistic; keep caption in sync while RPC is in flight.
        if effort.is_none() {
            self.models.set_current(model_id.clone(), None);
        }
        match self.submit_effect(
            transport,
            UiIntent::SelectSessionModel {
                provider: model_id.provider.clone(),
                model: model_id.model.clone(),
                reasoning_effort: effort,
            },
            &UiContext::from_session(session),
        ) {
            Ok(receipt)
                if matches!(
                    receipt.status,
                    UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
                ) =>
            {
                let display = self.models.display_name_for(&model_id);
                self.status = Some(format!("Switching to {display}…"));
            }
            Ok(receipt) => {
                self.pending_model = None;
                self.status = Some(format!(
                    "Couldn't switch model: {}",
                    receipt_status_message(&receipt, "Model")
                ));
            }
            Err(error) => {
                self.pending_model = None;
                self.status = Some(format!("Couldn't switch model: {error}"));
            }
        }
    }

    fn open_preset_picker(&mut self, transport: &mut RpcTransport, session: &SessionState) {
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        self.reconcile_preset_session(&snapshot);
        if !self.preset_editable(&snapshot) {
            self.status = Some(self.preset_fixed_message());
            return;
        }
        self.picker_kind = PickerKind::AgentPreset;
        self.shell.open_picker();
        let revision = self.preset_picker.open(
            self.pending_agent_preset
                .as_deref()
                .or(snapshot.agent_preset.as_deref()),
        );
        if !self.agent_preset_roster.is_empty() {
            let _ = self
                .preset_picker
                .apply_entries(revision, self.agent_preset_roster.clone());
        }
        let context = UiContext::from_session(session);
        match self.submit_effect(transport, UiIntent::ListAgentPresets { revision }, &context) {
            Ok(_) => self.status = None,
            Err(error) => {
                let message = format!("Agent preset list failed: {error}");
                let _ = self.preset_picker.fail_entries(revision, message.clone());
                self.status = Some(message);
            }
        }
    }

    fn select_agent_preset(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
        agent_preset: &str,
    ) {
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        self.reconcile_preset_session(&snapshot);
        if !self.preset_editable(&snapshot) {
            self.status = Some(self.preset_fixed_message());
            return;
        }
        let previous = self
            .pending_agent_preset
            .clone()
            .or_else(|| snapshot.agent_preset.clone());
        if previous.as_deref() == Some(agent_preset) {
            self.preset_picker.close();
            self.shell.close_overlay();
            self.status = Some(format!(
                "Preset already {}",
                current_agent_preset_label(Some(agent_preset), &self.agent_preset_roster)
            ));
            return;
        }
        self.pending_agent_preset = Some(agent_preset.to_string());
        match self.submit_effect(
            transport,
            UiIntent::SelectAgentPreset {
                agent_preset: agent_preset.to_string(),
            },
            &UiContext::from_session(session),
        ) {
            Ok(receipt)
                if matches!(
                    receipt.status,
                    UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
                ) =>
            {
                self.pending_agent_preset_switch = Some(PendingAgentPresetSwitch {
                    operation: receipt.operation,
                    previous,
                });
                let label =
                    current_agent_preset_label(Some(agent_preset), &self.agent_preset_roster);
                self.preset_picker.close();
                self.shell.close_overlay();
                self.status = Some(format!("Switching preset → {label}…"));
            }
            Ok(receipt) => {
                self.pending_agent_preset = previous;
                self.status = Some(receipt_status_message(&receipt, "Agent preset"));
            }
            Err(error) => {
                self.pending_agent_preset = previous;
                self.status = Some(format!("Agent preset failed: {error}"));
            }
        }
    }

    fn start_new_session(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<()> {
        self.start_new_session_with_preset(transport, session, None)
    }

    fn start_new_session_with_preset(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        preset_override: Option<&str>,
    ) -> PagerResult<()> {
        let cwd = transport
            .control_plane()
            .snapshot(session.session_id())
            .and_then(|snapshot| snapshot.cwd.clone())
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| ".".to_string());
        let requested = preset_override
            .map(str::to_string)
            .or_else(|| std::env::var("DSH_PAGER_PRESET").ok())
            .or_else(|| std::env::var("DSH_TUI_PRESET").ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let created = create_blank_session(transport, &cwd, requested.as_deref())?;
        match load_session_id(transport, session.generation(), created.session_id.clone()) {
            Ok(next) => {
                *session = next;
                self.reset_transcript_view();
                self.bind_preset_session(session, created.agent_preset.or(requested));
                self.models = ModelState::default();
                self.models_for_session = None;
                self.pending_model = None;
                self.command_catalog.clear();
                self.commands_for_session = None;
                self.command_catalog_revision = self.command_catalog_revision.saturating_add(1);
                self.model_picker.close();
                self.welcome_animation.observe_session(session.session_id());
                self.prompt.reset();
                self.slash.reset();
                self.prompt_history_index = None;
                self.preset_picker.close();
                self.resume_picker.close();
                self.shell.close_overlay();
                let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                    session,
                    Some(transport.control_plane()),
                );
                self.status = Some(format!(
                    "New session · {}",
                    self.agent_preset_label(&snapshot)
                ));
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn open_resume_picker(&mut self, transport: &mut RpcTransport, session: &SessionState) {
        self.picker_kind = PickerKind::Resume;
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
        if self.pending_agent_preset_switch.is_some() {
            self.status =
                Some("Agent preset is switching; wait for the Host before sending".into());
            return Ok(());
        }
        if self.pending_first_prompt.is_some() {
            self.status = Some("First prompt is still waiting for Host admission".into());
            return Ok(());
        }
        if let Some(pending) = self.pending_host_command.as_ref() {
            self.status = Some(format!(
                "Command is still running: {} ({})",
                pending.line, pending.operation.request_id
            ));
            return Ok(());
        }
        if let Some(pending) = self.pending_permission.as_ref() {
            self.status = Some(format!(
                "Permission switch to {} is still running ({})",
                pending.target, pending.operation.request_id
            ));
            return Ok(());
        }
        let text = self.prompt.text().to_string();
        if text.trim().is_empty() {
            self.status = Some("Prompt is empty".into());
            return Ok(());
        }
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        self.reconcile_preset_session(&snapshot);
        let context = UiContext::from_session(session);
        let receipt = self.submit_effect(
            transport,
            UiIntent::SubmitPrompt {
                text: text.clone(),
                mode: PromptMode::Steer,
            },
            &context,
        )?;
        if snapshot.session_blank
            && matches!(
                receipt.status,
                UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending
            )
        {
            self.pending_first_prompt = Some(receipt.operation.clone());
        }
        if prompt_receipt_admitted(&receipt.status) {
            self.record_prompt_history(&text);
            self.prompt.reset();
            self.slash.reset();
            self.status = Some(prompt_admission_message(&receipt.status));
        } else {
            self.status = Some(receipt_status_message(&receipt, "Prompt"));
        }
        Ok(())
    }

    fn refresh_slash(&mut self, snapshot: &GrokHostSnapshot) {
        self.slash.refresh(
            self.prompt.text(),
            self.prompt.cursor(),
            &self.models,
            &self.command_catalog,
            &snapshot.controls.permission.options,
        );
    }

    /// Grok `AgentView::handle_prompt_key` slash-first input tranche.
    fn handle_slash_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        snapshot: &GrokHostSnapshot,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> PagerResult<bool> {
        if !self.slash.is_open() {
            return Ok(false);
        }
        match key.code {
            KeyCode::PageUp => {
                self.slash
                    .scroll_selection(-(crate::slash::MAX_VISIBLE_SUGGESTIONS as isize));
                return Ok(true);
            }
            KeyCode::PageDown => {
                self.slash
                    .scroll_selection(crate::slash::MAX_VISIBLE_SUGGESTIONS as isize);
                return Ok(true);
            }
            KeyCode::Up => {
                self.slash.move_selection(-1);
                return Ok(true);
            }
            KeyCode::Down => {
                self.slash.move_selection(1);
                return Ok(true);
            }
            KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                self.slash.move_selection(-1);
                return Ok(true);
            }
            KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
                self.slash.move_selection(1);
                return Ok(true);
            }
            KeyCode::Esc => {
                self.slash.dismiss(self.prompt.text());
                self.status = None;
                return Ok(true);
            }
            KeyCode::Tab => {
                if let Some(accepted) = self.slash.accepted_text(self.prompt.text()) {
                    let _ = self.prompt.replace_text(&accepted);
                    self.refresh_slash(snapshot);
                    self.status = None;
                }
                return Ok(true);
            }
            KeyCode::Enter if key.modifiers.is_empty() => {
                if self.slash.typed_complete_selected(self.prompt.text()) {
                    self.slash.close();
                    self.dispatch_prompt_submission(transport, session)?;
                    return Ok(true);
                }
                let chains = self.slash.selected_chains();
                if let Some(accepted) = self.slash.accepted_text(self.prompt.text()) {
                    let _ = self.prompt.replace_text(&accepted);
                    self.refresh_slash(snapshot);
                    self.status = None;
                    if chains {
                        return Ok(true);
                    }
                    self.slash.close();
                    self.dispatch_prompt_submission(transport, session)?;
                }
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
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
                self.slash.reset();
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
        self.scrollback_state.reset();
        self.geometry_lines.clear();
        self.last_transcript_click = None;
        self.selected_transcript = None;
        self.selection.clear();
        self.hover_link = None;
        self.transcript_mouse_pos = None;
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
                self.bind_preset_session(session, None);
                self.command_catalog.clear();
                self.commands_for_session = None;
                self.command_catalog_revision = self.command_catalog_revision.saturating_add(1);
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
            .and_then(|id| {
                snapshot
                    .queue
                    .iter()
                    .find(|item| item.id == id && queue_item_is_visible(item))
            })
            .or_else(|| visible_queue_items(&snapshot.queue).next())
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

    fn current_question_has_options(&self, interaction: &DshInteraction) -> bool {
        question_state(
            interaction,
            self.interaction_question_index,
            self.interaction_selected,
            false,
            self.interaction_args_expanded,
        )
        .is_some_and(|state| !state.options.is_empty())
    }

    fn save_current_question_draft(&mut self, has_options: bool, explicit: bool) {
        let custom = self.interaction_editor.text().to_string();
        let Some(draft) = self
            .interaction_answer_drafts
            .get_mut(self.interaction_question_index)
        else {
            return;
        };
        draft.selected_option = self.interaction_selected;
        if has_options {
            if explicit {
                draft.answered = true;
            }
        } else {
            draft.custom = custom;
            draft.answered = !draft.custom.trim().is_empty();
        }
    }

    fn load_question_draft(&mut self, question_index: usize) {
        if self.interaction_answer_drafts.is_empty() {
            return;
        }
        self.interaction_question_index =
            question_index.min(self.interaction_answer_drafts.len().saturating_sub(1));
        let draft = &self.interaction_answer_drafts[self.interaction_question_index];
        self.interaction_selected = draft.selected_option;
        let _ = self.interaction_editor.replace_text(&draft.custom);
        self.interaction_args_expanded = false;
        self.hovered_permission_item = None;
        self.last_permission_click = None;
    }

    fn move_question(&mut self, interaction: &DshInteraction, backwards: bool) {
        let total = self.interaction_answer_drafts.len();
        if total <= 1 {
            return;
        }
        let has_options = self.current_question_has_options(interaction);
        self.save_current_question_draft(has_options, false);
        let next = if backwards {
            if self.interaction_question_index == 0 {
                total - 1
            } else {
                self.interaction_question_index - 1
            }
        } else {
            (self.interaction_question_index + 1) % total
        };
        self.load_question_draft(next);
        self.status = Some(format!("Question {}/{}", next + 1, total));
    }

    fn complete_question_or_submit(
        &mut self,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        interaction: &DshInteraction,
    ) -> PagerResult<()> {
        let has_options = self.current_question_has_options(interaction);
        self.save_current_question_draft(has_options, true);
        let total = self.interaction_answer_drafts.len();
        if total == 0 {
            self.status = Some("Question has no answerable items".into());
            return Ok(());
        }
        if let Some(next) = (1..=total)
            .map(|offset| (self.interaction_question_index + offset) % total)
            .find(|index| !self.interaction_answer_drafts[*index].answered)
        {
            self.load_question_draft(next);
            let answered = self
                .interaction_answer_drafts
                .iter()
                .filter(|draft| draft.answered)
                .count();
            self.status = Some(format!(
                "Question {}/{} · {answered}/{total} answered",
                next + 1,
                total
            ));
            return Ok(());
        }
        let Some(response) = response_for(interaction, &self.interaction_answer_drafts) else {
            self.status = Some("Complete every question before submitting".into());
            return Ok(());
        };
        self.submit_interaction(transport, session, interaction, response)
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
        self.handle_question_key(key, transport, session, interaction)
    }

    fn handle_question_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        transport: &mut RpcTransport,
        session: &mut SessionState,
        interaction: &DshInteraction,
    ) -> PagerResult<()> {
        if key.code == KeyCode::Esc {
            self.shell.park_permission();
            self.status = Some(if self.interaction_pending.is_some() {
                "Question response pending; focus moved to scrollback".into()
            } else {
                "Question parked; press Enter or i to return".into()
            });
            return Ok(());
        }
        if self.interaction_pending.is_some() {
            return Ok(());
        }
        let Some(mut state) = question_state(
            interaction,
            self.interaction_question_index,
            self.interaction_selected,
            false,
            self.interaction_args_expanded,
        ) else {
            return Ok(());
        };
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
            if state.has_collapsible_display(self.permission_area.width.saturating_sub(5) as usize)
            {
                self.interaction_args_expanded = !self.interaction_args_expanded;
            }
            return Ok(());
        }
        let count = state.options.len();
        let question_count = self.interaction_answer_drafts.len();
        match key.code {
            KeyCode::Tab if question_count > 1 => {
                self.move_question(interaction, false);
            }
            KeyCode::BackTab if question_count > 1 => {
                self.move_question(interaction, true);
            }
            KeyCode::Tab if count > 0 => {
                self.interaction_selected = (self.interaction_selected + 1) % count;
            }
            KeyCode::BackTab if count > 0 => {
                self.interaction_selected = if self.interaction_selected == 0 {
                    count - 1
                } else {
                    self.interaction_selected - 1
                };
            }
            KeyCode::Up | KeyCode::Char('k') if count > 0 => {
                self.interaction_selected = self.interaction_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') if count > 0 => {
                self.interaction_selected = self
                    .interaction_selected
                    .saturating_add(1)
                    .min(count.saturating_sub(1));
            }
            KeyCode::Char(digit @ '1'..='9') if count > 0 => {
                let index = digit.to_digit(10).unwrap_or(1) as usize - 1;
                if index < count {
                    self.interaction_selected = index;
                    self.complete_question_or_submit(transport, session, interaction)?;
                }
            }
            KeyCode::Enter => {
                if count == 0 {
                    if self.interaction_editor.text().trim().is_empty() {
                        self.status = Some("Answer is empty".into());
                    } else {
                        self.complete_question_or_submit(transport, session, interaction)?;
                    }
                } else {
                    state.active_idx = self.interaction_selected;
                    state.clamp_selection();
                    self.interaction_selected = state.active_idx;
                    self.complete_question_or_submit(transport, session, interaction)?;
                }
            }
            _ if count == 0 => {
                let _ = self.interaction_editor.handle_key(&key);
            }
            _ => {}
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
        // Grok's approval overlay keeps the global Ctrl+O YOLO toggle
        // reachable before the pending-response guard.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            return self.toggle_yolo(transport, session);
        }
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
                    self.submit_approval(transport, session, interaction, choice)?;
                }
            }
            KeyCode::Enter => {
                permission.active_idx = self.interaction_selected;
                permission.clamp_selection();
                if let Some(choice) = permission.selected() {
                    self.submit_approval(transport, session, interaction, choice)?;
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
        self.handle_question_mouse(mouse, transport, session, interaction)
    }

    fn handle_question_mouse(
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
                        self.complete_question_or_submit(transport, session, interaction)?;
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
                        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
                            session,
                            Some(transport.control_plane()),
                        );
                        let Some(permission) =
                            permission_state(interaction, &snapshot.transcript, index, false)
                        else {
                            return Ok(());
                        };
                        if let Some(choice) =
                            permission.options.get(index).map(|option| option.choice)
                        {
                            self.submit_approval(transport, session, interaction, choice)?;
                        }
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
        choice: PermissionChoice,
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
                outcome: approval_outcome(choice).into(),
            },
        )?;
        Ok(())
    }

    fn toggle_yolo(
        &mut self,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> PagerResult<()> {
        if let Some(pending) = self.pending_host_command.as_ref() {
            self.status = Some(format!(
                "Command is still running: {} ({})",
                pending.line, pending.operation.request_id
            ));
            return Ok(());
        }
        if let Some(pending) = self.pending_permission.as_ref() {
            self.status = Some(format!(
                "Permission switch to {} is already pending ({})",
                pending.target, pending.operation.request_id
            ));
            return Ok(());
        }
        let snapshot = GrokHostSnapshot::from_session_with_control_plane(
            session,
            Some(transport.control_plane()),
        );
        let target = next_permission_preset(snapshot.controls.permission.current_value.as_deref());
        if !snapshot.controls.permission.supports(target) {
            self.status = Some(format!(
                "Permission preset {target} is unavailable from the host"
            ));
            return Ok(());
        }
        let receipt = self.submit_effect(
            transport,
            UiIntent::SetPermissionPreset {
                preset: target.to_string(),
            },
            &UiContext::from_session(session),
        )?;
        match receipt.status {
            UiEffectStatus::Accepted | UiEffectStatus::Queued | UiEffectStatus::Pending => {
                self.pending_permission = Some(PendingPermissionSwitch {
                    operation: receipt.operation.clone(),
                    target: target.to_string(),
                });
                self.status = Some(format!("Permission → {target}"));
            }
            _ => {
                self.status = Some(receipt_status_message(&receipt, "Permission preset"));
            }
        }
        Ok(())
    }

    fn reconcile_permission(&mut self, snapshot: &GrokHostSnapshot) {
        let Some(pending) = self.pending_permission.as_ref() else {
            return;
        };
        if snapshot.controls.permission.current_value.as_deref() == Some(pending.target.as_str()) {
            let enabled = pending.target == YOLO_PRESET;
            self.pending_permission = None;
            self.status = Some(if enabled {
                "YOLO enabled".into()
            } else {
                "YOLO disabled; permission restored to workspace-write".into()
            });
        }
    }

    fn effective_permission_preset<'a>(&'a self, snapshot: &'a GrokHostSnapshot) -> &'a str {
        self.pending_permission
            .as_ref()
            .map(|pending| pending.target.as_str())
            .or(snapshot.controls.permission.current_value.as_deref())
            .unwrap_or("unknown")
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

    fn handle_preset_picker_event(
        &mut self,
        event: Event,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> bool {
        match self.preset_picker.handle_event(event) {
            PresetPickerOutcome::Closed => {
                self.preset_picker.close();
                self.shell.close_overlay();
                self.status = Some("Preset picker closed".into());
                false
            }
            PresetPickerOutcome::Selected(id) => {
                self.select_agent_preset(transport, session, &id);
                false
            }
            PresetPickerOutcome::Changed => {
                self.status = None;
                false
            }
            PresetPickerOutcome::Unchanged => false,
        }
    }

    fn handle_model_picker_event(
        &mut self,
        event: Event,
        transport: &mut RpcTransport,
        session: &SessionState,
    ) -> bool {
        match self.model_picker.handle_event(event, &self.models) {
            ModelPickerOutcome::Closed => {
                self.model_picker.close();
                self.shell.close_overlay();
                self.status = Some("Model picker closed".into());
                false
            }
            ModelPickerOutcome::Submit(command) => {
                self.model_picker.close();
                self.shell.close_overlay();
                self.apply_model_slash(transport, session, &command);
                false
            }
            ModelPickerOutcome::Changed => {
                self.status = None;
                false
            }
            ModelPickerOutcome::Unchanged => false,
        }
    }

    fn handle_picker_event(
        &mut self,
        event: Event,
        transport: &mut RpcTransport,
        session: &mut SessionState,
    ) -> bool {
        if self.picker_kind == PickerKind::AgentPreset {
            return self.handle_preset_picker_event(event, transport, session);
        }
        if self.picker_kind == PickerKind::Model {
            return self.handle_model_picker_event(event, transport, session);
        }
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
                        self.bind_preset_session(session, None);
                        self.command_catalog.clear();
                        self.commands_for_session = None;
                        self.command_catalog_revision =
                            self.command_catalog_revision.saturating_add(1);
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

fn next_permission_preset(current: Option<&str>) -> &'static str {
    if current == Some(YOLO_PRESET) {
        DEFAULT_PERMISSION_PRESET
    } else {
        YOLO_PRESET
    }
}

/// Project DSH's user-message history into Grok's newest-first rewind list.
/// DSH `session.fork(atSeq)` rounds an anchor forward to the first completed
/// turn boundary, so the previous user message is the stable anchor that
/// retains the previous turn while excluding the selected one.
fn rewind_points(snapshot: &GrokHostSnapshot) -> Vec<RewindPoint> {
    let users = snapshot
        .transcript
        .iter()
        .filter(|row| row.kind == DshRenderKind::User)
        .collect::<Vec<_>>();
    let mut points = users
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let preview = row.text.split_whitespace().collect::<Vec<_>>().join(" ");
            RewindPoint {
                prompt_index: index,
                prompt_preview: if preview.is_empty() {
                    "(no preview)".into()
                } else {
                    preview
                },
                prompt_text: row.text.clone(),
                fork_at_seq: index
                    .checked_sub(1)
                    .map(|previous| DshSeq::new(users[previous].source_seq)),
            }
        })
        .collect::<Vec<_>>();
    points.reverse();
    points
}

#[cfg(test)]
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
    use super::next_permission_preset;
    use super::prompt_admission_message;
    use super::prompt_receipt_admitted;
    use super::render_file_search_content;
    use super::render_transcript_selection_box;
    use super::steer_capability_available;
    use super::{
        DEEPSEEK_LOGIN_PROVIDER, MediaPreviewBuffer, NOTIFICATION_BUDGET, PendingHostCommand,
        PendingLogin, PendingLoginKind, UiState, agent_overlay_close_click,
        agent_overlay_key_closes, register_model_label_click, render_agent_tasks_content,
        render_image_preview_content, rewind_points,
    };
    use crate::app::Overlay;
    use crate::effects::{
        OperationKey, SensitiveString, UiEffect, UiEffectCompletion, UiEffectStatus,
    };
    use crate::geometry::{GeometryLine, HitMap, HitRegion, HitTarget};
    use crate::host_adapter::{
        AgentSnapshot, FeatureStatus, FileSearchSnapshot, GrokHostSnapshot, MediaSnapshot,
    };
    use crate::modal_window_state::ModalWindowState;
    use crate::theme::Theme;
    use dsh_pager::{DshGeneration, DshRequestId};

    #[test]
    fn notification_batch_matches_grok_streaming_input_fairness_contract() {
        assert_eq!(NOTIFICATION_BUDGET, 32);
    }

    #[test]
    fn model_label_opens_only_on_a_bounded_second_click() {
        let started = std::time::Instant::now();
        let mut last_click = None;
        assert!(!register_model_label_click(&mut last_click, started));
        assert!(register_model_label_click(
            &mut last_click,
            started + std::time::Duration::from_millis(100)
        ));
        assert_eq!(last_click, None);

        assert!(!register_model_label_click(
            &mut last_click,
            started + std::time::Duration::from_secs(1)
        ));
        assert!(!register_model_label_click(
            &mut last_click,
            started + std::time::Duration::from_millis(1_451)
        ));
    }
    use crate::views::agent_panes::{AgentItemId, AgentPaneController};
    use crate::views::modal_window::{
        ModalSizing, ModalWindowConfig, Shortcut, render_modal_window,
    };
    use crate::views::picker::{PickerState, render_picker_in_modal};
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use dsh_pager::{
        ControlPlaneStore, DshInteraction, DshRenderEntryId, DshSeq, SessionState,
        scrollback::Scrollback,
    };
    use dsh_pager_protocol::{
        CommandDescriptor, HistoryEntry, JsonRpcNotification, SessionEvent, SessionHistoryValue,
    };
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};
    use serde_json::json;

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn agent_overlay_keys_close_without_waiting_on_rpc() {
        assert!(agent_overlay_key_closes(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE
        )));
        assert!(agent_overlay_key_closes(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE
        )));
        assert!(agent_overlay_key_closes(KeyEvent::new(
            KeyCode::Char('Q'),
            KeyModifiers::NONE
        )));
        assert!(agent_overlay_key_closes(KeyEvent::new(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL
        )));
        assert!(!agent_overlay_key_closes(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        )));
        let mut ui = UiState::default();
        ui.shell.open_agent_tasks();
        ui.agent_detail = Some(AgentItemId::Subagent("child".into()));
        ui.close_agent_overlay();
        assert_eq!(ui.shell.overlay(), Overlay::None);
        assert!(ui.agent_detail.is_none());
        assert!(ui.child_scrollback.is_none());
    }

    #[test]
    fn agent_overlay_close_click_accepts_button_and_title_corner() {
        let mut modal = ModalWindowState::default();
        modal.popup_area = Some(Rect::new(10, 4, 60, 20));
        modal.close_button_rect = Some(Rect::new(63, 4, 5, 1));
        assert!(agent_overlay_close_click(
            &modal,
            &mouse(MouseEventKind::Down(MouseButton::Left), 65, 4)
        ));
        assert!(agent_overlay_close_click(
            &modal,
            &mouse(MouseEventKind::Up(MouseButton::Left), 65, 4)
        ));
        assert!(agent_overlay_close_click(
            &modal,
            &mouse(MouseEventKind::Down(MouseButton::Left), 69, 4)
        ));
        assert!(agent_overlay_close_click(
            &modal,
            &mouse(MouseEventKind::Down(MouseButton::Left), 2, 2)
        ));
        assert!(!agent_overlay_close_click(
            &modal,
            &mouse(MouseEventKind::Down(MouseButton::Left), 30, 12)
        ));
    }

    #[test]
    fn demo_snapshot_keeps_host_data_out_of_grok_views() {
        let snapshot = GrokHostSnapshot::demo();
        assert_eq!(snapshot.model, "deepseek-reasoner");
        assert_eq!(snapshot.picker_entries().len(), 3);
    }

    #[test]
    fn rewind_points_are_newest_first_and_anchor_before_the_selected_turn() {
        let event = |seq: i64, event_type: &str, text: Option<&str>| HistoryEntry {
            event: SessionEvent {
                event_type: event_type.into(),
                seq,
                time: seq as f64,
                data: text.map_or_else(
                    || json!({}),
                    |text| {
                        json!({
                            "role": "user",
                            "content": [{"type": "text", "text": text}]
                        })
                    },
                ),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        };
        let mut session = SessionState::new("rewind".into(), 1);
        session
            .install_initial(SessionHistoryValue {
                events: vec![
                    event(0, "user/message", Some("alpha\nline")),
                    event(1, "assistant/message", None),
                    event(2, "turn/end", None),
                    event(3, "user/message", Some("bravo")),
                    event(4, "assistant/message", None),
                    event(5, "turn/end", None),
                ],
                has_more: false,
                projections: None,
            })
            .expect("history");
        let points = rewind_points(&GrokHostSnapshot::from_session(&session));
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].prompt_index, 1);
        assert_eq!(points[0].prompt_preview, "bravo");
        assert_eq!(points[0].fork_at_seq.map(DshSeq::get), Some(0));
        assert_eq!(points[1].prompt_index, 0);
        assert_eq!(points[1].prompt_preview, "alpha line");
        assert_eq!(points[1].prompt_text, "alpha\nline");
        assert_eq!(points[1].fork_at_seq, None);
    }

    #[test]
    fn dsh_prompt_replaces_the_upstream_mode_hint_with_grok_toggle_yolo() {
        let snapshot = GrokHostSnapshot::demo();
        let mut ui = UiState::default();
        ui.shell.focus_prompt();
        let (hints, _) = ui.pane_shortcut_hints(&snapshot, false);
        assert!(hints.iter().any(|hint| hint.label == "yolo"));
        assert!(!hints.iter().any(|hint| hint.label == "mode"));
    }

    #[test]
    fn blank_session_exposes_preset_and_yolo_as_separate_shortcuts() {
        let mut snapshot = GrokHostSnapshot::demo();
        snapshot.session_blank = true;
        let mut ui = UiState::default();
        ui.shell.focus_prompt();

        let (hints, _) = ui.pane_shortcut_hints(&snapshot, false);

        assert!(hints.iter().any(|hint| hint.label == "preset"));
        assert!(hints.iter().any(|hint| hint.label == "yolo"));
        assert!(!hints.iter().any(|hint| hint.label == "mode"));
        assert!(
            ui.prompt_flags(&snapshot, &Theme::current())[0]
                .text
                .ends_with(" ▾")
        );

        ui.preset_locked_locally = true;
        assert!(
            !ui.prompt_flags(&snapshot, &Theme::current())[0]
                .text
                .ends_with(" ▾")
        );
    }

    #[test]
    fn accepted_real_prompt_locks_preset_but_official_command_does_not() {
        let session = SessionState::new("blank-session".into(), 7);
        let turn_operation = crate::effects::OperationKey {
            session_id: dsh_pager::DshSessionId::new("blank-session"),
            generation: dsh_pager::DshGeneration::new(7),
            request_id: dsh_pager::DshRequestId::new("turn"),
            action: "submit".into(),
            dedupe_key: "submit:turn".into(),
        };
        let mut turn_ui = UiState {
            pending_first_prompt: Some(turn_operation.clone()),
            ..UiState::default()
        };
        turn_ui.apply_effect_completion(
            crate::effects::UiEffectCompletion {
                effect: crate::effects::UiEffect::SubmitPrompt {
                    operation: turn_operation.clone(),
                    text: "hello".into(),
                    mode: dsh_pager_protocol::PromptMode::Steer,
                },
                receipt: crate::effects::UiEffectReceipt {
                    status: UiEffectStatus::Accepted,
                    operation: turn_operation,
                    diagnostic: None,
                    retryable: Some(false),
                },
                session_list: None,
                session_search: None,
                forked_session_id: None,
                file_references: None,
                attachment_preview: None,
                commands: None,
                command_execution: None,
                agent_preset_list: None,
                selected_agent_preset: None,
                session_models: None,
                selected_model: None,
                credential_info: None,
            },
            &session,
        );
        assert!(turn_ui.preset_locked_locally);
        assert!(turn_ui.pending_first_prompt.is_none());

        let command_operation = crate::effects::OperationKey {
            session_id: dsh_pager::DshSessionId::new("blank-session"),
            generation: dsh_pager::DshGeneration::new(7),
            request_id: dsh_pager::DshRequestId::new("command"),
            action: "execute-command".into(),
            dedupe_key: "execute-command:command".into(),
        };
        let mut command_ui = UiState {
            pending_host_command: Some(PendingHostCommand {
                operation: command_operation.clone(),
                line: "/plan".into(),
            }),
            ..UiState::default()
        };
        let _ = command_ui.prompt.replace_text("/plan");
        command_ui.apply_effect_completion(
            crate::effects::UiEffectCompletion {
                effect: crate::effects::UiEffect::ExecuteCommand {
                    operation: command_operation.clone(),
                    line: "/plan".into(),
                },
                receipt: crate::effects::UiEffectReceipt {
                    status: UiEffectStatus::Accepted,
                    operation: command_operation,
                    diagnostic: None,
                    retryable: Some(false),
                },
                session_list: None,
                session_search: None,
                forked_session_id: None,
                file_references: None,
                attachment_preview: None,
                commands: None,
                command_execution: Some(dsh_pager_protocol::CommandExecution {
                    command_id: "plan".into(),
                    result: dsh_pager_protocol::CommandResultValue {
                        kind: dsh_pager_protocol::CommandResultKind::Success,
                        text: Some("Plan mode enabled".into()),
                        source_event_seq: None,
                    },
                }),
                agent_preset_list: None,
                selected_agent_preset: None,
                session_models: None,
                selected_model: None,
                credential_info: None,
            },
            &session,
        );
        assert!(!command_ui.preset_locked_locally);
        assert!(command_ui.pending_host_command.is_none());
        assert_eq!(command_ui.prompt.text(), "");
        assert_eq!(command_ui.status.as_deref(), Some("Plan mode enabled"));
    }

    #[test]
    fn yolo_toggle_restores_the_off_preset_without_touching_plan() {
        assert_eq!(next_permission_preset(None), "danger-full-access");
        assert_eq!(
            next_permission_preset(Some("workspace-write")),
            "danger-full-access"
        );
        assert_eq!(
            next_permission_preset(Some("danger-full-access")),
            "workspace-write"
        );
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
                forked_session_id: None,
                file_references: None,
                attachment_preview: None,
                commands: None,
                command_execution: None,
                agent_preset_list: None,
                selected_agent_preset: None,
                session_models: None,
                selected_model: None,
                credential_info: None,
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

    #[test]
    fn stale_cancel_failure_cannot_clear_the_latest_retry() {
        let session = SessionState::new("cancel-session".into(), 4);
        let operation = |request_id: &str| crate::effects::OperationKey {
            session_id: dsh_pager::DshSessionId::new("cancel-session"),
            generation: dsh_pager::DshGeneration::new(4),
            request_id: dsh_pager::DshRequestId::new(request_id),
            action: "cancel-session".into(),
            dedupe_key: format!("cancel-session:{request_id}"),
        };
        let stale = operation("cancel-1");
        let latest = operation("cancel-2");
        let mut ui = UiState {
            cancel_pending: Some(latest.clone()),
            status: Some("Cancellation retry requested".into()),
            ..UiState::default()
        };

        ui.apply_effect_completion(
            crate::effects::UiEffectCompletion {
                effect: crate::effects::UiEffect::CancelSession {
                    operation: stale.clone(),
                },
                receipt: crate::effects::UiEffectReceipt {
                    status: UiEffectStatus::Timeout,
                    operation: stale,
                    diagnostic: Some("old cancel timed out".into()),
                    retryable: Some(true),
                },
                session_list: None,
                session_search: None,
                forked_session_id: None,
                file_references: None,
                attachment_preview: None,
                commands: None,
                command_execution: None,
                agent_preset_list: None,
                selected_agent_preset: None,
                session_models: None,
                selected_model: None,
                credential_info: None,
            },
            &session,
        );

        assert_eq!(ui.cancel_pending.as_ref(), Some(&latest));
        assert_eq!(ui.status.as_deref(), Some("Cancellation retry requested"));
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
    fn empty_session_renders_cwd_and_whale_without_exposing_session_id() {
        let mut session = SessionState::new("private-session-id".into(), 1);
        session
            .install_initial(SessionHistoryValue {
                events: Vec::new(),
                has_more: false,
                projections: None,
            })
            .expect("empty session fixture");
        assert!(session.set_projection("cwd", 1, json!("/work/project")));
        assert!(session.set_projection("model", 1, json!("deepseek")));

        let control_plane = ControlPlaneStore::default();
        let mut ui = UiState::default();
        ui.models.set_current(
            crate::model_state::ModelId::new("deepseek-official", "DeepSeek-V4-Flash"),
            Some("high".into()),
        );
        let mut wide_terminal = Terminal::new(TestBackend::new(80, 24)).expect("wide terminal");
        wide_terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render wide welcome");
        let wide = buffer_text(wide_terminal.backend().buffer(), 80, 24);
        let wide_buffer = wide_terminal.backend().buffer();
        assert!(wide.contains("/work/project"));
        assert!(wide.contains("DeepSeek Harness"));
        assert!(wide.contains("Explore the uncharted!"));
        assert!(wide.contains("Shift+Tab preset"));
        assert!(wide.contains("preset"));
        assert!(wide.contains("dsv4 flash (high)"));
        assert!(ui.hit_map.regions().iter().any(|region| {
            matches!(&region.target, HitTarget::Overlay(name) if name == "agent-preset")
                && region.rect.width > 0
        }));
        assert!(ui.hit_map.regions().iter().any(|region| {
            matches!(&region.target, HitTarget::Overlay(name) if name == "model-label")
                && region.rect.width == "dsv4 flash (high)".len() as u16
        }));
        assert!(!wide.contains("private-session-id"));
        assert!(!wide.contains("No transcript events yet"));
        assert!(wide_buffer.content.iter().any(|cell| {
            cell.fg == ratatui::style::Color::Rgb(78, 111, 255)
                || cell.bg == ratatui::style::Color::Rgb(78, 111, 255)
        }));
        assert!(wide_buffer.content.iter().any(|cell| {
            cell.fg == ratatui::style::Color::Rgb(190, 225, 255)
                || cell.bg == ratatui::style::Color::Rgb(190, 225, 255)
        }));

        let mut compact_terminal =
            Terminal::new(TestBackend::new(40, 12)).expect("compact terminal");
        compact_terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render compact welcome");
        let compact = buffer_text(compact_terminal.backend().buffer(), 40, 12);
        assert!(compact.contains("DeepSeek Harness"));
        assert!(compact.contains("Shift+Tab preset"));
        assert!(compact.contains("dsv4 flash (high)"));
        assert!(!compact.contains("Explore the uncharted!"));
        assert!(!compact.contains("private-session-id"));
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
        assert!(!screen.contains("don't ask again"));
        assert!(screen.contains('◆'));
        assert!(screen.contains("List project files…"));
        assert!(ui.hit_map.regions().iter().any(|region| {
            matches!(&region.target, HitTarget::Overlay(name) if name == "turn-stop")
        }));
        assert!(!screen.contains("Interaction · host request"));
    }

    #[test]
    fn question_replaces_composer_with_grok_question_card() {
        let mut session = SessionState::new("question-session".into(), 4);
        session
            .install_initial(SessionHistoryValue {
                events: Vec::new(),
                has_more: false,
                projections: None,
            })
            .expect("empty session fixture");
        session
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "question/requested",
                    "sessionId": "question-session",
                    "requestId": "rpc-plan",
                    "questions": [{
                        "id": "plan-review",
                        "question": "Approve this plan and leave plan mode?",
                        "options": [
                            { "label": "Approve", "description": "Leave plan mode" },
                            { "label": "Keep planning", "description": "Stay in plan mode" }
                        ],
                        "detail": "# Plan\n\n- ship the HTML report",
                        "intent": { "kind": "plan-review", "approve": "Approve" }
                    }]
                })),
            })
            .expect("question fixture");
        session
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.host".into(),
                params: Some(json!({
                    "type": "host/session-status",
                    "sessionId": "question-session",
                    "generation": 4,
                    "running": true
                })),
            })
            .expect("running fixture");

        let control_plane = ControlPlaneStore::default();
        let mut ui = UiState::default();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
        terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render question");
        let screen = buffer_text(terminal.backend().buffer(), 80, 24);

        assert_eq!(ui.shell.overlay(), crate::app::Overlay::Interaction);
        assert_eq!(ui.shell.owner(), crate::app::KeyOwner::Interaction);
        assert_eq!(ui.permission_option_rows.len(), 2);
        assert!(screen.contains("Plan review"));
        assert!(screen.contains("Approve this plan and leave plan mode?"));
        assert!(screen.contains("1 (●) Approve"));
        assert!(screen.contains("2 (○) Keep planning"));
        assert!(screen.contains("Leave plan mode"));
        assert!(screen.contains('┃'));
        assert!(ui.hit_map.regions().iter().any(|region| {
            matches!(&region.target, HitTarget::Overlay(name) if name == "question")
        }));
        assert!(!screen.contains("Interaction · host request"));
        assert!(!screen.contains("answer:"));
    }

    #[test]
    fn multi_question_tabs_preserve_each_questions_local_choice() {
        let interaction = DshInteraction::Question {
            request_id: "rpc-multi".into(),
            questions: vec![
                json!({
                    "id": "target",
                    "header": "Connection target",
                    "question": "What should happen?",
                    "options": [{"label": "Inspect"}, {"label": "Build"}]
                }),
                json!({
                    "id": "auth",
                    "header": "Authentication",
                    "question": "How should SSH authenticate?",
                    "options": [{"label": "Key"}, {"label": "Password"}]
                }),
            ],
        };
        let mut ui = UiState {
            interaction_answer_drafts: vec![Default::default(); 2],
            ..UiState::default()
        };

        ui.interaction_selected = 1;
        ui.save_current_question_draft(true, true);
        ui.move_question(&interaction, false);
        assert_eq!(ui.interaction_question_index, 1);
        assert_eq!(ui.interaction_selected, 0);

        ui.interaction_selected = 1;
        ui.save_current_question_draft(true, true);
        ui.move_question(&interaction, false);
        assert_eq!(ui.interaction_question_index, 0);
        assert_eq!(ui.interaction_selected, 1);
        assert!(
            ui.interaction_answer_drafts
                .iter()
                .all(|draft| draft.answered)
        );

        let first = crate::views::interaction::question_state(
            &interaction,
            ui.interaction_question_index,
            ui.interaction_selected,
            false,
            false,
        )
        .expect("first question");
        assert_eq!(first.header.as_deref(), Some("Connection target · 1/2"));
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
                joiner_to_previous: None,
                rect: Rect::new(4, 2, 12, 1),
            },
            GeometryLine {
                target: target.clone(),
                line_index: 1,
                text: "second line".into(),
                joiner_to_previous: Some(" ".into()),
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
        assert_eq!(summary.copy_text, "◆ Run Query the current workspace");
        assert!(summary.line.to_string().starts_with("❙  ◆ Run "));
        assert!(summary.line.spans.iter().any(|span| {
            span.content == "❙  "
                && span.style.fg.is_some()
                && span.style.fg != Some(theme.accent_success)
        }));
        assert!(summary.line.spans.iter().any(|span| {
            span.content == "◆ "
                && span.style.fg.is_some()
                && span.style.fg != Some(theme.accent_success)
        }));
        let target = HitTarget::TranscriptEntry(id);
        let rect = Rect::new(3, 2, 60, 1);
        ui.hit_map.insert(HitRegion {
            target: target.clone(),
            rect,
            label: "◆ Run Query the current workspace".into(),
            link: None,
            priority: 10,
        });
        ui.geometry_lines.push(GeometryLine {
            target,
            line_index: 0,
            text: "◆ Run Query the current workspace".into(),
            joiner_to_previous: None,
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

        ui.scrollback_pane
            .sync(&mut scrollback, 80, *Theme::current());
        let expanded = ui
            .scrollback_pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .map(|line| line.copy_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(expanded.contains("◆ Run Query the current workspace"));
        assert!(!expanded.contains('⌄'));
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
    fn context_only_inbox_does_not_open_or_populate_the_queue_pane() {
        let mut session = SessionState::new("context-only-queue".into(), 1);
        session
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "session/queue",
                    "sessionId": "context-only-queue",
                    "items": [{
                        "id": "approval-policy-context",
                        "placement": "context",
                        "message": {
                            "role": "user",
                            "content": [{
                                "type": "text",
                                "text": "The approval policy changed from \"never\" to \"ask\" (changed by the user)."
                            }],
                            "source": { "kind": "plugin", "plugin": "user-approval" }
                        }
                    }]
                })),
            })
            .expect("context queue notification");
        let control_plane = ControlPlaneStore::default();
        let mut ui = UiState::default();
        let mut terminal =
            Terminal::new(TestBackend::new(100, 30)).expect("context queue terminal");

        terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render context-only queue");
        let screen = buffer_text(terminal.backend().buffer(), 100, 30);
        assert!(!screen.contains("Queue · revision"));
        assert!(!screen.contains("The approval policy changed"));
        assert!(ui.queue_selected_id.is_none());

        ui.shell.open_queue();
        terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render manually opened queue");
        let overlay = buffer_text(terminal.backend().buffer(), 100, 30);
        assert!(overlay.contains("Queue · host authority"));
        assert!(overlay.contains("No queued prompts"));
        assert!(!overlay.contains("The approval policy changed"));
        assert!(!overlay.contains("[context]"));
        assert!(ui.queue_selected_id.is_none());
    }

    #[test]
    fn slash_dropdown_paints_above_inline_queue_rows() {
        let mut session = SessionState::new("slash-over-queue".into(), 1);
        session
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "session/queue",
                    "sessionId": "slash-over-queue",
                    "items": [
                        {
                            "id": "queued-1",
                            "placement": "queued",
                            "message": {
                                "role": "user",
                                "content": [{"type": "text", "text": "queued one"}],
                                "source": {"kind": "user"}
                            }
                        },
                        {
                            "id": "queued-2",
                            "placement": "steering",
                            "message": {
                                "role": "user",
                                "content": [{"type": "text", "text": "queued two"}],
                                "source": {"kind": "user"}
                            }
                        }
                    ]
                })),
            })
            .expect("queue notification");
        let control_plane = ControlPlaneStore::default();
        let mut ui = UiState {
            command_catalog: vec![
                CommandDescriptor {
                    name: "permission".into(),
                    description: "Switch the permission preset".into(),
                    input: None,
                },
                CommandDescriptor {
                    name: "plan".into(),
                    description: "Enter or leave plan mode".into(),
                    input: None,
                },
            ],
            ..UiState::default()
        };
        let _ = ui.prompt.replace_text("/p");
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("slash terminal");
        terminal
            .draw(|frame| ui.render(frame, &mut session, &control_plane))
            .expect("render slash over queue");
        let screen = buffer_text(terminal.backend().buffer(), 100, 30);
        assert!(screen.contains("/permission"), "{screen}");
        assert!(screen.contains("/plan"), "{screen}");
        assert!(screen.contains("Enter or leave plan mode"), "{screen}");
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
    fn slash_controller_fuzzy_ranks_official_commands_and_keeps_local_builtins() {
        let mut ui = UiState::default();
        let snapshot = GrokHostSnapshot::demo();
        ui.command_catalog = vec![
            CommandDescriptor {
                name: "help".into(),
                description: "Show DSH help".into(),
                input: None,
            },
            CommandDescriptor {
                name: "history".into(),
                description: "Show DSH history".into(),
                input: None,
            },
            CommandDescriptor {
                name: "model".into(),
                description: "Official collision".into(),
                input: None,
            },
        ];
        let _ = ui.prompt.replace_text("/h");
        ui.refresh_slash(&snapshot);
        assert_eq!(
            ui.slash
                .snapshot()
                .matches
                .iter()
                .map(|row| row.display.as_str())
                .collect::<Vec<_>>(),
            vec!["/help", "/history"]
        );
        ui.slash.move_selection(1);
        assert_eq!(ui.slash.accepted_text("/h").as_deref(), Some("/history"));

        ui.command_catalog.clear();
        let _ = ui.prompt.replace_text("/r");
        ui.refresh_slash(&snapshot);
        assert_eq!(
            ui.slash
                .snapshot()
                .matches
                .iter()
                .map(|row| row.display.as_str())
                .collect::<Vec<_>>(),
            vec!["/resume"]
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

    fn four_subagent_history() -> Vec<HistoryEntry> {
        let mut events = vec![HistoryEntry {
            event: SessionEvent {
                event_type: "user/message".into(),
                seq: 0,
                time: 1.0,
                data: json!({
                    "source": { "kind": "user" },
                    "content": [{
                        "type": "text",
                        "text": "分析一下项目架构，最终给我一个 html，重点分析 dsh 插件，使用子agent"
                    }]
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        }];
        let children = [
            (
                "call_00_ENGpqIy3Lh6Rcf0CDW279308",
                "分析 Rust host runtime 层",
                "1f07b684-99a2-4215-bcd4-6b055a052726",
            ),
            (
                "call_01_1UrVFLY5LO6Qdb2y2mtD4417",
                "分析 grok-ui 前端层",
                "c38bbc5f-79af-4546-8d90-caf5136a7823",
            ),
            (
                "call_02_rU562acst7I26wccGsea6685",
                "深度分析 DSH 插件包",
                "f025ea7a-a3b3-4a1e-b105-d2630c6196cd",
            ),
            (
                "call_03_JzgljJkw8y5C9pdy3QFm0707",
                "分析治理文档与验证体系",
                "06f12c70-6573-4765-aa0e-f897159ad248",
            ),
        ];
        for (index, (call_id, title, child_id)) in children.iter().enumerate() {
            let seq = i64::try_from(index).expect("index") + 1;
            let prompt = format!(
                "{title}\n\n【约束】只能 read/glob/grep。\n\n```md\n## 报告\n- 结论\n```\n"
            );
            events.push(HistoryEntry {
                event: SessionEvent {
                    event_type: "tool/call".into(),
                    seq,
                    time: 2.0,
                    data: json!({
                        "turn": 1,
                        "step": 7,
                        "callId": call_id,
                        "name": "subagent",
                        "arguments": json!({
                            "description": title,
                            "prompt": prompt
                        })
                        .to_string()
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: Some(json!({
                    "for": "call",
                    "view": { "card": "generic", "title": title, "kind": "other" }
                })),
            });
            events.push(HistoryEntry {
                event: SessionEvent {
                    event_type: "tool/result".into(),
                    seq: seq + 4,
                    time: 3.0,
                    data: json!({
                        "turn": 1,
                        "step": 7,
                        "message": {
                            "source": { "kind": "tool", "callId": call_id },
                            "content": [{
                                "type": "tool-result",
                                "toolCallId": call_id,
                                "content": [{
                                    "type": "text",
                                    "text": format!("started subagent {child_id}")
                                }],
                                "isError": false
                            }]
                        }
                    }),
                    source_event_seqs: Some(vec![seq]),
                    surface_op: Some("append".into()),
                    ignorable: None,
                },
                view: None,
            });
        }
        events.sort_by_key(|entry| entry.event.seq);
        events
    }

    #[test]
    fn four_parallel_subagent_burst_renders_catalog_and_transcript() {
        crate::diag::install_panic_hook();
        let mut session =
            SessionState::new("session-d0b39f12-2b2d-49d3-a745-0093f5a68c85".into(), 1);
        session
            .install_initial(SessionHistoryValue {
                events: four_subagent_history(),
                has_more: false,
                projections: None,
            })
            .expect("four-subagent history");
        session
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.host".into(),
                params: Some(json!({
                    "type": "host/session-status",
                    "sessionId": session.session_id(),
                    "generation": 1,
                    "running": true
                })),
            })
            .expect("parent running");

        let mut store = ControlPlaneStore::default();
        store.set_generation(1);
        let children = [
            "1f07b684-99a2-4215-bcd4-6b055a052726",
            "c38bbc5f-79af-4546-8d90-caf5136a7823",
            "f025ea7a-a3b3-4a1e-b105-d2630c6196cd",
            "06f12c70-6573-4765-aa0e-f897159ad248",
        ];
        for child in children {
            store
                .apply_notification(&JsonRpcNotification {
                    jsonrpc: "2.0".into(),
                    method: "events.host".into(),
                    params: Some(json!({
                        "generation": 1,
                        "type": "host/session-added",
                        "sessionId": child,
                        "parentSessionId": session.session_id(),
                        "origin": "subagent",
                        "running": true
                    })),
                })
                .expect("child published");
        }
        store
            .apply_notification(&JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "generation": 1,
                    "type": "session/jobs",
                    "sessionId": session.session_id(),
                    "jobs": children.iter().enumerate().map(|(index, _child)| {
                        json!({
                            "id": format!("subagent-{index}"),
                            "kind": "subagent",
                            "label": format!("child {index}"),
                            "status": "running",
                            "startedAt": 20
                        })
                    }).collect::<Vec<_>>()
                })),
            })
            .expect("jobs");
        store.apply_subagent_list(
            session.session_id(),
            &serde_json::from_value(json!({
                "parentAvailable": true,
                "entries": children.iter().map(|child| {
                    json!({
                        "kind": "child",
                        "id": child,
                        "mode": "one-shot",
                        "activity": "running",
                        "hasChildren": false,
                        "label": child
                    })
                }).collect::<Vec<_>>()
            }))
            .expect("catalog"),
        );

        let snapshot = GrokHostSnapshot::from_session_with_control_plane(&session, Some(&store));
        assert!(snapshot.agent.subagents.len() >= 4, "{:?}", snapshot.agent);
        assert!(snapshot.agent.subagents.iter().all(|row| row.running));

        let mut ui = UiState::default();
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("test terminal");
        terminal
            .draw(|frame| ui.render(frame, &mut session, &store))
            .expect("render four-subagent burst");
        let screen = buffer_text(terminal.backend().buffer(), 100, 30);
        assert!(
            screen.contains("Subagents") || screen.to_ascii_lowercase().contains("subagent"),
            "top pane should show spawned subagents: {screen}"
        );
        assert!(
            ui.scrollback_pane.total_height(&mut session.scrollback) > 0,
            "transcript must keep the four tool calls"
        );
    }

    #[test]
    fn host_global_credential_completion_survives_session_generation_change() {
        let operation = OperationKey {
            session_id: dsh_pager::DshSessionId::new("old-session"),
            generation: DshGeneration::new(1),
            request_id: DshRequestId::new("credential-1"),
            action: "set-credential".into(),
            dedupe_key: "set-credential:deepseek:DEEPSEEK_API_KEY".into(),
        };
        let mut ui = UiState {
            pending_login: Some(PendingLogin {
                operation: operation.clone(),
                kind: PendingLoginKind::Set,
            }),
            ..UiState::default()
        };
        ui.shell.open_login();
        ui.login.open(DEEPSEEK_LOGIN_PROVIDER);
        ui.login.apply_info(dsh_pager_protocol::CredentialInfo {
            configured: false,
            source: None,
            writable: true,
        });
        ui.login.mark_saving();

        let current = SessionState::new("new-session".into(), 9);
        ui.apply_effect_completion(
            UiEffectCompletion {
                effect: UiEffect::SetCredential {
                    operation: operation.clone(),
                    provider_id: "deepseek".into(),
                    credential_ref: "DEEPSEEK_API_KEY".into(),
                    value: SensitiveString::new(String::new()),
                },
                receipt: crate::effects::UiEffectReceipt {
                    status: UiEffectStatus::Accepted,
                    operation,
                    diagnostic: None,
                    retryable: Some(false),
                },
                session_list: None,
                session_search: None,
                forked_session_id: None,
                file_references: None,
                attachment_preview: None,
                commands: None,
                command_execution: None,
                agent_preset_list: None,
                selected_agent_preset: None,
                session_models: None,
                selected_model: None,
                credential_info: None,
            },
            &current,
        );

        assert!(ui.pending_login.is_none());
        assert_eq!(ui.shell.overlay(), Overlay::None);
        assert_eq!(
            ui.status.as_deref(),
            Some("DeepSeek API key saved. The next request will use it.")
        );
    }
}
