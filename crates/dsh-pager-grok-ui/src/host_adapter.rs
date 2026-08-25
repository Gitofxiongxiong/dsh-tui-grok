//! Convert DSH-owned snapshots into data accepted by the copied Grok views.
//!
//! The old fallback rows remain as a compatibility projection until the full
//! Grok shell consumes every snapshot partition. They are intentionally
//! stable and must not become a second host-owned view model.

use dsh_pager::{
    ControlPlaneStore, Diagnostic, DshGeneration, DshInteraction, DshPresentationModel,
    DshQueueItem, DshRenderBlock, DshRenderContent, DshRenderEntry, DshRenderEntryId,
    DshRenderFinish, DshRenderKind, DshRenderVisibility, DshSeq, DshSessionId, SessionState,
    event_time_epoch_ms,
};
use dsh_pager_protocol::{
    PromptMode, SessionEvent, SessionListValue, SessionModeId, SessionSearchValue, SessionSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A renderer-facing transcript row.  It deliberately contains no protocol
/// or `SessionState` references, which keeps view code host-agnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptRow {
    pub id: DshRenderEntryId,
    pub created_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    pub label: String,
    pub text: String,
    pub kind: DshRenderKind,
    pub visibility: DshRenderVisibility,
    pub finish: DshRenderFinish,
    pub group_key: Option<String>,
    pub selectable: bool,
    pub source_seq: i64,
    pub seq: DshSeq,
    pub content: DshRenderContent,
}

/// Explicit terminal and host capability contract. `false` means unavailable
/// or not negotiated; it never means the UI should silently fake support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct CapabilityMatrix {
    pub mouse: bool,
    pub paste: bool,
    pub osc52: bool,
    pub image: bool,
    pub bidi: bool,
    pub external_editor: bool,
    pub external_pager: bool,
    pub queue_steer: bool,
    pub workspace_actions: bool,
    pub prompt_history: bool,
    pub prompt_suggestions: bool,
    pub file_search: bool,
    pub subagents: bool,
}

impl Default for CapabilityMatrix {
    fn default() -> Self {
        Self {
            mouse: false,
            paste: true,
            osc52: false,
            image: false,
            bidi: false,
            external_editor: false,
            external_pager: false,
            queue_steer: true,
            workspace_actions: false,
            prompt_history: false,
            prompt_suggestions: false,
            file_search: false,
            subagents: false,
        }
    }
}

impl CapabilityMatrix {
    fn from_session(session: &SessionState) -> Self {
        let mut capabilities = Self::default();
        let Some(value) = session.projection("capabilities") else {
            return capabilities;
        };
        let Some(object) = value.as_object() else {
            return capabilities;
        };
        macro_rules! read {
            ($field:ident) => {
                if let Some(value) = object
                    .get(stringify!($field))
                    .or_else(|| object.get(capability_camel_key(stringify!($field))))
                    .and_then(|value| value.as_bool())
                {
                    capabilities.$field = value;
                }
            };
        }
        read!(mouse);
        read!(paste);
        read!(osc52);
        read!(image);
        read!(bidi);
        read!(external_editor);
        read!(external_pager);
        read!(queue_steer);
        read!(workspace_actions);
        read!(prompt_history);
        read!(prompt_suggestions);
        read!(file_search);
        read!(subagents);
        capabilities
    }
}

fn capability_camel_key(key: &str) -> &str {
    match key {
        "external_editor" => "externalEditor",
        "external_pager" => "externalPager",
        "queue_steer" => "queueSteer",
        "workspace_actions" => "workspaceActions",
        "prompt_history" => "promptHistory",
        "prompt_suggestions" => "promptSuggestions",
        "file_search" => "fileSearch",
        "subagents" => "subagents",
        _ => key,
    }
}

/// Availability of a user-visible feature in the current host snapshot.
///
/// `Pending` is deliberately distinct from `Unsupported`: the former means
/// the UI must wait for an authoritative host result, while the latter must
/// render the Grok-defined fallback and diagnostic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeatureStatus {
    Available,
    Pending,
    #[default]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearchPreview {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearchRow {
    pub id: String,
    pub path: String,
    /// Path-only providers expose a kind (file/directory/etc.) but do not
    /// imply that line preview is available.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<FileSearchPreview>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearchSnapshot {
    pub status: FeatureStatus,
    pub query: String,
    #[serde(default)]
    pub revision: u64,
    /// Availability of line/snippet preview is separate from path candidates.
    /// A path-only result must never be rendered as a fabricated line 0.
    #[serde(default)]
    pub preview_status: FeatureStatus,
    pub selected_id: Option<String>,
    pub rows: Vec<FileSearchRow>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionSnapshot {
    pub status: FeatureStatus,
    pub active: bool,
    pub selected: Option<usize>,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRow {
    pub id: String,
    #[serde(default)]
    pub attachment_id: Option<String>,
    pub media_type: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaSnapshot {
    pub status: FeatureStatus,
    pub rows: Vec<MediaRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRow {
    pub id: String,
    pub title: String,
    pub path: String,
    pub session_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub status: FeatureStatus,
    pub actions_supported: bool,
    pub order: Vec<String>,
    pub rows: Vec<WorkspaceRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentRow {
    pub id: String,
    pub parent_id: String,
    pub label: String,
    pub mode: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub activity: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub started_at_ms: Option<u64>,
    #[serde(default)]
    pub finished_at_ms: Option<u64>,
    #[serde(default)]
    pub context_pct: Option<u8>,
    #[serde(default)]
    pub running: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub status: FeatureStatus,
    pub tasks: Vec<TaskRow>,
    pub subagents: Vec<SubagentRow>,
}

/// Authoritative context pressure published by Harness's token-meter
/// projection. `used_tokens` is replay-aware projected occupancy when
/// available; no transcript-side estimate is stored here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageSnapshot {
    pub used_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

/// DSH-neutral activity consumed by Grok's single-row turn status renderer.
/// The adapter deliberately names only states that can be proven from the
/// Harness event log; Grok-only watcher/MCP/goal states are not fabricated.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TurnActivitySnapshot {
    Thinking,
    Responding,
    ToolRunning {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Compacting,
    Retrying {
        attempt: u64,
    },
    WritingToolCall,
    #[default]
    Waiting,
    WaitingForInput,
}

/// Replay-stable turn timing and activity projection. Timestamps are host
/// event epoch milliseconds; the renderer computes elapsed time per frame so
/// constructing a snapshot remains deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStatusSnapshot {
    pub visible: bool,
    pub activity: TurnActivitySnapshot,
    pub turn_started_at_ms: Option<u64>,
    pub activity_started_at_ms: Option<u64>,
    pub pending_user_input: bool,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHeader {
    pub id: DshSessionId,
    pub generation: DshGeneration,
    pub title: String,
    pub model: String,
    pub connection: String,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentViewSnapshot {
    pub status: Option<String>,
    pub queue_revision: u64,
    pub interaction: Option<DshInteraction>,
    pub diagnostics: Vec<DiagnosticView>,
}

/// Host-owned job/subagent projection used by task/status surfaces.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    pub detail: Option<String>,
    #[serde(default)]
    pub started_at_ms: Option<u64>,
    #[serde(default)]
    pub finished_at_ms: Option<u64>,
}

impl TaskRow {
    /// DSH's registry keeps a job live through the cancellation handshake.
    pub fn is_live(&self) -> bool {
        matches!(
            self.status.trim().to_ascii_lowercase().as_str(),
            "running" | "stopping"
        )
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.status.trim().to_ascii_lowercase().as_str(),
            "running" | "stopping" | "pending" | "queued" | "active" | "watching"
        )
    }
}

impl SubagentRow {
    pub fn is_running(&self) -> bool {
        self.running
            || self.status.as_deref().is_some_and(|status| {
                matches!(
                    status.trim().to_ascii_lowercase().as_str(),
                    "running" | "active" | "thinking" | "working" | "waiting"
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticView {
    pub level: String,
    pub code: String,
    pub message: String,
}

impl From<&Diagnostic> for DiagnosticView {
    fn from(diagnostic: &Diagnostic) -> Self {
        Self {
            level: format!("{:?}", diagnostic.level).to_lowercase(),
            code: diagnostic.code.clone(),
            message: diagnostic.message.clone(),
        }
    }
}

/// Prompt metadata is a contract, not the local editor draft. The draft is
/// owned by the Grok input state and becomes authoritative only after receipt
/// and a subsequent host snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSnapshot {
    pub default_mode: PromptMode,
    pub supports_multiline: bool,
    pub authoritative: bool,
    #[serde(default)]
    pub history: Vec<String>,
    #[serde(default)]
    pub suggestions: Vec<String>,
}

/// Snapshot consumed by the Grok shell in one draw pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrokHostSnapshot {
    pub session_title: String,
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    pub model: String,
    pub connection: String,
    pub status: Option<String>,
    pub running: bool,
    pub transcript: Vec<TranscriptRow>,
    #[serde(default)]
    pub transcript_len: usize,
    pub picker_rows: Vec<HostRowOwned>,
    pub session_header: SessionHeader,
    pub agent_view: AgentViewSnapshot,
    pub prompt: PromptSnapshot,
    pub queue: Vec<DshQueueItem>,
    pub queue_revision: u64,
    pub interaction: Option<DshInteraction>,
    pub tasks: Vec<TaskRow>,
    pub diagnostics: Vec<DiagnosticView>,
    #[serde(default)]
    pub file_search: FileSearchSnapshot,
    #[serde(default)]
    pub suggestions: SuggestionSnapshot,
    #[serde(default)]
    pub media: MediaSnapshot,
    #[serde(default)]
    pub workspace: WorkspaceSnapshot,
    #[serde(default)]
    pub agent: AgentSnapshot,
    #[serde(default)]
    pub context_usage: ContextUsageSnapshot,
    #[serde(default)]
    pub turn_status: TurnStatusSnapshot,
    #[serde(default)]
    pub session_mode: SessionModeId,
    pub capabilities: CapabilityMatrix,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRowOwned {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub expanded: bool,
}

/// Project the authoritative `session.list` value into the native-only Grok
/// resume picker DTO. Blank, subagent and non-root rows stay on their dedicated
/// DSH surfaces instead of appearing as resumable top-level conversations.
pub fn resume_picker_entries(
    list: &SessionListValue,
    current: &SessionState,
) -> Vec<crate::views::session_picker::SessionPickerEntry> {
    let mut entries = list
        .items
        .iter()
        .filter(|summary| {
            !summary.blank
                && summary.parent_session_id.is_none()
                && !summary
                    .origin
                    .as_deref()
                    .is_some_and(|origin| origin.eq_ignore_ascii_case("subagent"))
        })
        .map(resume_picker_entry)
        .collect::<Vec<_>>();
    if !entries.iter().any(|entry| entry.id == current.session_id()) {
        entries.push(crate::views::session_picker::SessionPickerEntry {
            id: current.session_id().to_string(),
            summary: current
                .title()
                .map(str::to_string)
                .unwrap_or_else(|| format!("Session {}", current.session_id())),
            updated_at_ms: 0,
            cwd: current
                .projection("cwd")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            model_id: current
                .projection("model")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    entries
}

fn resume_picker_entry(
    summary: &SessionSummary,
) -> crate::views::session_picker::SessionPickerEntry {
    let projection = |keys: &[&str]| {
        summary.projections.as_ref().and_then(|projections| {
            keys.iter().find_map(|key| {
                projections
                    .values
                    .get(*key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
            })
        })
    };
    crate::views::session_picker::SessionPickerEntry {
        id: summary.session_id.clone(),
        summary: projection(&["title", "summary", "lastTurnSummary"])
            .unwrap_or_default()
            .to_string(),
        updated_at_ms: finite_epoch_ms(summary.updated_at),
        cwd: summary.cwd.clone().unwrap_or_else(|| "unknown".to_string()),
        model_id: summary
            .agent_preset
            .clone()
            .or_else(|| projection(&["model", "modelId"]).map(str::to_string)),
    }
}

pub fn resume_picker_search_hits(
    value: &SessionSearchValue,
) -> Vec<crate::views::session_picker::SessionSearchHit> {
    value
        .items
        .iter()
        .map(|item| crate::views::session_picker::SessionSearchHit {
            session_id: item.session_id.clone(),
            snippet: item.snippet.clone(),
        })
        .collect()
}

fn finite_epoch_ms(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value.min(u64::MAX as f64) as u64
    } else {
        0
    }
}

impl GrokHostSnapshot {
    pub fn from_session(session: &SessionState) -> Self {
        Self::build_from_session(session, None, true)
    }

    /// Project the current session and all control-plane roster rows into the
    /// picker contract. The control-plane store is authoritative for rows;
    /// the loaded SessionState remains authoritative for the active transcript.
    pub fn from_session_with_control_plane(
        session: &SessionState,
        control_plane: Option<&ControlPlaneStore>,
    ) -> Self {
        Self::build_from_session(session, control_plane, true)
    }

    /// Production draw snapshot. Transcript storage stays host-owned; this
    /// keeps only a small tail for interaction/turn-status projection while
    /// the scrollback renderer consumes borrowed/windowed entries directly.
    pub fn for_render(session: &SessionState, control_plane: Option<&ControlPlaneStore>) -> Self {
        Self::build_from_session(session, control_plane, false)
    }

    fn build_from_session(
        session: &SessionState,
        control_plane: Option<&ControlPlaneStore>,
        include_transcript: bool,
    ) -> Self {
        let cwd = control_plane
            .and_then(|store| store.snapshot(session.session_id()))
            .and_then(|snapshot| snapshot.cwd.clone())
            .or_else(|| {
                session
                    .projection("cwd")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            });
        let model = session
            .projection("model")
            .and_then(|value| value.as_str())
            .unwrap_or("deepseek")
            .to_string();
        let history = projection_strings(session, &["promptHistory", "history"]);
        let suggestions = projection_strings(session, &["promptSuggestions", "suggestions"]);
        let title = session
            .title()
            .map(str::to_string)
            .unwrap_or_else(|| format!("Session {}", session.session_id()));
        let mut presentation = if include_transcript {
            session.presentation_model()
        } else {
            session.presentation_controls()
        };
        let interaction = presentation.interaction.clone();
        let transcript_len = session.scrollback.entries().len();
        let transcript: Vec<TranscriptRow> = if include_transcript {
            std::mem::take(&mut presentation.entries)
                .into_iter()
                .map(TranscriptRow::from)
                .collect()
        } else {
            render_support_transcript(session, interaction.as_ref())
        };
        let mut picker_rows = Vec::new();
        if let Some(control_plane) = control_plane {
            for row in control_plane.snapshots() {
                let row_title = row
                    .projections
                    .get("title")
                    .and_then(|projection| projection.value.as_str())
                    .unwrap_or(row.session_id.as_str())
                    .to_string();
                let state = if row.removed {
                    "gone"
                } else if row.archived {
                    "archived"
                } else if row.running == Some(true) {
                    "running"
                } else {
                    "idle"
                };
                let detail = row.workspace_id.as_deref().map_or_else(
                    || state.to_string(),
                    |workspace| format!("{state} · {workspace}"),
                );
                picker_rows.push(HostRowOwned {
                    id: row.session_id.clone(),
                    label: row_title,
                    detail,
                    expanded: row.session_id == session.session_id(),
                });
            }
        }
        if !picker_rows.iter().any(|row| row.id == session.session_id()) {
            picker_rows.push(HostRowOwned {
                id: session.session_id().to_string(),
                label: title.clone(),
                detail: if session.running() {
                    "attached · running".into()
                } else {
                    "attached · idle".into()
                },
                expanded: true,
            });
        }
        if !session.queue().is_empty() {
            picker_rows.push(HostRowOwned {
                id: format!("{}:queue", session.session_id()),
                label: "Queued prompts".into(),
                detail: format!("{} item(s)", session.queue().len()),
                expanded: false,
            });
        }
        if let Some(diagnostic) = session.latest_diagnostic() {
            picker_rows.push(HostRowOwned {
                id: format!(
                    "{}:diagnostic:{}",
                    session.session_id(),
                    session.diagnostics().len()
                ),
                label: "Latest diagnostic".into(),
                detail: diagnostic.message.clone(),
                expanded: false,
            });
        }
        let diagnostics: Vec<DiagnosticView> = session
            .diagnostics()
            .iter()
            .map(DiagnosticView::from)
            .collect();
        let capabilities = CapabilityMatrix::from_session(session);
        let context_usage = context_usage_snapshot(session);
        let turn_status = turn_status_snapshot(
            session,
            &transcript,
            interaction.as_ref(),
            context_usage.used_tokens,
        );
        let queue = presentation.queue.clone();
        let queue_revision = presentation.queue_revision;
        let tasks = control_plane
            .and_then(|store| store.snapshot(session.session_id()))
            .map(|snapshot| {
                snapshot
                    .jobs
                    .iter()
                    .map(|job| TaskRow {
                        id: job.id.clone(),
                        kind: job.kind.clone(),
                        label: job.label.clone(),
                        status: job.status.clone(),
                        detail: job.detail.clone(),
                        started_at_ms: job.started_at.and_then(|value| u64::try_from(value).ok()),
                        finished_at_ms: job.finished_at.and_then(|value| u64::try_from(value).ok()),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let file_search = file_search_snapshot(session, capabilities.file_search);
        let suggestion_snapshot = SuggestionSnapshot {
            status: feature_status(capabilities.prompt_suggestions),
            active: false,
            selected: None,
            items: suggestions.clone(),
        };
        let media = media_snapshot(&transcript, capabilities.image);
        let workspace = workspace_snapshot(control_plane, capabilities.workspace_actions);
        let agent = AgentSnapshot {
            status: feature_status(control_plane.is_some() || !tasks.is_empty()),
            tasks: tasks.clone(),
            subagents: Vec::new(),
        };
        let session_header = SessionHeader {
            id: DshSessionId::new(session.session_id()),
            generation: DshGeneration::new(session.generation()),
            title: title.clone(),
            model: model.clone(),
            connection: format!("{:?}", session.connection_phase()).to_lowercase(),
            running: session.running(),
        };
        let agent_view = AgentViewSnapshot {
            status: session.status_message().map(str::to_string),
            queue_revision,
            interaction: interaction.clone(),
            diagnostics: diagnostics.clone(),
        };
        Self {
            session_title: title,
            session_id: session.session_id().to_string(),
            cwd,
            model,
            connection: format!("{:?}", session.connection_phase()).to_lowercase(),
            status: session.status_message().map(str::to_string),
            running: session.running(),
            transcript,
            transcript_len,
            picker_rows,
            session_header,
            agent_view,
            prompt: PromptSnapshot {
                default_mode: PromptMode::Steer,
                supports_multiline: true,
                authoritative: false,
                history,
                suggestions,
            },
            queue,
            queue_revision,
            interaction,
            tasks,
            diagnostics,
            file_search,
            suggestions: suggestion_snapshot,
            media,
            workspace,
            agent,
            context_usage,
            turn_status,
            session_mode: crate::session_mode::derive_session_mode(session),
            capabilities,
        }
    }

    /// A deterministic fixture used by adapter/render tests without starting
    /// a backend process.
    pub fn demo() -> Self {
        Self {
            session_title: "DeepSeek / Grok UI adapter".into(),
            session_id: "demo".into(),
            cwd: "/work/demo".into(),
            model: "deepseek-reasoner".into(),
            connection: "connected".into(),
            status: None,
            running: true,
            transcript: Vec::new(),
            transcript_len: 0,
            picker_rows: vec![
                HostRowOwned {
                    id: "demo".into(),
                    label: "Current session".into(),
                    detail: "attached · live".into(),
                    expanded: true,
                },
                HostRowOwned {
                    id: "demo:tasks".into(),
                    label: "Workspace tasks".into(),
                    detail: "3 jobs".into(),
                    expanded: false,
                },
                HostRowOwned {
                    id: "demo:archive".into(),
                    label: "Archived sessions".into(),
                    detail: "12 sessions".into(),
                    expanded: false,
                },
            ],
            session_header: SessionHeader {
                id: DshSessionId::new("demo"),
                generation: DshGeneration::new(1),
                title: "DeepSeek / Grok UI adapter".into(),
                model: "deepseek-reasoner".into(),
                connection: "connected".into(),
                running: true,
            },
            agent_view: AgentViewSnapshot {
                status: None,
                queue_revision: 0,
                interaction: None,
                diagnostics: Vec::new(),
            },
            prompt: PromptSnapshot {
                default_mode: PromptMode::Steer,
                supports_multiline: true,
                authoritative: false,
                history: Vec::new(),
                suggestions: Vec::new(),
            },
            queue: Vec::new(),
            queue_revision: 0,
            interaction: None,
            tasks: Vec::new(),
            diagnostics: Vec::new(),
            file_search: FileSearchSnapshot::default(),
            suggestions: SuggestionSnapshot::default(),
            media: MediaSnapshot::default(),
            workspace: WorkspaceSnapshot::default(),
            agent: AgentSnapshot::default(),
            context_usage: ContextUsageSnapshot::default(),
            turn_status: TurnStatusSnapshot {
                visible: true,
                activity: TurnActivitySnapshot::Thinking,
                total_tokens: Some(12_000),
                ..TurnStatusSnapshot::default()
            },
            session_mode: SessionModeId::Normal,
            capabilities: CapabilityMatrix::default(),
        }
    }

    pub fn picker_entries(&self) -> Vec<crate::views::picker::PickerEntry<'_>> {
        self.picker_entries_filtered("")
    }

    /// Apply the host-side fuzzy boundary expected by the Grok picker caller.
    /// The picker itself still owns cursor/navigation state.
    pub fn picker_entries_filtered(
        &self,
        query: &str,
    ) -> Vec<crate::views::picker::PickerEntry<'_>> {
        let query = query.trim().to_lowercase();
        self.picker_rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                query.is_empty()
                    || row.label.to_lowercase().contains(&query)
                    || row.detail.to_lowercase().contains(&query)
            })
            .map(|(_, row)| {
                crate::views::picker::PickerEntry::Row(crate::views::picker::PickerRow {
                    label: &row.label,
                    right_label: &row.detail,
                    selected: false,
                    expanded: row.expanded,
                    fields: &[],
                    description_lines: &[],
                    summary_lines: &[],
                    dimmed: false,
                    indent: 0,
                    badge: "",
                    badge_color: None,
                    collapsible: true,
                    underline_last_desc: false,
                })
            })
            .collect()
    }

    /// Stable target IDs in the exact order used by the filtered picker.
    /// Non-session helper rows (queue/diagnostic) are intentionally excluded.
    pub fn picker_session_ids_filtered(&self, query: &str) -> Vec<&str> {
        let query = query.trim().to_lowercase();
        self.picker_rows
            .iter()
            .filter(|row| {
                !row.id.contains(':')
                    && (query.is_empty()
                        || row.label.to_lowercase().contains(&query)
                        || row.detail.to_lowercase().contains(&query))
            })
            .map(|row| row.id.as_str())
            .collect()
    }

    pub fn picker_row_ids_filtered(&self, query: &str) -> Vec<&str> {
        let query = query.trim().to_lowercase();
        self.picker_rows
            .iter()
            .filter(|row| {
                query.is_empty()
                    || row.label.to_lowercase().contains(&query)
                    || row.detail.to_lowercase().contains(&query)
            })
            .map(|row| row.id.as_str())
            .collect()
    }
}

fn render_support_transcript(
    session: &SessionState,
    interaction: Option<&DshInteraction>,
) -> Vec<TranscriptRow> {
    let wanted_call = interaction.and_then(|interaction| match interaction {
        DshInteraction::Approval { call_id, .. } => call_id.as_deref(),
        DshInteraction::Question { .. } => None,
    });
    let mut rows = session
        .scrollback
        .render_entry_refs()
        .rev()
        .filter(|entry| {
            entry.kind == DshRenderKind::ToolCall
                && (entry.finish == DshRenderFinish::Running
                    || wanted_call.is_some_and(|wanted| {
                        entry.content.blocks.iter().any(|block| {
                            matches!(
                                block,
                                DshRenderBlock::ToolCall {
                                    call_id: Some(candidate),
                                    ..
                                } if candidate == wanted
                            )
                        })
                    }))
        })
        .take(4)
        .map(|entry| TranscriptRow::from(entry.to_owned()))
        .collect::<Vec<_>>();
    rows.reverse();
    rows
}

impl From<DshRenderEntry> for TranscriptRow {
    fn from(entry: DshRenderEntry) -> Self {
        Self {
            id: entry.id,
            created_at_ms: entry.created_at_ms,
            started_at_ms: entry.started_at_ms,
            finished_at_ms: entry.finished_at_ms,
            label: entry.kind.label().to_string(),
            text: entry.text,
            kind: entry.kind,
            visibility: entry.visibility,
            finish: entry.finish,
            group_key: entry.group_key,
            selectable: entry.selectable,
            source_seq: entry.source_seq,
            seq: DshSeq::new(entry.source_seq),
            content: entry.content,
        }
    }
}

fn feature_status(enabled: bool) -> FeatureStatus {
    if enabled {
        FeatureStatus::Available
    } else {
        FeatureStatus::Unsupported
    }
}

fn context_usage_snapshot(session: &SessionState) -> ContextUsageSnapshot {
    let Some(object) = session
        .projection("contextPressure")
        .and_then(Value::as_object)
    else {
        return ContextUsageSnapshot::default();
    };
    let used_tokens = object
        .get("projectedTokens")
        .and_then(Value::as_u64)
        .or_else(|| object.get("pressureTokens").and_then(Value::as_u64));
    let total_tokens = object
        .get("contextWindow")
        .and_then(Value::as_u64)
        .filter(|total| *total > 0);
    ContextUsageSnapshot {
        used_tokens,
        total_tokens,
    }
}

fn turn_status_snapshot(
    session: &SessionState,
    transcript: &[TranscriptRow],
    interaction: Option<&DshInteraction>,
    total_tokens: Option<u64>,
) -> TurnStatusSnapshot {
    if !session.running() {
        return TurnStatusSnapshot::default();
    }

    let history = session.history();
    let turn_start = history
        .iter()
        .rposition(|entry| entry.event.event_type == "turn/start");
    let turn_started_at_ms = turn_start.and_then(|index| event_epoch_ms(history[index].event.time));
    let mut activity = TurnActivitySnapshot::Waiting;
    let mut activity_started_at_ms = turn_started_at_ms;

    for entry in turn_start.map_or(history, |index| &history[index.saturating_add(1)..]) {
        let Some(next) = activity_for_event(&entry.event, entry.view.as_ref()) else {
            continue;
        };
        if next != activity {
            activity = next;
            activity_started_at_ms = event_epoch_ms(entry.event.time).or(activity_started_at_ms);
        }
    }

    // The typed presentation surface is authoritative for a still-running
    // tool's final title/description. Event folding above owns phase timing,
    // so streaming deltas cannot reset the timer on every frame.
    if let Some(tool) = transcript.iter().rev().find_map(running_tool_activity) {
        if !matches!(activity, TurnActivitySnapshot::ToolRunning { .. }) {
            activity_started_at_ms = transcript
                .iter()
                .rev()
                .find(|row| row.finish == DshRenderFinish::Running)
                .and_then(|row| event_time_for_seq(history, row.source_seq))
                .or(activity_started_at_ms);
        }
        activity = tool;
    }

    let pending_user_input = interaction.is_some();
    if pending_user_input && !matches!(activity, TurnActivitySnapshot::ToolRunning { .. }) {
        activity = TurnActivitySnapshot::WaitingForInput;
    }

    TurnStatusSnapshot {
        visible: true,
        activity,
        turn_started_at_ms,
        activity_started_at_ms,
        pending_user_input,
        total_tokens,
    }
}

fn activity_for_event(event: &SessionEvent, view: Option<&Value>) -> Option<TurnActivitySnapshot> {
    match event.event_type.as_str() {
        "turn/start" | "step/start" | "tool/result" | "step/end" => {
            Some(TurnActivitySnapshot::Waiting)
        }
        "assistant/chunk" => assistant_chunk_activity(&event.data),
        "assistant/message" => assistant_message_activity(&event.data),
        "tool/call" | "command/run" => Some(tool_activity(&event.data, view)),
        "llm/retry" => Some(TurnActivitySnapshot::Retrying {
            attempt: event
                .data
                .get("retry")
                .or_else(|| event.data.get("attempt"))
                .and_then(Value::as_u64)
                .unwrap_or(1),
        }),
        "compaction/start" => Some(TurnActivitySnapshot::Compacting),
        "compaction/end" => Some(TurnActivitySnapshot::Waiting),
        _ => None,
    }
}

fn assistant_chunk_activity(data: &Value) -> Option<TurnActivitySnapshot> {
    let chunk = data.get("chunk")?;
    let chunk_type = chunk.get("type").and_then(Value::as_str)?;
    match chunk_type {
        "reasoning-delta" => Some(TurnActivitySnapshot::Thinking),
        "text-delta" => Some(TurnActivitySnapshot::Responding),
        "tool-call-delta" => Some(TurnActivitySnapshot::WritingToolCall),
        "block-start" => match chunk.get("blockType").and_then(Value::as_str) {
            Some("reasoning") => Some(TurnActivitySnapshot::Thinking),
            Some("text") => Some(TurnActivitySnapshot::Responding),
            Some("tool-call") => Some(TurnActivitySnapshot::WritingToolCall),
            _ => None,
        },
        "block-end" => match chunk
            .get("block")
            .and_then(|block| block.get("type"))
            .and_then(Value::as_str)
        {
            Some("reasoning") => Some(TurnActivitySnapshot::Thinking),
            Some("text") => Some(TurnActivitySnapshot::Responding),
            Some("tool-call") => Some(TurnActivitySnapshot::WritingToolCall),
            _ => None,
        },
        _ => None,
    }
}

fn assistant_message_activity(data: &Value) -> Option<TurnActivitySnapshot> {
    data.pointer("/message/content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks
                .iter()
                .rev()
                .find_map(|block| match block.get("type").and_then(Value::as_str) {
                    Some("reasoning") => Some(TurnActivitySnapshot::Thinking),
                    Some("text") => Some(TurnActivitySnapshot::Responding),
                    Some("tool-call") => Some(TurnActivitySnapshot::WritingToolCall),
                    _ => None,
                })
        })
}

fn tool_activity(data: &Value, view: Option<&Value>) -> TurnActivitySnapshot {
    let presented = view.map(|value| value.get("view").unwrap_or(value));
    let arguments = data
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|value| serde_json::from_str::<Value>(value).ok());
    let title = presented
        .and_then(|value| value.get("title"))
        .and_then(Value::as_str)
        .or_else(|| {
            arguments.as_ref().and_then(|value| {
                value
                    .get("command")
                    .or_else(|| value.get("cmd"))
                    .and_then(Value::as_str)
            })
        })
        .or_else(|| data.get("command").and_then(Value::as_str))
        .or_else(|| data.get("name").and_then(Value::as_str))
        .unwrap_or("tool")
        .to_string();
    let description = presented
        .and_then(|value| value.get("description"))
        .and_then(Value::as_str)
        .or_else(|| {
            arguments
                .as_ref()
                .and_then(|value| value.get("description"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);
    TurnActivitySnapshot::ToolRunning { title, description }
}

fn running_tool_activity(row: &TranscriptRow) -> Option<TurnActivitySnapshot> {
    if row.kind != DshRenderKind::ToolCall || row.finish != DshRenderFinish::Running {
        return None;
    }
    row.content.blocks.iter().find_map(|block| {
        let DshRenderBlock::ToolCall {
            name,
            arguments,
            view,
            ..
        } = block
        else {
            return None;
        };
        let (title, description) = match view {
            Some(dsh_pager::DshToolCallView::Terminal {
                title, description, ..
            }) => (title.clone(), description.clone()),
            Some(view) => (view.title().to_string(), None),
            None => {
                let data = serde_json::from_str::<Value>(arguments).unwrap_or(Value::Null);
                let title = data
                    .get("command")
                    .or_else(|| data.get("cmd"))
                    .and_then(Value::as_str)
                    .unwrap_or(name)
                    .to_string();
                let description = data
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                (title, description)
            }
        };
        Some(TurnActivitySnapshot::ToolRunning { title, description })
    })
}

fn event_time_for_seq(history: &[dsh_pager_protocol::HistoryEntry], seq: i64) -> Option<u64> {
    history
        .iter()
        .find(|entry| entry.event.seq == seq)
        .and_then(|entry| event_epoch_ms(entry.event.time))
}

fn event_epoch_ms(time: f64) -> Option<u64> {
    event_time_epoch_ms(time)
}

/// Project the optional host-owned file search result. The projection is
/// deliberately permissive at this boundary: older hosts may omit `status`
/// or stable row ids, while the renderer still receives deterministic DTOs.
fn file_search_snapshot(session: &SessionState, enabled: bool) -> FileSearchSnapshot {
    let fallback = || FileSearchSnapshot {
        status: if enabled {
            FeatureStatus::Pending
        } else {
            FeatureStatus::Unsupported
        },
        query: String::new(),
        revision: 0,
        preview_status: FeatureStatus::Unsupported,
        selected_id: None,
        rows: Vec::new(),
        diagnostic: (!enabled).then(|| "DeepSeek Harness file search is unavailable".into()),
    };
    let Some(value) = ["fileSearch", "file_search"]
        .iter()
        .find_map(|key| session.projection(key))
    else {
        return fallback();
    };
    let Some(object) = value.as_object() else {
        return fallback();
    };
    let query = object
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let revision = object
        .get("revision")
        .or_else(|| object.get("generation"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rows = object
        .get("rows")
        .or_else(|| object.get("items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let item = item.as_object()?;
                    let path = item.get("path").and_then(Value::as_str)?.to_string();
                    let line = item
                        .get("line")
                        .or_else(|| item.get("lineNumber"))
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok());
                    let snippet = item
                        .get("snippet")
                        .or_else(|| item.get("text"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            line.map_or_else(|| path.clone(), |line| format!("{path}:{line}"))
                        });
                    let preview = match (line, snippet) {
                        (Some(line), Some(snippet)) => Some(FileSearchPreview {
                            line: Some(line),
                            snippet,
                        }),
                        (None, Some(snippet)) => Some(FileSearchPreview {
                            line: None,
                            snippet,
                        }),
                        _ => None,
                    };
                    Some(FileSearchRow {
                        id,
                        path,
                        kind: item
                            .get("kind")
                            .or_else(|| item.get("type"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        preview,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let status = if !enabled {
        FeatureStatus::Unsupported
    } else {
        object
            .get("status")
            .and_then(Value::as_str)
            .and_then(parse_feature_status)
            .unwrap_or_else(|| {
                if object.contains_key("rows") || object.contains_key("items") {
                    FeatureStatus::Available
                } else {
                    FeatureStatus::Pending
                }
            })
    };
    let preview_status = if !enabled {
        FeatureStatus::Unsupported
    } else if status == FeatureStatus::Pending {
        FeatureStatus::Pending
    } else if rows.iter().any(|row| row.preview.is_some()) {
        FeatureStatus::Available
    } else {
        FeatureStatus::Unsupported
    };
    FileSearchSnapshot {
        status,
        query,
        revision,
        preview_status,
        selected_id: object
            .get("selectedId")
            .or_else(|| object.get("selected_id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        rows,
        diagnostic: object
            .get("diagnostic")
            .or_else(|| object.get("error"))
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn parse_feature_status(value: &str) -> Option<FeatureStatus> {
    match value.to_ascii_lowercase().as_str() {
        "available" | "ready" | "complete" | "completed" => Some(FeatureStatus::Available),
        "pending" | "loading" | "searching" => Some(FeatureStatus::Pending),
        "unsupported" | "unavailable" | "error" | "failed" => Some(FeatureStatus::Unsupported),
        _ => None,
    }
}

fn media_snapshot(transcript: &[TranscriptRow], enabled: bool) -> MediaSnapshot {
    let rows =
        transcript
            .iter()
            .flat_map(|entry| {
                entry.content.blocks.iter().enumerate().filter_map(
                    move |(index, block)| match block {
                        DshRenderBlock::Image {
                            attachment_id,
                            media_type,
                            name,
                            ..
                        } => Some(MediaRow {
                            id: format!("{}:image:{index}", render_entry_id(entry.id)),
                            attachment_id: attachment_id.clone(),
                            media_type: media_type.clone(),
                            name: name.clone().or_else(|| attachment_id.clone()),
                        }),
                        _ => None,
                    },
                )
            })
            .collect();
    MediaSnapshot {
        status: feature_status(enabled),
        rows,
    }
}

/// Revision-time media projection for the production draw path. It scans
/// borrowed entry metadata and clones only image identifiers, never the full
/// transcript or unrelated block payloads.
pub fn media_snapshot_from_scrollback(
    scrollback: &dsh_pager::scrollback::Scrollback,
    enabled: bool,
) -> MediaSnapshot {
    let rows =
        scrollback
            .render_entry_refs()
            .flat_map(|entry| {
                entry.content.blocks.iter().enumerate().filter_map(
                    move |(index, block)| match block {
                        DshRenderBlock::Image {
                            attachment_id,
                            media_type,
                            name,
                            ..
                        } => Some(MediaRow {
                            id: format!("{}:image:{index}", render_entry_id(entry.id)),
                            attachment_id: attachment_id.clone(),
                            media_type: media_type.clone(),
                            name: name.clone().or_else(|| attachment_id.clone()),
                        }),
                        _ => None,
                    },
                )
            })
            .collect();
    MediaSnapshot {
        status: feature_status(enabled),
        rows,
    }
}

fn workspace_snapshot(
    control_plane: Option<&ControlPlaneStore>,
    actions_supported: bool,
) -> WorkspaceSnapshot {
    let Some(control_plane) = control_plane else {
        return WorkspaceSnapshot {
            status: FeatureStatus::Pending,
            actions_supported,
            order: Vec::new(),
            rows: Vec::new(),
        };
    };
    WorkspaceSnapshot {
        status: FeatureStatus::Available,
        actions_supported,
        order: control_plane.workspace_order().to_vec(),
        rows: control_plane
            .workspaces()
            .map(|workspace| WorkspaceRow {
                id: workspace.workspace_id.clone(),
                title: workspace.title.clone(),
                path: workspace.path.clone(),
                session_ids: workspace.session_ids.clone(),
            })
            .collect(),
    }
}

/// Keep this conversion explicit at the boundary; a future Codex adapter can
/// implement the same shape without importing DSH's presentation module.
pub fn snapshot_from_model(model: DshPresentationModel) -> GrokHostSnapshot {
    let session_id = model.session_id.clone();
    let generation = model.generation;
    let queue_revision = model.queue_revision;
    let interaction = model.interaction.clone();
    let transcript_len = model.entries.len();
    GrokHostSnapshot {
        session_title: format!("Session {session_id}"),
        session_id: session_id.clone(),
        cwd: ".".into(),
        model: "deepseek".into(),
        connection: "connected".into(),
        status: None,
        running: false,
        transcript: model.entries.into_iter().map(TranscriptRow::from).collect(),
        transcript_len,
        picker_rows: Vec::new(),
        session_header: SessionHeader {
            id: DshSessionId::new(session_id.clone()),
            generation: DshGeneration::new(generation),
            title: format!("Session {session_id}"),
            model: "deepseek".into(),
            connection: "connected".into(),
            running: false,
        },
        agent_view: AgentViewSnapshot {
            status: None,
            queue_revision,
            interaction: interaction.clone(),
            diagnostics: Vec::new(),
        },
        prompt: PromptSnapshot {
            default_mode: PromptMode::Steer,
            supports_multiline: true,
            authoritative: false,
            history: Vec::new(),
            suggestions: Vec::new(),
        },
        queue: model.queue,
        queue_revision,
        interaction,
        tasks: Vec::new(),
        diagnostics: Vec::new(),
        file_search: FileSearchSnapshot::default(),
        suggestions: SuggestionSnapshot::default(),
        media: MediaSnapshot::default(),
        workspace: WorkspaceSnapshot::default(),
        agent: AgentSnapshot::default(),
        context_usage: ContextUsageSnapshot::default(),
        turn_status: TurnStatusSnapshot::default(),
        session_mode: SessionModeId::Normal,
        capabilities: CapabilityMatrix::default(),
    }
}

fn render_entry_id(id: DshRenderEntryId) -> String {
    match id {
        DshRenderEntryId::Event { seq } => format!("event:{seq}"),
        DshRenderEntryId::Partial {
            turn,
            step,
            surface,
        } => format!("partial:{turn}:{step}:{surface}"),
    }
}

fn projection_strings(session: &SessionState, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| session.projection(key))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .filter(|value| !value.is_empty())
                .take(100)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::{
        HistoryEntry, JsonRpcNotification, SessionEvent, SessionHistoryValue,
    };
    use serde_json::json;

    fn state() -> SessionState {
        let mut state = SessionState::new("session-1".into(), 9);
        state
            .install_initial(SessionHistoryValue {
                events: vec![HistoryEntry {
                    event: SessionEvent {
                        event_type: "assistant/message".into(),
                        seq: 4,
                        time: 4.0,
                        data: json!({
                            "message": { "content": [
                                { "type": "text", "text": "answer" },
                                { "type": "reasoning", "text": "thinking" },
                                { "type": "future", "value": 1 }
                            ] }
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
        state.set_projection("model", 5, json!("deepseek-reasoner"));
        state.set_projection("title", 6, json!("Review"));
        state.set_projection(
            "capabilities",
            7,
            json!({
                "mouse": true,
                "image": true,
                "workspace_actions": true,
                "prompt_history": true,
                "prompt_suggestions": true,
                "file_search": true,
                "subagents": true
            }),
        );
        state.set_projection("promptHistory", 8, json!(["first", "second"]));
        state.set_projection("promptSuggestions", 9, json!(["/help", "/model"]));
        state
    }

    fn mark_running(state: &mut SessionState) {
        state
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.host".into(),
                params: Some(json!({
                    "type": "host/session-status",
                    "sessionId": state.session_id(),
                    "generation": state.generation(),
                    "running": true
                })),
            })
            .expect("running status");
    }

    #[test]
    fn turn_status_folds_stream_activity_without_resetting_each_delta() {
        let base = 1_787_500_000_000_f64;
        let mut state = SessionState::new("turn-session".into(), 2);
        state
            .install_initial(SessionHistoryValue {
                events: vec![
                    HistoryEntry {
                        event: SessionEvent {
                            event_type: "turn/start".into(),
                            seq: 0,
                            time: base,
                            data: json!({"turn": 1}),
                            source_event_seqs: None,
                            surface_op: None,
                            ignorable: None,
                        },
                        view: None,
                    },
                    HistoryEntry {
                        event: SessionEvent {
                            event_type: "assistant/chunk".into(),
                            seq: 1,
                            time: base + 100.0,
                            data: json!({
                                "turn": 1,
                                "step": 1,
                                "chunk": {"type": "reasoning-delta", "index": 0, "text": "a"}
                            }),
                            source_event_seqs: None,
                            surface_op: None,
                            ignorable: None,
                        },
                        view: None,
                    },
                    HistoryEntry {
                        event: SessionEvent {
                            event_type: "assistant/chunk".into(),
                            seq: 2,
                            time: base + 800.0,
                            data: json!({
                                "turn": 1,
                                "step": 1,
                                "chunk": {"type": "reasoning-delta", "index": 0, "text": "b"}
                            }),
                            source_event_seqs: None,
                            surface_op: None,
                            ignorable: None,
                        },
                        view: None,
                    },
                ],
                has_more: false,
                projections: None,
            })
            .expect("turn history");
        mark_running(&mut state);

        let snapshot = GrokHostSnapshot::from_session(&state);
        assert!(snapshot.turn_status.visible);
        assert_eq!(
            snapshot.turn_status.activity,
            TurnActivitySnapshot::Thinking
        );
        assert_eq!(snapshot.turn_status.turn_started_at_ms, Some(base as u64));
        assert_eq!(
            snapshot.turn_status.activity_started_at_ms,
            Some(base as u64 + 100),
            "the second reasoning delta must not restart the phase timer"
        );
    }

    #[test]
    fn running_tool_and_approval_project_title_tokens_and_user_wait() {
        let base = 1_787_500_000_000_f64;
        let mut state = SessionState::new("approval-turn".into(), 3);
        state
            .install_initial(SessionHistoryValue {
                events: vec![
                    HistoryEntry {
                        event: SessionEvent {
                            event_type: "turn/start".into(),
                            seq: 0,
                            time: base,
                            data: json!({"turn": 1}),
                            source_event_seqs: None,
                            surface_op: None,
                            ignorable: None,
                        },
                        view: None,
                    },
                    HistoryEntry {
                        event: SessionEvent {
                            event_type: "tool/call".into(),
                            seq: 1,
                            time: base + 250.0,
                            data: json!({
                                "turn": 1,
                                "step": 1,
                                "name": "bash",
                                "callId": "call-1",
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
                    },
                ],
                has_more: false,
                projections: None,
            })
            .expect("tool history");
        state.set_projection("contextPressure", 3, json!({"projectedTokens": 12_345}));
        mark_running(&mut state);
        state
            .accept_notification(JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: "events.mux".into(),
                params: Some(json!({
                    "type": "approval/requested",
                    "sessionId": "approval-turn",
                    "generation": 3,
                    "requestId": "rpc-1",
                    "approvalId": "approval-1",
                    "callId": "call-1",
                    "toolName": "bash"
                })),
            })
            .expect("approval");

        let snapshot = GrokHostSnapshot::from_session(&state);
        assert_eq!(
            snapshot.turn_status.activity,
            TurnActivitySnapshot::ToolRunning {
                title: "find /work -maxdepth 3".into(),
                description: Some("List project files".into()),
            }
        );
        assert_eq!(
            snapshot.turn_status.activity_started_at_ms,
            Some(base as u64 + 250)
        );
        assert_eq!(snapshot.turn_status.total_tokens, Some(12_345));
        assert!(snapshot.turn_status.pending_user_input);

        let render_snapshot = GrokHostSnapshot::for_render(&state, None);
        assert_eq!(render_snapshot.transcript_len, 1);
        assert_eq!(render_snapshot.transcript.len(), 1);
        assert_eq!(
            render_snapshot.turn_status.activity,
            snapshot.turn_status.activity
        );
    }

    #[test]
    fn snapshot_is_deterministic_and_keeps_rich_blocks_and_identity() {
        let first = GrokHostSnapshot::from_session(&state());
        let second = GrokHostSnapshot::from_session(&state());
        assert_eq!(first, second);
        assert_eq!(first.session_header.id.as_str(), "session-1");
        assert_eq!(first.session_header.generation.get(), 9);
        assert_eq!(first.transcript[0].seq.get(), 4);
        assert_eq!(first.transcript[0].content.blocks.len(), 3);
        assert!(first.capabilities.mouse);
        assert!(first.capabilities.image);
        assert!(first.capabilities.workspace_actions);
        assert!(first.capabilities.prompt_history);
        assert!(first.capabilities.file_search);
        assert!(first.capabilities.subagents);
        assert_eq!(first.prompt.history, vec!["first", "second"]);
        assert_eq!(first.prompt.suggestions, vec!["/help", "/model"]);
        assert!(!first.prompt.authoritative);
        assert_eq!(first.file_search.status, FeatureStatus::Pending);
        assert_eq!(first.file_search.preview_status, FeatureStatus::Unsupported);
        assert_eq!(first.suggestions.status, FeatureStatus::Available);
        assert_eq!(first.workspace.status, FeatureStatus::Pending);
        assert_eq!(first.agent.status, FeatureStatus::Unsupported);
        assert_eq!(first.context_usage, ContextUsageSnapshot::default());
    }

    #[test]
    fn production_snapshot_does_not_clone_settled_transcript_rows() {
        let mut state = SessionState::new("bounded-render".into(), 1);
        let events = (0..100)
            .map(|seq| HistoryEntry {
                event: SessionEvent {
                    event_type: "user/message".into(),
                    seq,
                    time: 1_787_500_000_000.0 + seq as f64,
                    data: json!({
                        "source": { "kind": "user" },
                        "content": [{ "type": "text", "text": format!("entry {seq}") }]
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            })
            .collect();
        state
            .install_initial(SessionHistoryValue {
                events,
                has_more: false,
                projections: None,
            })
            .expect("history");

        let snapshot = GrokHostSnapshot::for_render(&state, None);
        assert_eq!(snapshot.transcript_len, 100);
        assert!(snapshot.transcript.is_empty());
    }

    #[test]
    fn context_pressure_prefers_replay_aware_projection() {
        let mut state = state();
        assert!(state.set_projection(
            "contextPressure",
            10,
            json!({
                "contextWindow": 1_000_000,
                "pressureTokens": 80_000,
                "projectedTokens": 85_500
            })
        ));
        let snapshot = GrokHostSnapshot::from_session(&state);
        assert_eq!(
            snapshot.context_usage,
            ContextUsageSnapshot {
                used_tokens: Some(85_500),
                total_tokens: Some(1_000_000),
            }
        );
    }

    #[test]
    fn context_pressure_falls_back_to_sample_and_rejects_zero_window() {
        let mut state = state();
        assert!(state.set_projection(
            "contextPressure",
            10,
            json!({"contextWindow": 0, "pressureTokens": 42_000})
        ));
        let snapshot = GrokHostSnapshot::from_session(&state);
        assert_eq!(snapshot.context_usage.used_tokens, Some(42_000));
        assert_eq!(snapshot.context_usage.total_tokens, None);
    }

    #[test]
    fn file_search_projection_is_authoritative_and_keeps_revision() {
        let mut state = state();
        assert!(state.set_projection(
            "fileSearch",
            10,
            json!({
                "status": "available",
                "query": "src",
                "revision": 4,
                "rows": [{"path": "src/main.rs", "line": 12, "snippet": "fn main()"}],
                "selectedId": "src/main.rs:12"
            })
        ));
        let snapshot = GrokHostSnapshot::from_session(&state);
        assert_eq!(snapshot.file_search.status, FeatureStatus::Available);
        assert_eq!(snapshot.file_search.query, "src");
        assert_eq!(snapshot.file_search.revision, 4);
        assert_eq!(snapshot.file_search.rows[0].id, "src/main.rs:12");
        assert_eq!(
            snapshot.file_search.preview_status,
            FeatureStatus::Available
        );
        assert_eq!(
            snapshot.file_search.rows[0].preview,
            Some(FileSearchPreview {
                line: Some(12),
                snippet: "fn main()".into(),
            })
        );
        assert_eq!(
            snapshot.file_search.selected_id.as_deref(),
            Some("src/main.rs:12")
        );
    }

    #[test]
    fn file_search_projection_keeps_path_only_rows_without_fabricated_preview() {
        let mut state = state();
        assert!(state.set_projection(
            "fileSearch",
            10,
            json!({
                "status": "available",
                "query": "src",
                "revision": 5,
                "rows": [{"path": "src/lib.rs", "kind": "file"}]
            })
        ));
        let snapshot = GrokHostSnapshot::from_session(&state);
        assert_eq!(snapshot.file_search.status, FeatureStatus::Available);
        assert_eq!(
            snapshot.file_search.preview_status,
            FeatureStatus::Unsupported
        );
        assert_eq!(snapshot.file_search.rows[0].id, "src/lib.rs");
        assert_eq!(snapshot.file_search.rows[0].kind.as_deref(), Some("file"));
        assert_eq!(snapshot.file_search.rows[0].preview, None);
    }

    #[test]
    fn picker_rows_have_stable_ids_and_filter_does_not_reindex_targets() {
        let snapshot = GrokHostSnapshot::from_session(&state());
        let all = snapshot
            .picker_rows
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>();
        let filtered = snapshot.picker_entries_filtered("review");
        assert_eq!(filtered.len(), 1);
        assert_eq!(all[0], "session-1");
    }

    #[test]
    fn resume_projection_keeps_only_native_top_level_nonblank_sessions() {
        let mut values = serde_json::Map::new();
        values.insert("title".into(), json!("Native conversation"));
        values.insert("model".into(), json!("deepseek-reasoner"));
        let list = SessionListValue {
            items: vec![
                SessionSummary {
                    session_id: "native".into(),
                    updated_at: 42_000.0,
                    running: false,
                    blank: false,
                    parent_session_id: None,
                    origin: Some("native".into()),
                    cwd: Some("/work/native/repo".into()),
                    agent_preset: None,
                    projections: Some(dsh_pager_protocol::SessionProjectionsBlock {
                        as_of_seq: 4,
                        values,
                    }),
                },
                SessionSummary {
                    session_id: "blank".into(),
                    updated_at: 41_000.0,
                    running: false,
                    blank: true,
                    parent_session_id: None,
                    origin: None,
                    cwd: None,
                    agent_preset: None,
                    projections: None,
                },
                SessionSummary {
                    session_id: "child".into(),
                    updated_at: 40_000.0,
                    running: true,
                    blank: false,
                    parent_session_id: Some("native".into()),
                    origin: Some("subagent".into()),
                    cwd: Some("/work/native/repo".into()),
                    agent_preset: None,
                    projections: None,
                },
            ],
        };
        let current = state();
        let entries = resume_picker_entries(&list, &current);
        assert_eq!(entries.len(), 2);
        let native = entries
            .iter()
            .find(|entry| entry.id == "native")
            .expect("native row");
        assert_eq!(native.summary, "Native conversation");
        assert_eq!(native.updated_at_ms, 42_000);
        assert_eq!(native.model_id.as_deref(), Some("deepseek-reasoner"));
        assert!(entries.iter().any(|entry| entry.id == "session-1"));
        assert!(!entries.iter().any(|entry| entry.id == "blank"));
        assert!(!entries.iter().any(|entry| entry.id == "child"));
    }
}
