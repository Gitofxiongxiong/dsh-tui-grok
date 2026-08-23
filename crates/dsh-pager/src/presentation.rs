//! DSH-owned presentation DTOs and the event-to-render adapter.
//!
//! The protocol deliberately keeps event payloads value-backed because the
//! host owns the domain schema.  This module is the only place where those
//! values become pager presentation data.  Rendering and layout consume the
//! resulting DTOs without knowing about `SessionEvent` or host JSON shapes.

use std::collections::BTreeMap;

use dsh_pager_protocol::{HistoryEntry, QueuePlacement, SessionQueueItem};

use crate::identity::DshSessionId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable identity for a rendered transcript block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DshRenderEntryId {
    Event { seq: i64 },
    Partial { turn: i64, step: i64 },
}

/// View-time visibility for a canonical transcript entry. Hidden entries are
/// retained in history and can still be inspected through diagnostics/copy
/// paths; they simply occupy no space in the default transcript projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DshRenderVisibility {
    #[default]
    Visible,
    Collapsed,
    Hidden,
}

/// Terminal state of a streaming render surface. `Running` is the only state
/// that may keep a surface marked `partial`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DshRenderFinish {
    #[default]
    Completed,
    Running,
    Interrupted,
    Failed,
    Eof,
}

/// Presentation category used by the DSH renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DshRenderKind {
    User,
    Assistant,
    Thinking,
    ToolCall,
    ToolResult,
    Context,
    SystemInstruction,
    AgentContext,
    Status,
    Error,
    Compaction,
}

impl DshRenderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "You",
            Self::Assistant => "Assistant",
            Self::Thinking => "Thinking",
            Self::ToolCall => "Tool",
            Self::ToolResult => "Result",
            Self::Context => "Context",
            Self::SystemInstruction => "System",
            Self::AgentContext => "Context",
            Self::Status => "Status",
            Self::Error => "Error",
            Self::Compaction => "Compaction",
        }
    }
}

/// An edit payload attached to a tool call when the host exposes the old and
/// new text.  The fields are deliberately optional because tools use several
/// argument spellings and an incomplete call must still render losslessly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DshEditDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub old_text: String,
    pub new_text: String,
}

/// One content block crossing the DSH presentation boundary.  `Unknown`
/// retains the complete value so a newer host block can be displayed and
/// copied without teaching the pager about every future domain type first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DshRenderBlock {
    Markdown {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Plain {
        text: String,
    },
    Image {
        #[serde(skip_serializing_if = "Option::is_none")]
        attachment_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        raw: String,
    },
    ToolCall {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        arguments: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        edit: Option<DshEditDetail>,
    },
    ToolResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        blocks: Vec<DshRenderBlock>,
        is_error: bool,
    },
    /// A structured edit/diff block. Keeping both sides here lets the Grok
    /// renderer paint additions/removals and lets copy reconstruct the exact
    /// source without parsing a flattened tool string.
    Diff {
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        old_text: String,
        new_text: String,
    },
    Unknown {
        kind: String,
        raw: String,
    },
}

/// Structured content for one render entry.  `fallback` is the deterministic
/// plain-text projection used by the main scrollback and by terminals without
/// rich rendering support; `blocks` remains the authoritative typed view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DshRenderContent {
    pub blocks: Vec<DshRenderBlock>,
    pub fallback: String,
}

/// Semantic line role shared by the viewer and the terminal paint boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DshRenderRole {
    Header,
    Markdown,
    Code,
    Tool,
    ToolResult,
    DiffAdd,
    DiffRemove,
    DiffContext,
    Plain,
}

/// One stable renderable transcript entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DshRenderEntry {
    pub id: DshRenderEntryId,
    pub source_seq: i64,
    pub kind: DshRenderKind,
    pub text: String,
    /// True while the host is still streaming this surface. A final message
    /// uses the same stable surface lineage with this bit cleared.
    #[serde(default, skip_serializing_if = "is_false")]
    pub partial: bool,
    /// Whether the canonical entry is visible in the default transcript.
    #[serde(default)]
    pub visibility: DshRenderVisibility,
    /// Explicit terminal state for streaming surfaces and diagnostics.
    #[serde(default)]
    pub finish: DshRenderFinish,
    /// Stable grouping anchor for context/tool projections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_key: Option<String>,
    /// Whether this entry can be selected/copied from the default view.
    #[serde(default = "default_selectable")]
    pub selectable: bool,
    /// Source event ids that caused this surface replacement or projection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage: Vec<i64>,
    #[serde(default)]
    pub content: DshRenderContent,
}

impl DshRenderEntry {
    /// Return the semantic content used by structured viewers and copy paths.
    pub fn structured_content(&self) -> &DshRenderContent {
        &self.content
    }

    /// Construct a backwards-compatible plain entry for callers that only
    /// have a stable id/kind/text projection (fixtures and older integrations).
    pub fn plain(
        id: DshRenderEntryId,
        source_seq: i64,
        kind: DshRenderKind,
        text: impl Into<String>,
    ) -> Self {
        let text = text.into();
        let block = match kind {
            DshRenderKind::Thinking => DshRenderBlock::Reasoning { text: text.clone() },
            DshRenderKind::ToolCall => DshRenderBlock::ToolCall {
                name: text.clone(),
                call_id: None,
                arguments: String::new(),
                edit: None,
            },
            DshRenderKind::ToolResult | DshRenderKind::Error => DshRenderBlock::ToolResult {
                call_id: None,
                blocks: vec![DshRenderBlock::Plain { text: text.clone() }],
                is_error: kind == DshRenderKind::Error,
            },
            DshRenderKind::Status | DshRenderKind::Compaction => {
                DshRenderBlock::Plain { text: text.clone() }
            }
            _ => DshRenderBlock::Markdown { text: text.clone() },
        };
        Self {
            id,
            source_seq,
            kind,
            text: text.clone(),
            partial: false,
            visibility: default_visibility(kind),
            finish: DshRenderFinish::Completed,
            group_key: None,
            selectable: default_selectable_for_kind(kind),
            lineage: Vec::new(),
            content: DshRenderContent {
                blocks: vec![block],
                fallback: text,
            },
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn default_selectable() -> bool {
    true
}

fn default_selectable_for_kind(kind: DshRenderKind) -> bool {
    !matches!(
        kind,
        DshRenderKind::SystemInstruction
            | DshRenderKind::AgentContext
            | DshRenderKind::Context
            | DshRenderKind::Status
            | DshRenderKind::Compaction
    )
}

fn default_visibility(kind: DshRenderKind) -> DshRenderVisibility {
    match kind {
        DshRenderKind::SystemInstruction => DshRenderVisibility::Hidden,
        DshRenderKind::AgentContext | DshRenderKind::Context | DshRenderKind::Compaction => {
            DshRenderVisibility::Collapsed
        }
        _ => DshRenderVisibility::Visible,
    }
}

impl DshRenderContent {
    fn from_message(message: &Value) -> Self {
        Self::from_value(message.get("content").unwrap_or(&Value::Null))
    }

    fn from_value(content: &Value) -> Self {
        let blocks = match content {
            Value::Array(values) => values.iter().map(parse_render_block).collect(),
            Value::Object(_) => vec![parse_render_block(content)],
            Value::String(text) => vec![DshRenderBlock::Markdown { text: text.clone() }],
            _ => Vec::new(),
        };
        Self::from_blocks(blocks)
    }

    fn from_blocks(blocks: Vec<DshRenderBlock>) -> Self {
        let fallback = blocks
            .iter()
            .map(DshRenderBlock::display_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        Self { blocks, fallback }
    }

    /// Deterministic plain projection for terminals and copy paths. The
    /// structured blocks remain authoritative; this is only a display-safe
    /// fallback when a rich renderer is unavailable.
    pub fn display_text(&self) -> String {
        self.blocks
            .iter()
            .map(DshRenderBlock::display_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl DshRenderBlock {
    /// Lossless-enough text projection used by non-rich terminals and copy.
    pub fn display_text(&self) -> String {
        match self {
            Self::Markdown { text } | Self::Reasoning { text } | Self::Plain { text } => {
                text.clone()
            }
            Self::Image {
                attachment_id,
                media_type,
                name,
                ..
            } => {
                let label = name
                    .as_deref()
                    .or(media_type.as_deref())
                    .or(attachment_id.as_deref());
                label.map_or_else(|| "[image]".into(), |label| format!("[image: {label}]"))
            }
            Self::ToolCall {
                name,
                arguments,
                edit,
                ..
            } => {
                if arguments.is_empty() {
                    edit.as_ref().map_or_else(
                        || name.clone(),
                        |edit| format!("{name}\n-{}\n+{}", edit.old_text, edit.new_text),
                    )
                } else {
                    edit.as_ref().map_or_else(
                        || format!("{name} {arguments}"),
                        |edit| {
                            format!("{name} {arguments}\n-{}\n+{}", edit.old_text, edit.new_text)
                        },
                    )
                }
            }
            Self::ToolResult { blocks, .. } => blocks
                .iter()
                .map(Self::display_text)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Diff {
                path,
                old_text,
                new_text,
            } => {
                let header = path
                    .as_deref()
                    .map_or_else(|| "diff".to_string(), |path| format!("diff {path}"));
                format!("{header}\n-{old_text}\n+{new_text}")
            }
            Self::Unknown { kind, raw } => {
                format!("[unsupported block: {kind}]\n{raw}")
            }
        }
    }
}

fn parse_render_block(value: &Value) -> DshRenderBlock {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    match kind {
        "text" => DshRenderBlock::Markdown {
            text: value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        },
        "reasoning" => DshRenderBlock::Reasoning {
            text: value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        },
        "image" => {
            let attachment = value.get("attachment").unwrap_or(value);
            DshRenderBlock::Image {
                attachment_id: string_field(attachment, "attachmentId").map(str::to_string),
                media_type: string_field(attachment, "mediaType").map(str::to_string),
                name: string_field(attachment, "name").map(str::to_string),
                raw: serde_json::to_string(value).unwrap_or_else(|_| "{}".into()),
            }
        }
        "tool-call" => {
            let arguments = string_field(value, "arguments").unwrap_or("").to_string();
            DshRenderBlock::ToolCall {
                name: string_field(value, "name").unwrap_or("tool").to_string(),
                call_id: string_field(value, "id")
                    .or_else(|| string_field(value, "callId"))
                    .map(str::to_string),
                edit: edit_detail_from_arguments(&arguments, None),
                arguments,
            }
        }
        "tool-result" => DshRenderBlock::ToolResult {
            call_id: string_field(value, "toolCallId")
                .or_else(|| string_field(value, "callId"))
                .map(str::to_string),
            blocks: value
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(parse_render_block)
                .collect(),
            is_error: value
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "diff" | "patch" => DshRenderBlock::Diff {
            path: string_field(value, "path")
                .or_else(|| string_field(value, "file"))
                .map(str::to_string),
            old_text: string_field(value, "oldText")
                .or_else(|| string_field(value, "old_string"))
                .or_else(|| string_field(value, "old"))
                .unwrap_or("")
                .to_string(),
            new_text: string_field(value, "newText")
                .or_else(|| string_field(value, "new_string"))
                .or_else(|| string_field(value, "new"))
                .unwrap_or("")
                .to_string(),
        },
        _ => DshRenderBlock::Unknown {
            kind: kind.to_string(),
            raw: serde_json::to_string(value).unwrap_or_else(|_| "{}".into()),
        },
    }
}

fn edit_detail_from_arguments(arguments: &str, view: Option<&Value>) -> Option<DshEditDetail> {
    let arguments = serde_json::from_str::<Value>(arguments).ok();
    let first_diff = view.and_then(|view| {
        view.pointer("/view/diffs/0")
            .or_else(|| view.pointer("/diffs/0"))
    });
    let old_text = arguments
        .as_ref()
        .and_then(|value| {
            string_field(value, "old_string")
                .or_else(|| string_field(value, "oldString"))
                .or_else(|| string_field(value, "old_str"))
        })
        .or_else(|| first_diff.and_then(|value| string_field(value, "oldText")))?;
    let new_text = arguments
        .as_ref()
        .and_then(|value| {
            string_field(value, "new_string")
                .or_else(|| string_field(value, "newString"))
                .or_else(|| string_field(value, "new_str"))
        })
        .or_else(|| first_diff.and_then(|value| string_field(value, "newText")))?;
    let path = arguments
        .as_ref()
        .and_then(|value| string_field(value, "path"))
        .or_else(|| first_diff.and_then(|value| string_field(value, "path")))
        .map(str::to_string);
    Some(DshEditDetail {
        path,
        old_text: old_text.to_string(),
        new_text: new_text.to_string(),
    })
}

/// Queue content after the host-owned message has crossed the presentation
/// boundary.
///
/// The host keeps the canonical value-backed message. The pager only needs a
/// deterministic projection for display and editing, so unknown content is
/// represented as a labelled line instead of being discarded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DshQueueContent {
    /// Complete display lines, retaining blank and trailing lines.
    pub lines: Vec<String>,
    /// First non-empty display line, trimmed for compact queue rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Text that can safely be sent back through `session.updateQueue`.
    /// Mixed/rich content is deliberately not editable by the text editor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editable_text: Option<String>,
    /// Number of top-level content blocks represented by this projection.
    pub block_count: usize,
}

impl DshQueueContent {
    pub fn from_message(message: &Value) -> Self {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            return Self::default();
        };

        let mut lines = Vec::new();
        let mut editable_parts = Vec::with_capacity(blocks.len());
        let mut editable = true;
        for block in blocks {
            let block_type = block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            match block_type {
                "text" => {
                    let Some(text) = block.get("text").and_then(Value::as_str) else {
                        editable = false;
                        lines.push("[text block]".into());
                        continue;
                    };
                    push_display_text(&mut lines, text);
                    editable_parts.push(text.to_string());
                }
                "reasoning" => {
                    editable = false;
                    push_labelled_text(&mut lines, "[reasoning]", block.get("text"));
                }
                "image" => {
                    editable = false;
                    lines.push("[image]".into());
                }
                "tool-call" => {
                    editable = false;
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                    let arguments = block.get("arguments").and_then(Value::as_str).unwrap_or("");
                    let header = if arguments.is_empty() {
                        format!("[tool-call] {name}")
                    } else {
                        format!("[tool-call] {name} {arguments}")
                    };
                    push_display_text(&mut lines, &header);
                }
                "tool-result" => {
                    editable = false;
                    let prefix = if block.get("isError").and_then(Value::as_bool) == Some(true) {
                        "[tool-result error]"
                    } else {
                        "[tool-result]"
                    };
                    let nested = DshQueueContent::from_content(block.get("content"));
                    if nested.lines.is_empty() {
                        lines.push(prefix.into());
                    } else {
                        for (index, line) in nested.lines.iter().enumerate() {
                            if index == 0 {
                                lines.push(format!("{prefix} {line}"));
                            } else {
                                lines.push(format!("          {line}"));
                            }
                        }
                    }
                }
                other => {
                    editable = false;
                    lines.push(format!("[{other}]"));
                }
            }
        }

        let editable_text = editable.then(|| editable_parts.join("\n"));
        Self::from_parts(lines, editable_text, blocks.len())
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn from_content(content: Option<&Value>) -> Self {
        let Some(blocks) = content.and_then(Value::as_array) else {
            return Self::default();
        };
        let mut lines = Vec::new();
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("text") | Some("reasoning") => {
                    push_display_text(
                        &mut lines,
                        block.get("text").and_then(Value::as_str).unwrap_or(""),
                    );
                }
                Some("image") => lines.push("[image]".into()),
                Some(other) => lines.push(format!("[{other}]")),
                None => lines.push("[unknown]".into()),
            }
        }
        Self::from_parts(lines, None, blocks.len())
    }

    fn from_parts(lines: Vec<String>, editable_text: Option<String>, block_count: usize) -> Self {
        let summary = lines
            .iter()
            .map(|line| line.trim())
            .find(|line| !line.is_empty())
            .map(str::to_string);
        Self {
            lines,
            summary,
            editable_text,
            block_count,
        }
    }
}

fn push_display_text(lines: &mut Vec<String>, text: &str) {
    lines.extend(text.split('\n').map(str::to_string));
}

fn push_labelled_text(lines: &mut Vec<String>, label: &str, value: Option<&Value>) {
    let text = value.and_then(Value::as_str).unwrap_or("");
    let mut split = text.split('\n');
    if let Some(first) = split.next() {
        lines.push(if first.is_empty() {
            label.to_string()
        } else {
            format!("{label} {first}")
        });
    }
    lines.extend(split.map(|line| format!("          {line}")));
}

/// Queue data after the host-owned message has crossed the presentation
/// boundary. The raw host message stays in `SessionState`; widgets consume
/// this projection exclusively.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DshQueueItem {
    pub id: String,
    pub placement: QueuePlacement,
    pub content: DshQueueContent,
}

impl From<&SessionQueueItem> for DshQueueItem {
    fn from(item: &SessionQueueItem) -> Self {
        Self {
            id: item.id.clone(),
            placement: item.placement,
            content: DshQueueContent::from_message(&item.message),
        }
    }
}

/// A server-owned approval or question ready for an interaction widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DshInteraction {
    Approval {
        request_id: String,
        approval_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Question {
        request_id: String,
        questions: Vec<Value>,
    },
}

impl DshInteraction {
    /// Decode the mux approval frame without leaking its value shape into UI
    /// code.  Missing identifiers remain explicit and deterministic so a
    /// malformed frame cannot accidentally answer another request.
    pub fn approval_from_frame(frame: &Value) -> Self {
        Self::Approval {
            request_id: request_id(frame),
            approval_id: string_field(frame, "approvalId")
                .unwrap_or("unknown-approval")
                .to_string(),
            tool_name: string_field(frame, "toolName").map(str::to_string),
            reason: string_field(frame, "reason").map(str::to_string),
        }
    }

    /// Decode the mux question frame into an interaction DTO.
    pub fn question_from_frame(frame: &Value) -> Self {
        Self::Question {
            request_id: request_id(frame),
            questions: frame
                .get("questions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::Approval { request_id, .. } | Self::Question { request_id, .. } => request_id,
        }
    }
}

/// Snapshot consumed by picker, queue, interaction, and block-viewer
/// widgets.  It is intentionally independent of the session runtime state.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DshPresentationModel {
    pub session_id: String,
    pub generation: u64,
    pub entries: Vec<DshRenderEntry>,
    pub queue: Vec<DshQueueItem>,
    pub queue_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction: Option<DshInteraction>,
}

impl DshPresentationModel {
    pub fn new(session_id: impl Into<String>, generation: u64) -> Self {
        Self {
            session_id: session_id.into(),
            generation,
            ..Self::default()
        }
    }

    pub fn session_identity(&self) -> DshSessionId {
        DshSessionId::new(self.session_id.clone())
    }
}

/// Incremental changes produced by [`DshPresentationAdapter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DshRenderUpdate {
    Upsert(DshRenderEntry),
    Remove(DshRenderEntryId),
    RemoveSourceRange { start: i64, end: i64 },
}

/// Converts value-backed session history into stable DSH presentation data.
#[derive(Debug, Default)]
pub struct DshPresentationAdapter {
    partials: BTreeMap<(i64, i64), PartialMessage>,
}

impl DshPresentationAdapter {
    pub fn reset(&mut self) {
        self.partials.clear();
    }

    /// Finalize every currently running assistant surface. Host stream error,
    /// EOF and generation changes can end a stream without adding a history
    /// event, so they use this same reducer seam as `turn/end`.
    pub fn finalize_all(
        &mut self,
        seq: i64,
        finish: DshRenderFinish,
        reason: Option<&str>,
    ) -> Vec<DshRenderUpdate> {
        let keys = self.partials.keys().copied().collect::<Vec<_>>();
        let mut updates = Vec::new();
        for key in keys {
            self.finalize_partial(key, seq, finish, reason, &mut updates);
        }
        updates
    }

    /// Rebuild a coherent presentation baseline from a history window.
    pub fn adapt_history(&mut self, history: &[HistoryEntry]) -> Vec<DshRenderUpdate> {
        self.reset();
        history
            .iter()
            .flat_map(|entry| self.adapt_event(entry))
            .collect()
    }

    /// Adapt one ordered live/replay event.  A single event may first remove
    /// a replaced surface range and then upsert its new render block.
    pub fn adapt_event(&mut self, entry: &HistoryEntry) -> Vec<DshRenderUpdate> {
        let event = &entry.event;
        let mut updates = Vec::new();
        if let Some((start, end)) = surface_replace(event.surface_op.as_ref()) {
            self.partials.clear();
            updates.push(DshRenderUpdate::RemoveSourceRange { start, end });
        }
        match event.event_type.as_str() {
            "user/message" => self.adapt_user_message(event.seq, &event.data, &mut updates),
            "assistant/chunk" => self.adapt_assistant_chunk(event.seq, &event.data, &mut updates),
            "assistant/message" => {
                self.adapt_assistant_message(event.seq, &event.data, &mut updates)
            }
            "tool/call" => {
                self.adapt_tool_call(event.seq, &event.data, entry.view.as_ref(), &mut updates)
            }
            "tool/result" => self.adapt_tool_result(event.seq, &event.data, &mut updates),
            "todo/write" => self.adapt_todos(event.seq, &event.data, &mut updates),
            "command/done" => self.adapt_command(event.seq, &event.data, &mut updates),
            "llm/retry" => self.adapt_retry(event.seq, &event.data, &mut updates),
            "turn/end" => self.adapt_turn_end(event.seq, &event.data, &mut updates),
            "agent/error" | "stream/error" | "stream/eof" => {
                self.adapt_stream_terminal(event.seq, &event.data, &mut updates)
            }
            "compaction/start" => self.adapt_compaction_start(event.seq, &event.data, &mut updates),
            "compaction/end" => self.adapt_compaction_end(event.seq, &event.data, &mut updates),
            "compaction/summary" => {
                self.adapt_compaction_summary(event.seq, &event.data, &mut updates)
            }
            "compaction/prune" => self.adapt_compaction_prune(event.seq, &event.data, &mut updates),
            _ => {}
        }
        let lineage = event
            .source_event_seqs
            .clone()
            .unwrap_or_else(|| vec![event.seq]);
        for update in &mut updates {
            if let DshRenderUpdate::Upsert(render) = update {
                if matches!(render.id, DshRenderEntryId::Event { seq: render_seq } if render_seq == event.seq)
                {
                    render.lineage = lineage.clone();
                } else {
                    append_lineage(&mut render.lineage, &lineage);
                }
            }
        }
        updates
    }

    fn adapt_user_message(&mut self, seq: i64, data: &Value, updates: &mut Vec<DshRenderUpdate>) {
        let content = DshRenderContent::from_message(data);
        if content.blocks.is_empty() {
            return;
        }
        let (kind, visibility, group_key, selectable) = classify_user_message(data);
        updates.push(upsert_event_content_with_projection(
            seq,
            kind,
            content,
            visibility,
            DshRenderFinish::Completed,
            group_key,
            selectable,
        ));
    }

    fn adapt_assistant_message(
        &mut self,
        seq: i64,
        data: &Value,
        updates: &mut Vec<DshRenderUpdate>,
    ) {
        let key = integer(data, "turn").zip(integer(data, "step"));
        let Some(message) = data.get("message") else {
            if let Some(key) = key {
                self.finalize_partial(key, seq, DshRenderFinish::Completed, None, updates);
            }
            return;
        };
        let content = DshRenderContent::from_message(message);
        if let Some(key) = key {
            if let Some(partial) = self.partials.get_mut(&key) {
                if partial.final_content_applied {
                    return;
                }
                if matches!(
                    partial.finish,
                    DshRenderFinish::Interrupted | DshRenderFinish::Failed | DshRenderFinish::Eof
                ) {
                    // A late final frame from an older provider generation
                    // must not resurrect a surface already terminated by an
                    // abort/error/EOF signal.
                    return;
                }
                if !content.blocks.is_empty() {
                    partial.replace_with_final(content);
                }
                partial.finish = DshRenderFinish::Completed;
                partial.final_content_applied = true;
                if let Some((kind, content)) = self.partials.get(&key).map(PartialMessage::display)
                {
                    let partial = self.partials.get(&key).expect("partial exists");
                    updates.push(DshRenderUpdate::Upsert(partial_entry(
                        key, seq, kind, content, partial,
                    )));
                }
            } else if !content.blocks.is_empty() {
                updates.push(upsert_event_content(seq, DshRenderKind::Assistant, content));
            }
        } else if !content.blocks.is_empty() {
            updates.push(upsert_event_content(seq, DshRenderKind::Assistant, content));
        }
    }

    fn adapt_assistant_chunk(
        &mut self,
        seq: i64,
        data: &Value,
        updates: &mut Vec<DshRenderUpdate>,
    ) {
        let (Some(turn), Some(step), Some(chunk)) = (
            integer(data, "turn"),
            integer(data, "step"),
            data.get("chunk"),
        ) else {
            updates.push(upsert_event(
                seq,
                DshRenderKind::Error,
                "Malformed assistant stream chunk".into(),
            ));
            return;
        };
        let key = (turn, step);
        let chunk_type = chunk.get("type").and_then(Value::as_str).unwrap_or("");
        if chunk_type == "finish" {
            let finish = finish_from_value(
                chunk
                    .get("reason")
                    .or_else(|| chunk.get("finish"))
                    .or_else(|| chunk.get("finishReason"))
                    .or_else(|| chunk.get("status")),
            );
            self.finalize_partial(key, seq, finish, None, updates);
            return;
        }
        // Usage is terminal metadata, not a transcript block. Keep the
        // surface alive until an explicit final/turn-end/error signal while
        // still advancing the surface lineage instead of silently dropping
        // the frame.
        if chunk_type == "usage" {
            let partial = self.partials.entry(key).or_default();
            if partial.finish != DshRenderFinish::Running {
                return;
            }
            append_lineage(&mut partial.lineage, &[seq]);
            let Some((kind, content)) = self.partials.get(&key).map(PartialMessage::display) else {
                return;
            };
            if !content.blocks.is_empty() {
                let partial = self.partials.get(&key).expect("partial exists");
                updates.push(DshRenderUpdate::Upsert(partial_entry(
                    key, seq, kind, content, partial,
                )));
            }
            return;
        }
        let index = chunk.get("index").and_then(Value::as_i64).unwrap_or(0);
        let partial = self.partials.entry(key).or_default();
        if partial.finish != DshRenderFinish::Running {
            return;
        }
        append_lineage(&mut partial.lineage, &[seq]);
        match chunk_type {
            "block-start" => {
                let kind = match chunk.get("blockType").and_then(Value::as_str) {
                    Some("text") => PartialKind::Text,
                    Some("reasoning") => PartialKind::Reasoning,
                    Some("tool-call") => PartialKind::ToolCall,
                    Some("image") => PartialKind::Image,
                    _ => PartialKind::Unknown,
                };
                partial
                    .blocks
                    .entry(index)
                    .or_insert_with(|| PartialBlock::new(kind));
            }
            "text-delta" | "reasoning-delta" => {
                let kind = if chunk_type == "text-delta" {
                    PartialKind::Text
                } else {
                    PartialKind::Reasoning
                };
                let block = partial
                    .blocks
                    .entry(index)
                    .or_insert_with(|| PartialBlock::new(kind.clone()));
                if block.kind != kind {
                    *block = PartialBlock::new(kind);
                }
                if let Some(text) = chunk.get("text").and_then(Value::as_str) {
                    block.text.push_str(text);
                }
            }
            "tool-call-delta" => {
                let block = partial
                    .blocks
                    .entry(index)
                    .or_insert_with(|| PartialBlock::new(PartialKind::ToolCall));
                block.kind = PartialKind::ToolCall;
                block.call_id = string_field(chunk, "id").map(str::to_string);
                if let Some(name) = chunk.get("name").and_then(Value::as_str) {
                    block.name = name.into();
                }
                if let Some(delta) = chunk.get("argumentsDelta").and_then(Value::as_str) {
                    block.text.push_str(delta);
                }
            }
            "block-end" => {
                if let Some(block_value) = chunk.get("block") {
                    let block = partial
                        .blocks
                        .entry(index)
                        .or_insert_with(|| PartialBlock::new(PartialKind::Unknown));
                    *block = PartialBlock::from_render_block(parse_render_block(block_value));
                }
            }
            _ => {
                updates.push(upsert_event(
                    seq,
                    DshRenderKind::Error,
                    format!("Unsupported assistant chunk: {chunk_type}"),
                ));
                return;
            }
        }
        let Some((kind, content)) = self.partials.get(&key).map(PartialMessage::display) else {
            return;
        };
        if !content.blocks.is_empty() {
            let partial = self.partials.get(&key).expect("partial exists");
            updates.push(DshRenderUpdate::Upsert(partial_entry(
                key, seq, kind, content, partial,
            )));
        }
    }

    fn adapt_tool_call(
        &mut self,
        seq: i64,
        data: &Value,
        view: Option<&Value>,
        updates: &mut Vec<DshRenderUpdate>,
    ) {
        let name = data.get("name").and_then(Value::as_str).unwrap_or("tool");
        let arguments = data.get("arguments").and_then(Value::as_str).unwrap_or("");
        let content = DshRenderContent::from_blocks(vec![DshRenderBlock::ToolCall {
            name: name.to_string(),
            call_id: string_field(data, "callId")
                .or_else(|| string_field(data, "id"))
                .map(str::to_string),
            arguments: arguments.to_string(),
            edit: edit_detail_from_arguments(arguments, view),
        }]);
        updates.push(upsert_event_content(seq, DshRenderKind::ToolCall, content));
    }

    fn adapt_tool_result(&mut self, seq: i64, data: &Value, updates: &mut Vec<DshRenderUpdate>) {
        let Some(message) = data.get("message") else {
            return;
        };
        let content = DshRenderContent::from_message(message);
        if content.blocks.is_empty() {
            return;
        }
        let nested_error = content
            .blocks
            .iter()
            .any(|block| matches!(block, DshRenderBlock::ToolResult { is_error: true, .. }));
        let kind = if nested_error || data.get("error").is_some_and(|value| !value.is_null()) {
            DshRenderKind::Error
        } else {
            DshRenderKind::ToolResult
        };
        updates.push(upsert_event_content(seq, kind, content));
    }

    fn adapt_todos(&mut self, seq: i64, data: &Value, updates: &mut Vec<DshRenderUpdate>) {
        let Some(todos) = data.get("todos").and_then(Value::as_array) else {
            return;
        };
        let text = todos
            .iter()
            .filter_map(|todo| {
                let content = todo.get("content")?.as_str()?;
                let status = todo
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("pending");
                Some(format!("[{status}] {content}"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            updates.push(upsert_event(seq, DshRenderKind::Status, text));
        }
    }

    fn adapt_command(&mut self, seq: i64, data: &Value, updates: &mut Vec<DshRenderUpdate>) {
        if let Some(text) = data.get("text").and_then(Value::as_str) {
            if !text.is_empty() {
                updates.push(upsert_event(seq, DshRenderKind::Status, text.into()));
            }
        }
    }

    fn adapt_retry(&mut self, seq: i64, data: &Value, updates: &mut Vec<DshRenderUpdate>) {
        let message = data
            .pointer("/failure/message")
            .and_then(Value::as_str)
            .unwrap_or("model request retrying");
        updates.push(upsert_event(seq, DshRenderKind::Status, message.into()));
    }

    fn adapt_turn_end(&mut self, seq: i64, data: &Value, updates: &mut Vec<DshRenderUpdate>) {
        let kind = data
            .pointer("/reason/kind")
            .or_else(|| data.pointer("/reason/type"))
            .or_else(|| data.get("reason"))
            .and_then(Value::as_str)
            .unwrap_or("completed");
        let finish = finish_from_kind(kind);
        let turn = integer(data, "turn").or_else(|| integer(data, "turnId"));
        let keys = self
            .partials
            .keys()
            .copied()
            .filter(|(partial_turn, _)| turn.is_none_or(|turn| turn == *partial_turn))
            .collect::<Vec<_>>();
        for key in keys {
            self.finalize_partial(key, seq, finish, Some(kind), updates);
        }
        if kind != "completed" {
            updates.push(upsert_event(
                seq,
                DshRenderKind::Status,
                format!("Turn ended: {kind}"),
            ));
        }
    }

    fn adapt_stream_terminal(
        &mut self,
        seq: i64,
        data: &Value,
        updates: &mut Vec<DshRenderUpdate>,
    ) {
        let code = data
            .pointer("/error/code")
            .or_else(|| data.get("code"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let reason = data
            .pointer("/error/message")
            .or_else(|| data.get("message"))
            .and_then(Value::as_str);
        let finish = if code.eq_ignore_ascii_case("eof")
            || code.eq_ignore_ascii_case("closed")
            || reason.is_some_and(|message| {
                let message = message.to_ascii_lowercase();
                message.contains("eof") || message.contains("closed")
            }) {
            DshRenderFinish::Eof
        } else {
            DshRenderFinish::Failed
        };
        self.finalize_all(seq, finish, reason)
            .into_iter()
            .for_each(|update| updates.push(update));
    }

    fn finalize_partial(
        &mut self,
        key: (i64, i64),
        seq: i64,
        finish: DshRenderFinish,
        reason: Option<&str>,
        updates: &mut Vec<DshRenderUpdate>,
    ) {
        let Some(partial) = self.partials.get_mut(&key) else {
            return;
        };
        if partial.finish != DshRenderFinish::Running {
            return;
        }
        partial.finish = finish;
        partial.terminal_seq = Some(seq);
        append_lineage(&mut partial.lineage, &[seq]);
        let Some((kind, content)) = self.partials.get(&key).map(PartialMessage::display) else {
            return;
        };
        if !content.blocks.is_empty() {
            let partial = self.partials.get(&key).expect("partial exists");
            updates.push(DshRenderUpdate::Upsert(partial_entry(
                key, seq, kind, content, partial,
            )));
        }
        let _ = reason;
    }

    fn adapt_compaction_start(
        &mut self,
        seq: i64,
        data: &Value,
        updates: &mut Vec<DshRenderUpdate>,
    ) {
        let reason = data
            .get("reason")
            .or_else(|| data.get("trigger"))
            .and_then(Value::as_str);
        let text = reason.map_or_else(
            || "Conversation compaction started".to_string(),
            |reason| format!("Conversation compaction started: {reason}"),
        );
        updates.push(upsert_event(seq, DshRenderKind::Compaction, text));
    }

    fn adapt_compaction_end(&mut self, seq: i64, data: &Value, updates: &mut Vec<DshRenderUpdate>) {
        if let Some(error) = data.get("error").and_then(Value::as_str) {
            updates.push(upsert_event(
                seq,
                DshRenderKind::Compaction,
                format!("Conversation compaction failed: {error}"),
            ));
            return;
        }
        let removed = data
            .get("removed")
            .or_else(|| data.get("pruned"))
            .and_then(Value::as_i64);
        let text = removed.map_or_else(
            || "Conversation compaction complete".to_string(),
            |removed| format!("Conversation compaction complete (removed {removed})"),
        );
        updates.push(upsert_event(seq, DshRenderKind::Compaction, text));
    }

    fn adapt_compaction_summary(
        &mut self,
        seq: i64,
        data: &Value,
        updates: &mut Vec<DshRenderUpdate>,
    ) {
        let summary = data
            .get("summary")
            .map(|value| match value {
                Value::String(text) => {
                    DshRenderContent::from_blocks(vec![DshRenderBlock::Markdown {
                        text: text.clone(),
                    }])
                }
                Value::Array(_) => DshRenderContent::from_value(value),
                value => DshRenderContent::from_value(value.get("content").unwrap_or(&Value::Null)),
            })
            .unwrap_or_default();
        let mut text = if summary.fallback.is_empty() {
            "Conversation compacted".to_string()
        } else {
            format!("Conversation compacted\n{}", summary.fallback)
        };
        if let Some(removed) = data
            .get("shadowedTokenCount")
            .or_else(|| data.get("removed"))
            .and_then(Value::as_i64)
        {
            text.push_str(&format!("\nshadowed tokens: {removed}"));
        }
        updates.push(upsert_event_content(
            seq,
            DshRenderKind::Compaction,
            DshRenderContent::from_blocks(vec![DshRenderBlock::Markdown { text }]),
        ));
    }

    fn adapt_compaction_prune(
        &mut self,
        seq: i64,
        data: &Value,
        updates: &mut Vec<DshRenderUpdate>,
    ) {
        let shadowed = data
            .get("shadowedSeqs")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let tokens = data
            .get("shadowedTokenCount")
            .and_then(Value::as_i64)
            .map(|count| format!(", shadowed tokens: {count}"))
            .unwrap_or_default();
        updates.push(upsert_event(
            seq,
            DshRenderKind::Compaction,
            format!("Compaction pruned {shadowed} surface node(s){tokens}"),
        ));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PartialKind {
    Text,
    Reasoning,
    ToolCall,
    Image,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartialBlock {
    kind: PartialKind,
    text: String,
    name: String,
    call_id: Option<String>,
    finished: Option<DshRenderBlock>,
}

impl PartialBlock {
    fn new(kind: PartialKind) -> Self {
        Self {
            kind,
            text: String::new(),
            name: String::new(),
            call_id: None,
            finished: None,
        }
    }

    fn from_render_block(block: DshRenderBlock) -> Self {
        let kind = match &block {
            DshRenderBlock::Markdown { .. } => PartialKind::Text,
            DshRenderBlock::Reasoning { .. } => PartialKind::Reasoning,
            DshRenderBlock::ToolCall { .. } => PartialKind::ToolCall,
            DshRenderBlock::Image { .. } => PartialKind::Image,
            _ => PartialKind::Unknown,
        };
        Self {
            kind,
            text: String::new(),
            name: String::new(),
            call_id: None,
            finished: Some(block),
        }
    }

    fn render_block(&self) -> Option<DshRenderBlock> {
        if let Some(block) = &self.finished {
            return Some(block.clone());
        }
        match self.kind {
            PartialKind::Text => Some(DshRenderBlock::Markdown {
                text: self.text.clone(),
            }),
            PartialKind::Reasoning => Some(DshRenderBlock::Reasoning {
                text: self.text.clone(),
            }),
            PartialKind::ToolCall => Some(DshRenderBlock::ToolCall {
                name: if self.name.is_empty() {
                    "tool".into()
                } else {
                    self.name.clone()
                },
                call_id: self.call_id.clone(),
                arguments: self.text.clone(),
                edit: edit_detail_from_arguments(&self.text, None),
            }),
            PartialKind::Image => Some(DshRenderBlock::Image {
                attachment_id: None,
                media_type: None,
                name: None,
                raw: "{}".into(),
            }),
            PartialKind::Unknown => None,
        }
    }
}

#[derive(Debug)]
struct PartialMessage {
    blocks: BTreeMap<i64, PartialBlock>,
    finish: DshRenderFinish,
    terminal_seq: Option<i64>,
    final_content_applied: bool,
    lineage: Vec<i64>,
}

impl Default for PartialMessage {
    fn default() -> Self {
        Self {
            blocks: BTreeMap::new(),
            finish: DshRenderFinish::Running,
            terminal_seq: None,
            final_content_applied: false,
            lineage: Vec::new(),
        }
    }
}

impl PartialMessage {
    fn display(&self) -> (DshRenderKind, DshRenderContent) {
        let only_reasoning = !self.blocks.is_empty()
            && self
                .blocks
                .values()
                .all(|block| block.kind == PartialKind::Reasoning);
        let blocks = self
            .blocks
            .values()
            .filter_map(PartialBlock::render_block)
            .collect();
        (
            if only_reasoning {
                DshRenderKind::Thinking
            } else {
                DshRenderKind::Assistant
            },
            DshRenderContent::from_blocks(blocks),
        )
    }

    fn replace_with_final(&mut self, content: DshRenderContent) {
        self.blocks = content
            .blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| (index as i64, PartialBlock::from_render_block(block)))
            .collect();
    }
}

fn upsert_event(seq: i64, kind: DshRenderKind, text: String) -> DshRenderUpdate {
    upsert_event_content(
        seq,
        kind,
        DshRenderContent::from_blocks(vec![DshRenderBlock::Plain { text }]),
    )
}

fn upsert_event_content(
    seq: i64,
    kind: DshRenderKind,
    content: DshRenderContent,
) -> DshRenderUpdate {
    upsert_event_content_with_projection(
        seq,
        kind,
        content,
        default_visibility(kind),
        DshRenderFinish::Completed,
        None,
        default_selectable_for_kind(kind),
    )
}

fn upsert_event_content_with_projection(
    seq: i64,
    kind: DshRenderKind,
    content: DshRenderContent,
    visibility: DshRenderVisibility,
    finish: DshRenderFinish,
    group_key: Option<String>,
    selectable: bool,
) -> DshRenderUpdate {
    DshRenderUpdate::Upsert(DshRenderEntry {
        id: DshRenderEntryId::Event { seq },
        source_seq: seq,
        kind,
        text: content.fallback.clone(),
        partial: false,
        visibility,
        finish,
        group_key,
        selectable,
        lineage: vec![seq],
        content,
    })
}

fn partial_entry(
    key: (i64, i64),
    seq: i64,
    kind: DshRenderKind,
    content: DshRenderContent,
    partial: &PartialMessage,
) -> DshRenderEntry {
    DshRenderEntry {
        id: DshRenderEntryId::Partial {
            turn: key.0,
            step: key.1,
        },
        source_seq: seq,
        kind,
        text: content.fallback.clone(),
        partial: partial.finish == DshRenderFinish::Running,
        visibility: if partial.finish == DshRenderFinish::Running {
            DshRenderVisibility::Visible
        } else if kind == DshRenderKind::Thinking {
            DshRenderVisibility::Collapsed
        } else {
            DshRenderVisibility::Visible
        },
        finish: partial.finish,
        group_key: Some(format!("assistant:{}:{}", key.0, key.1)),
        selectable: true,
        lineage: partial.lineage.clone(),
        content,
    }
}

fn classify_user_message(
    data: &Value,
) -> (DshRenderKind, DshRenderVisibility, Option<String>, bool) {
    let source_kind = data
        .pointer("/source/kind")
        .or_else(|| data.pointer("/source/type"))
        .or_else(|| data.pointer("/source/role"))
        .or_else(|| data.get("role"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    let plugin = data
        .pointer("/source/plugin")
        .or_else(|| data.pointer("/source/pluginId"))
        .or_else(|| data.pointer("/source/pluginName"))
        .or_else(|| data.get("plugin"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if plugin.as_deref() == Some("compact") {
        return (
            DshRenderKind::Compaction,
            DshRenderVisibility::Collapsed,
            Some("compaction".into()),
            false,
        );
    }
    match source_kind.as_deref() {
        Some("user") => (
            DshRenderKind::User,
            DshRenderVisibility::Visible,
            None,
            true,
        ),
        Some("system")
        | Some("developer")
        | Some("agent-instructions")
        | Some("instructions")
        | Some("system-instruction") => (
            DshRenderKind::SystemInstruction,
            DshRenderVisibility::Hidden,
            Some("system-instructions".into()),
            false,
        ),
        Some("plugin")
        | Some("agent-context")
        | Some("context")
        | Some("injected")
        | Some("tool-context") => (
            DshRenderKind::AgentContext,
            DshRenderVisibility::Collapsed,
            Some(format!(
                "agent-context:{}",
                plugin.as_deref().unwrap_or("default")
            )),
            false,
        ),
        // Unknown source kinds use a safe collapsed context projection. This
        // prevents a new host injection type from leaking as a user row.
        _ if plugin.is_some() => (
            DshRenderKind::AgentContext,
            DshRenderVisibility::Collapsed,
            Some(format!(
                "agent-context:{}",
                plugin.as_deref().unwrap_or("default")
            )),
            false,
        ),
        _ => (
            DshRenderKind::AgentContext,
            DshRenderVisibility::Collapsed,
            Some("agent-context:unknown".into()),
            false,
        ),
    }
}

fn finish_from_kind(kind: &str) -> DshRenderFinish {
    match kind.to_ascii_lowercase().as_str() {
        "completed" | "complete" | "success" | "done" => DshRenderFinish::Completed,
        "abort" | "aborted" | "cancel" | "cancelled" | "canceled" | "interrupted" => {
            DshRenderFinish::Interrupted
        }
        "eof" | "closed" | "disconnect" | "disconnected" => DshRenderFinish::Eof,
        "error" | "failed" | "failure" => DshRenderFinish::Failed,
        _ => DshRenderFinish::Completed,
    }
}

fn finish_from_value(value: Option<&Value>) -> DshRenderFinish {
    let Some(value) = value else {
        return DshRenderFinish::Completed;
    };
    value
        .as_str()
        .map(finish_from_kind)
        .or_else(|| {
            value
                .get("kind")
                .or_else(|| value.get("type"))
                .and_then(Value::as_str)
                .map(finish_from_kind)
        })
        .unwrap_or(DshRenderFinish::Completed)
}

fn append_lineage(lineage: &mut Vec<i64>, values: &[i64]) {
    for value in values {
        if !lineage.contains(value) {
            lineage.push(*value);
        }
    }
}

fn surface_replace(surface_op: Option<&Value>) -> Option<(i64, i64)> {
    let object = surface_op?.as_object()?;
    if object.get("op").and_then(Value::as_str) != Some("replace") {
        return None;
    }
    Some((
        object.get("start").and_then(Value::as_i64)?,
        object.get("end").and_then(Value::as_i64)?,
    ))
}

fn integer(value: &Value, field: &str) -> Option<i64> {
    value.get(field).and_then(Value::as_i64)
}

fn string_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

fn request_id(frame: &Value) -> String {
    string_field(frame, "requestId")
        .or_else(|| string_field(frame, "rpcId"))
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::SessionEvent;
    use serde_json::json;

    fn entry(seq: i64, event_type: &str, data: Value) -> HistoryEntry {
        HistoryEntry {
            event: SessionEvent {
                event_type: event_type.into(),
                seq,
                time: seq as f64,
                data,
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        }
    }

    #[test]
    fn partial_assistant_stream_becomes_one_replaceable_entry() {
        let history = vec![
            entry(
                0,
                "assistant/chunk",
                json!({
                    "turn": 1,
                    "step": 0,
                    "chunk": { "type": "text-delta", "index": 0, "text": "hel" }
                }),
            ),
            entry(
                1,
                "assistant/chunk",
                json!({
                    "turn": 1,
                    "step": 0,
                    "chunk": { "type": "text-delta", "index": 0, "text": "lo" }
                }),
            ),
        ];
        let mut adapter = DshPresentationAdapter::default();
        let updates = adapter.adapt_history(&history);
        assert_eq!(updates.len(), 2);
        assert!(matches!(
            &updates[1],
            DshRenderUpdate::Upsert(DshRenderEntry {
                id: DshRenderEntryId::Partial { turn: 1, step: 0 },
                kind: DshRenderKind::Assistant,
                text,
                ..
            }) if text == "hello"
        ));
    }

    #[test]
    fn injected_context_is_hidden_or_collapsed_by_source_semantics() {
        let mut adapter = DshPresentationAdapter::default();
        let system = adapter.adapt_event(&entry(
            1,
            "user/message",
            json!({
                "source": { "kind": "agent-instructions" },
                "content": [{ "type": "text", "text": "never show this in transcript" }]
            }),
        ));
        let plugin = adapter.adapt_event(&entry(
            2,
            "user/message",
            json!({
                "source": { "kind": "plugin", "plugin": "repo-context" },
                "content": [{ "type": "text", "text": "plugin context" }]
            }),
        ));
        let unknown = adapter.adapt_event(&entry(
            3,
            "user/message",
            json!({
                "source": { "kind": "future-injection" },
                "content": [{ "type": "text", "text": "future context" }]
            }),
        ));
        assert!(matches!(
            system.as_slice(),
            [DshRenderUpdate::Upsert(DshRenderEntry {
                kind: DshRenderKind::SystemInstruction,
                visibility: DshRenderVisibility::Hidden,
                selectable: false,
                ..
            })]
        ));
        assert!(matches!(
            plugin.as_slice(),
            [DshRenderUpdate::Upsert(DshRenderEntry {
                kind: DshRenderKind::AgentContext,
                visibility: DshRenderVisibility::Collapsed,
                group_key: Some(_),
                ..
            })]
        ));
        assert!(matches!(
            unknown.as_slice(),
            [DshRenderUpdate::Upsert(DshRenderEntry {
                kind: DshRenderKind::AgentContext,
                visibility: DshRenderVisibility::Collapsed,
                ..
            })]
        ));
    }

    #[test]
    fn turn_end_finalizes_partial_without_waiting_for_final_message() {
        let mut adapter = DshPresentationAdapter::default();
        adapter.adapt_event(&entry(
            0,
            "assistant/chunk",
            json!({
                "turn": 4,
                "step": 0,
                "chunk": { "type": "text-delta", "index": 0, "text": "partial" }
            }),
        ));
        let updates = adapter.adapt_event(&entry(
            1,
            "turn/end",
            json!({ "turn": 4, "reason": { "kind": "aborted" } }),
        ));
        assert!(matches!(
            updates.first(),
            Some(DshRenderUpdate::Upsert(DshRenderEntry {
                id: DshRenderEntryId::Partial { turn: 4, step: 0 },
                partial: false,
                finish: DshRenderFinish::Interrupted,
                text,
                ..
            })) if text == "partial"
        ));
        let repeated = adapter.adapt_event(&entry(
            2,
            "turn/end",
            json!({ "turn": 4, "reason": { "kind": "aborted" } }),
        ));
        assert!(!repeated.iter().any(|update| matches!(
            update,
            DshRenderUpdate::Upsert(DshRenderEntry {
                id: DshRenderEntryId::Partial { turn: 4, step: 0 },
                ..
            })
        )));
    }

    #[test]
    fn stream_error_finalizes_surface_and_keeps_terminal_state_idempotent() {
        let mut adapter = DshPresentationAdapter::default();
        adapter.adapt_event(&entry(
            0,
            "assistant/chunk",
            json!({
                "turn": 5,
                "step": 1,
                "chunk": { "type": "text-delta", "index": 0, "text": "before eof" }
            }),
        ));
        let updates = adapter.adapt_event(&entry(
            1,
            "stream/error",
            json!({ "error": { "code": "eof", "message": "provider closed" } }),
        ));
        assert!(matches!(
            updates.first(),
            Some(DshRenderUpdate::Upsert(DshRenderEntry {
                id: DshRenderEntryId::Partial { turn: 5, step: 1 },
                partial: false,
                finish: DshRenderFinish::Eof,
                ..
            }))
        ));
        let repeated = adapter.finalize_all(2, DshRenderFinish::Eof, Some("closed"));
        assert!(repeated.is_empty());
    }

    #[test]
    fn malformed_stream_chunk_becomes_a_visible_diagnostic() {
        let mut adapter = DshPresentationAdapter::default();
        let updates = adapter.adapt_event(&entry(
            9,
            "assistant/chunk",
            json!({ "chunk": { "type": "text-delta", "text": "orphan" } }),
        ));
        assert!(matches!(
            updates.as_slice(),
            [DshRenderUpdate::Upsert(DshRenderEntry {
                kind: DshRenderKind::Error,
                text,
                ..
            })] if text.contains("Malformed")
        ));
    }

    #[test]
    fn final_message_finalizes_the_same_stable_surface() {
        let mut adapter = DshPresentationAdapter::default();
        adapter.adapt_event(&entry(
            0,
            "assistant/chunk",
            json!({
                "turn": 1,
                "step": 0,
                "chunk": { "type": "text-delta", "index": 0, "text": "draft" }
            }),
        ));
        let updates = adapter.adapt_event(&entry(
            1,
            "assistant/message",
            json!({
                "turn": 1,
                "step": 0,
                "message": { "content": [{ "type": "text", "text": "final" }] }
            }),
        ));
        assert!(matches!(
            updates.first(),
            Some(DshRenderUpdate::Upsert(DshRenderEntry {
                id: DshRenderEntryId::Partial { turn: 1, step: 0 },
                kind: DshRenderKind::Assistant,
                text,
                partial: false,
                finish: DshRenderFinish::Completed,
                ..
            })) if text == "final"
        ));
    }

    #[test]
    fn queue_and_interaction_frames_are_typed() {
        let item = SessionQueueItem {
            id: "q1".into(),
            placement: QueuePlacement::Queued,
            message: json!({ "content": [{ "type": "text", "text": "hello" }] }),
        };
        let queue = DshQueueItem::from(&item);
        assert_eq!(queue.content.summary.as_deref(), Some("hello"));
        assert_eq!(queue.content.editable_text.as_deref(), Some("hello"));
        let interaction = DshInteraction::approval_from_frame(&json!({
            "requestId": "rpc-1",
            "approvalId": "approval-1",
            "toolName": "bash"
        }));
        assert_eq!(interaction.request_id(), "rpc-1");
        assert!(matches!(interaction, DshInteraction::Approval { .. }));
    }

    #[test]
    fn queue_content_keeps_multiline_and_rich_blocks_visible() {
        let content = DshQueueContent::from_message(&json!({
            "content": [
                { "type": "text", "text": " first\n\nsecond" },
                { "type": "image" },
                { "type": "tool-call", "name": "read", "arguments": "{\"path\":\"a\"}" },
                { "type": "tool-result", "content": [{ "type": "text", "text": "ok\nnext" }] }
            ]
        }));
        assert_eq!(content.summary.as_deref(), Some("first"));
        assert_eq!(content.lines[0], " first");
        assert_eq!(content.lines[1], "");
        assert!(content.lines.iter().any(|line| line == "[image]"));
        assert!(content
            .lines
            .iter()
            .any(|line| line.starts_with("[tool-call] read")));
        assert!(content.lines.iter().any(|line| line == "          next"));
        assert_eq!(content.editable_text, None);
        assert_eq!(content.block_count, 4);
    }

    #[test]
    fn tool_call_and_result_use_distinct_render_kinds() {
        let mut adapter = DshPresentationAdapter::default();
        let call = adapter.adapt_event(&entry(
            2,
            "tool/call",
            json!({ "name": "bash", "arguments": "-lc pwd" }),
        ));
        let result = adapter.adapt_event(&entry(
            3,
            "tool/result",
            json!({
                "message": { "content": [{ "type": "text", "text": "/work" }] }
            }),
        ));
        assert!(matches!(
            call.as_slice(),
            [DshRenderUpdate::Upsert(DshRenderEntry {
                kind: DshRenderKind::ToolCall,
                text,
                ..
            })] if text == "bash -lc pwd"
        ));
        assert!(matches!(
            result.as_slice(),
            [DshRenderUpdate::Upsert(DshRenderEntry {
                kind: DshRenderKind::ToolResult,
                text,
                ..
            })] if text == "/work"
        ));
    }

    #[test]
    fn compaction_events_render_as_a_stable_compaction_entry() {
        let mut adapter = DshPresentationAdapter::default();
        let updates = adapter.adapt_event(&entry(
            8,
            "compaction/summary",
            json!({
                "summary": [{ "type": "text", "text": "kept context" }],
                "shadowedTokenCount": 42
            }),
        ));
        assert!(matches!(
            updates.as_slice(),
            [DshRenderUpdate::Upsert(DshRenderEntry {
                kind: DshRenderKind::Compaction,
                text,
                ..
            })] if text.contains("kept context")
        ));
        let prune = adapter.adapt_event(&entry(
            9,
            "compaction/prune",
            json!({ "shadowedSeqs": [1, 2], "shadowedTokenCount": 7 }),
        ));
        assert!(matches!(
            prune.as_slice(),
            [DshRenderUpdate::Upsert(DshRenderEntry {
                kind: DshRenderKind::Compaction,
                text,
                ..
            })] if text.contains("2 surface node") && text.contains("7")
        ));
    }

    #[test]
    fn surface_replacement_removes_old_source_range_before_new_block() {
        let mut adapter = DshPresentationAdapter::default();
        adapter.adapt_event(&entry(
            1,
            "assistant/message",
            json!({
                "message": { "content": [{ "type": "text", "text": "old" }] }
            }),
        ));
        let mut replacement = entry(
            4,
            "assistant/message",
            json!({
                "message": { "content": [{ "type": "text", "text": "new" }] }
            }),
        );
        replacement.event.surface_op = Some(json!({ "op": "replace", "start": 0, "end": 3 }));
        let updates = adapter.adapt_event(&replacement);
        assert!(matches!(
            updates.first(),
            Some(DshRenderUpdate::RemoveSourceRange { start: 0, end: 3 })
        ));
        assert!(matches!(
            updates.get(1),
            Some(DshRenderUpdate::Upsert(DshRenderEntry { text, .. })) if text == "new"
        ));
    }
}
