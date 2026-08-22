//! Convert DSH-owned snapshots into data accepted by the copied Grok views.

use dsh_pager::{DshPresentationModel, DshRenderEntry, DshRenderKind, SessionState};

/// A renderer-facing transcript row.  It deliberately contains no protocol
/// or `SessionState` references, which keeps view code host-agnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRow {
    pub label: String,
    pub text: String,
    pub kind: DshRenderKind,
    pub source_seq: i64,
}

/// Snapshot consumed by the Grok shell in one draw pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrokHostSnapshot {
    pub session_title: String,
    pub session_id: String,
    pub model: String,
    pub connection: String,
    pub status: Option<String>,
    pub running: bool,
    pub transcript: Vec<TranscriptRow>,
    pub picker_rows: Vec<HostRowOwned>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRowOwned {
    pub label: String,
    pub detail: String,
    pub expanded: bool,
}

impl GrokHostSnapshot {
    pub fn from_session(session: &SessionState) -> Self {
        let model = session
            .projection("model")
            .and_then(|value| value.as_str())
            .unwrap_or("deepseek")
            .to_string();
        let title = session
            .title()
            .map(str::to_string)
            .unwrap_or_else(|| format!("Session {}", session.session_id()));
        let transcript = session
            .presentation_model()
            .entries
            .into_iter()
            .map(TranscriptRow::from)
            .collect();
        let mut picker_rows = vec![HostRowOwned {
            label: title.clone(),
            detail: if session.running() {
                "attached · running".into()
            } else {
                "attached · idle".into()
            },
            expanded: true,
        }];
        if !session.queue().is_empty() {
            picker_rows.push(HostRowOwned {
                label: "Queued prompts".into(),
                detail: format!("{} item(s)", session.queue().len()),
                expanded: false,
            });
        }
        if let Some(diagnostic) = session.latest_diagnostic() {
            picker_rows.push(HostRowOwned {
                label: "Latest diagnostic".into(),
                detail: diagnostic.message.clone(),
                expanded: false,
            });
        }
        Self {
            session_title: title,
            session_id: session.session_id().to_string(),
            model,
            connection: format!("{:?}", session.connection_phase()).to_lowercase(),
            status: session.status_message().map(str::to_string),
            running: session.running(),
            transcript,
            picker_rows,
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
                    label: "Current session".into(),
                    detail: "attached · live".into(),
                    expanded: true,
                },
                HostRowOwned {
                    label: "Workspace tasks".into(),
                    detail: "3 jobs".into(),
                    expanded: false,
                },
                HostRowOwned {
                    label: "Archived sessions".into(),
                    detail: "12 sessions".into(),
                    expanded: false,
                },
            ],
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
            .map(|(index, row)| {
                crate::views::picker::PickerEntry::Row(crate::views::picker::PickerRow {
                    label: &row.label,
                    right_label: &row.detail,
                    selected: index == 0,
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
}

impl From<DshRenderEntry> for TranscriptRow {
    fn from(entry: DshRenderEntry) -> Self {
        Self {
            label: entry.kind.label().to_string(),
            text: entry.text,
            kind: entry.kind,
            source_seq: entry.source_seq,
        }
    }
}

/// Keep this conversion explicit at the boundary; a future Codex adapter can
/// implement the same shape without importing DSH's presentation module.
pub fn snapshot_from_model(model: DshPresentationModel) -> GrokHostSnapshot {
    GrokHostSnapshot {
        session_title: format!("Session {}", model.session_id),
        session_id: model.session_id,
        model: "deepseek".into(),
        connection: "connected".into(),
        status: None,
        running: false,
        transcript: model.entries.into_iter().map(TranscriptRow::from).collect(),
        picker_rows: Vec::new(),
    }
}
