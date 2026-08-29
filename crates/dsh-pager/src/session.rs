use std::collections::{BTreeMap, HashMap};

use dsh_pager_protocol::{
    HistoryEntry, JsonRpcNotification, SessionHistoryValue, SessionProjectionsBlock,
    SessionQueueItem,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{PagerError, PagerResult};
use crate::presentation::{DshInteraction, DshPresentationModel, DshQueueItem, DshRenderFinish};
use crate::scrollback::Scrollback;

#[derive(Debug, Clone, PartialEq)]
struct ProjectionCell {
    seq: i64,
    value: Value,
}

/// Result of applying one server notification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionUpdate {
    pub changed: bool,
    pub gap_detected: bool,
}

/// Connection lifecycle observed by the native client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionPhase {
    Connected,
    Reconnecting,
    BaselineRequired,
    Draining,
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: String,
    pub message: String,
}

/// Identity carried by an asynchronous pager operation.
///
/// A sequence number alone is not sufficient after a reconnect: the same
/// session can be observed through more than one transport generation.  The
/// native client keeps all three coordinates available to callers so a late
/// response can be rejected without relying on a view-array index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationToken {
    pub session_id: String,
    pub seq: Option<i64>,
    pub generation: u64,
}

/// Kind of server-owned interaction currently blocking a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InteractionKind {
    Approval,
    Question,
}

/// A pending approval or question delivered on the mux stream.
///
/// `request_id` is the original server-request correlation id. It is kept
/// separately from the domain approval id because `tui.respond` must echo the
/// former while the approval payload also carries the latter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingInteraction {
    pub request_id: String,
    pub kind: InteractionKind,
    #[serde(default)]
    pub approval_id: Option<String>,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub questions: Vec<Value>,
}

impl PendingInteraction {
    pub fn approval(
        request_id: String,
        approval_id: String,
        call_id: Option<String>,
        tool_name: Option<String>,
        reason: Option<String>,
    ) -> Self {
        Self {
            request_id,
            kind: InteractionKind::Approval,
            approval_id: Some(approval_id),
            call_id,
            tool_name,
            reason,
            questions: Vec::new(),
        }
    }

    pub fn question(request_id: String, questions: Vec<Value>) -> Self {
        Self {
            request_id,
            kind: InteractionKind::Question,
            approval_id: None,
            call_id: None,
            tool_name: None,
            reason: None,
            questions,
        }
    }
}

/// One loaded session window plus live event and projection state.
pub struct SessionState {
    session_id: String,
    generation: u64,
    history: Vec<HistoryEntry>,
    has_more: bool,
    subscribed_last_seq: Option<i64>,
    pending_events: BTreeMap<i64, HistoryEntry>,
    projections: HashMap<String, ProjectionCell>,
    running: bool,
    status_message: Option<String>,
    connection_phase: ConnectionPhase,
    diagnostics: Vec<Diagnostic>,
    queue: Vec<SessionQueueItem>,
    queue_revision: u64,
    pending_interactions: Vec<PendingInteraction>,
    pub scrollback: Scrollback,
}

impl SessionState {
    pub fn new(session_id: String, generation: u64) -> Self {
        Self {
            session_id,
            generation,
            history: Vec::new(),
            has_more: false,
            subscribed_last_seq: None,
            pending_events: BTreeMap::new(),
            projections: HashMap::new(),
            running: false,
            status_message: None,
            connection_phase: ConnectionPhase::BaselineRequired,
            diagnostics: Vec::new(),
            queue: Vec::new(),
            queue_revision: 0,
            pending_interactions: Vec::new(),
            scrollback: Scrollback::default(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn operation_token(&self, seq: Option<i64>) -> OperationToken {
        OperationToken {
            session_id: self.session_id.clone(),
            seq,
            generation: self.generation,
        }
    }

    /// Check that an asynchronous result still belongs to this exact view
    /// baseline.  Callers should use this before applying any local response
    /// state after a reconnect or session switch.
    pub fn accepts_operation(&self, token: &OperationToken) -> bool {
        token.session_id == self.session_id
            && token.generation == self.generation
            && token
                .seq
                .is_none_or(|seq| self.tail_seq().is_some_and(|tail| tail >= seq))
    }

    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    pub fn has_more(&self) -> bool {
        self.has_more
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn connection_phase(&self) -> ConnectionPhase {
        self.connection_phase
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn latest_diagnostic(&self) -> Option<&Diagnostic> {
        self.diagnostics.last()
    }

    /// Record a bounded diagnostic produced by a local UI operation.
    pub fn add_diagnostic(
        &mut self,
        level: DiagnosticLevel,
        code: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.record_diagnostic(level, code, message);
    }

    pub fn queue(&self) -> &[SessionQueueItem] {
        &self.queue
    }

    /// Monotonic local revision of authoritative queue snapshots.
    pub fn queue_revision(&self) -> u64 {
        self.queue_revision
    }

    pub fn queue_item(&self, item_id: &str) -> Option<&SessionQueueItem> {
        self.queue.iter().find(|item| item.id == item_id)
    }

    pub fn mark_reconnecting(&mut self, message: impl Into<String>) {
        self.connection_phase = ConnectionPhase::Reconnecting;
        self.record_diagnostic(DiagnosticLevel::Warning, "reconnecting", message);
    }

    pub fn mark_baseline_required(&mut self) {
        self.connection_phase = ConnectionPhase::BaselineRequired;
    }

    pub fn mark_connected(&mut self) {
        self.connection_phase = ConnectionPhase::Connected;
        self.status_message = None;
        self.diagnostics.retain(|diagnostic| {
            !matches!(
                diagnostic.code.as_str(),
                "reconnecting" | "disconnected" | "draining"
            )
        });
    }

    pub fn mark_disconnected(&mut self, message: impl Into<String>) {
        let message = message.into();
        let seq = self.tail_seq().unwrap_or(-1).saturating_add(1);
        let _ = self
            .scrollback
            .finalize_stream(seq, DshRenderFinish::Eof, Some(&message));
        self.connection_phase = ConnectionPhase::Disconnected;
        self.record_diagnostic(DiagnosticLevel::Error, "disconnected", message);
    }

    pub fn mark_draining(&mut self) {
        self.connection_phase = ConnectionPhase::Draining;
        self.record_diagnostic(DiagnosticLevel::Info, "draining", "Backend is draining");
    }

    pub fn set_generation(&mut self, generation: u64) {
        if generation != self.generation {
            let seq = self.tail_seq().unwrap_or(-1).saturating_add(1);
            let _ = self.scrollback.finalize_stream(
                seq,
                DshRenderFinish::Eof,
                Some("session generation changed"),
            );
        }
        self.generation = generation;
        self.subscribed_last_seq = None;
        self.pending_events.clear();
        self.projections.clear();
        self.pending_interactions.clear();
        self.queue.clear();
        self.queue_revision = self.queue_revision.saturating_add(1);
        self.mark_baseline_required();
    }

    /// Apply the current session slice of a control-plane baseline.  The
    /// transcript still comes from `session.history`, but queue, projections,
    /// interactions and host status must be restored even when no duplicate
    /// live frame follows the baseline notification.
    pub fn apply_control_snapshot(
        &mut self,
        snapshot: &crate::control_plane::SessionControlSnapshot,
    ) -> SessionUpdate {
        if snapshot.session_id != self.session_id || snapshot.generation != self.generation {
            return SessionUpdate::default();
        }
        let mut changed = false;

        if self.subscribed_last_seq != snapshot.subscribed_last_seq {
            self.subscribed_last_seq = snapshot.subscribed_last_seq;
            changed = true;
        }

        let next_projections = snapshot
            .projections
            .iter()
            .map(|(key, cell)| {
                (
                    key.clone(),
                    ProjectionCell {
                        seq: cell.seq,
                        value: cell.value.clone(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        if self.projections != next_projections {
            self.projections = next_projections;
            changed = true;
        }

        if snapshot.queue_initialized && self.queue != snapshot.queue {
            self.queue = snapshot.queue.clone();
            self.queue_revision = self.queue_revision.saturating_add(1);
            changed = true;
        }

        if self.pending_interactions != snapshot.pending_interactions {
            self.pending_interactions = snapshot.pending_interactions.clone();
            changed = true;
        }
        if self.running != snapshot.running.unwrap_or(self.running) {
            self.running = snapshot.running.unwrap_or(self.running);
            changed = true;
        }

        if let Some(message) = snapshot.last_error.as_deref() {
            let already_recorded = self.latest_diagnostic().is_some_and(|diagnostic| {
                diagnostic.code == "agent-error" && diagnostic.message == message
            });
            if !already_recorded {
                self.record_diagnostic(DiagnosticLevel::Error, "agent-error", message);
                changed = true;
            }
        }
        changed |= self.refresh_interaction_status();
        SessionUpdate {
            changed,
            gap_detected: self.needs_repair(),
        }
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn pending_interactions(&self) -> &[PendingInteraction] {
        &self.pending_interactions
    }

    pub fn pending_interaction(&self) -> Option<&PendingInteraction> {
        self.pending_interactions.first()
    }

    /// Return the DSH-owned presentation snapshot consumed by render and
    /// interaction widgets.  Runtime/session state remains private; callers
    /// receive stable block ids, queue items, and interaction DTOs only.
    pub fn presentation_model(&self) -> DshPresentationModel {
        self.build_presentation_model(true)
    }

    /// Return the control-plane part of the presentation model without
    /// cloning the complete transcript.  The interactive draw loop uses this
    /// hot-path variant; block viewers can request the full model above.
    pub fn presentation_controls(&self) -> DshPresentationModel {
        self.build_presentation_model(false)
    }

    fn build_presentation_model(&self, include_entries: bool) -> DshPresentationModel {
        let interaction = self
            .pending_interaction()
            .map(|pending| match pending.kind {
                InteractionKind::Approval => DshInteraction::Approval {
                    request_id: pending.request_id.clone(),
                    approval_id: pending
                        .approval_id
                        .clone()
                        .unwrap_or_else(|| "unknown-approval".into()),
                    call_id: pending.call_id.clone(),
                    tool_name: pending.tool_name.clone(),
                    reason: pending.reason.clone(),
                },
                InteractionKind::Question => DshInteraction::Question {
                    request_id: pending.request_id.clone(),
                    questions: pending.questions.clone(),
                },
            });
        let mut model = DshPresentationModel::new(self.session_id.clone(), self.generation);
        if include_entries {
            model.entries = self.scrollback.render_entries();
        }
        model.queue = self.queue.iter().map(DshQueueItem::from).collect();
        model.queue_revision = self.queue_revision;
        model.interaction = interaction;
        model
    }

    /// Return the latest host-computed projection value for a key.
    pub fn projection(&self, key: &str) -> Option<&Value> {
        self.projections.get(key).map(|cell| &cell.value)
    }

    /// Apply a host receipt to the local projection cache using the same
    /// higher-sequence-wins rule as pushed projection frames. Unary receipts
    /// can arrive before their mux notification, so callers should seed the
    /// value immediately and let the later push become a no-op replay.
    pub fn set_projection(&mut self, key: impl Into<String>, seq: i64, value: Value) -> bool {
        self.apply_projection(key.into(), seq, value)
    }

    /// Durable host-owned title, if the optional session-title projection is
    /// mounted. A missing projection is capability absence, not an error.
    pub fn title(&self) -> Option<&str> {
        self.projection("title").and_then(|value| {
            value
                .as_str()
                .or_else(|| value.get("title").and_then(Value::as_str))
        })
    }

    pub fn base_seq(&self) -> Option<i64> {
        self.history.first().map(|entry| entry.event.seq)
    }

    pub fn tail_seq(&self) -> Option<i64> {
        self.history.last().map(|entry| entry.event.seq)
    }

    pub fn install_initial(&mut self, page: SessionHistoryValue) -> PagerResult<()> {
        validate_page(&page.events)?;
        self.history = page.events;
        self.has_more = page.has_more;
        if let Some(projections) = page.projections {
            self.seed_projections(projections);
        }
        if let Some(tail) = self.tail_seq() {
            self.pending_events.retain(|seq, _| *seq > tail);
        }
        self.scrollback.rebuild(&self.history);
        self.drain_pending();
        self.mark_connected();
        Ok(())
    }

    /// Replace the volatile tail while retaining already loaded older pages.
    pub fn repair_tail(&mut self, page: SessionHistoryValue) -> PagerResult<()> {
        validate_page(&page.events)?;
        if page.events.is_empty() {
            // An empty tail is a valid empty-session baseline. Do not retain
            // transcript rows from the previous generation/window.
            self.history.clear();
            self.pending_events.clear();
            self.has_more = page.has_more;
            if let Some(projections) = page.projections {
                self.seed_projections(projections);
            }
            self.scrollback.rebuild(&self.history);
            self.mark_connected();
            return Ok(());
        }
        let new_base = page.events.first().map(|entry| entry.event.seq);
        let mut prefix = match new_base {
            Some(base) => self
                .history
                .iter()
                .take_while(|entry| entry.event.seq < base)
                .cloned()
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };
        if let (Some(last), Some(base)) = (prefix.last(), new_base) {
            if last.event.seq + 1 != base {
                // The old window has a hole relative to the fresh tail. Drop the
                // stale prefix and show the coherent page; retaining it would
                // expose an out-of-order transcript.
                prefix.clear();
            }
        }
        prefix.extend(page.events);
        validate_page(&prefix)?;
        self.history = prefix;
        self.has_more = page.has_more;
        if let Some(projections) = page.projections {
            self.seed_projections(projections);
        }
        if let Some(tail) = self.tail_seq() {
            self.pending_events.retain(|seq, _| *seq > tail);
        }
        self.scrollback.rebuild(&self.history);
        self.drain_pending();
        self.mark_connected();
        Ok(())
    }

    pub fn prepend_older(&mut self, page: SessionHistoryValue) -> PagerResult<bool> {
        validate_page(&page.events)?;
        if page.events.is_empty() {
            self.has_more = page.has_more;
            return Ok(false);
        }
        if let Some(base) = self.base_seq() {
            let older_tail = page
                .events
                .last()
                .map(|entry| entry.event.seq)
                .ok_or_else(|| PagerError::new("history page unexpectedly empty"))?;
            if older_tail + 1 != base {
                return Err(PagerError::new(format!(
                    "older history is discontinuous: page ends at {older_tail}, window starts at {base}"
                )));
            }
        }
        let mut joined = page.events;
        joined.append(&mut self.history);
        self.history = joined;
        self.has_more = page.has_more;
        self.scrollback.rebuild(&self.history);
        Ok(true)
    }

    pub fn accept_notification(
        &mut self,
        notification: JsonRpcNotification,
    ) -> PagerResult<SessionUpdate> {
        let Some(params) = notification.params else {
            if notification.method == "tui.serverDraining" {
                self.mark_draining();
                return Ok(SessionUpdate {
                    changed: true,
                    gap_detected: false,
                });
            }
            return Ok(SessionUpdate::default());
        };
        match notification.method.as_str() {
            "events.mux" => self.accept_mux(params),
            "events.host" => self.accept_host(params),
            "tui.serverDraining" => {
                self.mark_draining();
                Ok(SessionUpdate {
                    changed: true,
                    gap_detected: false,
                })
            }
            _ => Ok(SessionUpdate::default()),
        }
    }

    pub fn needs_repair(&self) -> bool {
        if !self.pending_events.is_empty() {
            return true;
        }
        match (self.subscribed_last_seq, self.tail_seq()) {
            (Some(last), Some(tail)) => last > tail,
            (Some(last), None) => last >= 0,
            _ => false,
        }
    }

    fn accept_mux(&mut self, frame: Value) -> PagerResult<SessionUpdate> {
        let frame_type = string_field(&frame, "type")?;
        if self.reject_stale_generation(&frame, "mux") {
            return Ok(SessionUpdate::default());
        }
        if frame_type == "stream/error" {
            return Ok(self.accept_stream_error(&frame, "mux"));
        }
        let Some(session_id) = frame.get("sessionId").and_then(Value::as_str) else {
            return Ok(SessionUpdate::default());
        };
        if session_id != self.session_id {
            return Ok(SessionUpdate::default());
        }

        match frame_type {
            "session/event" => {
                let event = frame
                    .get("event")
                    .cloned()
                    .ok_or_else(|| PagerError::new("session/event omitted event"))?;
                let entry = HistoryEntry {
                    event: serde_json::from_value(event)?,
                    view: frame.get("view").cloned(),
                };
                let changed = self.accept_live(entry);
                Ok(SessionUpdate {
                    changed,
                    gap_detected: self.needs_repair(),
                })
            }
            "session/subscribed" => {
                let last_seq = frame
                    .get("lastSeq")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| PagerError::new("session/subscribed omitted lastSeq"))?;
                self.subscribed_last_seq = Some(last_seq);
                Ok(SessionUpdate {
                    changed: false,
                    gap_detected: self.needs_repair(),
                })
            }
            "session/projection" => {
                let key = string_field(&frame, "key")?.to_string();
                let seq = frame
                    .get("seq")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| PagerError::new("session/projection omitted seq"))?;
                let value = frame.get("value").cloned().unwrap_or(Value::Null);
                let changed = self.apply_projection(key, seq, value);
                Ok(SessionUpdate {
                    changed,
                    gap_detected: false,
                })
            }
            "session/queue" => {
                let items = frame
                    .get("items")
                    .cloned()
                    .ok_or_else(|| PagerError::new("session/queue omitted items"))?;
                let parsed: Vec<SessionQueueItem> = serde_json::from_value(items)?;
                let changed = parsed != self.queue;
                self.queue = parsed;
                // Count every snapshot, including an identical one: an
                // accepted mutation that leaves the visible value unchanged
                // still needs an observable acknowledgement boundary.
                self.queue_revision = self.queue_revision.saturating_add(1);
                Ok(SessionUpdate {
                    changed,
                    gap_detected: false,
                })
            }
            "approval/requested" => {
                let interaction = DshInteraction::approval_from_frame(&frame);
                let DshInteraction::Approval {
                    request_id,
                    approval_id,
                    call_id,
                    tool_name,
                    reason,
                } = interaction
                else {
                    return Err(PagerError::new("approval adapter returned a question"));
                };
                let tool = tool_name.unwrap_or_else(|| "tool".into());
                let changed = self.upsert_interaction(PendingInteraction::approval(
                    request_id,
                    approval_id,
                    call_id,
                    Some(tool.clone()),
                    reason,
                ));
                let status = format!("Approval required for {tool}");
                let status_changed = self.status_message.as_deref() != Some(status.as_str());
                self.status_message = Some(status);
                Ok(SessionUpdate {
                    changed: changed || status_changed,
                    gap_detected: false,
                })
            }
            "question/requested" => {
                let interaction = DshInteraction::question_from_frame(&frame);
                let DshInteraction::Question {
                    request_id,
                    questions,
                } = interaction
                else {
                    return Err(PagerError::new("question adapter returned an approval"));
                };
                let changed =
                    self.upsert_interaction(PendingInteraction::question(request_id, questions));
                let status_changed =
                    self.status_message.as_deref() != Some("Question requires an answer");
                self.status_message = Some("Question requires an answer".into());
                Ok(SessionUpdate {
                    changed: changed || status_changed,
                    gap_detected: false,
                })
            }
            "approval/resolved" => {
                let approval_id = optional_string(&frame, "approvalId");
                let before = self.pending_interactions.len();
                self.pending_interactions.retain(|interaction| {
                    interaction.kind != InteractionKind::Approval
                        || interaction.approval_id.as_deref() != approval_id.as_deref()
                });
                let removed = before != self.pending_interactions.len();
                let status_changed = removed && self.refresh_interaction_status();
                Ok(SessionUpdate {
                    changed: removed || status_changed,
                    gap_detected: false,
                })
            }
            "question/resolved" => {
                let request_id = optional_string(&frame, "questionRpcId")
                    .or_else(|| optional_string(&frame, "requestId"));
                let before = self.pending_interactions.len();
                self.pending_interactions.retain(|interaction| {
                    interaction.kind != InteractionKind::Question
                        || request_id.as_deref() != Some(interaction.request_id.as_str())
                });
                let removed = before != self.pending_interactions.len();
                let status_changed = removed && self.refresh_interaction_status();
                Ok(SessionUpdate {
                    changed: removed || status_changed,
                    gap_detected: false,
                })
            }
            _ => Ok(SessionUpdate::default()),
        }
    }

    fn accept_host(&mut self, frame: Value) -> PagerResult<SessionUpdate> {
        let frame_type = string_field(&frame, "type")?;
        if self.reject_stale_generation(&frame, "host") {
            return Ok(SessionUpdate::default());
        }
        if frame_type == "stream/error" {
            return Ok(self.accept_stream_error(&frame, "host"));
        }
        let Some(session_id) = frame.get("sessionId").and_then(Value::as_str) else {
            return Ok(SessionUpdate::default());
        };
        if session_id != self.session_id {
            return Ok(SessionUpdate::default());
        }
        match frame_type {
            "host/session-status" => {
                let running = frame
                    .get("running")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| PagerError::new("host/session-status omitted running"))?;
                let changed = running != self.running;
                self.running = running;
                Ok(SessionUpdate {
                    changed,
                    gap_detected: false,
                })
            }
            "host/agent-error" => {
                let message = frame
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("agent error");
                self.record_diagnostic(DiagnosticLevel::Error, "agent-error", message);
                let seq = self.tail_seq().unwrap_or(-1).saturating_add(1);
                let _finalized =
                    self.scrollback
                        .finalize_stream(seq, DshRenderFinish::Failed, Some(message));
                Ok(SessionUpdate {
                    changed: true,
                    gap_detected: false,
                })
            }
            _ => Ok(SessionUpdate::default()),
        }
    }

    fn accept_stream_error(&mut self, frame: &Value, stream: &str) -> SessionUpdate {
        let code = frame
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("internal");
        let message = frame
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("stream error");
        self.mark_reconnecting(format!("{stream} stream error: {message}"));
        self.record_diagnostic(
            DiagnosticLevel::Error,
            format!("{stream}-stream/{code}"),
            message,
        );
        let finish = if code.eq_ignore_ascii_case("eof")
            || code.eq_ignore_ascii_case("closed")
            || frame
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    let value = value.to_ascii_lowercase();
                    value.contains("eof") || value.contains("closed")
                }) {
            DshRenderFinish::Eof
        } else {
            DshRenderFinish::Failed
        };
        let seq = self.tail_seq().unwrap_or(-1).saturating_add(1);
        let _finalized = self.scrollback.finalize_stream(seq, finish, Some(message));
        SessionUpdate {
            changed: true,
            gap_detected: false,
        }
    }

    /// Mark the current generation's running surfaces as EOF when the reader
    /// closes before a typed `stream/error` frame is available.
    pub fn accept_stream_eof(&mut self, message: impl Into<String>) -> SessionUpdate {
        let message = message.into();
        let seq = self.tail_seq().unwrap_or(-1).saturating_add(1);
        let finalized = self
            .scrollback
            .finalize_stream(seq, DshRenderFinish::Eof, Some(&message));
        let phase_changed = self.connection_phase != ConnectionPhase::Reconnecting;
        if phase_changed {
            self.mark_reconnecting(message);
        }
        SessionUpdate {
            changed: finalized || phase_changed,
            gap_detected: false,
        }
    }

    fn reject_stale_generation(&mut self, frame: &Value, stream: &str) -> bool {
        let Some(generation) = frame.get("generation").and_then(Value::as_u64) else {
            return false;
        };
        if generation == self.generation {
            return false;
        }
        self.record_diagnostic(
            DiagnosticLevel::Warning,
            "stale-generation",
            format!(
                "ignored {stream} frame for generation {generation}; current generation is {}",
                self.generation
            ),
        );
        true
    }

    fn accept_live(&mut self, entry: HistoryEntry) -> bool {
        let seq = entry.event.seq;
        let next = self.tail_seq().map_or(0, |tail| tail + 1);
        if seq < next {
            return false;
        }
        if seq > next {
            self.pending_events.entry(seq).or_insert(entry);
            return false;
        }
        self.scrollback.apply_event(&entry);
        self.history.push(entry);
        self.drain_pending();
        true
    }

    fn drain_pending(&mut self) {
        loop {
            let next = self.tail_seq().map_or(0, |tail| tail + 1);
            let Some(entry) = self.pending_events.remove(&next) else {
                return;
            };
            self.scrollback.apply_event(&entry);
            self.history.push(entry);
        }
    }

    fn seed_projections(&mut self, baseline: SessionProjectionsBlock) {
        for (key, value) in baseline.values {
            self.apply_projection(key, baseline.as_of_seq, value);
        }
    }

    fn apply_projection(&mut self, key: String, seq: i64, value: Value) -> bool {
        if self
            .projections
            .get(&key)
            .is_some_and(|current| current.seq >= seq)
        {
            return false;
        }
        self.projections.insert(key, ProjectionCell { seq, value });
        true
    }

    fn upsert_interaction(&mut self, interaction: PendingInteraction) -> bool {
        if interaction.kind == InteractionKind::Approval {
            if let Some(approval_id) = interaction.approval_id.as_deref() {
                // approvalId is stable across mux replay; the transport
                // request id is not guaranteed to be, so replaying an
                // approval must not create a second answerable row.
                if self.pending_interactions.iter().any(|existing| {
                    existing.kind == InteractionKind::Approval
                        && existing.approval_id.as_deref() == Some(approval_id)
                }) {
                    return false;
                }
            }
        }
        if interaction.request_id.is_empty() {
            if self
                .pending_interactions
                .iter()
                .any(|existing| existing == &interaction)
            {
                return false;
            }
            self.pending_interactions.push(interaction);
            return true;
        }
        if let Some(existing) = self
            .pending_interactions
            .iter_mut()
            .find(|existing| existing.request_id == interaction.request_id)
        {
            if *existing == interaction {
                return false;
            }
            *existing = interaction;
            return true;
        }
        self.pending_interactions.push(interaction);
        true
    }

    fn record_diagnostic(
        &mut self,
        level: DiagnosticLevel,
        code: impl Into<String>,
        message: impl Into<String>,
    ) {
        let code = code.into();
        let message = message.into();
        self.status_message = Some(message.clone());
        self.diagnostics.push(Diagnostic {
            level,
            code,
            message,
        });
        const MAX_DIAGNOSTICS: usize = 32;
        if self.diagnostics.len() > MAX_DIAGNOSTICS {
            let excess = self.diagnostics.len() - MAX_DIAGNOSTICS;
            self.diagnostics.drain(..excess);
        }
    }

    fn refresh_interaction_status(&mut self) -> bool {
        let next = self
            .pending_interactions
            .first()
            .map(|interaction| match interaction.kind {
                InteractionKind::Approval => interaction.tool_name.as_deref().map_or_else(
                    || "Approval required".to_string(),
                    |tool| format!("Approval required for {tool}"),
                ),
                InteractionKind::Question => "Question requires an answer".to_string(),
            });
        let changed = self.status_message != next;
        self.status_message = next;
        changed
    }
}

fn string_field<'a>(value: &'a Value, field: &str) -> PagerResult<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| PagerError::new(format!("frame omitted string field {field}")))
}

fn optional_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_string)
}

fn validate_page(entries: &[HistoryEntry]) -> PagerResult<()> {
    for entry in entries {
        if entry.event.seq < 0 {
            return Err(PagerError::new(format!(
                "history contains negative event seq {}",
                entry.event.seq
            )));
        }
    }
    for pair in entries.windows(2) {
        if pair[1].event.seq != pair[0].event.seq + 1 {
            return Err(PagerError::new(format!(
                "history is discontinuous between seq {} and {}",
                pair[0].event.seq, pair[1].event.seq
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::{SessionEvent, SessionHistoryValue};
    use serde_json::json;

    fn entry(seq: i64) -> HistoryEntry {
        HistoryEntry {
            event: SessionEvent {
                event_type: "turn/start".into(),
                seq,
                time: seq as f64,
                data: json!({ "turn": seq }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        }
    }

    fn page(start: i64, end: i64) -> SessionHistoryValue {
        SessionHistoryValue {
            events: (start..=end).map(entry).collect(),
            has_more: false,
            projections: None,
        }
    }

    fn event_notification(seq: i64) -> JsonRpcNotification {
        JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "session/event",
                "sessionId": "s",
                "event": serde_json::to_value(entry(seq).event).unwrap(),
            })),
        }
    }

    #[test]
    fn out_of_order_live_events_are_buffered_and_drained_by_seq() {
        let mut state = SessionState::new("s".into(), 1);
        state.install_initial(page(0, 1)).unwrap();
        let first = state.accept_notification(event_notification(3)).unwrap();
        assert!(!first.changed);
        assert!(first.gap_detected);
        let second = state.accept_notification(event_notification(2)).unwrap();
        assert!(second.changed);
        assert!(!second.gap_detected);
        assert_eq!(
            state
                .history
                .iter()
                .map(|item| item.event.seq)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn repair_drops_replayed_pending_overlap() {
        let mut state = SessionState::new("s".into(), 1);
        state.install_initial(page(0, 1)).unwrap();
        state.accept_notification(event_notification(4)).unwrap();
        state.repair_tail(page(0, 4)).unwrap();
        assert!(!state.needs_repair());
        assert_eq!(state.tail_seq(), Some(4));
    }

    #[test]
    fn empty_repair_tail_clears_the_previous_transcript() {
        let mut state = SessionState::new("s".into(), 1);
        state.install_initial(page(0, 1)).unwrap();
        state
            .repair_tail(SessionHistoryValue {
                events: Vec::new(),
                has_more: false,
                projections: None,
            })
            .unwrap();
        assert!(state.history().is_empty());
        assert!(state.scrollback.entries().is_empty());
        assert_eq!(state.connection_phase(), ConnectionPhase::Connected);
    }

    #[test]
    fn projection_values_use_higher_seq_wins() {
        let mut state = SessionState::new("s".into(), 1);
        let newer = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "session/projection", "sessionId": "s", "key": "title",
                "seq": 9, "value": "new"
            })),
        };
        let older = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "session/projection", "sessionId": "s", "key": "title",
                "seq": 8, "value": "old"
            })),
        };
        assert!(state.accept_notification(newer).unwrap().changed);
        assert!(!state.accept_notification(older).unwrap().changed);
        assert_eq!(state.projection("title"), Some(&json!("new")));
    }

    #[test]
    fn unary_projection_receipt_is_visible_before_push_replay() {
        let mut state = SessionState::new("s".into(), 1);
        assert!(state.set_projection("title", 4, json!("receipt")));
        assert_eq!(state.title(), Some("receipt"));
        assert!(!state.set_projection("title", 4, json!("duplicate")));
        assert_eq!(state.title(), Some("receipt"));
    }

    #[test]
    fn reconnect_generation_drops_projection_values_until_new_baseline() {
        let mut state = SessionState::new("s".into(), 1);
        state
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "session/projection", "sessionId": "s", "key": "title",
                    "seq": 1, "value": "old"
                })),
            })
            .unwrap();
        assert_eq!(state.title(), Some("old"));
        state.set_generation(2);
        assert_eq!(state.title(), None);
    }

    #[test]
    fn history_page_continuity_is_checked() {
        let mut state = SessionState::new("s".into(), 1);
        let invalid = SessionHistoryValue {
            events: vec![entry(0), entry(2)],
            has_more: false,
            projections: None,
        };
        assert!(state.install_initial(invalid).is_err());
    }

    #[test]
    fn answerable_interactions_are_deduplicated_and_resolved_by_wire_id() {
        let mut state = SessionState::new("s".into(), 1);
        let approval = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "approval/requested",
                "sessionId": "s",
                "requestId": "rpc-a",
                "approvalId": "approval-a",
                "toolName": "bash",
                "reason": "run command"
            })),
        };
        assert!(state.accept_notification(approval.clone()).unwrap().changed);
        assert!(!state.accept_notification(approval).unwrap().changed);
        assert_eq!(state.pending_interactions().len(), 1);
        assert_eq!(state.pending_interaction().unwrap().request_id, "rpc-a");

        let resolved = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "approval/resolved",
                "sessionId": "s",
                "approvalId": "approval-a",
                "outcome": "allowed-once"
            })),
        };
        assert!(state.accept_notification(resolved).unwrap().changed);
        assert!(state.pending_interactions().is_empty());
    }

    #[test]
    fn question_resolution_does_not_clear_a_different_pending_question() {
        let mut state = SessionState::new("s".into(), 1);
        for request_id in ["rpc-a", "rpc-b"] {
            let note = JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "question/requested",
                    "sessionId": "s",
                    "requestId": request_id,
                    "questions": [{ "id": "q", "question": "pick" }]
                })),
            };
            state.accept_notification(note).unwrap();
        }
        let resolved = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "question/resolved",
                "sessionId": "s",
                "questionRpcId": "rpc-a",
                "outcome": "answered"
            })),
        };
        state.accept_notification(resolved).unwrap();
        assert_eq!(state.pending_interactions().len(), 1);
        assert_eq!(state.pending_interaction().unwrap().request_id, "rpc-b");
    }

    #[test]
    fn queue_snapshot_replaces_previous_items_atomically() {
        let mut state = SessionState::new("s".into(), 1);
        let note = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "session/queue",
                "sessionId": "s",
                "items": [{
                    "id": "m-1",
                    "placement": "queued",
                    "message": { "role": "user", "content": [], "source": { "kind": "user" } }
                }]
            })),
        };
        assert!(state.accept_notification(note.clone()).unwrap().changed);
        assert_eq!(state.queue_revision(), 1);
        assert!(!state.accept_notification(note).unwrap().changed);
        assert_eq!(state.queue_revision(), 2);
        assert_eq!(state.queue().len(), 1);
        assert_eq!(state.queue_item("m-1").unwrap().id, "m-1");
    }

    #[test]
    fn presentation_snapshot_contains_adapted_blocks_queue_and_interaction() {
        let mut state = SessionState::new("s".into(), 7);
        state
            .install_initial(SessionHistoryValue {
                events: vec![HistoryEntry {
                    event: SessionEvent {
                        event_type: "assistant/message".into(),
                        seq: 0,
                        time: 0.0,
                        data: json!({
                            "message": {
                                "content": [{ "type": "text", "text": "hello" }]
                            }
                        }),
                        source_event_seqs: None,
                        surface_op: None,
                        ignorable: None,
                    },
                    view: None,
                }],
                has_more: false,
                projections: None,
            })
            .unwrap();
        state
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "session/queue",
                    "sessionId": "s",
                    "items": [{
                        "id": "q1",
                        "placement": "queued",
                        "message": { "content": [{ "type": "text", "text": "queued" }] }
                    }]
                })),
            })
            .unwrap();
        state
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "approval/requested",
                    "sessionId": "s",
                    "requestId": "rpc-1",
                    "approvalId": "approval-1",
                    "callId": "call-approval",
                    "toolName": "bash"
                })),
            })
            .unwrap();
        let model = state.presentation_model();
        assert_eq!(model.session_id, "s");
        assert_eq!(model.generation, 7);
        assert_eq!(model.entries[0].text, "hello");
        assert_eq!(model.queue[0].content.summary.as_deref(), Some("queued"));
        assert_eq!(
            model.queue[0].content.editable_text.as_deref(),
            Some("queued")
        );
        assert_eq!(model.queue_revision, 1);
        assert!(matches!(
            model.interaction,
            Some(DshInteraction::Approval {
                ref request_id,
                call_id: Some(ref call_id),
                ..
            }) if request_id == "rpc-1" && call_id == "call-approval"
        ));
        assert!(state.presentation_controls().entries.is_empty());
        assert_eq!(state.presentation_controls().queue[0].id, "q1");
    }

    #[test]
    fn connection_diagnostics_are_bounded_and_phase_is_explicit() {
        let mut state = SessionState::new("s".into(), 1);
        assert_eq!(state.connection_phase(), ConnectionPhase::BaselineRequired);
        for index in 0..40 {
            state.mark_reconnecting(format!("attempt {index}"));
        }
        assert_eq!(state.connection_phase(), ConnectionPhase::Reconnecting);
        assert_eq!(state.diagnostics().len(), 32);
        state.mark_connected();
        assert_eq!(state.connection_phase(), ConnectionPhase::Connected);
    }

    #[test]
    fn host_stream_error_enters_reconnect_state_without_a_session_id() {
        let mut state = SessionState::new("s".into(), 3);
        let note = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.host".into(),
            params: Some(json!({
                "type": "stream/error",
                "error": { "code": "internal", "message": "host pipe closed", "details": {} }
            })),
        };
        let update = state.accept_notification(note).unwrap();
        assert!(update.changed);
        assert_eq!(state.connection_phase(), ConnectionPhase::Reconnecting);
        assert!(
            state
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "host-stream/internal")
        );
    }

    #[test]
    fn host_stream_error_finalizes_a_partial_surface() {
        let mut state = SessionState::new("s".into(), 3);
        state
            .install_initial(SessionHistoryValue {
                events: vec![HistoryEntry {
                    event: SessionEvent {
                        event_type: "assistant/chunk".into(),
                        seq: 0,
                        time: 0.0,
                        data: json!({
                            "turn": 1,
                            "step": 0,
                            "chunk": { "type": "text-delta", "index": 0, "text": "draft" }
                        }),
                        source_event_seqs: None,
                        surface_op: None,
                        ignorable: None,
                    },
                    view: None,
                }],
                has_more: false,
                projections: None,
            })
            .unwrap();
        let note = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.host".into(),
            params: Some(json!({
                "type": "stream/error",
                "error": { "code": "eof", "message": "host pipe closed" }
            })),
        };
        assert!(state.accept_notification(note).unwrap().changed);
        let entry = &state.presentation_model().entries[0];
        assert_eq!(entry.finish, DshRenderFinish::Eof);
        assert!(!entry.partial);
        assert_eq!(entry.text, "draft");
    }

    #[test]
    fn stale_stream_error_cannot_finalize_current_generation() {
        let mut state = SessionState::new("s".into(), 3);
        state
            .install_initial(SessionHistoryValue {
                events: vec![HistoryEntry {
                    event: SessionEvent {
                        event_type: "assistant/chunk".into(),
                        seq: 0,
                        time: 0.0,
                        data: json!({
                            "turn": 1,
                            "step": 0,
                            "chunk": { "type": "text-delta", "index": 0, "text": "draft" }
                        }),
                        source_event_seqs: None,
                        surface_op: None,
                        ignorable: None,
                    },
                    view: None,
                }],
                has_more: false,
                projections: None,
            })
            .unwrap();
        let note = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "stream/error",
                "generation": 2,
                "error": { "code": "eof", "message": "old stream closed" }
            })),
        };
        assert!(!state.accept_notification(note).unwrap().changed);
        assert!(state.presentation_model().entries[0].partial);
    }

    #[test]
    fn stale_generation_frames_are_ignored_and_reported() {
        let mut state = SessionState::new("s".into(), 4);
        let note = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "events.mux".into(),
            params: Some(json!({
                "type": "session/queue",
                "sessionId": "s",
                "generation": 3,
                "items": []
            })),
        };
        let update = state.accept_notification(note).unwrap();
        assert!(!update.changed);
        assert!(state.queue().is_empty());
        assert!(
            state
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "stale-generation")
        );
    }

    #[test]
    fn operation_token_contains_session_sequence_and_generation() {
        let state = SessionState::new("s".into(), 9);
        let token = state.operation_token(Some(12));
        assert_eq!(
            token,
            OperationToken {
                session_id: "s".into(),
                seq: Some(12),
                generation: 9,
            }
        );
        assert!(!state.accepts_operation(&token));
        assert!(state.accepts_operation(&state.operation_token(None)));
    }
}
