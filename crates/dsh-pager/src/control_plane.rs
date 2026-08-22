//! Multi-session control-plane snapshots and routing.
//!
//! The Harness owns the session log and domain schemas.  This module keeps a
//! bounded, value-backed mirror of the control facts that a native TUI needs
//! to render a roster and recover after reconnect.  Transcript frames are
//! still handed to [`crate::session::SessionState`] for presentation; they are
//! never treated as the complete reconnect baseline.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dsh_pager_protocol::{JsonRpcNotification, SessionListValue, SessionQueueItem};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{PagerError, PagerResult};
use crate::session::{ConnectionPhase, PendingInteraction, SessionState, SessionUpdate};

const DEFAULT_MAX_SESSIONS: usize = 512;
const DEFAULT_MAX_RECORDS: usize = 256;
const DEFAULT_TTL: Duration = Duration::from_secs(60 * 60);

/// One generic projection cell from `session/projection`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjection {
    pub key: String,
    pub seq: i64,
    pub value: Value,
}

/// Host workspace view. Unknown future fields remain in `raw`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub workspace_id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub session_ids: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub raw: Value,
}

impl WorkspaceView {
    fn from_value(value: Value) -> PagerResult<Self> {
        let workspace_id = value
            .get("workspaceId")
            .and_then(Value::as_str)
            .ok_or_else(|| PagerError::new("workspace frame omitted workspaceId"))?
            .to_string();
        Ok(Self {
            workspace_id,
            path: value
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            session_ids: value
                .get("sessionIds")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            created_at: value
                .get("createdAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            updated_at: value
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            raw: value,
        })
    }
}

/// Value-backed background job view. The raw value is retained for newer job kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub finished_at: Option<i64>,
    #[serde(default)]
    pub raw: Value,
}

impl JobView {
    fn from_value(value: &Value) -> Self {
        Self {
            id: value
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            kind: value
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            label: value
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status: value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            detail: value
                .get("detail")
                .and_then(Value::as_str)
                .map(str::to_string),
            started_at: value.get("startedAt").and_then(Value::as_i64),
            finished_at: value.get("finishedAt").and_then(Value::as_i64),
            raw: value.clone(),
        }
    }
}

/// Child catalog row retained without assuming a future subagent schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentListEntry {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub activity: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub has_children: Option<bool>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub raw: Value,
}

impl SubagentListEntry {
    /// Decode a value-backed child row while preserving unknown fields.
    pub fn from_value(value: Value) -> Self {
        Self {
            kind: value
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            id: value
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            mode: value
                .get("mode")
                .and_then(Value::as_str)
                .map(str::to_string),
            activity: value
                .get("activity")
                .and_then(Value::as_str)
                .map(str::to_string),
            label: value
                .get("label")
                .and_then(Value::as_str)
                .map(str::to_string),
            has_children: value.get("hasChildren").and_then(Value::as_bool),
            reason: value
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string),
            raw: value,
        }
    }
}

/// Connection state exposed to model/view code. `phase` mirrors the existing
/// session lifecycle enum while generation and last error remain explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionState {
    pub phase: ConnectionPhase,
    pub generation: u64,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl ConnectionState {
    pub fn new(generation: u64) -> Self {
        Self {
            phase: ConnectionPhase::BaselineRequired,
            generation,
            last_error: None,
        }
    }
}

/// One session's control-plane snapshot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct SessionControlSnapshot {
    pub session_id: String,
    pub generation: u64,
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// Host `session.list.updatedAt`, separate from the cache eviction clock.
    #[serde(default)]
    pub updated_at_ms: Option<u64>,
    pub last_seen_seq: Option<i64>,
    pub subscribed_last_seq: Option<i64>,
    pub projection_watermark: Option<i64>,
    pub projections: BTreeMap<String, SessionProjection>,
    pub queue: Vec<SessionQueueItem>,
    #[serde(default)]
    pub queue_initialized: bool,
    pub jobs: Vec<JobView>,
    #[serde(default)]
    pub jobs_initialized: bool,
    pub pending_interactions: Vec<PendingInteraction>,
    pub blank: Option<bool>,
    pub parent_session_id: Option<String>,
    pub origin: Option<String>,
    pub cwd: Option<String>,
    pub agent_preset: Option<String>,
    pub running: Option<bool>,
    pub last_error: Option<String>,
    pub removed: bool,
    pub archived: bool,
    pub last_activity_ms: u64,
}

impl SessionControlSnapshot {
    fn new(session_id: String, generation: u64, now: u64) -> Self {
        Self {
            session_id,
            generation,
            workspace_id: None,
            updated_at_ms: None,
            last_seen_seq: None,
            subscribed_last_seq: None,
            projection_watermark: None,
            projections: BTreeMap::new(),
            queue: Vec::new(),
            queue_initialized: false,
            jobs: Vec::new(),
            jobs_initialized: false,
            pending_interactions: Vec::new(),
            blank: None,
            parent_session_id: None,
            origin: None,
            cwd: None,
            agent_preset: None,
            running: None,
            last_error: None,
            removed: false,
            archived: false,
            last_activity_ms: now,
        }
    }
}

/// One bounded replay record. Only control frames are retained; history is
/// refetched through `session.history` at a load barrier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlPlaneRecord {
    pub stream: String,
    pub generation: u64,
    pub session_id: Option<String>,
    pub sequence: Option<i64>,
    pub frame: Value,
    #[serde(rename = "at", alias = "atMs")]
    pub at_ms: u64,
}

/// Result of routing one notification through the control plane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlPlaneUpdate {
    pub accepted: bool,
    pub duplicate: bool,
    pub stale: bool,
    pub control: bool,
    pub presentation: bool,
    pub session_id: Option<String>,
    pub sequence: Option<i64>,
}

/// Backwards-compatible descriptive alias used by callers that name the
/// result after the store operation rather than the router update.
pub type ControlPlaneApplyResult = ControlPlaneUpdate;

/// Bounded store options.
#[derive(Debug, Clone, Copy)]
pub struct ControlPlaneStoreOptions {
    pub max_sessions: usize,
    pub max_records_per_session: usize,
    pub ttl: Duration,
}

impl Default for ControlPlaneStoreOptions {
    fn default() -> Self {
        Self {
            max_sessions: DEFAULT_MAX_SESSIONS,
            max_records_per_session: DEFAULT_MAX_RECORDS,
            ttl: DEFAULT_TTL,
        }
    }
}

/// Multi-session control-plane cache.
#[derive(Debug)]
pub struct ControlPlaneStore {
    options: ControlPlaneStoreOptions,
    generation: u64,
    sessions: BTreeMap<String, SessionControlSnapshot>,
    records: BTreeMap<String, VecDeque<ControlPlaneRecord>>,
    seen_sequences: BTreeMap<String, BTreeSet<i64>>,
    host_records: VecDeque<ControlPlaneRecord>,
    workspaces: BTreeMap<String, WorkspaceView>,
    workspace_order: Vec<String>,
    archived_sessions: BTreeSet<String>,
    request_ids: BTreeMap<String, String>,
    host_fingerprints: BTreeSet<String>,
    fingerprint_order: VecDeque<String>,
    connection: ConnectionState,
    revision: u64,
}

impl Default for ControlPlaneStore {
    fn default() -> Self {
        Self::new(ControlPlaneStoreOptions::default())
    }
}

impl ControlPlaneStore {
    pub fn new(mut options: ControlPlaneStoreOptions) -> Self {
        options.max_sessions = options.max_sessions.max(1);
        options.max_records_per_session = options.max_records_per_session.max(1);
        options.ttl = options.ttl.max(Duration::from_millis(1));
        Self {
            options,
            generation: 0,
            sessions: BTreeMap::new(),
            records: BTreeMap::new(),
            seen_sequences: BTreeMap::new(),
            host_records: VecDeque::new(),
            workspaces: BTreeMap::new(),
            workspace_order: Vec::new(),
            archived_sessions: BTreeSet::new(),
            request_ids: BTreeMap::new(),
            host_fingerprints: BTreeSet::new(),
            fingerprint_order: VecDeque::new(),
            connection: ConnectionState::new(0),
            revision: 0,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn connection(&self) -> &ConnectionState {
        &self.connection
    }

    /// Monotonic revision incremented whenever a non-stale frame changes the
    /// control mirror. UI overlays use it instead of polling every row value.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Atomically clear a previous generation. Lower generations are ignored.
    pub fn set_generation(&mut self, generation: u64) -> bool {
        if generation == self.generation {
            return false;
        }
        if generation < self.generation {
            return false;
        }
        self.generation = generation;
        self.sessions.clear();
        self.records.clear();
        self.seen_sequences.clear();
        self.host_records.clear();
        self.workspaces.clear();
        self.workspace_order.clear();
        self.archived_sessions.clear();
        self.request_ids.clear();
        self.host_fingerprints.clear();
        self.fingerprint_order.clear();
        self.connection = ConnectionState::new(generation);
        self.revision = self.revision.saturating_add(1);
        true
    }

    /// Seed roster metadata from the host's `session.list` response. This is
    /// the synchronous baseline used before the first stream frame arrives.
    pub fn seed_session_list(&mut self, list: &SessionListValue) {
        let now = now_ms();
        let listed = list
            .items
            .iter()
            .map(|summary| summary.session_id.as_str())
            .collect::<BTreeSet<_>>();
        // `session.list` is the host's complete roster baseline.  Retain a
        // missing snapshot as an explicit removed row so a stale Dashboard
        // selection can render “gone” once before falling back, rather than
        // silently reusing a session that no longer exists.
        for snapshot in self.sessions.values_mut() {
            if !listed.contains(snapshot.session_id.as_str()) {
                snapshot.removed = true;
                snapshot.last_activity_ms = now;
            }
        }
        for summary in &list.items {
            let archived = self.archived_sessions.contains(&summary.session_id);
            let snapshot = self.ensure_session(summary.session_id.clone(), now);
            snapshot.blank = Some(summary.blank);
            snapshot.running = Some(summary.running);
            snapshot.parent_session_id = summary.parent_session_id.clone();
            snapshot.origin = summary.origin.clone();
            snapshot.cwd = summary.cwd.clone();
            snapshot.agent_preset = summary.agent_preset.clone();
            snapshot.archived = archived;
            snapshot.removed = false;
            snapshot.updated_at_ms = finite_epoch_ms(summary.updated_at);
            if let Some(projections) = &summary.projections {
                for (key, value) in &projections.values {
                    let cell = SessionProjection {
                        key: key.clone(),
                        seq: projections.as_of_seq,
                        value: value.clone(),
                    };
                    let replace = snapshot
                        .projections
                        .get(key)
                        .is_none_or(|existing| existing.seq < cell.seq);
                    if replace {
                        snapshot.projections.insert(key.clone(), cell);
                    }
                }
                snapshot.projection_watermark = Some(
                    snapshot
                        .projection_watermark
                        .unwrap_or(projections.as_of_seq)
                        .max(projections.as_of_seq),
                );
                snapshot.last_seen_seq = Some(
                    snapshot
                        .last_seen_seq
                        .unwrap_or(projections.as_of_seq)
                        .max(projections.as_of_seq),
                );
            }
            snapshot.last_activity_ms = now;
        }
        self.revision = self.revision.saturating_add(1);
        self.prune(now);
    }

    /// Seed workspace order and archive metadata from `workspace.list`.
    pub fn seed_workspace_list(&mut self, value: &Value) -> PagerResult<()> {
        let now = now_ms();
        self.workspaces.clear();
        self.workspace_order.clear();
        if let Some(items) = value.get("items").and_then(Value::as_array) {
            for item in items {
                let workspace = WorkspaceView::from_value(item.clone())?;
                self.workspace_order.push(workspace.workspace_id.clone());
                self.workspaces
                    .insert(workspace.workspace_id.clone(), workspace);
            }
        }
        self.archived_sessions = value
            .get("archivedSessionIds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        for snapshot in self.sessions.values_mut() {
            snapshot.archived = self.archived_sessions.contains(&snapshot.session_id);
            snapshot.workspace_id = None;
        }
        let memberships = self
            .workspaces
            .values()
            .map(|workspace| {
                (
                    workspace.workspace_id.clone(),
                    workspace.session_ids.clone(),
                )
            })
            .collect::<Vec<_>>();
        for (workspace_id, session_ids) in memberships {
            self.update_workspace_membership(&workspace_id, &session_ids);
        }
        self.revision = self.revision.saturating_add(1);
        self.prune(now);
        Ok(())
    }

    pub fn mark_phase(&mut self, phase: ConnectionPhase, error: Option<String>) {
        if self.connection.phase != phase || self.connection.last_error != error {
            self.connection.phase = phase;
            self.connection.last_error = error;
            self.revision = self.revision.saturating_add(1);
        }
    }

    pub fn snapshot(&self, session_id: &str) -> Option<&SessionControlSnapshot> {
        self.sessions.get(session_id)
    }

    pub fn snapshots(&self) -> impl Iterator<Item = &SessionControlSnapshot> {
        self.sessions.values()
    }

    pub fn workspaces(&self) -> impl Iterator<Item = &WorkspaceView> {
        self.workspace_order
            .iter()
            .filter_map(|id| self.workspaces.get(id))
            .chain(self.workspaces.iter().filter_map(|(id, value)| {
                (!self.workspace_order.iter().any(|ordered| ordered == id)).then_some(value)
            }))
    }

    pub fn workspace_order(&self) -> &[String] {
        &self.workspace_order
    }

    pub fn archived_session_ids(&self) -> impl Iterator<Item = &String> {
        self.archived_sessions.iter()
    }

    pub fn replay(&self, session_id: Option<&str>, since: Option<i64>) -> Vec<ControlPlaneRecord> {
        let mut records: Vec<ControlPlaneRecord> = if let Some(session_id) = session_id {
            self.records
                .get(session_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .chain(
                    self.host_records
                        .iter()
                        .filter(|record| record.session_id.as_deref() == Some(session_id))
                        .cloned(),
                )
                .collect()
        } else {
            self.host_records
                .iter()
                .cloned()
                .chain(
                    self.records
                        .values()
                        .flat_map(|items| items.iter().cloned()),
                )
                .collect()
        };
        records.retain(|record| {
            record.generation == self.generation
                && since.is_none_or(|watermark| record.sequence.is_none_or(|seq| seq > watermark))
        });
        records.sort_by_key(|record| record.at_ms);
        records
    }

    /// Return true only when the bounded cache can cover the requested event
    /// watermark. Missing watermarks always require an explicit baseline.
    pub fn can_resume(&self, session_id: Option<&str>, since: Option<i64>) -> bool {
        let Some(since) = since.filter(|value| *value >= -1) else {
            return false;
        };
        // A cursor is meaningful only for a session that was present in the
        // retained baseline. Treat an unknown id as a cache miss instead of
        // accidentally accepting an empty iterator as a lossless resume.
        if let Some(session_id) = session_id {
            if !self.sessions.contains_key(session_id) {
                return false;
            }
        }
        let targets = self
            .sessions
            .values()
            .filter(|snapshot| session_id.is_none_or(|id| snapshot.session_id == id));
        if session_id.is_none() && self.sessions.is_empty() {
            return false;
        }
        if session_id.is_none()
            && self
                .host_records
                .iter()
                .any(|record| record.sequence.is_none())
        {
            return false;
        }
        for snapshot in targets {
            // Queue/jobs/interaction/status records have no independent
            // cursor. If one is retained, the cache cannot prove it predates
            // the caller's watermark, so a fresh baseline is required.
            let retained = self.replay(Some(&snapshot.session_id), None);
            if retained.iter().any(|record| record.sequence.is_none()) {
                return false;
            }
            let Some(latest) = snapshot
                .last_seen_seq
                .or(snapshot.subscribed_last_seq)
                .or(snapshot.projection_watermark)
            else {
                continue;
            };
            if latest <= since {
                continue;
            }
            let sequenced = retained
                .into_iter()
                .filter_map(|record| record.sequence.filter(|seq| *seq > since))
                .collect::<Vec<_>>();
            let mut unique = sequenced;
            unique.sort_unstable();
            unique.dedup();
            let Some(first) = unique.first() else {
                return false;
            };
            if unique
                .windows(2)
                .any(|window| window[1] > window[0].saturating_add(1))
            {
                return false;
            }
            let Some(last) = unique.last() else {
                return false;
            };
            if *first > since.saturating_add(1) || *last < latest {
                return false;
            }
        }
        true
    }

    /// Apply a client request id exactly once.
    pub fn remember_request(&mut self, request_id: &str, payload: &Value) -> bool {
        if request_id.is_empty() {
            return false;
        }
        let fingerprint = stable_json(payload);
        if self.request_ids.contains_key(request_id) {
            return true;
        }
        self.request_ids.insert(request_id.to_string(), fingerprint);
        while self.request_ids.len() > self.options.max_records_per_session * 4 {
            let Some(first) = self.request_ids.keys().next().cloned() else {
                break;
            };
            self.request_ids.remove(&first);
        }
        false
    }

    pub fn prune(&mut self, now_ms: u64) {
        let ttl_ms = self.options.ttl.as_millis().min(u128::from(u64::MAX)) as u64;
        let expiry = now_ms.saturating_sub(ttl_ms);
        while self
            .host_records
            .front()
            .is_some_and(|record| record.at_ms < expiry)
        {
            self.host_records.pop_front();
        }
        for records in self.records.values_mut() {
            while records.front().is_some_and(|record| record.at_ms < expiry) {
                records.pop_front();
            }
        }
        self.sessions.retain(|id, snapshot| {
            let keep = snapshot.last_activity_ms >= expiry;
            if !keep {
                self.records.remove(id);
                self.seen_sequences.remove(id);
            }
            keep
        });
        if self.sessions.len() > self.options.max_sessions {
            let mut ids = self
                .sessions
                .values()
                .map(|snapshot| (snapshot.last_activity_ms, snapshot.session_id.clone()))
                .collect::<Vec<_>>();
            ids.sort_by_key(|(at, _)| *at);
            for (_, id) in ids
                .into_iter()
                .take(self.sessions.len() - self.options.max_sessions)
            {
                self.sessions.remove(&id);
                self.records.remove(&id);
                self.seen_sequences.remove(&id);
            }
        }
    }

    /// Fold one notification. All session ids are accepted; filtering to the
    /// attached presentation session happens in [`ControlPlaneRouter`].
    pub fn apply_notification(
        &mut self,
        notification: &JsonRpcNotification,
    ) -> PagerResult<ControlPlaneUpdate> {
        let now = now_ms();
        self.prune(now);
        let params = notification.params.clone().unwrap_or(Value::Null);
        let frame_generation = params.get("generation").and_then(Value::as_u64);
        if let Some(generation) = frame_generation {
            if generation < self.generation {
                return Ok(ControlPlaneUpdate {
                    accepted: false,
                    duplicate: false,
                    stale: true,
                    control: true,
                    presentation: false,
                    session_id: session_id(&params),
                    sequence: sequence(&params),
                });
            }
            if generation > self.generation {
                self.set_generation(generation);
            }
        }
        match notification.method.as_str() {
            "events.mux" => self.apply_mux(params, now),
            "events.host" => self.apply_host(params, now),
            "tui.controlPlaneBaseline" => self.apply_baseline(params, now),
            "tui.serverDraining" => {
                self.mark_phase(
                    ConnectionPhase::Draining,
                    Some("Backend is draining".into()),
                );
                Ok(ControlPlaneUpdate {
                    accepted: true,
                    control: true,
                    ..ControlPlaneUpdate::default()
                })
            }
            _ => Ok(ControlPlaneUpdate::default()),
        }
    }

    fn apply_mux(&mut self, frame: Value, now: u64) -> PagerResult<ControlPlaneUpdate> {
        let frame_type = frame
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| PagerError::new("events.mux frame omitted type"))?;
        let id = session_id(&frame);
        let seq = sequence(&frame);
        let presentation = frame_type == "session/event";
        if frame_type == "stream/error" {
            if self.remember_frame_fingerprint("mux", &frame) {
                return Ok(ControlPlaneUpdate {
                    accepted: true,
                    duplicate: true,
                    control: true,
                    session_id: id,
                    sequence: seq,
                    ..ControlPlaneUpdate::default()
                });
            }
            let message = frame
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("mux stream error")
                .to_string();
            self.mark_phase(ConnectionPhase::Reconnecting, Some(message));
            self.record("mux", id.clone(), seq, frame, now);
            return Ok(ControlPlaneUpdate {
                accepted: true,
                control: true,
                session_id: id,
                sequence: seq,
                ..ControlPlaneUpdate::default()
            });
        }
        let Some(session_id) = id.clone() else {
            return Ok(ControlPlaneUpdate {
                accepted: true,
                control: !presentation,
                presentation,
                sequence: seq,
                ..ControlPlaneUpdate::default()
            });
        };
        let event_duplicate = if frame_type == "session/event" {
            seq.is_some_and(|value| {
                let seen = self.seen_sequences.entry(session_id.clone()).or_default();
                let duplicate = !seen.insert(value);
                while seen.len() > self.options.max_records_per_session.saturating_mul(4) {
                    let Some(first) = seen.iter().next().copied() else {
                        break;
                    };
                    seen.remove(&first);
                }
                duplicate
            })
        } else {
            false
        };
        let snapshot = self.ensure_session(session_id.clone(), now);
        let mut duplicate = false;
        match frame_type {
            "session/event" => {
                duplicate = event_duplicate;
                if !duplicate {
                    if let Some(seq) = seq {
                        snapshot.last_seen_seq =
                            Some(snapshot.last_seen_seq.unwrap_or(seq).max(seq));
                    }
                }
            }
            "session/subscribed" => {
                let last = frame
                    .get("lastSeq")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| PagerError::new("session/subscribed omitted lastSeq"))?;
                if snapshot.subscribed_last_seq.is_some_and(|old| last <= old) {
                    duplicate = true;
                } else {
                    snapshot.subscribed_last_seq = Some(last);
                    snapshot.last_seen_seq = Some(snapshot.last_seen_seq.unwrap_or(last).max(last));
                }
            }
            "session/projection" => {
                let key = frame
                    .get("key")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PagerError::new("session/projection omitted key"))?;
                let seq = frame
                    .get("seq")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| PagerError::new("session/projection omitted seq"))?;
                if snapshot
                    .projections
                    .get(key)
                    .is_some_and(|current| current.seq >= seq)
                {
                    duplicate = true;
                } else {
                    snapshot.projections.insert(
                        key.to_string(),
                        SessionProjection {
                            key: key.to_string(),
                            seq,
                            value: frame.get("value").cloned().unwrap_or(Value::Null),
                        },
                    );
                    snapshot.projection_watermark =
                        Some(snapshot.projection_watermark.unwrap_or(seq).max(seq));
                    snapshot.last_seen_seq = Some(snapshot.last_seen_seq.unwrap_or(seq).max(seq));
                }
            }
            "session/queue" => {
                let items: Vec<SessionQueueItem> =
                    serde_json::from_value(frame.get("items").cloned().unwrap_or(Value::Null))?;
                duplicate = snapshot.queue_initialized && snapshot.queue == items;
                if !duplicate {
                    snapshot.queue = items;
                }
                snapshot.queue_initialized = true;
            }
            "session/jobs" => {
                let values = frame
                    .get("jobs")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let jobs = values.iter().map(JobView::from_value).collect::<Vec<_>>();
                duplicate = snapshot.jobs_initialized && snapshot.jobs == jobs;
                if !duplicate {
                    snapshot.jobs = jobs;
                }
                snapshot.jobs_initialized = true;
            }
            "approval/requested" | "question/requested" => {
                let interaction = interaction_from_frame(&frame)?;
                // approvalId/question request id are host-owned interaction
                // identities.  A replay may carry a different transport
                // request id for an approval, but it must remain one
                // actionable pending row.
                duplicate = match &interaction.kind {
                    crate::session::InteractionKind::Approval => {
                        snapshot.pending_interactions.iter().any(|existing| {
                            existing.kind == crate::session::InteractionKind::Approval
                                && existing.approval_id == interaction.approval_id
                        })
                    }
                    crate::session::InteractionKind::Question => {
                        snapshot.pending_interactions.iter().any(|existing| {
                            existing.kind == crate::session::InteractionKind::Question
                                && existing.request_id == interaction.request_id
                        })
                    }
                };
                if !duplicate {
                    snapshot.pending_interactions.push(interaction);
                }
            }
            "approval/resolved" => {
                let approval = frame.get("approvalId").and_then(Value::as_str);
                let before = snapshot.pending_interactions.len();
                snapshot
                    .pending_interactions
                    .retain(|pending| pending.approval_id.as_deref() != approval);
                duplicate = before == snapshot.pending_interactions.len();
            }
            "question/resolved" => {
                let request_id = frame
                    .get("questionRpcId")
                    .and_then(Value::as_str)
                    .or_else(|| frame.get("requestId").and_then(Value::as_str));
                let before = snapshot.pending_interactions.len();
                snapshot
                    .pending_interactions
                    .retain(|pending| request_id != Some(pending.request_id.as_str()));
                duplicate = before == snapshot.pending_interactions.len();
            }
            _ => {}
        }
        if !duplicate {
            snapshot.last_activity_ms = now;
            snapshot.updated_at_ms = Some(now);
        }
        if !duplicate && !presentation {
            self.record("mux", Some(session_id.clone()), seq, frame, now);
        }
        if !duplicate {
            self.revision = self.revision.saturating_add(1);
        }
        Ok(ControlPlaneUpdate {
            accepted: true,
            duplicate,
            control: !presentation,
            presentation,
            session_id: Some(session_id),
            sequence: seq,
            ..ControlPlaneUpdate::default()
        })
    }

    fn apply_host(&mut self, frame: Value, now: u64) -> PagerResult<ControlPlaneUpdate> {
        let frame_type = frame
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| PagerError::new("events.host frame omitted type"))?;
        let id = session_id(&frame);
        if self.remember_frame_fingerprint("host", &frame) {
            return Ok(ControlPlaneUpdate {
                accepted: true,
                duplicate: true,
                control: true,
                session_id: id,
                sequence: sequence(&frame),
                ..ControlPlaneUpdate::default()
            });
        }
        if frame_type == "stream/error" {
            let message = frame
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("host stream error")
                .to_string();
            self.mark_phase(ConnectionPhase::Reconnecting, Some(message));
        }
        if let Some(session_id) = id.clone() {
            let snapshot = self.ensure_session(session_id.clone(), now);
            match frame_type {
                "host/session-added" => {
                    snapshot.removed = false;
                    snapshot.blank = frame.get("blank").and_then(Value::as_bool);
                    snapshot.parent_session_id = frame
                        .get("parentSessionId")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    snapshot.origin = frame
                        .get("origin")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    snapshot.cwd = frame.get("cwd").and_then(Value::as_str).map(str::to_string);
                    snapshot.agent_preset = frame
                        .get("agentPreset")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                "host/session-removed" => snapshot.removed = true,
                "host/session-status" => {
                    snapshot.running = frame.get("running").and_then(Value::as_bool)
                }
                "host/agent-error" => {
                    snapshot.last_error = frame
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }
                _ => {}
            }
            snapshot.last_activity_ms = now;
            snapshot.updated_at_ms = Some(now);
        }
        match frame_type {
            "host/workspace-changed" => {
                let workspace = WorkspaceView::from_value(
                    frame.get("workspace").cloned().unwrap_or(Value::Null),
                )?;
                let id = workspace.workspace_id.clone();
                let session_ids = workspace.session_ids.clone();
                self.workspaces.insert(id.clone(), workspace);
                if !self.workspace_order.iter().any(|item| item == &id) {
                    self.workspace_order.push(id.clone());
                }
                self.update_workspace_membership(&id, &session_ids);
            }
            "host/workspace-removed" => {
                let id = frame
                    .get("workspaceId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PagerError::new("workspace removal omitted workspaceId"))?;
                self.workspaces.remove(id);
                self.workspace_order.retain(|item| item != id);
                for snapshot in self.sessions.values_mut() {
                    if snapshot.workspace_id.as_deref() == Some(id) {
                        snapshot.workspace_id = None;
                    }
                }
            }
            "host/workspace-order-changed" => {
                self.workspace_order = frame
                    .get("workspaceIds")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            "host/archived-sessions-changed" => {
                self.archived_sessions = frame
                    .get("archivedSessionIds")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                for (id, snapshot) in &mut self.sessions {
                    snapshot.archived = self.archived_sessions.contains(id);
                }
            }
            _ => {}
        }
        self.record("host", id.clone(), sequence(&frame), frame, now);
        self.revision = self.revision.saturating_add(1);
        Ok(ControlPlaneUpdate {
            accepted: true,
            control: true,
            session_id: id,
            sequence: None,
            ..ControlPlaneUpdate::default()
        })
    }

    fn update_workspace_membership(&mut self, workspace_id: &str, session_ids: &[String]) {
        let members = session_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let now = now_ms();
        for session_id in &members {
            self.ensure_session((*session_id).to_string(), now)
                .workspace_id = Some(workspace_id.to_string());
        }
        for snapshot in self.sessions.values_mut() {
            if members.contains(snapshot.session_id.as_str()) {
                snapshot.workspace_id = Some(workspace_id.to_string());
            } else if snapshot.workspace_id.as_deref() == Some(workspace_id) {
                snapshot.workspace_id = None;
            }
        }
    }

    fn apply_baseline(&mut self, frame: Value, now: u64) -> PagerResult<ControlPlaneUpdate> {
        let generation = frame
            .get("generation")
            .and_then(Value::as_u64)
            .unwrap_or(self.generation);
        if generation < self.generation {
            return Ok(ControlPlaneUpdate {
                accepted: false,
                stale: true,
                control: true,
                ..ControlPlaneUpdate::default()
            });
        }
        if generation > self.generation {
            self.set_generation(generation);
        }
        // A baseline is an atomic replacement, including the exact-sequence
        // deduplication index. Keeping sequence ids from the previous
        // baseline could make a new event look like a replay.
        self.seen_sequences.clear();
        let archived_sessions = frame
            .get("archivedSessionIds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let mut sessions = BTreeMap::new();
        if let Some(values) = frame.get("sessions").and_then(Value::as_array) {
            for value in values {
                let Some(id) = value.get("sessionId").and_then(Value::as_str) else {
                    continue;
                };
                let mut snapshot = SessionControlSnapshot::new(id.to_string(), generation, now);
                snapshot.workspace_id = value
                    .get("workspaceId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                snapshot.updated_at_ms = value
                    .get("updatedAtMs")
                    .and_then(Value::as_u64)
                    .or_else(|| value.get("updatedAt").and_then(Value::as_u64));
                snapshot.last_seen_seq = value.get("lastSeenSeq").and_then(Value::as_i64);
                snapshot.subscribed_last_seq =
                    value.get("subscribedLastSeq").and_then(Value::as_i64);
                snapshot.projection_watermark =
                    value.get("projectionWatermark").and_then(Value::as_i64);
                snapshot.running = value.get("running").and_then(Value::as_bool);
                snapshot.blank = value.get("blank").and_then(Value::as_bool);
                snapshot.parent_session_id = value
                    .get("parentSessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                snapshot.origin = value
                    .get("origin")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                snapshot.cwd = value.get("cwd").and_then(Value::as_str).map(str::to_string);
                snapshot.agent_preset = value
                    .get("agentPreset")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                snapshot.removed = value
                    .get("removed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                snapshot.archived = value
                    .get("archived")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || archived_sessions.contains(id);
                snapshot.last_activity_ms = value
                    .get("lastActivityAt")
                    .and_then(Value::as_u64)
                    .unwrap_or(now);
                if let Some(seq) = snapshot.last_seen_seq {
                    self.seen_sequences
                        .entry(id.to_string())
                        .or_default()
                        .insert(seq);
                }
                if let Some(projections) = value.get("projections").and_then(Value::as_object) {
                    for (key, cell) in projections {
                        let Some(seq) = cell.get("seq").and_then(Value::as_i64) else {
                            continue;
                        };
                        snapshot.projections.insert(
                            key.clone(),
                            SessionProjection {
                                key: key.clone(),
                                seq,
                                value: cell.get("value").cloned().unwrap_or(Value::Null),
                            },
                        );
                    }
                }
                if let Some(items) = value.get("queue").and_then(Value::as_array) {
                    snapshot.queue = serde_json::from_value(Value::Array(items.clone()))?;
                    snapshot.queue_initialized = value
                        .get("queueInitialized")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                }
                if let Some(items) = value.get("jobs").and_then(Value::as_array) {
                    snapshot.jobs = items.iter().map(JobView::from_value).collect();
                    snapshot.jobs_initialized = value
                        .get("jobsInitialized")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                }
                if let Some(items) = value.get("pendingInteractions").and_then(Value::as_array) {
                    snapshot.pending_interactions = items
                        .iter()
                        .cloned()
                        .map(serde_json::from_value)
                        .collect::<Result<Vec<PendingInteraction>, _>>()?;
                }
                snapshot.last_error = value.get("lastError").and_then(|error| {
                    error.as_str().map(str::to_string).or_else(|| {
                        error
                            .get("message")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                });
                sessions.insert(id.to_string(), snapshot);
            }
        }
        let mut workspaces = BTreeMap::new();
        if let Some(values) = frame.get("workspaces").and_then(Value::as_array) {
            for value in values {
                let raw = value.get("value").cloned().unwrap_or_else(|| value.clone());
                let workspace = WorkspaceView::from_value(raw)?;
                workspaces.insert(workspace.workspace_id.clone(), workspace);
            }
        }
        let workspace_order = frame
            .get("workspaceOrder")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        self.sessions = sessions;
        self.workspaces = workspaces;
        self.workspace_order = workspace_order;
        self.archived_sessions = archived_sessions;
        let memberships = self
            .workspaces
            .values()
            .map(|workspace| {
                (
                    workspace.workspace_id.clone(),
                    workspace.session_ids.clone(),
                )
            })
            .collect::<Vec<_>>();
        for (workspace_id, session_ids) in memberships {
            self.update_workspace_membership(&workspace_id, &session_ids);
        }
        self.host_records.clear();
        self.records.clear();
        self.host_fingerprints.clear();
        self.fingerprint_order.clear();
        if let Some(records) = frame.get("records").and_then(Value::as_array) {
            for value in records {
                let record: ControlPlaneRecord = serde_json::from_value(value.clone())?;
                if record.generation != self.generation {
                    continue;
                }
                self.insert_record(record);
            }
        }
        self.mark_phase(ConnectionPhase::Connected, None);
        self.revision = self.revision.saturating_add(1);
        Ok(ControlPlaneUpdate {
            accepted: true,
            control: true,
            ..ControlPlaneUpdate::default()
        })
    }

    fn ensure_session(&mut self, session_id: String, now: u64) -> &mut SessionControlSnapshot {
        let archived = self.archived_sessions.contains(&session_id);
        let snapshot = self
            .sessions
            .entry(session_id.clone())
            .or_insert_with(|| SessionControlSnapshot::new(session_id, self.generation, now));
        snapshot.generation = self.generation;
        if archived {
            snapshot.archived = true;
        }
        snapshot
    }

    fn remember_frame_fingerprint(&mut self, stream: &str, frame: &Value) -> bool {
        let fingerprint = format!("{stream}:{}", stable_json(frame));
        if self.host_fingerprints.contains(&fingerprint) {
            return true;
        }
        self.host_fingerprints.insert(fingerprint.clone());
        self.fingerprint_order.push_back(fingerprint);
        let limit = self.options.max_records_per_session.saturating_mul(4);
        while self.fingerprint_order.len() > limit {
            if let Some(oldest) = self.fingerprint_order.pop_front() {
                self.host_fingerprints.remove(&oldest);
            }
        }
        false
    }

    fn record(
        &mut self,
        stream: &str,
        session_id: Option<String>,
        sequence: Option<i64>,
        frame: Value,
        at_ms: u64,
    ) {
        let record = ControlPlaneRecord {
            stream: stream.to_string(),
            generation: self.generation,
            session_id: session_id.clone(),
            sequence,
            frame,
            at_ms,
        };
        self.insert_record(record);
    }

    fn insert_record(&mut self, record: ControlPlaneRecord) {
        if let Some(id) = record.session_id.clone() {
            let records = self.records.entry(id).or_default();
            records.push_back(record);
            while records.len() > self.options.max_records_per_session {
                records.pop_front();
            }
        } else {
            self.host_records.push_back(record);
            while self.host_records.len() > self.options.max_records_per_session {
                self.host_records.pop_front();
            }
        }
    }
}

/// Router that first updates the all-session store, then forwards only the
/// current session's presentation frame to `SessionState`.
#[derive(Debug, Default)]
pub struct ControlPlaneRouter {
    pub store: ControlPlaneStore,
}

impl ControlPlaneRouter {
    pub fn new(store: ControlPlaneStore) -> Self {
        Self { store }
    }

    pub fn set_generation(&mut self, generation: u64) -> bool {
        self.store.set_generation(generation)
    }

    pub fn route(
        &mut self,
        notification: JsonRpcNotification,
        session: Option<&mut SessionState>,
    ) -> PagerResult<SessionUpdate> {
        let update = self.store.apply_notification(&notification)?;
        if update.duplicate {
            return Ok(SessionUpdate::default());
        }
        let Some(session) = session else {
            return Ok(SessionUpdate {
                changed: update.accepted && !update.duplicate,
                gap_detected: false,
            });
        };
        if update.stale
            || update
                .session_id
                .as_deref()
                .is_some_and(|id| id != session.session_id())
        {
            return Ok(SessionUpdate::default());
        }
        if notification.method == "tui.controlPlaneBaseline" {
            let snapshot = self.store.snapshot(session.session_id()).cloned();
            return Ok(snapshot
                .as_ref()
                .map(|snapshot| session.apply_control_snapshot(snapshot))
                .unwrap_or_default());
        }
        session.accept_notification(notification)
    }
}

fn interaction_from_frame(frame: &Value) -> PagerResult<PendingInteraction> {
    let request_id = frame
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match frame.get("type").and_then(Value::as_str) {
        Some("approval/requested") => Ok(PendingInteraction::approval(
            request_id,
            frame
                .get("approvalId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            frame
                .get("toolName")
                .and_then(Value::as_str)
                .map(str::to_string),
            frame
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string),
        )),
        Some("question/requested") => Ok(PendingInteraction::question(
            request_id,
            frame
                .get("questions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        )),
        _ => Err(PagerError::new("unsupported interaction frame")),
    }
}

fn session_id(frame: &Value) -> Option<String> {
    frame
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn sequence(frame: &Value) -> Option<i64> {
    frame
        .pointer("/event/seq")
        .and_then(Value::as_i64)
        .or_else(|| frame.get("seq").and_then(Value::as_i64))
        .or_else(|| frame.get("lastSeq").and_then(Value::as_i64))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

/// Canonical JSON used for semantic duplicate detection. Object insertion
/// order is a transport detail; arrays retain their host-defined order.
fn stable_json(value: &Value) -> String {
    match value {
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(stable_json).collect::<Vec<_>>().join(",")
        ),
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(key, _)| *key);
            format!(
                "{{{}}}",
                entries
                    .into_iter()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        stable_json(value)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        scalar => serde_json::to_string(scalar).unwrap_or_else(|_| "null".into()),
    }
}

fn finite_epoch_ms(value: f64) -> Option<u64> {
    (value.is_finite() && value >= 0.0 && value <= u64::MAX as f64).then_some(value as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn note(method: &str, params: Value) -> JsonRpcNotification {
        JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params: Some(params),
        }
    }

    #[test]
    fn interleaved_sessions_are_folded_without_cross_session_loss() {
        let mut router = ControlPlaneRouter::default();
        router.set_generation(2);
        router
            .store
            .apply_notification(&note(
                "events.mux",
                json!({"generation":2,"type":"session/projection","sessionId":"a","key":"title","seq":4,"value":"A"}),
            ))
            .unwrap();
        router
            .store
            .apply_notification(&note(
                "events.mux",
                json!({"generation":2,"type":"session/projection","sessionId":"b","key":"title","seq":3,"value":"B"}),
            ))
            .unwrap();
        assert_eq!(
            router.store.snapshot("a").unwrap().projections["title"].value,
            json!("A")
        );
        assert_eq!(
            router.store.snapshot("b").unwrap().projections["title"].value,
            json!("B")
        );
    }

    #[test]
    fn duplicate_and_old_generation_frames_do_not_mutate_state() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(4);
        let frame = note(
            "events.mux",
            json!({"generation":4,"type":"session/subscribed","sessionId":"s","lastSeq":8}),
        );
        let first = store.apply_notification(&frame).unwrap();
        let second = store.apply_notification(&frame).unwrap();
        assert!(first.accepted);
        assert!(second.duplicate);
        let stale = store
            .apply_notification(&note(
                "events.mux",
                json!({"generation":3,"type":"session/subscribed","sessionId":"s","lastSeq":99}),
            ))
            .unwrap();
        assert!(stale.stale);
        assert_eq!(store.snapshot("s").unwrap().subscribed_last_seq, Some(8));
    }

    #[test]
    fn router_only_presents_current_session_but_stores_other_sessions() {
        let mut router = ControlPlaneRouter::default();
        router.set_generation(1);
        let mut state = SessionState::new("a".into(), 1);
        state
            .install_initial(dsh_pager_protocol::SessionHistoryValue {
                events: vec![],
                has_more: false,
                projections: None,
            })
            .unwrap();
        let update = router
            .route(
                note(
                    "events.mux",
                    json!({"generation":1,"type":"session/queue","sessionId":"b","items":[]}),
                ),
                Some(&mut state),
            )
            .unwrap();
        assert!(!update.changed);
        assert!(router.store.snapshot("b").is_some());
        assert!(state.queue().is_empty());
    }

    #[test]
    fn host_workspace_and_archive_snapshots_are_value_backed() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(1);
        store
            .apply_notification(&note(
                "events.host",
                json!({"generation":1,"type":"host/workspace-changed","workspace":{"workspaceId":"w","path":"/work","title":"Work","sessionIds":["s"]}}),
            ))
            .unwrap();
        store
            .apply_notification(&note(
                "events.host",
                json!({"generation":1,"type":"host/archived-sessions-changed","archivedSessionIds":["s"]}),
            ))
            .unwrap();
        assert_eq!(store.workspaces().next().unwrap().workspace_id, "w");
        assert_eq!(
            store.archived_session_ids().next().map(String::as_str),
            Some("s")
        );
    }

    #[test]
    fn baseline_replaces_state_and_restores_control_snapshots() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(5);
        store
            .apply_notification(&note(
                "tui.controlPlaneBaseline",
                json!({
                    "generation": 5,
                    "resumeClass": "baseline-required",
                    "sessions": [{
                        "sessionId": "s",
                        "generation": 5,
                        "workspaceId": "w",
                        "lastSeenSeq": 4,
                        "projections": {"title": {"seq": 4, "value": "Title"}},
                        "queue": [],
                        "jobs": [{"id": "j", "kind": "task", "status": "running"}],
                        "pendingInteractions": [{"requestId": "r", "kind": "approval", "approvalId": "a"}],
                        "lastActivityAt": 42
                    }],
                    "workspaces": [{"workspaceId": "w", "value": {"workspaceId": "w", "path": "/work", "title": "Work", "sessionIds": ["s"]}}],
                    "workspaceOrder": ["w"],
                    "archivedSessionIds": [],
                    "records": [{"stream": "mux", "generation": 5, "sessionId": "s", "sequence": 4, "frame": {"type": "session/projection", "sessionId": "s", "key": "title", "seq": 4, "value": "Title"}, "at": 42}]
                }),
            ))
            .unwrap();
        let snapshot = store.snapshot("s").unwrap();
        assert_eq!(snapshot.workspace_id.as_deref(), Some("w"));
        assert_eq!(snapshot.projections["title"].value, json!("Title"));
        assert_eq!(snapshot.jobs.len(), 1);
        assert_eq!(snapshot.pending_interactions.len(), 1);
        assert_eq!(store.workspaces().next().unwrap().workspace_id, "w");
        assert_eq!(store.replay(Some("s"), None).len(), 1);
        assert_eq!(store.connection().phase, ConnectionPhase::Connected);
    }

    #[test]
    fn baseline_routes_current_session_controls_into_session_state() {
        let mut router = ControlPlaneRouter::default();
        router.set_generation(2);
        let mut state = SessionState::new("s".into(), 2);
        state
            .install_initial(dsh_pager_protocol::SessionHistoryValue {
                events: vec![],
                has_more: false,
                projections: None,
            })
            .unwrap();
        let update = router
            .route(
                note(
                    "tui.controlPlaneBaseline",
                    json!({
                        "generation": 2,
                        "resumeClass": "baseline-required",
                        "sessions": [{
                            "sessionId":"s",
                            "generation":2,
                            "subscribedLastSeq":3,
                            "projections":{"title":{"seq":3,"value":"from baseline"}},
                            "queue":[],
                            "queueInitialized":true,
                            "pendingInteractions":[{"requestId":"q","kind":"question","questions":[]}]
                        }],
                        "workspaces": [],
                        "workspaceOrder": [],
                        "archivedSessionIds": [],
                        "records": []
                    }),
                ),
                Some(&mut state),
            )
            .unwrap();
        assert!(update.changed);
        assert_eq!(state.title(), Some("from baseline"));
        assert!(state.queue().is_empty());
        assert_eq!(state.pending_interactions().len(), 1);
        assert!(state.needs_repair());
    }

    #[test]
    fn host_duplicates_and_replay_watermarks_are_idempotent() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(1);
        let status = note(
            "events.host",
            json!({"generation": 1, "type": "host/session-status", "sessionId": "s", "running": true}),
        );
        assert!(!store.apply_notification(&status).unwrap().duplicate);
        assert!(store.apply_notification(&status).unwrap().duplicate);
        let projection = note(
            "events.mux",
            json!({"generation": 1, "type": "session/projection", "sessionId": "s", "key": "title", "seq": 2, "value": "x"}),
        );
        store.apply_notification(&projection).unwrap();
        assert!(!store.can_resume(Some("s"), Some(1)));
        assert!(!store.can_resume(Some("s"), None));
        assert!(!store.can_resume(Some("missing"), Some(-1)));
        store
            .apply_notification(&note(
                "events.mux",
                json!({"generation":1,"type":"session/queue","sessionId":"s","items":[]}),
            ))
            .unwrap();
        assert!(!store.can_resume(Some("s"), Some(2)));
    }

    #[test]
    fn baseline_resets_exact_sequence_index() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(1);
        let event = |seq| {
            note(
                "events.mux",
                json!({"generation":1,"type":"session/event","sessionId":"s","event":{"type":"assistant/message","seq":seq,"time":seq,"data":{}}}),
            )
        };
        assert!(!store.apply_notification(&event(7)).unwrap().duplicate);
        store
            .apply_notification(&note(
                "tui.controlPlaneBaseline",
                json!({
                    "generation": 1,
                    "resumeClass": "baseline-required",
                    "sessions": [{"sessionId":"s","generation":1,"lastSeenSeq":7}],
                    "workspaces": [],
                    "workspaceOrder": [],
                    "archivedSessionIds": [],
                    "records": []
                }),
            ))
            .unwrap();
        // The baseline represents the current cursor, so an exact replay of
        // that cursor is ignored, while a later unseen event is kept.
        assert!(store.apply_notification(&event(7)).unwrap().duplicate);
        assert!(!store.apply_notification(&event(8)).unwrap().duplicate);
    }

    #[test]
    fn unseen_out_of_order_events_are_not_confused_with_exact_replays() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(1);
        let event = |seq| {
            note(
                "events.mux",
                json!({"generation":1,"type":"session/event","sessionId":"s","event":{"type":"assistant/message","seq":seq,"time":seq,"data":{}}}),
            )
        };
        assert!(!store.apply_notification(&event(5)).unwrap().duplicate);
        assert!(!store.apply_notification(&event(4)).unwrap().duplicate);
        assert!(store.apply_notification(&event(4)).unwrap().duplicate);
        assert_eq!(store.snapshot("s").unwrap().last_seen_seq, Some(5));
    }

    #[test]
    fn resume_requires_contiguous_retained_sequences() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(1);
        let event = |seq| {
            note(
                "events.mux",
                json!({"generation":1,"type":"session/event","sessionId":"s","event":{"type":"assistant/message","seq":seq,"time":seq,"data":{}}}),
            )
        };
        store.apply_notification(&event(0)).unwrap();
        store.apply_notification(&event(2)).unwrap();
        assert!(!store.can_resume(Some("s"), Some(-1)));
    }

    #[test]
    fn approval_replay_uses_approval_id_not_transport_request_id() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(1);
        let approval = |request_id| {
            note(
                "events.mux",
                json!({"generation":1,"type":"approval/requested","sessionId":"s","requestId":request_id,"approvalId":"a","toolName":"rm"}),
            )
        };
        assert!(
            !store
                .apply_notification(&approval("rpc-a"))
                .unwrap()
                .duplicate
        );
        assert!(
            store
                .apply_notification(&approval("rpc-b"))
                .unwrap()
                .duplicate
        );
        assert_eq!(store.snapshot("s").unwrap().pending_interactions.len(), 1);
    }

    #[test]
    fn session_list_preserves_host_activity_without_affecting_ttl_clock() {
        let mut store = ControlPlaneStore::default();
        store.set_generation(1);
        let list: SessionListValue = serde_json::from_value(json!({
            "items": [{"sessionId":"s","updatedAt":7,"running":false,"blank":false}]
        }))
        .unwrap();
        store.seed_session_list(&list);
        let snapshot = store.snapshot("s").unwrap();
        assert_eq!(snapshot.updated_at_ms, Some(7));
        assert!(snapshot.last_activity_ms >= 7);
    }
}
