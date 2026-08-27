//! Host-independent effects emitted by Grok UI interactions.

use std::collections::{HashMap, HashSet, VecDeque};

use dsh_pager::{
    DshGeneration, DshQueueItemId, DshRequestId, DshSeq, DshSessionId, PagerError, PagerResult,
    RpcTransport, SessionState,
};
use dsh_pager_protocol::{
    AgentPresetListValue, ModelSelection, PromptMode, QueueAction, SessionModeId,
    SessionModelsValue, SubagentAddress, TuiInteractionResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Semantic user intent emitted by a Grok view. It has no transport or
/// renderer references and can be reduced in a host-neutral test harness.
#[derive(Debug, Clone, PartialEq)]
pub enum UiIntent {
    CancelSession,
    SubmitPrompt {
        text: String,
        mode: PromptMode,
    },
    AttachSession {
        session_id: DshSessionId,
    },
    ListSessions {
        revision: u64,
    },
    SearchSessions {
        query: String,
        revision: u64,
    },
    QueueMutation {
        item_id: DshQueueItemId,
        action: QueueAction,
    },
    RespondInteraction {
        request_id: DshRequestId,
        interaction: TuiInteractionResponse,
    },
    RenameSession {
        title: String,
    },
    ForkSession {
        at_seq: Option<DshSeq>,
    },
    ArchiveSession,
    ArchiveSessionTarget {
        session_id: DshSessionId,
    },
    FileSearchQuery {
        query: String,
        revision: u64,
    },
    PreviewMedia {
        attachment_id: String,
    },
    ReorderSession {
        workspace_id: String,
        session_id: DshSessionId,
        before_session_id: Option<String>,
    },
    InterruptSubagent {
        address: SubagentAddress,
    },
    SetSessionMode {
        mode_id: Option<SessionModeId>,
    },
    ListAgentPresets {
        revision: u64,
    },
    SelectAgentPreset {
        agent_preset: String,
    },
    ListSessionModels {
        revision: u64,
    },
    SelectSessionModel {
        provider: String,
        model: String,
        reasoning_effort: Option<String>,
    },
}

/// Minimal host context supplied by the runtime adapter. Keeping this small
/// prevents a view or generic effect sink from acquiring `SessionState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiContext {
    pub session_id: DshSessionId,
    pub generation: DshGeneration,
    pub request_id: DshRequestId,
}

impl UiContext {
    pub fn from_session(session: &SessionState) -> Self {
        Self {
            session_id: DshSessionId::new(session.session_id()),
            generation: DshGeneration::new(session.generation()),
            request_id: DshRequestId::new("pending"),
        }
    }

    pub fn for_operation(session: &SessionState, request_id: DshRequestId) -> Self {
        Self {
            session_id: DshSessionId::new(session.session_id()),
            generation: DshGeneration::new(session.generation()),
            request_id,
        }
    }
}

/// Host operation identity used for idempotency and receipt diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationKey {
    pub session_id: DshSessionId,
    pub generation: DshGeneration,
    pub request_id: DshRequestId,
    pub action: String,
    pub dedupe_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiEffectStatus {
    Accepted,
    Queued,
    Pending,
    Rejected,
    Stale,
    Conflict,
    Unsupported,
    Failed,
    Timeout,
}

/// Explicit host response. A receipt is admission only; authoritative state
/// still arrives through the next snapshot/notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiEffectReceipt {
    pub status: UiEffectStatus,
    pub operation: OperationKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

/// Keep receipt presentation consistent across prompt, queue and interaction
/// surfaces. Admission never implies authoritative convergence.
pub fn receipt_status_message(receipt: &UiEffectReceipt, subject: &str) -> String {
    if let Some(diagnostic) = receipt.diagnostic.as_deref() {
        return match receipt.status {
            UiEffectStatus::Conflict => {
                format!("{subject} conflict: {diagnostic}; refresh and retry")
            }
            UiEffectStatus::Stale => {
                format!("{subject} stale: {diagnostic}; action was not applied")
            }
            UiEffectStatus::Unsupported => format!("{subject} unavailable: {diagnostic}"),
            UiEffectStatus::Timeout => format!("{subject} timed out: {diagnostic}; retry is safe"),
            UiEffectStatus::Rejected => format!("{subject} rejected: {diagnostic}"),
            UiEffectStatus::Failed => format!("{subject} failed: {diagnostic}"),
            _ => diagnostic.to_string(),
        };
    }
    match receipt.status {
        UiEffectStatus::Rejected => format!("{subject} rejected by host"),
        UiEffectStatus::Conflict => format!("{subject} conflict; refresh and retry"),
        UiEffectStatus::Stale => format!("{subject} is stale; action was not applied"),
        UiEffectStatus::Unsupported => format!("{subject} is unavailable"),
        UiEffectStatus::Timeout => format!("{subject} timed out; retry is safe"),
        UiEffectStatus::Failed => format!("{subject} failed"),
        _ => format!("{subject} completed"),
    }
}

/// DSH-neutral effect after intent compilation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiEffect {
    CancelSession {
        operation: OperationKey,
    },
    SubmitPrompt {
        operation: OperationKey,
        text: String,
        mode: PromptMode,
    },
    AttachSession {
        operation: OperationKey,
        session_id: DshSessionId,
    },
    ListSessions {
        operation: OperationKey,
        revision: u64,
    },
    SearchSessions {
        operation: OperationKey,
        query: String,
        revision: u64,
    },
    QueueMutation {
        operation: OperationKey,
        item_id: DshQueueItemId,
        action: QueueAction,
    },
    RespondInteraction {
        operation: OperationKey,
        request_id: DshRequestId,
        interaction: TuiInteractionResponse,
    },
    RenameSession {
        operation: OperationKey,
        title: String,
    },
    ForkSession {
        operation: OperationKey,
        at_seq: Option<DshSeq>,
    },
    ArchiveSession {
        operation: OperationKey,
    },
    ArchiveSessionTarget {
        operation: OperationKey,
        session_id: DshSessionId,
    },
    FileSearchQuery {
        operation: OperationKey,
        query: String,
        revision: u64,
    },
    PreviewMedia {
        operation: OperationKey,
        attachment_id: String,
    },
    ReorderSession {
        operation: OperationKey,
        workspace_id: String,
        session_id: DshSessionId,
        before_session_id: Option<String>,
    },
    InterruptSubagent {
        operation: OperationKey,
        address: SubagentAddress,
    },
    SetSessionMode {
        operation: OperationKey,
        mode_id: Option<SessionModeId>,
    },
    ListAgentPresets {
        operation: OperationKey,
        revision: u64,
    },
    SelectAgentPreset {
        operation: OperationKey,
        agent_preset: String,
    },
    ListSessionModels {
        operation: OperationKey,
        revision: u64,
    },
    SelectSessionModel {
        operation: OperationKey,
        provider: String,
        model: String,
        reasoning_effort: Option<String>,
    },
}

/// Boundary a non-DSH host (for example Codex CLI) can implement later.
pub trait UiEffectSink {
    fn submit(&mut self, intent: UiIntent, context: &UiContext) -> PagerResult<UiEffectReceipt>;
}

/// DSH's concrete effect sink. All RPC knowledge stays here instead of in
/// copied Grok view modules.
const COMPLETED_OPERATION_LIMIT: usize = 1_024;

/// Runtime-owned operation identity and duplicate ledger.
///
/// A sink is intentionally short-lived because it borrows the transport for
/// one dispatch.  The ledger is not: keeping it in the UI/host coordinator
/// preserves request sequencing and accepted-operation identity across those
/// sink instances.
#[derive(Debug)]
pub struct EffectLedger {
    next_request: u64,
    completed: HashSet<OperationKey>,
    completed_order: VecDeque<OperationKey>,
}

impl Default for EffectLedger {
    fn default() -> Self {
        Self {
            next_request: 1,
            completed: HashSet::new(),
            completed_order: VecDeque::new(),
        }
    }
}

impl EffectLedger {
    fn prepare_operation(&mut self, operation: &mut OperationKey) {
        if operation.request_id.as_str() == "pending" {
            operation.request_id = DshRequestId::new(format!("ui-{}", self.next_request));
            self.next_request = self.next_request.saturating_add(1);
        }
    }

    fn contains(&self, operation: &OperationKey) -> bool {
        self.completed.contains(operation)
    }

    fn complete(&mut self, operation: OperationKey) {
        if !self.completed.insert(operation.clone()) {
            return;
        }
        self.completed_order.push_back(operation);
        while self.completed_order.len() > COMPLETED_OPERATION_LIMIT {
            if let Some(expired) = self.completed_order.pop_front() {
                self.completed.remove(&expired);
            }
        }
    }

    fn duplicate_receipt(&self, operation: OperationKey) -> UiEffectReceipt {
        UiEffectReceipt {
            status: UiEffectStatus::Accepted,
            operation,
            diagnostic: Some("duplicate operation suppressed".into()),
            retryable: Some(false),
        }
    }
}

pub struct DshEffectSink<'transport, 'ledger> {
    pub transport: &'transport mut RpcTransport,
    ledger: &'ledger mut EffectLedger,
    last_attachment_preview: Option<dsh_pager::AttachmentPreview>,
    last_file_references: Option<dsh_pager_protocol::FileReferencesListValue>,
}

impl<'transport, 'ledger> DshEffectSink<'transport, 'ledger> {
    pub fn new(
        transport: &'transport mut RpcTransport,
        ledger: &'ledger mut EffectLedger,
    ) -> DshEffectSink<'transport, 'ledger> {
        DshEffectSink {
            transport,
            ledger,
            last_attachment_preview: None,
            last_file_references: None,
        }
    }

    /// Return the bounded attachment payload admitted by the most recent
    /// media-preview effect. The receipt remains admission-only; this buffer
    /// is a host adapter handoff for the ephemeral preview surface.
    pub fn take_attachment_preview(&mut self) -> Option<dsh_pager::AttachmentPreview> {
        self.last_attachment_preview.take()
    }

    pub fn take_file_references(&mut self) -> Option<dsh_pager_protocol::FileReferencesListValue> {
        self.last_file_references.take()
    }
}

impl UiEffectSink for DshEffectSink<'_, '_> {
    fn submit(&mut self, intent: UiIntent, context: &UiContext) -> PagerResult<UiEffectReceipt> {
        self.dispatch_effect(compile_intent(intent, context))
    }
}

/// Compile semantic intent into a transport-free effect. Request ids are
/// deliberately `pending` until the concrete sink admits the operation.
pub fn compile_intent(intent: UiIntent, context: &UiContext) -> UiEffect {
    let target_session_id = match &intent {
        UiIntent::AttachSession { session_id } => session_id.clone(),
        UiIntent::ArchiveSessionTarget { session_id } => session_id.clone(),
        UiIntent::ReorderSession { session_id, .. } => session_id.clone(),
        UiIntent::InterruptSubagent { address } => {
            DshSessionId::new(address.parent_session_id.clone())
        }
        _ => context.session_id.clone(),
    };
    let action_name = match &intent {
        UiIntent::CancelSession => "cancel-session",
        UiIntent::SubmitPrompt { .. } => "submit",
        UiIntent::AttachSession { .. } => "attach",
        UiIntent::ListSessions { .. } => "list-sessions",
        UiIntent::SearchSessions { .. } => "search-sessions",
        UiIntent::QueueMutation { .. } => "queue-mutation",
        UiIntent::RespondInteraction { .. } => "respond-interaction",
        UiIntent::RenameSession { .. } => "rename",
        UiIntent::ForkSession { .. } => "fork",
        UiIntent::ArchiveSession => "archive",
        UiIntent::ArchiveSessionTarget { .. } => "archive",
        UiIntent::FileSearchQuery { .. } => "file-search-query",
        UiIntent::PreviewMedia { .. } => "media-preview",
        UiIntent::ReorderSession { .. } => "reorder-session",
        UiIntent::InterruptSubagent { .. } => "subagent-interrupt",
        UiIntent::SetSessionMode { .. } => "set-session-mode",
        UiIntent::ListAgentPresets { .. } => "list-agent-presets",
        UiIntent::SelectAgentPreset { .. } => "select-agent-preset",
        UiIntent::ListSessionModels { .. } => "list-session-models",
        UiIntent::SelectSessionModel { .. } => "select-session-model",
    };
    let dedupe_key = match &intent {
        UiIntent::CancelSession => action_name.to_string(),
        UiIntent::SubmitPrompt { text, mode } => {
            format!(
                "{action_name}:{}:{}",
                prompt_digest(text),
                mode_label(*mode)
            )
        }
        UiIntent::AttachSession { session_id } => format!("{action_name}:{session_id}"),
        UiIntent::ListSessions { revision } => format!("{action_name}:{revision}"),
        UiIntent::SearchSessions { query, revision } => {
            format!("{action_name}:{revision}:{}", prompt_digest(query))
        }
        UiIntent::QueueMutation { item_id, action } => {
            format!("{action_name}:{item_id}:{action:?}")
        }
        UiIntent::RespondInteraction { request_id, .. } => format!("{action_name}:{request_id}"),
        UiIntent::RenameSession { title } => format!("{action_name}:{}", prompt_digest(title)),
        UiIntent::ForkSession { at_seq } => format!("{action_name}:{at_seq:?}"),
        UiIntent::ArchiveSession => action_name.to_string(),
        UiIntent::ArchiveSessionTarget { session_id } => format!("{action_name}:{session_id}"),
        UiIntent::FileSearchQuery { query, revision } => {
            format!("{action_name}:{revision}:{}", prompt_digest(query))
        }
        UiIntent::PreviewMedia { attachment_id } => {
            format!("{action_name}:{attachment_id}")
        }
        UiIntent::ReorderSession {
            workspace_id,
            session_id,
            before_session_id,
        } => format!("{action_name}:{workspace_id}:{session_id}:{before_session_id:?}"),
        UiIntent::InterruptSubagent { address } => {
            format!(
                "{action_name}:{}:{}",
                address.child_session_id, address.mode as u8
            )
        }
        UiIntent::SetSessionMode { mode_id } => {
            format!(
                "{action_name}:{}",
                mode_id.map(SessionModeId::as_str).unwrap_or("cycle")
            )
        }
        UiIntent::ListAgentPresets { revision } => format!("{action_name}:{revision}"),
        UiIntent::SelectAgentPreset { agent_preset } => {
            format!("{action_name}:{agent_preset}")
        }
        UiIntent::ListSessionModels { revision } => format!("{action_name}:{revision}"),
        UiIntent::SelectSessionModel {
            provider,
            model,
            reasoning_effort,
        } => format!(
            "{action_name}:{provider}:{model}:{}",
            reasoning_effort.as_deref().unwrap_or("-")
        ),
    };
    // Interaction request ids are host-owned correlation ids. Preserve them
    // even when a caller did not pre-seed an operation context; generation is
    // still taken from the active session context below.
    let request_id = match &intent {
        UiIntent::RespondInteraction { request_id, .. }
            if context.request_id.as_str() == "pending" =>
        {
            request_id.clone()
        }
        _ => context.request_id.clone(),
    };
    let dedupe_key = if request_id.as_str() == "pending" {
        dedupe_key
    } else {
        format!("{action_name}:{request_id}")
    };
    let operation = OperationKey {
        session_id: target_session_id,
        generation: context.generation,
        request_id,
        action: action_name.to_string(),
        dedupe_key,
    };
    match intent {
        UiIntent::CancelSession => UiEffect::CancelSession { operation },
        UiIntent::SubmitPrompt { text, mode } => UiEffect::SubmitPrompt {
            operation,
            text,
            mode,
        },
        UiIntent::AttachSession { session_id } => UiEffect::AttachSession {
            operation,
            session_id,
        },
        UiIntent::ListSessions { revision } => UiEffect::ListSessions {
            operation,
            revision,
        },
        UiIntent::SearchSessions { query, revision } => UiEffect::SearchSessions {
            operation,
            query,
            revision,
        },
        UiIntent::QueueMutation { item_id, action } => UiEffect::QueueMutation {
            operation,
            item_id,
            action,
        },
        UiIntent::RespondInteraction {
            request_id,
            interaction,
        } => UiEffect::RespondInteraction {
            operation,
            request_id,
            interaction,
        },
        UiIntent::RenameSession { title } => UiEffect::RenameSession { operation, title },
        UiIntent::ForkSession { at_seq } => UiEffect::ForkSession { operation, at_seq },
        UiIntent::ArchiveSession => UiEffect::ArchiveSession { operation },
        UiIntent::ArchiveSessionTarget { session_id } => UiEffect::ArchiveSessionTarget {
            operation,
            session_id,
        },
        UiIntent::FileSearchQuery { query, revision } => UiEffect::FileSearchQuery {
            operation,
            query,
            revision,
        },
        UiIntent::PreviewMedia { attachment_id } => UiEffect::PreviewMedia {
            operation,
            attachment_id,
        },
        UiIntent::ReorderSession {
            workspace_id,
            session_id,
            before_session_id,
        } => UiEffect::ReorderSession {
            operation,
            workspace_id,
            session_id,
            before_session_id,
        },
        UiIntent::InterruptSubagent { address } => {
            UiEffect::InterruptSubagent { operation, address }
        }
        UiIntent::SetSessionMode { mode_id } => UiEffect::SetSessionMode { operation, mode_id },
        UiIntent::ListAgentPresets { revision } => UiEffect::ListAgentPresets {
            operation,
            revision,
        },
        UiIntent::SelectAgentPreset { agent_preset } => UiEffect::SelectAgentPreset {
            operation,
            agent_preset,
        },
        UiIntent::ListSessionModels { revision } => UiEffect::ListSessionModels {
            operation,
            revision,
        },
        UiIntent::SelectSessionModel {
            provider,
            model,
            reasoning_effort,
        } => UiEffect::SelectSessionModel {
            operation,
            provider,
            model,
            reasoning_effort,
        },
    }
}

impl DshEffectSink<'_, '_> {
    pub fn dispatch_effect(&mut self, effect: UiEffect) -> PagerResult<UiEffectReceipt> {
        let (operation, result) = match effect {
            UiEffect::CancelSession { mut operation } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result =
                    dsh_pager::cancel_session_id(self.transport, operation.session_id.as_str());
                (operation, result.map(|value| value.accepted))
            }
            UiEffect::SubmitPrompt {
                mut operation,
                text,
                mode,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::submit_prompt_for_session(
                    self.transport,
                    operation.session_id.as_str(),
                    text,
                    mode,
                );
                (operation, result.map(|value| value.accepted))
            }
            UiEffect::ListSessions {
                mut operation,
                revision: _,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::list_sessions(self.transport);
                (operation, result.map(|_| true))
            }
            UiEffect::SearchSessions {
                mut operation,
                query,
                revision: _,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::search_sessions(self.transport, &query);
                (operation, result.map(|_| true))
            }
            UiEffect::QueueMutation {
                mut operation,
                item_id,
                action,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let session =
                    SessionState::new(operation.session_id.to_string(), operation.generation.get());
                let result =
                    dsh_pager::update_queue(self.transport, &session, item_id.to_string(), action);
                (operation, result.map(|value| value.accepted))
            }
            UiEffect::RespondInteraction {
                mut operation,
                request_id,
                interaction,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let session =
                    SessionState::new(operation.session_id.to_string(), operation.generation.get());
                let result = dsh_pager::respond(
                    self.transport,
                    &session,
                    request_id.to_string(),
                    interaction,
                );
                (operation, result.map(|value| value.accepted))
            }
            UiEffect::RenameSession {
                mut operation,
                title,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::rename_session_id(
                    self.transport,
                    operation.session_id.as_str(),
                    title,
                );
                (operation, result.map(|_| true))
            }
            UiEffect::ForkSession {
                mut operation,
                at_seq,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::fork_session_id(
                    self.transport,
                    operation.session_id.as_str(),
                    at_seq.map(DshSeq::get),
                );
                (operation, result.map(|_| true))
            }
            UiEffect::ArchiveSession { mut operation } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result =
                    dsh_pager::archive_session(self.transport, operation.session_id.as_str());
                (operation, result.map(|_| true))
            }
            UiEffect::ArchiveSessionTarget {
                mut operation,
                session_id,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::archive_session(self.transport, session_id.as_str());
                (operation, result.map(|_| true))
            }
            UiEffect::FileSearchQuery {
                mut operation,
                query,
                revision: _,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::list_file_references(
                    self.transport,
                    operation.session_id.as_str(),
                    &query,
                );
                if let Ok(rows) = &result {
                    self.last_file_references = Some(rows.clone());
                }
                (operation, result.map(|_| true))
            }
            UiEffect::PreviewMedia {
                mut operation,
                attachment_id,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::fetch_attachment(
                    self.transport,
                    operation.session_id.as_str(),
                    &attachment_id,
                );
                if let Ok(preview) = &result {
                    self.last_attachment_preview = Some(preview.clone());
                }
                (operation, result.map(|preview| !preview.data.is_empty()))
            }
            UiEffect::ReorderSession {
                mut operation,
                workspace_id,
                session_id,
                before_session_id,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::reorder_session(
                    self.transport,
                    &workspace_id,
                    session_id.as_str(),
                    before_session_id.as_deref(),
                );
                (operation, result.map(|_| true))
            }
            UiEffect::AttachSession { mut operation, .. } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                (
                    operation,
                    Err(dsh_pager::PagerError::new(
                        "attach requires loader/session swap",
                    )),
                )
            }
            UiEffect::InterruptSubagent {
                mut operation,
                address,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::interrupt_subagent(self.transport, &address);
                (operation, result.map(|value| value.accepted))
            }
            UiEffect::SetSessionMode {
                mut operation,
                mode_id,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let session =
                    SessionState::new(operation.session_id.to_string(), operation.generation.get());
                let result = dsh_pager::set_session_mode(self.transport, &session, mode_id);
                (operation, result.map(|value| value.accepted))
            }
            UiEffect::ListAgentPresets {
                mut operation,
                revision: _,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::list_agent_presets(self.transport);
                (operation, result.map(|_| true))
            }
            UiEffect::SelectAgentPreset {
                mut operation,
                agent_preset,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::select_agent_preset(
                    self.transport,
                    operation.session_id.as_str(),
                    &agent_preset,
                );
                (operation, result.map(|_| true))
            }
            UiEffect::ListSessionModels {
                mut operation,
                revision: _,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result =
                    dsh_pager::session_models(self.transport, operation.session_id.as_str());
                (operation, result.map(|_| true))
            }
            UiEffect::SelectSessionModel {
                mut operation,
                provider,
                model,
                reasoning_effort,
            } => {
                self.ledger.prepare_operation(&mut operation);
                if self.ledger.contains(&operation) {
                    return Ok(self.ledger.duplicate_receipt(operation));
                }
                let result = dsh_pager::select_session_model(
                    self.transport,
                    operation.session_id.as_str(),
                    &provider,
                    &model,
                    reasoning_effort.as_deref(),
                );
                (operation, result.map(|_| true))
            }
        };
        match result {
            Ok(true) => {
                self.ledger.complete(operation.clone());
                Ok(UiEffectReceipt {
                    status: UiEffectStatus::Accepted,
                    operation,
                    diagnostic: None,
                    retryable: Some(false),
                })
            }
            Ok(false) => Ok(UiEffectReceipt {
                status: UiEffectStatus::Rejected,
                operation,
                diagnostic: Some("host rejected operation".into()),
                retryable: Some(false),
            }),
            Err(error) => Ok(UiEffectReceipt {
                status: classify_effect_error(&error),
                operation,
                diagnostic: Some(error.to_string()),
                retryable: Some(true),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiEffectCompletion {
    pub effect: UiEffect,
    pub receipt: UiEffectReceipt,
    pub session_list: Option<dsh_pager_protocol::SessionListValue>,
    pub session_search: Option<dsh_pager_protocol::SessionSearchValue>,
    pub file_references: Option<dsh_pager_protocol::FileReferencesListValue>,
    pub attachment_preview: Option<dsh_pager::AttachmentPreview>,
    pub agent_preset_list: Option<AgentPresetListValue>,
    pub selected_agent_preset: Option<String>,
    pub session_models: Option<SessionModelsValue>,
    pub selected_model: Option<ModelSelection>,
}

#[derive(Debug, Clone)]
struct PendingEffect {
    effect: UiEffect,
}

#[derive(Debug, Default)]
pub struct AsyncEffectExecutor {
    pending: HashMap<u64, PendingEffect>,
}

impl AsyncEffectExecutor {
    pub fn submit(
        &mut self,
        transport: &mut RpcTransport,
        ledger: &mut EffectLedger,
        intent: UiIntent,
        context: &UiContext,
    ) -> PagerResult<UiEffectReceipt> {
        self.submit_effect(transport, ledger, compile_intent(intent, context))
    }

    pub fn submit_effect(
        &mut self,
        transport: &mut RpcTransport,
        ledger: &mut EffectLedger,
        mut effect: UiEffect,
    ) -> PagerResult<UiEffectReceipt> {
        let mut operation = effect_operation(&effect).clone();
        ledger.prepare_operation(&mut operation);
        if ledger.contains(&operation) {
            return Ok(ledger.duplicate_receipt(operation));
        }
        let Some((method, params)) = encode_async_request(&effect, &operation)? else {
            return Ok(UiEffectReceipt {
                status: UiEffectStatus::Unsupported,
                operation,
                diagnostic: Some("attach requires the session load barrier".into()),
                retryable: Some(false),
            });
        };
        set_effect_operation(&mut effect, operation.clone());
        let request_id = transport.begin_call_value(method, params)?;
        self.pending.insert(request_id, PendingEffect { effect });
        Ok(UiEffectReceipt {
            status: UiEffectStatus::Pending,
            operation,
            diagnostic: None,
            retryable: Some(true),
        })
    }

    pub fn poll(
        &mut self,
        transport: &mut RpcTransport,
        ledger: &mut EffectLedger,
    ) -> PagerResult<Vec<UiEffectCompletion>> {
        let ids = self.pending.keys().copied().collect::<Vec<_>>();
        let mut completions = Vec::new();
        for request_id in ids {
            let result = match transport.poll_call_value(request_id) {
                Ok(Some(value)) => decode_async_result(&self.pending[&request_id].effect, value),
                Ok(None) => continue,
                Err(error) => Err(error),
            };
            let Some(pending) = self.pending.remove(&request_id) else {
                continue;
            };
            completions.push(build_completion(pending.effect, ledger, result));
        }
        Ok(completions)
    }
}

fn effect_operation(effect: &UiEffect) -> &OperationKey {
    match effect {
        UiEffect::CancelSession { operation }
        | UiEffect::SubmitPrompt { operation, .. }
        | UiEffect::AttachSession { operation, .. }
        | UiEffect::ListSessions { operation, .. }
        | UiEffect::SearchSessions { operation, .. }
        | UiEffect::QueueMutation { operation, .. }
        | UiEffect::RespondInteraction { operation, .. }
        | UiEffect::RenameSession { operation, .. }
        | UiEffect::ForkSession { operation, .. }
        | UiEffect::ArchiveSession { operation }
        | UiEffect::ArchiveSessionTarget { operation, .. }
        | UiEffect::FileSearchQuery { operation, .. }
        | UiEffect::PreviewMedia { operation, .. }
        | UiEffect::ReorderSession { operation, .. }
        | UiEffect::InterruptSubagent { operation, .. }
        | UiEffect::SetSessionMode { operation, .. }
        | UiEffect::ListAgentPresets { operation, .. }
        | UiEffect::SelectAgentPreset { operation, .. }
        | UiEffect::ListSessionModels { operation, .. }
        | UiEffect::SelectSessionModel { operation, .. } => operation,
    }
}

fn set_effect_operation(effect: &mut UiEffect, operation: OperationKey) {
    match effect {
        UiEffect::CancelSession { operation: target }
        | UiEffect::SubmitPrompt {
            operation: target, ..
        }
        | UiEffect::AttachSession {
            operation: target, ..
        }
        | UiEffect::ListSessions {
            operation: target, ..
        }
        | UiEffect::SearchSessions {
            operation: target, ..
        }
        | UiEffect::QueueMutation {
            operation: target, ..
        }
        | UiEffect::RespondInteraction {
            operation: target, ..
        }
        | UiEffect::RenameSession {
            operation: target, ..
        }
        | UiEffect::ForkSession {
            operation: target, ..
        }
        | UiEffect::ArchiveSession { operation: target }
        | UiEffect::ArchiveSessionTarget {
            operation: target, ..
        }
        | UiEffect::FileSearchQuery {
            operation: target, ..
        }
        | UiEffect::PreviewMedia {
            operation: target, ..
        }
        | UiEffect::ReorderSession {
            operation: target, ..
        }
        | UiEffect::InterruptSubagent {
            operation: target, ..
        }
        | UiEffect::SetSessionMode {
            operation: target, ..
        }
        | UiEffect::ListAgentPresets {
            operation: target, ..
        }
        | UiEffect::SelectAgentPreset {
            operation: target, ..
        }
        | UiEffect::ListSessionModels {
            operation: target, ..
        }
        | UiEffect::SelectSessionModel {
            operation: target, ..
        } => *target = operation,
    }
}

fn encode_async_request(
    effect: &UiEffect,
    operation: &OperationKey,
) -> PagerResult<Option<(&'static str, Value)>> {
    let request = match effect {
        UiEffect::CancelSession { .. } => {
            Some(("session.cancel", json!({"sessionId": operation.session_id})))
        }
        UiEffect::SubmitPrompt { text, mode, .. } => Some((
            "session.prompt",
            json!({
                "sessionId": operation.session_id,
                "mode": mode,
                "content": [{"type": "text", "text": text}],
            }),
        )),
        UiEffect::ListSessions { .. } => Some(("session.list", json!({}))),
        UiEffect::SearchSessions { query, .. } => Some(("session.search", json!({"query": query}))),
        UiEffect::QueueMutation {
            item_id, action, ..
        } => Some((
            "session.updateQueue",
            json!({
                "sessionId": operation.session_id,
                "itemId": item_id,
                "action": action,
            }),
        )),
        UiEffect::RespondInteraction {
            request_id,
            interaction,
            ..
        } => Some((
            "tui.respond",
            json!({
                "sessionId": operation.session_id,
                "generation": operation.generation,
                "requestId": request_id,
                "interaction": interaction,
            }),
        )),
        UiEffect::RenameSession { title, .. } => Some((
            "session.rename",
            json!({"sessionId": operation.session_id, "title": title}),
        )),
        UiEffect::ForkSession { at_seq, .. } => Some((
            "session.fork",
            json!({"sessionId": operation.session_id, "atSeq": at_seq}),
        )),
        UiEffect::ArchiveSession { .. } => Some((
            "workspace.archiveSession",
            json!({"sessionId": operation.session_id}),
        )),
        UiEffect::ArchiveSessionTarget { session_id, .. } => {
            Some(("workspace.archiveSession", json!({"sessionId": session_id})))
        }
        UiEffect::FileSearchQuery { query, .. } => Some((
            "fileReferences.list",
            json!({"sessionId": operation.session_id, "query": query}),
        )),
        UiEffect::PreviewMedia { attachment_id, .. } => Some((
            "session.attachment",
            json!({
                "sessionId": operation.session_id,
                "attachmentId": attachment_id,
            }),
        )),
        UiEffect::ReorderSession {
            workspace_id,
            session_id,
            before_session_id,
            ..
        } => Some((
            "workspace.insertSessionBefore",
            json!({
                "workspaceId": workspace_id,
                "sessionId": session_id,
                "beforeSessionId": before_session_id,
            }),
        )),
        UiEffect::InterruptSubagent { address, .. } => Some((
            "subagent.interrupt",
            json!({
                "parentSessionId": address.parent_session_id,
                "childSessionId": address.child_session_id,
                "mode": address.mode,
            }),
        )),
        UiEffect::SetSessionMode { mode_id, .. } => {
            let mut params = json!({
                "sessionId": operation.session_id,
                "generation": operation.generation,
            });
            if let Some(mode_id) = mode_id {
                params["modeId"] = json!(mode_id);
            }
            Some(("tui.setSessionMode", params))
        }
        UiEffect::ListAgentPresets { .. } => Some(("agentPreset.list", json!({}))),
        UiEffect::SelectAgentPreset { agent_preset, .. } => Some((
            "agentPreset.select",
            json!({
                "sessionId": operation.session_id,
                "agentPreset": agent_preset,
            }),
        )),
        UiEffect::ListSessionModels { .. } => Some((
            "session.models",
            json!({ "sessionId": operation.session_id }),
        )),
        UiEffect::SelectSessionModel {
            provider,
            model,
            reasoning_effort,
            ..
        } => {
            let mut params = json!({
                "sessionId": operation.session_id,
                "provider": provider,
                "model": model,
            });
            if let Some(effort) = reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                params["reasoningEffort"] = json!(effort);
            }
            Some(("session.selectModel", params))
        }
        UiEffect::AttachSession { .. } => None,
    };
    Ok(request)
}

#[derive(Debug, Clone, PartialEq, Default)]
struct DecodedAsyncResult {
    accepted: bool,
    session_list: Option<dsh_pager_protocol::SessionListValue>,
    session_search: Option<dsh_pager_protocol::SessionSearchValue>,
    file_references: Option<dsh_pager_protocol::FileReferencesListValue>,
    attachment_preview: Option<dsh_pager::AttachmentPreview>,
    agent_preset_list: Option<AgentPresetListValue>,
    selected_agent_preset: Option<String>,
    session_models: Option<SessionModelsValue>,
    selected_model: Option<ModelSelection>,
}

fn decode_tui_result<T: serde::de::DeserializeOwned>(raw: Value) -> PagerResult<T> {
    let payload = if raw.get("ok").and_then(Value::as_bool) == Some(true) {
        raw.get("value").cloned().unwrap_or(raw)
    } else {
        raw
    };
    Ok(serde_json::from_value(payload)?)
}

fn decode_async_result(effect: &UiEffect, raw: Value) -> PagerResult<DecodedAsyncResult> {
    if matches!(effect, UiEffect::RespondInteraction { .. }) {
        let value: dsh_pager_protocol::TuiRespondResult = decode_tui_result(raw)?;
        return Ok(DecodedAsyncResult {
            accepted: value.accepted,
            session_list: None,
            session_search: None,
            file_references: None,
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: None,
            selected_model: None,
        });
    }
    if matches!(effect, UiEffect::SetSessionMode { .. }) {
        let value: dsh_pager_protocol::TuiSetSessionModeResult = decode_tui_result(raw)?;
        return Ok(DecodedAsyncResult {
            accepted: value.accepted,
            session_list: None,
            session_search: None,
            file_references: None,
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: None,
            selected_model: None,
        });
    }
    let value = unwrap_api_value(raw)?;
    if matches!(effect, UiEffect::ListSessions { .. }) {
        let session_list = serde_json::from_value(value)?;
        return Ok(DecodedAsyncResult {
            accepted: true,
            session_list: Some(session_list),
            session_search: None,
            file_references: None,
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: None,
            selected_model: None,
        });
    }
    if matches!(effect, UiEffect::SearchSessions { .. }) {
        let session_search = serde_json::from_value(value)?;
        return Ok(DecodedAsyncResult {
            accepted: true,
            session_list: None,
            session_search: Some(session_search),
            file_references: None,
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: None,
            selected_model: None,
        });
    }
    if matches!(effect, UiEffect::FileSearchQuery { .. }) {
        let file_references = serde_json::from_value(value)?;
        return Ok(DecodedAsyncResult {
            accepted: true,
            session_list: None,
            session_search: None,
            file_references: Some(file_references),
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: None,
            selected_model: None,
        });
    }
    if matches!(effect, UiEffect::PreviewMedia { .. }) {
        return Ok(DecodedAsyncResult {
            accepted: true,
            session_list: None,
            session_search: None,
            file_references: None,
            attachment_preview: Some(parse_attachment_preview(value)?),
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: None,
            selected_model: None,
        });
    }
    if matches!(effect, UiEffect::ListAgentPresets { .. }) {
        let agent_preset_list = serde_json::from_value(value)?;
        return Ok(DecodedAsyncResult {
            accepted: true,
            session_list: None,
            session_search: None,
            file_references: None,
            attachment_preview: None,
            agent_preset_list: Some(agent_preset_list),
            selected_agent_preset: None,
            session_models: None,
            selected_model: None,
        });
    }
    if matches!(effect, UiEffect::SelectAgentPreset { .. }) {
        let selected: dsh_pager_protocol::AgentPresetSelectValue = serde_json::from_value(value)?;
        return Ok(DecodedAsyncResult {
            accepted: true,
            session_list: None,
            session_search: None,
            file_references: None,
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: Some(selected.agent_preset),
            session_models: None,
            selected_model: None,
        });
    }
    if matches!(effect, UiEffect::ListSessionModels { .. }) {
        let session_models = serde_json::from_value(value)?;
        return Ok(DecodedAsyncResult {
            accepted: true,
            session_list: None,
            session_search: None,
            file_references: None,
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: Some(session_models),
            selected_model: None,
        });
    }
    if matches!(effect, UiEffect::SelectSessionModel { .. }) {
        let selected: dsh_pager_protocol::SessionSelectModelValue = serde_json::from_value(value)?;
        return Ok(DecodedAsyncResult {
            accepted: true,
            session_list: None,
            session_search: None,
            file_references: None,
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: None,
            selected_model: Some(selected.selected),
        });
    }
    let accepted = value
        .get("accepted")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    Ok(DecodedAsyncResult {
        accepted,
        session_list: None,
        session_search: None,
        file_references: None,
        attachment_preview: None,
        agent_preset_list: None,
        selected_agent_preset: None,
        session_models: None,
        selected_model: None,
    })
}

fn unwrap_api_value(raw: Value) -> PagerResult<Value> {
    let result: dsh_pager_protocol::ApiResult<Value> = serde_json::from_value(raw)?;
    result.into_result().map_err(PagerError::from)
}

fn parse_attachment_preview(value: Value) -> PagerResult<dsh_pager::AttachmentPreview> {
    const MAX_DATA_BYTES: usize = 1_048_576;
    let attachment = value.get("attachment").unwrap_or(&value);
    let data = value
        .get("data")
        .and_then(Value::as_str)
        .filter(|data| !data.is_empty())
        .ok_or_else(|| PagerError::new("session.attachment returned no image data"))?;
    if data.len() > MAX_DATA_BYTES {
        return Err(PagerError::new(format!(
            "session.attachment preview exceeds {MAX_DATA_BYTES} base64 bytes"
        )));
    }
    let attachment_id = attachment
        .get("attachmentId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if attachment_id.is_empty() {
        return Err(PagerError::new(
            "session.attachment returned no attachment id",
        ));
    }
    Ok(dsh_pager::AttachmentPreview {
        attachment_id,
        media_type: attachment
            .get("mediaType")
            .and_then(Value::as_str)
            .unwrap_or("application/octet-stream")
            .to_string(),
        data: data.to_string(),
        bytes: attachment.get("bytes").and_then(Value::as_u64),
        width: attachment
            .get("width")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok()),
        height: attachment
            .get("height")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok()),
    })
}

fn build_completion(
    effect: UiEffect,
    ledger: &mut EffectLedger,
    result: PagerResult<DecodedAsyncResult>,
) -> UiEffectCompletion {
    let operation = effect_operation(&effect).clone();
    match result {
        Ok(decoded) if decoded.accepted => {
            ledger.complete(operation.clone());
            UiEffectCompletion {
                effect,
                receipt: UiEffectReceipt {
                    status: UiEffectStatus::Accepted,
                    operation,
                    diagnostic: None,
                    retryable: Some(false),
                },
                session_list: decoded.session_list,
                session_search: decoded.session_search,
                file_references: decoded.file_references,
                attachment_preview: decoded.attachment_preview,
                agent_preset_list: decoded.agent_preset_list,
                selected_agent_preset: decoded.selected_agent_preset,
                session_models: decoded.session_models,
                selected_model: decoded.selected_model,
            }
        }
        Ok(decoded) => UiEffectCompletion {
            effect,
            receipt: UiEffectReceipt {
                status: UiEffectStatus::Rejected,
                operation,
                diagnostic: Some("host rejected operation".into()),
                retryable: Some(false),
            },
            session_list: decoded.session_list,
            session_search: decoded.session_search,
            file_references: decoded.file_references,
            attachment_preview: decoded.attachment_preview,
            agent_preset_list: decoded.agent_preset_list,
            selected_agent_preset: decoded.selected_agent_preset,
            session_models: decoded.session_models,
            selected_model: decoded.selected_model,
        },
        Err(error) => UiEffectCompletion {
            effect,
            receipt: UiEffectReceipt {
                status: classify_effect_error(&error),
                operation,
                diagnostic: Some(error.to_string()),
                retryable: Some(true),
            },
            session_list: None,
            session_search: None,
            file_references: None,
            attachment_preview: None,
            agent_preset_list: None,
            selected_agent_preset: None,
            session_models: None,
            selected_model: None,
        },
    }
}

fn classify_effect_error(error: &dsh_pager::PagerError) -> UiEffectStatus {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("timeout") || message.contains("timed out") {
        UiEffectStatus::Timeout
    } else if message.contains("conflict") || message.contains("revision") {
        UiEffectStatus::Conflict
    } else if message.contains("stale")
        || message.contains("generation")
        || message.contains("gone")
    {
        UiEffectStatus::Stale
    } else if message.contains("unsupported") || message.contains("capability") {
        UiEffectStatus::Unsupported
    } else {
        UiEffectStatus::Failed
    }
}

fn mode_label(mode: PromptMode) -> &'static str {
    match mode {
        PromptMode::Queue => "queue",
        PromptMode::Steer => "steer",
    }
}

fn prompt_digest(text: &str) -> String {
    // Stable FNV-1a is sufficient for an in-process dedupe key; the prompt
    // remains in the effect payload and is never represented by this digest.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::{PromptMode, SessionModeId};
    use serde_json::json;

    #[test]
    fn cancel_effect_keeps_session_generation_and_stable_dedupe_identity() {
        let session = SessionState::new("cancel-me".into(), 7);
        let effect = compile_intent(UiIntent::CancelSession, &UiContext::from_session(&session));
        let UiEffect::CancelSession { operation } = effect else {
            panic!("expected cancel effect");
        };
        assert_eq!(operation.session_id.as_str(), "cancel-me");
        assert_eq!(operation.generation.get(), 7);
        assert_eq!(operation.action, "cancel-session");
        assert_eq!(operation.dedupe_key, "cancel-session");
        let (method, params) = encode_async_request(
            &UiEffect::CancelSession {
                operation: operation.clone(),
            },
            &operation,
        )
        .expect("encode cancel")
        .expect("cancel is supported");
        assert_eq!(method, "session.cancel");
        assert_eq!(params["sessionId"], "cancel-me");
    }

    #[test]
    fn compile_intent_is_neutral_and_carries_generation_and_operation_kind() {
        let session = SessionState::new("s".into(), 4);
        let effect = compile_intent(
            UiIntent::SubmitPrompt {
                text: "hello".into(),
                mode: PromptMode::Queue,
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::SubmitPrompt {
            operation, text, ..
        } = effect
        else {
            panic!("expected submit effect");
        };
        assert_eq!(operation.session_id.as_str(), "s");
        assert_eq!(operation.generation.get(), 4);
        assert_eq!(operation.request_id.as_str(), "pending");
        assert_eq!(operation.action, "submit");
        assert!(operation.dedupe_key.starts_with("submit:"));
        assert_eq!(text, "hello");
    }

    #[test]
    fn interrupt_subagent_effect_preserves_parent_child_identity() {
        let session = SessionState::new("parent".into(), 9);
        let effect = compile_intent(
            UiIntent::InterruptSubagent {
                address: SubagentAddress {
                    parent_session_id: "parent".into(),
                    child_session_id: "child".into(),
                    mode: dsh_pager_protocol::SubagentMode::Continuable,
                },
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::InterruptSubagent { operation, address } = effect else {
            panic!("expected interrupt effect");
        };
        assert_eq!(operation.action, "subagent-interrupt");
        assert_eq!(operation.session_id.as_str(), "parent");
        assert_eq!(address.parent_session_id, "parent");
        assert_eq!(address.child_session_id, "child");
    }

    #[test]
    fn set_session_mode_effect_cycles_or_sets_explicit_id() {
        let session = SessionState::new("s".into(), 3);
        let effect = compile_intent(
            UiIntent::SetSessionMode {
                mode_id: Some(SessionModeId::Plan),
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::SetSessionMode { operation, mode_id } = effect else {
            panic!("expected set-session-mode effect");
        };
        assert_eq!(operation.action, "set-session-mode");
        assert_eq!(mode_id, Some(SessionModeId::Plan));
        let (method, params) = encode_async_request(
            &UiEffect::SetSessionMode {
                operation: operation.clone(),
                mode_id: Some(SessionModeId::Plan),
            },
            &operation,
        )
        .expect("encode")
        .expect("supported");
        assert_eq!(method, "tui.setSessionMode");
        assert_eq!(params["sessionId"], "s");
        assert_eq!(params["modeId"], "plan");
    }

    #[test]
    fn agent_preset_effects_encode_list_and_select() {
        let session = SessionState::new("s".into(), 3);
        let list = compile_intent(
            UiIntent::ListAgentPresets { revision: 4 },
            &UiContext::from_session(&session),
        );
        let UiEffect::ListAgentPresets {
            operation,
            revision,
        } = list
        else {
            panic!("expected list-agent-presets");
        };
        assert_eq!(operation.action, "list-agent-presets");
        assert_eq!(revision, 4);
        let (method, params) = encode_async_request(
            &UiEffect::ListAgentPresets {
                operation: operation.clone(),
                revision: 4,
            },
            &operation,
        )
        .expect("encode")
        .expect("supported");
        assert_eq!(method, "agentPreset.list");
        assert_eq!(params, json!({}));

        let select = compile_intent(
            UiIntent::SelectAgentPreset {
                agent_preset: "code".into(),
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::SelectAgentPreset {
            operation,
            agent_preset,
        } = select
        else {
            panic!("expected select-agent-preset");
        };
        assert_eq!(agent_preset, "code");
        let (method, params) = encode_async_request(
            &UiEffect::SelectAgentPreset {
                operation: operation.clone(),
                agent_preset: "code".into(),
            },
            &operation,
        )
        .expect("encode")
        .expect("supported");
        assert_eq!(method, "agentPreset.select");
        assert_eq!(params["sessionId"], "s");
        assert_eq!(params["agentPreset"], "code");
    }

    #[test]
    fn session_model_effects_encode_list_and_select() {
        let session = SessionState::new("s".into(), 3);
        let list = compile_intent(
            UiIntent::ListSessionModels { revision: 2 },
            &UiContext::from_session(&session),
        );
        let UiEffect::ListSessionModels {
            operation,
            revision,
        } = list
        else {
            panic!("expected list-session-models");
        };
        assert_eq!(operation.action, "list-session-models");
        assert_eq!(revision, 2);
        let (method, params) = encode_async_request(
            &UiEffect::ListSessionModels {
                operation: operation.clone(),
                revision: 2,
            },
            &operation,
        )
        .expect("encode")
        .expect("supported");
        assert_eq!(method, "session.models");
        assert_eq!(params["sessionId"], "s");

        let select = compile_intent(
            UiIntent::SelectSessionModel {
                provider: "deepseek-official".into(),
                model: "deepseek-chat".into(),
                reasoning_effort: Some("high".into()),
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::SelectSessionModel {
            operation,
            provider,
            model,
            reasoning_effort,
        } = select
        else {
            panic!("expected select-session-model");
        };
        assert_eq!(provider, "deepseek-official");
        assert_eq!(model, "deepseek-chat");
        assert_eq!(reasoning_effort.as_deref(), Some("high"));
        let (method, params) = encode_async_request(
            &UiEffect::SelectSessionModel {
                operation: operation.clone(),
                provider: "deepseek-official".into(),
                model: "deepseek-chat".into(),
                reasoning_effort: Some("high".into()),
            },
            &operation,
        )
        .expect("encode")
        .expect("supported");
        assert_eq!(method, "session.selectModel");
        assert_eq!(params["sessionId"], "s");
        assert_eq!(params["provider"], "deepseek-official");
        assert_eq!(params["model"], "deepseek-chat");
        assert_eq!(params["reasoningEffort"], "high");
    }

    #[test]
    fn file_search_effect_preserves_query_revision_and_digest_identity() {
        let session = SessionState::new("parent".into(), 9);
        let effect = compile_intent(
            UiIntent::FileSearchQuery {
                query: "src/main".into(),
                revision: 7,
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::FileSearchQuery {
            operation,
            query,
            revision,
        } = effect
        else {
            panic!("expected file-search effect");
        };
        assert_eq!(operation.action, "file-search-query");
        assert!(operation.dedupe_key.contains(":7:"));
        assert_eq!(query, "src/main");
        assert_eq!(revision, 7);
    }

    #[test]
    fn resume_list_and_search_effects_keep_revision_and_decode_typed_values() {
        let session = SessionState::new("active".into(), 9);
        let list = compile_intent(
            UiIntent::ListSessions { revision: 3 },
            &UiContext::from_session(&session),
        );
        let UiEffect::ListSessions {
            operation,
            revision,
        } = &list
        else {
            panic!("list effect");
        };
        assert_eq!(*revision, 3);
        assert_eq!(operation.action, "list-sessions");
        assert!(operation.dedupe_key.ends_with(":3"));
        let (method, params) = encode_async_request(&list, operation)
            .expect("list encode")
            .expect("list supported");
        assert_eq!(method, "session.list");
        assert_eq!(params, json!({}));
        let decoded = decode_async_result(
            &list,
            json!({
                "ok": true,
                "value": {"items": [{
                    "sessionId": "s1",
                    "updatedAt": 1000.0,
                    "running": false,
                    "blank": false
                }]}
            }),
        )
        .expect("list decode");
        assert_eq!(
            decoded.session_list.expect("list").items[0].session_id,
            "s1"
        );

        let search = compile_intent(
            UiIntent::SearchSessions {
                query: "needle".into(),
                revision: 4,
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::SearchSessions {
            operation,
            query,
            revision,
        } = &search
        else {
            panic!("search effect");
        };
        assert_eq!(query, "needle");
        assert_eq!(*revision, 4);
        let (method, params) = encode_async_request(&search, operation)
            .expect("search encode")
            .expect("search supported");
        assert_eq!(method, "session.search");
        assert_eq!(params, json!({"query": "needle"}));
        let decoded = decode_async_result(
            &search,
            json!({
                "ok": true,
                "value": {
                    "items": [{"sessionId": "s2", "snippet": "needle here"}],
                    "hasMore": false
                }
            }),
        )
        .expect("search decode");
        assert_eq!(
            decoded.session_search.expect("search").items[0].snippet,
            "needle here"
        );
    }

    #[test]
    fn media_preview_effect_preserves_attachment_identity() {
        let session = SessionState::new("parent".into(), 9);
        let effect = compile_intent(
            UiIntent::PreviewMedia {
                attachment_id: "sha256:img".into(),
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::PreviewMedia {
            operation,
            attachment_id,
        } = effect
        else {
            panic!("expected media preview effect");
        };
        assert_eq!(operation.action, "media-preview");
        assert_eq!(operation.session_id.as_str(), "parent");
        assert_eq!(attachment_id, "sha256:img");
    }

    #[test]
    fn workspace_mutation_effects_preserve_target_identity() {
        let session = SessionState::new("active".into(), 9);
        let archive = compile_intent(
            UiIntent::ArchiveSessionTarget {
                session_id: DshSessionId::new("selected"),
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::ArchiveSessionTarget {
            operation,
            session_id,
        } = archive
        else {
            panic!("expected archive target effect");
        };
        assert_eq!(operation.session_id.as_str(), "selected");
        assert_eq!(session_id.as_str(), "selected");

        let reorder = compile_intent(
            UiIntent::ReorderSession {
                workspace_id: "workspace".into(),
                session_id: DshSessionId::new("selected"),
                before_session_id: Some("before".into()),
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::ReorderSession {
            operation,
            before_session_id,
            ..
        } = reorder
        else {
            panic!("expected reorder effect");
        };
        assert_eq!(operation.action, "reorder-session");
        assert_eq!(before_session_id.as_deref(), Some("before"));
    }

    #[test]
    fn dedupe_key_is_stable_for_retries_but_distinguishes_prompt_payloads() {
        let context = UiContext {
            session_id: DshSessionId::new("s"),
            generation: DshGeneration::new(4),
            request_id: DshRequestId::new("pending"),
        };
        let first = compile_intent(
            UiIntent::SubmitPrompt {
                text: "same".into(),
                mode: PromptMode::Queue,
            },
            &context,
        );
        let second = compile_intent(
            UiIntent::SubmitPrompt {
                text: "same".into(),
                mode: PromptMode::Queue,
            },
            &context,
        );
        let different = compile_intent(
            UiIntent::SubmitPrompt {
                text: "different".into(),
                mode: PromptMode::Queue,
            },
            &context,
        );
        let UiEffect::SubmitPrompt {
            operation: first, ..
        } = first
        else {
            panic!()
        };
        let UiEffect::SubmitPrompt {
            operation: second, ..
        } = second
        else {
            panic!()
        };
        let UiEffect::SubmitPrompt {
            operation: different,
            ..
        } = different
        else {
            panic!()
        };
        assert_eq!(first.dedupe_key, second.dedupe_key);
        assert_ne!(first.dedupe_key, different.dedupe_key);
        assert_eq!(first.request_id.as_str(), "pending");
        assert_eq!(second.request_id.as_str(), "pending");
    }

    #[test]
    fn effect_ledger_allocates_identity_across_short_lived_sinks() {
        let session = SessionState::new("s".into(), 4);
        let context = UiContext::from_session(&session);
        let effect = compile_intent(
            UiIntent::SubmitPrompt {
                text: "hello".into(),
                mode: PromptMode::Queue,
            },
            &context,
        );
        let UiEffect::SubmitPrompt { mut operation, .. } = effect else {
            panic!("expected submit effect");
        };
        let mut ledger = EffectLedger::default();
        ledger.prepare_operation(&mut operation);
        assert_eq!(operation.request_id.as_str(), "ui-1");

        let effect = compile_intent(
            UiIntent::SubmitPrompt {
                text: "again".into(),
                mode: PromptMode::Queue,
            },
            &context,
        );
        let UiEffect::SubmitPrompt { mut operation, .. } = effect else {
            panic!("expected submit effect");
        };
        ledger.prepare_operation(&mut operation);
        assert_eq!(operation.request_id.as_str(), "ui-2");
    }

    #[test]
    fn effect_ledger_deduplicates_completed_operations_with_bounded_history() {
        let session = SessionState::new("s".into(), 1);
        let context = UiContext::from_session(&session);
        let effect = compile_intent(
            UiIntent::RenameSession {
                title: "title".into(),
            },
            &context,
        );
        let UiEffect::RenameSession { mut operation, .. } = effect else {
            panic!("expected rename effect");
        };
        let mut ledger = EffectLedger::default();
        ledger.prepare_operation(&mut operation);
        let duplicate = operation.clone();
        ledger.complete(operation);
        assert!(ledger.contains(&duplicate));
        assert_eq!(
            ledger.duplicate_receipt(duplicate).diagnostic.as_deref(),
            Some("duplicate operation suppressed")
        );
    }

    #[test]
    fn async_request_encoding_keeps_wire_identity_and_does_not_call_transport() {
        let session = SessionState::new("s".into(), 4);
        let effect = compile_intent(
            UiIntent::SubmitPrompt {
                text: "hello".into(),
                mode: PromptMode::Queue,
            },
            &UiContext::for_operation(&session, DshRequestId::new("op-7")),
        );
        let operation = effect_operation(&effect).clone();
        let (method, params) = encode_async_request(&effect, &operation)
            .expect("encode")
            .expect("supported effect");
        assert_eq!(method, "session.prompt");
        assert_eq!(params["sessionId"], "s");
        assert_eq!(params["mode"], "queue");
        assert_eq!(params["content"][0]["text"], "hello");
    }

    #[test]
    fn async_completion_decodes_file_rows_and_media_metadata() {
        let session = SessionState::new("s".into(), 4);
        let file_effect = compile_intent(
            UiIntent::FileSearchQuery {
                query: "src".into(),
                revision: 2,
            },
            &UiContext::for_operation(&session, DshRequestId::new("file-2")),
        );
        let decoded = decode_async_result(
            &file_effect,
            json!({
                "ok": true,
                "value": {"items": [{"path": "src/lib.rs", "kind": "file"}]}
            }),
        )
        .expect("file completion");
        assert!(decoded.accepted);
        assert_eq!(decoded.file_references.unwrap().items[0].path, "src/lib.rs");

        let media_effect = compile_intent(
            UiIntent::PreviewMedia {
                attachment_id: "img-1".into(),
            },
            &UiContext::for_operation(&session, DshRequestId::new("media-1")),
        );
        let decoded = decode_async_result(
            &media_effect,
            json!({
                "ok": true,
                "value": {
                    "attachment": {
                        "attachmentId": "img-1",
                        "mediaType": "image/png",
                        "width": 4,
                        "height": 3
                    },
                    "data": "aGVsbG8="
                }
            }),
        )
        .expect("media completion");
        let preview = decoded.attachment_preview.unwrap();
        assert_eq!(preview.attachment_id, "img-1");
        assert_eq!(preview.media_type, "image/png");
        assert_eq!(preview.width, Some(4));
    }

    #[test]
    fn explicit_operation_identity_controls_retry_dedupe() {
        let session = SessionState::new("s".into(), 4);
        let context = UiContext::for_operation(&session, DshRequestId::new("op-1"));
        let effect = compile_intent(
            UiIntent::SubmitPrompt {
                text: "same".into(),
                mode: PromptMode::Queue,
            },
            &context,
        );
        let UiEffect::SubmitPrompt { operation, .. } = effect else {
            panic!()
        };
        assert_eq!(operation.request_id.as_str(), "op-1");
        assert_eq!(operation.dedupe_key, "submit:op-1");
    }

    #[test]
    fn interaction_request_id_is_bound_even_without_preseeded_operation() {
        let session = SessionState::new("s".into(), 8);
        let effect = compile_intent(
            UiIntent::RespondInteraction {
                request_id: DshRequestId::new("rpc-7"),
                interaction: TuiInteractionResponse::Question {
                    answers: json!({"answers": []}),
                },
            },
            &UiContext::from_session(&session),
        );
        let UiEffect::RespondInteraction { operation, .. } = effect else {
            panic!("expected interaction response effect");
        };
        assert_eq!(operation.request_id.as_str(), "rpc-7");
        assert_eq!(operation.generation.get(), 8);
        assert_eq!(operation.dedupe_key, "respond-interaction:rpc-7");
    }

    #[test]
    fn receipts_are_explicit_and_serializable() {
        let receipt = UiEffectReceipt {
            status: UiEffectStatus::Conflict,
            operation: OperationKey {
                session_id: DshSessionId::new("s"),
                generation: DshGeneration::new(2),
                request_id: DshRequestId::new("r"),
                action: "queue-mutation".into(),
                dedupe_key: "queue-mutation:q:remove".into(),
            },
            diagnostic: Some("queue revision changed".into()),
            retryable: Some(true),
        };
        let value = serde_json::to_value(&receipt).unwrap();
        assert_eq!(value["status"], json!("conflict"));
        assert_eq!(value["operation"]["generation"], json!(2));
        assert_eq!(value["retryable"], json!(true));
    }

    #[test]
    fn receipt_message_distinguishes_conflict_stale_and_timeout() {
        let operation = OperationKey {
            session_id: DshSessionId::new("s"),
            generation: DshGeneration::new(1),
            request_id: DshRequestId::new("r"),
            action: "queue-mutation".into(),
            dedupe_key: "k".into(),
        };
        let receipt = |status: UiEffectStatus, diagnostic: &str| UiEffectReceipt {
            status,
            operation: operation.clone(),
            diagnostic: Some(diagnostic.into()),
            retryable: Some(true),
        };
        assert!(
            receipt_status_message(
                &receipt(UiEffectStatus::Conflict, "revision changed"),
                "Queue"
            )
            .contains("conflict")
        );
        assert!(
            receipt_status_message(
                &receipt(UiEffectStatus::Stale, "old generation"),
                "Interaction"
            )
            .contains("stale")
        );
        assert!(
            receipt_status_message(&receipt(UiEffectStatus::Timeout, "deadline"), "Prompt")
                .contains("timed out")
        );
    }
}
