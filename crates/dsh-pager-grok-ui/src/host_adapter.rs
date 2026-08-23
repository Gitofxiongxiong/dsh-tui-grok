//! Convert DSH-owned snapshots into data accepted by the copied Grok views.
//!
//! The old fallback rows remain as a compatibility projection until the full
//! Grok shell consumes every snapshot partition. They are intentionally
//! stable and must not become a second host-owned view model.

use dsh_pager::{
    ControlPlaneStore, Diagnostic, DshGeneration, DshInteraction, DshPresentationModel,
    DshQueueItem, DshRenderContent, DshRenderEntry, DshRenderEntryId, DshRenderKind, DshSeq,
    DshSessionId, SessionState,
};
use dsh_pager_protocol::PromptMode;
use serde::{Deserialize, Serialize};

/// A renderer-facing transcript row.  It deliberately contains no protocol
/// or `SessionState` references, which keeps view code host-agnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptRow {
    pub id: DshRenderEntryId,
    pub label: String,
    pub text: String,
    pub kind: DshRenderKind,
    pub source_seq: i64,
    pub seq: DshSeq,
    pub content: DshRenderContent,
}

/// Explicit terminal and host capability contract. `false` means unavailable
/// or not negotiated; it never means the UI should silently fake support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
        _ => key,
    }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    pub detail: Option<String>,
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
    pub model: String,
    pub connection: String,
    pub status: Option<String>,
    pub running: bool,
    pub transcript: Vec<TranscriptRow>,
    pub picker_rows: Vec<HostRowOwned>,
    pub session_header: SessionHeader,
    pub agent_view: AgentViewSnapshot,
    pub prompt: PromptSnapshot,
    pub queue: Vec<DshQueueItem>,
    pub queue_revision: u64,
    pub interaction: Option<DshInteraction>,
    pub tasks: Vec<TaskRow>,
    pub diagnostics: Vec<DiagnosticView>,
    pub capabilities: CapabilityMatrix,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRowOwned {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub expanded: bool,
}

impl GrokHostSnapshot {
    pub fn from_session(session: &SessionState) -> Self {
        Self::from_session_with_control_plane(session, None)
    }

    /// Project the current session and all control-plane roster rows into the
    /// picker contract. The control-plane store is authoritative for rows;
    /// the loaded SessionState remains authoritative for the active transcript.
    pub fn from_session_with_control_plane(
        session: &SessionState,
        control_plane: Option<&ControlPlaneStore>,
    ) -> Self {
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
        let presentation = session.presentation_model();
        let transcript = presentation
            .entries
            .into_iter()
            .map(TranscriptRow::from)
            .collect();
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
        let interaction = presentation.interaction.clone();
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
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
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
            model,
            connection: format!("{:?}", session.connection_phase()).to_lowercase(),
            status: session.status_message().map(str::to_string),
            running: session.running(),
            transcript,
            picker_rows,
            session_header,
            agent_view,
            prompt: PromptSnapshot {
                default_mode: PromptMode::Queue,
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
            capabilities: CapabilityMatrix::from_session(session),
        }
    }

    /// A deterministic fixture used by adapter/render tests without starting
    /// a backend process.
    pub fn demo() -> Self {
        Self {
            session_title: "DeepSeek / Grok UI adapter".into(),
            session_id: "demo".into(),
            model: "deepseek-reasoner".into(),
            connection: "connected".into(),
            status: None,
            running: true,
            transcript: Vec::new(),
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
                default_mode: PromptMode::Queue,
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

impl From<DshRenderEntry> for TranscriptRow {
    fn from(entry: DshRenderEntry) -> Self {
        Self {
            id: entry.id,
            label: entry.kind.label().to_string(),
            text: entry.text,
            kind: entry.kind,
            source_seq: entry.source_seq,
            seq: DshSeq::new(entry.source_seq),
            content: entry.content,
        }
    }
}

/// Keep this conversion explicit at the boundary; a future Codex adapter can
/// implement the same shape without importing DSH's presentation module.
pub fn snapshot_from_model(model: DshPresentationModel) -> GrokHostSnapshot {
    let session_id = model.session_id.clone();
    let generation = model.generation;
    let queue_revision = model.queue_revision;
    let interaction = model.interaction.clone();
    GrokHostSnapshot {
        session_title: format!("Session {session_id}"),
        session_id: session_id.clone(),
        model: "deepseek".into(),
        connection: "connected".into(),
        status: None,
        running: false,
        transcript: model.entries.into_iter().map(TranscriptRow::from).collect(),
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
            default_mode: PromptMode::Queue,
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
        capabilities: CapabilityMatrix::default(),
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
    use dsh_pager_protocol::{HistoryEntry, SessionEvent, SessionHistoryValue};
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
                "prompt_suggestions": true
            }),
        );
        state.set_projection("promptHistory", 8, json!(["first", "second"]));
        state.set_projection("promptSuggestions", 9, json!(["/help", "/model"]));
        state
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
        assert_eq!(first.prompt.history, vec!["first", "second"]);
        assert_eq!(first.prompt.suggestions, vec!["/help", "/model"]);
        assert!(!first.prompt.authoritative);
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
}
