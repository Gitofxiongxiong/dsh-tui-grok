use dsh_pager_protocol::{
    AcceptedResult, AgentPresetListValue, AgentPresetSelectValue, ApiResult, CommandDescriptor,
    CommandExecuteValue, CredentialsDescribeParams, CredentialsDescribeValue, CredentialsSetParams,
    CredentialsSetValue, FileReferencesListValue, PromptContentPart, PromptMode, QueueAction,
    SessionCancelParams, SessionCreateValue, SessionForkParams, SessionForkResult,
    SessionHistoryValue, SessionListValue, SessionModelsValue, SessionPromptParams,
    SessionPromptResult, SessionRenameParams, SessionRenameResult, SessionSearchValue,
    SessionSelectModelValue, SessionUpdateQueueParams, SubagentAddress, SubagentHistoryValue,
    SubagentInterruptParams, SubagentInterruptResult, SubagentListValue, SubagentMode,
    SubagentPromptParams, SubagentPromptResult, TuiAttachParams, TuiAttachResult, TuiDetachParams,
    TuiHelloResult, TuiInteractionResponse, TuiRespondParams, TuiRespondResult, TuiSubscribeParams,
    TuiSubscribeResult, TuiSubscribeScope, WorkspaceArchiveSessionParams,
    WorkspaceArchiveSessionValue, WorkspaceInsertBeforeParams, WorkspaceInsertSessionBeforeParams,
    WorkspaceInsertSessionBeforeValue, WorkspaceOrderValue,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{PagerError, PagerResult};
use crate::session::{SessionState, SessionUpdate};
use crate::transport::RpcTransport;

const PAGE_MESSAGES: u64 = 50;
const MAX_INITIAL_REPAIRS: usize = 2;
const MAX_ATTACHMENT_PREVIEW_DATA_BYTES: usize = 1_048_576;
static DISPATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// How the first session is chosen after hello.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionChoice {
    RecentOrCreate,
    New,
    Id(String),
    Search(String),
}

/// Fetch the current session directory without choosing one.
pub fn list_sessions(transport: &mut RpcTransport) -> PagerResult<SessionListValue> {
    let result: SessionListValue = api_call(transport, "session.list", json!({}))?;
    transport
        .control_plane_mut()
        .store
        .seed_session_list(&result);
    // Workspace membership and archive state are a separate reconnect
    // baseline.  Keep it beside session.list so a standalone Dashboard has
    // grouping information before the first host change arrives.
    let _: Value = list_workspaces(transport)?;
    Ok(result)
}

/// Fetch the independent workspace/archive baseline. The generic value is
/// intentionally kept host-owned so newer workspace fields remain forward
/// compatible with the control-plane store.
pub fn list_workspaces(transport: &mut RpcTransport) -> PagerResult<Value> {
    api_call(transport, "workspace.list", json!({}))
}

/// Archive a session in the host registry.  Archiving is idempotent and does
/// not delete its history or workspace accounting slot.
pub fn archive_session(
    transport: &mut RpcTransport,
    session_id: &str,
) -> PagerResult<WorkspaceArchiveSessionValue> {
    api_call(
        transport,
        "workspace.archiveSession",
        serde_json::to_value(WorkspaceArchiveSessionParams {
            session_id: session_id.to_string(),
        })?,
    )
}

/// Move one workspace in the host-owned display order.  The returned order is
/// complete, so callers can replace local ordering without guessing a delta.
pub fn reorder_workspace(
    transport: &mut RpcTransport,
    workspace_id: &str,
    before_workspace_id: Option<&str>,
) -> PagerResult<WorkspaceOrderValue> {
    api_call(
        transport,
        "workspace.insertBefore",
        serde_json::to_value(WorkspaceInsertBeforeParams {
            workspace_id: workspace_id.to_string(),
            before_workspace_id: before_workspace_id.map(str::to_string),
        })?,
    )
}

/// Move one accounted session within a workspace's durable order.
pub fn reorder_session(
    transport: &mut RpcTransport,
    workspace_id: &str,
    session_id: &str,
    before_session_id: Option<&str>,
) -> PagerResult<WorkspaceInsertSessionBeforeValue> {
    api_call(
        transport,
        "workspace.insertSessionBefore",
        serde_json::to_value(WorkspaceInsertSessionBeforeParams {
            workspace_id: workspace_id.to_string(),
            session_id: session_id.to_string(),
            before_session_id: before_session_id.map(str::to_string),
        })?,
    )
}

/// Search session content through the host-owned search projection.
pub fn search_sessions(
    transport: &mut RpcTransport,
    query: &str,
) -> PagerResult<SessionSearchValue> {
    api_call(transport, "session.search", json!({ "query": query }))
}

/// Discover path-only file references through the host provider. This is
/// distinct from `session.search`, which searches durable conversation text.
pub fn list_file_references(
    transport: &mut RpcTransport,
    session_id: &str,
    query: &str,
) -> PagerResult<FileReferencesListValue> {
    api_call(
        transport,
        "fileReferences.list",
        json!({ "sessionId": session_id, "query": query }),
    )
}

/// Describe credential references without ever returning their values.
pub fn describe_credentials(
    transport: &mut RpcTransport,
    refs: &[String],
) -> PagerResult<CredentialsDescribeValue> {
    api_call(
        transport,
        "credentials.describe",
        serde_json::to_value(CredentialsDescribeParams {
            refs: refs.to_vec(),
        })?,
    )
}

/// Store one credential in the Host-owned writable credential layer.
pub fn set_credential(
    transport: &mut RpcTransport,
    credential_ref: &str,
    value: String,
) -> PagerResult<CredentialsSetValue> {
    api_call(
        transport,
        "credentials.set",
        serde_json::to_value(CredentialsSetParams {
            credential_ref: credential_ref.to_string(),
            value,
        })?,
    )
}

/// Authorized image attachment bytes for the media preview surface.
///
/// The host returns base64 data together with the durable attachment metadata.
/// Authorization and storage remain Harness-owned; the UI only receives this
/// bounded value through an effect seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentPreview {
    pub attachment_id: String,
    pub media_type: String,
    pub data: String,
    pub bytes: Option<u64>,
    pub width: Option<u16>,
    pub height: Option<u16>,
}

pub fn fetch_attachment(
    transport: &mut RpcTransport,
    session_id: &str,
    attachment_id: &str,
) -> PagerResult<AttachmentPreview> {
    let value: Value = api_call(
        transport,
        "session.attachment",
        json!({
            "sessionId": session_id,
            "attachmentId": attachment_id,
        }),
    )?;
    let attachment = value.get("attachment").unwrap_or(&value);
    let data = value
        .get("data")
        .and_then(Value::as_str)
        .filter(|data| !data.is_empty())
        .ok_or_else(|| PagerError::new("session.attachment returned no image data"))?;
    if data.len() > MAX_ATTACHMENT_PREVIEW_DATA_BYTES {
        return Err(PagerError::new(format!(
            "session.attachment preview exceeds {} base64 bytes",
            MAX_ATTACHMENT_PREVIEW_DATA_BYTES
        )));
    }
    let attachment_id = attachment
        .get("attachmentId")
        .and_then(Value::as_str)
        .unwrap_or(attachment_id)
        .to_string();
    let media_type = attachment
        .get("mediaType")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream")
        .to_string();
    Ok(AttachmentPreview {
        attachment_id,
        media_type,
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

/// Complete the create/list -> attach -> history -> buffered-live barrier.
pub fn load_session(
    transport: &mut RpcTransport,
    hello: &TuiHelloResult,
    choice: SessionChoice,
    cwd: &str,
) -> PagerResult<SessionState> {
    let session_id = choose_session(transport, choice, cwd)?;
    load_session_id(transport, hello.generation, session_id)
}

/// Attach to a known session id and cross the same history/live load barrier
/// used by initial startup.  The picker uses this entry point when switching
/// sessions without restarting the backend process.
pub fn load_session_id(
    transport: &mut RpcTransport,
    generation: u64,
    session_id: String,
) -> PagerResult<SessionState> {
    let _: Value = list_workspaces(transport)?;
    subscribe_control_plane(transport, generation)?;
    let attach: TuiAttachResult = transport.call(
        "tui.attach",
        &TuiAttachParams {
            session_id: session_id.clone(),
            generation,
        },
    )?;
    if !attach.attached {
        return Err(PagerError::new("tui.attach did not attach the session"));
    }

    let mut state = SessionState::new(session_id, generation);
    state.install_initial(fetch_tail(transport, state.session_id())?)?;
    drain_notifications(transport, &mut state)?;
    if matches!(
        state.connection_phase(),
        crate::session::ConnectionPhase::Reconnecting
            | crate::session::ConnectionPhase::Disconnected
    ) {
        return Err(PagerError::new("event stream failed during session load"));
    }

    for _ in 0..MAX_INITIAL_REPAIRS {
        if !state.needs_repair() {
            return Ok(state);
        }
        let page = fetch_tail(transport, state.session_id())?;
        state.repair_tail(page)?;
        drain_notifications(transport, &mut state)?;
        if matches!(
            state.connection_phase(),
            crate::session::ConnectionPhase::Reconnecting
                | crate::session::ConnectionPhase::Disconnected
        ) {
            return Err(PagerError::new(
                "event stream failed during session load repair",
            ));
        }
    }
    if state.needs_repair() {
        return Err(PagerError::new(format!(
            "session {} could not cross the load barrier without an event gap",
            state.session_id()
        )));
    }
    Ok(state)
}

/// Subscribe the connection to every session's control-plane frames before
/// selecting a presentation session. Older gateways returned `{}` here, so
/// the typed receipt keeps all fields optional for wire compatibility.
pub fn subscribe_control_plane(
    transport: &mut RpcTransport,
    generation: u64,
) -> PagerResult<TuiSubscribeResult> {
    transport.call(
        "tui.subscribe",
        &TuiSubscribeParams {
            generation,
            session_id: None,
            scope: Some(TuiSubscribeScope::All),
            since: None,
        },
    )
}

/// Drop a session subscription on a live connection.  A failed detach is
/// intentionally returned to the caller so it can surface a diagnostic; the
/// old session state remains locally usable until a replacement is loaded.
pub fn detach_session(transport: &mut RpcTransport, state: &SessionState) -> PagerResult<()> {
    let _: Value = transport.call(
        "tui.detach",
        &TuiDetachParams {
            session_id: state.session_id().to_string(),
            generation: state.generation(),
        },
    )?;
    Ok(())
}

/// Re-open a connection and rebuild the loaded session from a fresh baseline.
///
/// This is deliberately baseline-based: v1 does not claim lossless stream
/// resume, so every reconnect refetches the tail and then drains buffered live
/// frames through the same load barrier used at startup.
pub fn reconnect_session(
    transport: &mut RpcTransport,
    state: &mut SessionState,
    cwd: &str,
) -> PagerResult<TuiHelloResult> {
    transport.reconnect()?;
    let hello = transport.hello(cwd.to_string())?;
    state.set_generation(hello.generation);
    subscribe_control_plane(transport, hello.generation)?;
    let attach: TuiAttachResult = transport.call(
        "tui.attach",
        &TuiAttachParams {
            session_id: state.session_id().to_string(),
            generation: hello.generation,
        },
    )?;
    if !attach.attached {
        return Err(PagerError::new("tui.attach did not attach after reconnect"));
    }

    state.repair_tail(fetch_tail(transport, state.session_id())?)?;
    drain_notifications(transport, state)?;
    if matches!(
        state.connection_phase(),
        crate::session::ConnectionPhase::Reconnecting
            | crate::session::ConnectionPhase::Disconnected
    ) {
        return Err(PagerError::new(
            "event stream failed during reconnect baseline",
        ));
    }
    for _ in 0..MAX_INITIAL_REPAIRS {
        if !state.needs_repair() {
            state.mark_connected();
            return Ok(hello);
        }
        state.repair_tail(fetch_tail(transport, state.session_id())?)?;
        drain_notifications(transport, state)?;
        if matches!(
            state.connection_phase(),
            crate::session::ConnectionPhase::Reconnecting
                | crate::session::ConnectionPhase::Disconnected
        ) {
            return Err(PagerError::new(
                "event stream failed during reconnect repair",
            ));
        }
    }
    if state.needs_repair() {
        return Err(PagerError::new(format!(
            "session {} could not cross the reconnect load barrier",
            state.session_id()
        )));
    }
    state.mark_connected();
    Ok(hello)
}

pub fn repair_tail(
    transport: &mut RpcTransport,
    state: &mut SessionState,
) -> PagerResult<SessionUpdate> {
    let page = fetch_tail(transport, state.session_id())?;
    state.repair_tail(page)?;
    let mut update = SessionUpdate {
        changed: true,
        gap_detected: false,
    };
    let drained = drain_notifications(transport, state)?;
    update.changed |= drained.changed;
    update.gap_detected = state.needs_repair();
    Ok(update)
}

pub fn load_older(transport: &mut RpcTransport, state: &mut SessionState) -> PagerResult<bool> {
    let Some(before_seq) = state.base_seq() else {
        return Ok(false);
    };
    if !state.has_more() {
        return Ok(false);
    }
    if before_seq <= 0 {
        return Ok(false);
    }
    let page: SessionHistoryValue = api_call(
        transport,
        "session.history",
        json!({
            "sessionId": state.session_id(),
            "beforeSeq": before_seq,
            "maxMessages": PAGE_MESSAGES,
        }),
    )?;
    let changed = state.prepend_older(page)?;
    drain_notifications(transport, state)?;
    Ok(changed)
}

/// Fetch only the history tail for Dashboard peek. This never attaches or
/// resumes the target session; the caller owns the presentation load barrier.
pub fn peek_session_tail(
    transport: &mut RpcTransport,
    session_id: &str,
    max_messages: u64,
) -> PagerResult<SessionHistoryValue> {
    let max_messages = max_messages.clamp(1, 100);
    api_call(
        transport,
        "session.history",
        json!({
            "sessionId": session_id,
            "maxMessages": max_messages,
        }),
    )
}

/// List direct children without attaching or resuming the parent/child pair.
pub fn list_subagents(
    transport: &mut RpcTransport,
    parent_session_id: &str,
) -> PagerResult<SubagentListValue> {
    api_call(
        transport,
        "subagent.list",
        json!({ "parentSessionId": parent_session_id }),
    )
}

/// Read one child tail without activating its Agent.
pub fn peek_subagent_history(
    transport: &mut RpcTransport,
    address: &SubagentAddress,
    max_messages: u64,
) -> PagerResult<SubagentHistoryValue> {
    let max_messages = max_messages.clamp(1, 100);
    api_call(
        transport,
        "subagent.history",
        json!({
            "parentSessionId": address.parent_session_id,
            "childSessionId": address.child_session_id,
            "mode": address.mode,
            "maxMessages": max_messages,
        }),
    )
}

/// Follow up a continuable child. A successful receipt means admission into
/// the child inbox, not completion of its next turn.
pub fn prompt_subagent(
    transport: &mut RpcTransport,
    address: &SubagentAddress,
    text: String,
) -> PagerResult<SubagentPromptResult> {
    if address.mode != SubagentMode::Continuable {
        return Err(PagerError::new(
            "one-shot subagents do not accept follow-up prompts",
        ));
    }
    api_call(
        transport,
        "subagent.prompt",
        serde_json::to_value(SubagentPromptParams {
            parent_session_id: address.parent_session_id.clone(),
            child_session_id: address.child_session_id.clone(),
            mode: address.mode,
            content: vec![PromptContentPart::Text { text }],
            client_time_zone: None,
        })?,
    )
}

/// Admit an interrupt signal. The receipt deliberately does not claim that
/// the child has reached a stopped state; the next formal job/event snapshot
/// is the convergence surface.
pub fn interrupt_subagent(
    transport: &mut RpcTransport,
    address: &SubagentAddress,
) -> PagerResult<SubagentInterruptResult> {
    if address.mode != SubagentMode::Continuable {
        return Err(PagerError::new(
            "one-shot subagents do not accept interrupts",
        ));
    }
    api_call(
        transport,
        "subagent.interrupt",
        serde_json::to_value(SubagentInterruptParams {
            parent_session_id: address.parent_session_id.clone(),
            child_session_id: address.child_session_id.clone(),
            mode: address.mode,
        })?,
    )
}

/// Submit one text prompt through the session API.
pub fn submit_prompt(
    transport: &mut RpcTransport,
    state: &SessionState,
    text: String,
    mode: PromptMode,
) -> PagerResult<SessionPromptResult> {
    submit_prompt_for_session(transport, state.session_id(), text, mode)
}

/// Submit a prompt to an arbitrary existing session without attaching it.
/// The host remains the admission authority; this helper only forwards the
/// typed request and returns its accepted/queued receipt.
pub fn submit_prompt_for_session(
    transport: &mut RpcTransport,
    session_id: &str,
    text: String,
    mode: PromptMode,
) -> PagerResult<SessionPromptResult> {
    api_call(
        transport,
        "session.prompt",
        serde_json::to_value(SessionPromptParams {
            session_id: session_id.to_string(),
            mode,
            content: vec![PromptContentPart::Text { text }],
        })?,
    )
}

/// Receipt for the two-phase Dashboard dispatch operation.  `session_id` is
/// allocated before the first prompt so a failed prompt leaves a recoverable
/// blank session rather than losing the user's target.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchSessionReceipt {
    pub session_id: String,
    pub prompt: SessionPromptResult,
}

/// Create a blank session with a preallocated id and then admit its first
/// prompt.  The preallocated id makes retries of the create phase idempotent
/// under the Host contract; the Host remains the authority for prompt
/// admission.  If prompt admission fails, the error names the already-created
/// blank session so the caller can retry explicitly.
pub fn dispatch_session(
    transport: &mut RpcTransport,
    cwd: &str,
    text: String,
    mode: PromptMode,
) -> PagerResult<DispatchSessionReceipt> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = DISPATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let session_id = format!("tui-dispatch-{stamp:x}-{sequence:x}");
    dispatch_session_with_id(transport, cwd, text, mode, session_id)
}

/// Idempotent-id variant used by callers that persist a retry token (and by
/// tests).  `session_id` is sent to `session.create`; a repeated create with
/// the same cwd is a Host no-op, while a conflicting cwd is surfaced.
pub fn dispatch_session_with_id(
    transport: &mut RpcTransport,
    cwd: &str,
    text: String,
    mode: PromptMode,
    session_id: String,
) -> PagerResult<DispatchSessionReceipt> {
    if session_id.trim().is_empty() {
        return Err(PagerError::new("dispatch session id must not be empty"));
    }
    let created: SessionCreateValue = api_call(
        transport,
        "session.create",
        json!({ "cwd": cwd, "sessionId": session_id }),
    )?;
    let id = created.session_id.clone();
    let prompt = match submit_prompt_for_session(transport, &id, text, mode) {
        Ok(prompt) => prompt,
        Err(error) => {
            return Err(PagerError::new(format!(
                "session {id} created as a blank session; initial prompt failed: {error}"
            )));
        }
    };
    Ok(DispatchSessionReceipt {
        session_id: id,
        prompt,
    })
}

/// Answer the currently displayed server-owned approval or question.
pub fn respond(
    transport: &mut RpcTransport,
    state: &SessionState,
    request_id: String,
    interaction: TuiInteractionResponse,
) -> PagerResult<TuiRespondResult> {
    transport.call(
        "tui.respond",
        &TuiRespondParams {
            session_id: state.session_id().to_string(),
            generation: state.generation(),
            request_id,
            interaction,
        },
    )
}

/// Mutate one authoritative pending queue item.
pub fn update_queue(
    transport: &mut RpcTransport,
    state: &SessionState,
    item_id: String,
    action: QueueAction,
) -> PagerResult<AcceptedResult> {
    api_call(
        transport,
        "session.updateQueue",
        serde_json::to_value(SessionUpdateQueueParams {
            session_id: state.session_id().to_string(),
            item_id,
            action,
        })?,
    )
}

/// Cancel the active turn while preserving pending queue work on the host.
pub fn cancel_session(
    transport: &mut RpcTransport,
    state: &SessionState,
) -> PagerResult<AcceptedResult> {
    cancel_session_id(transport, state.session_id())
}

pub fn cancel_session_id(
    transport: &mut RpcTransport,
    session_id: &str,
) -> PagerResult<AcceptedResult> {
    api_call(
        transport,
        "session.cancel",
        serde_json::to_value(SessionCancelParams {
            session_id: session_id.to_string(),
        })?,
    )
}

/// Persist a manual title through the host session-title service.
///
/// The request is tied to the current session/generation baseline. A caller
/// that switched sessions while the RPC was in flight must not apply the
/// receipt to the new view, so the generation guard is checked before the
/// result leaves this boundary.
pub fn rename_session(
    transport: &mut RpcTransport,
    state: &mut SessionState,
    title: String,
) -> PagerResult<SessionRenameResult> {
    let token = state.operation_token(None);
    let result = rename_session_id(transport, state.session_id(), title)?;
    if !state.accepts_operation(&token) {
        return Err(PagerError::new(
            "session rename response became stale after the view changed",
        ));
    }
    if result.seq < 0 {
        return Err(PagerError::new(
            "session rename response contained a negative projection sequence",
        ));
    }
    state.set_projection("title", result.seq, Value::String(result.title.clone()));
    Ok(result)
}

pub fn rename_session_id(
    transport: &mut RpcTransport,
    session_id: &str,
    title: String,
) -> PagerResult<SessionRenameResult> {
    api_call(
        transport,
        "session.rename",
        serde_json::to_value(SessionRenameParams {
            session_id: session_id.to_string(),
            title,
        })?,
    )
}

/// Fork a completed-turn prefix without mutating the currently attached view.
pub fn fork_session(
    transport: &mut RpcTransport,
    state: &SessionState,
    at_seq: Option<i64>,
) -> PagerResult<SessionForkResult> {
    if at_seq.is_some_and(|seq| seq < 0) {
        return Err(PagerError::new("session fork anchor must be non-negative"));
    }
    let token = state.operation_token(None);
    let result = fork_session_id(transport, state.session_id(), at_seq)?;
    if !state.accepts_operation(&token) {
        return Err(PagerError::new(
            "session fork response became stale after the view changed",
        ));
    }
    Ok(result)
}

pub fn fork_session_id(
    transport: &mut RpcTransport,
    session_id: &str,
    at_seq: Option<i64>,
) -> PagerResult<SessionForkResult> {
    if at_seq.is_some_and(|seq| seq < 0) {
        return Err(PagerError::new("session fork anchor must be non-negative"));
    }
    api_call(
        transport,
        "session.fork",
        serde_json::to_value(SessionForkParams {
            session_id: session_id.to_string(),
            at_seq,
        })?,
    )
}

pub fn drain_notifications(
    transport: &mut RpcTransport,
    state: &mut SessionState,
) -> PagerResult<SessionUpdate> {
    let mut combined = SessionUpdate::default();
    loop {
        let note = match transport.try_notification() {
            Ok(Some(note)) => note,
            Ok(None) => break,
            Err(error) => {
                let update = state.accept_stream_eof(error.to_string());
                combined.changed |= update.changed;
                combined.gap_detected |= update.gap_detected;
                break;
            }
        };
        let update = transport.route_notification(state, note)?;
        combined.changed |= update.changed;
        combined.gap_detected |= update.gap_detected;
    }
    combined.gap_detected |= state.needs_repair();
    Ok(combined)
}

fn choose_session(
    transport: &mut RpcTransport,
    choice: SessionChoice,
    cwd: &str,
) -> PagerResult<String> {
    match choice {
        SessionChoice::Id(session_id) => Ok(session_id),
        SessionChoice::New => create_session(transport, cwd, None),
        SessionChoice::Search(query) => {
            let result = search_sessions(transport, &query)?;
            result
                .items
                .first()
                .map(|item| item.session_id.clone())
                .ok_or_else(|| PagerError::new(format!("no session matched query {query:?}")))
        }
        SessionChoice::RecentOrCreate => {
            let list = list_sessions(transport)?;
            let recent = list
                .items
                .iter()
                .filter(|item| {
                    !item.blank && item.parent_session_id.is_none() && item.origin.is_none()
                })
                .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
                .or_else(|| {
                    list.items
                        .iter()
                        .filter(|item| !item.blank)
                        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
                });
            match recent {
                Some(item) => Ok(item.session_id.clone()),
                None => create_session(transport, cwd, None),
            }
        }
    }
}

fn create_session(
    transport: &mut RpcTransport,
    cwd: &str,
    agent_preset: Option<&str>,
) -> PagerResult<String> {
    Ok(create_blank_session(transport, cwd, agent_preset)?.session_id)
}

/// Create a blank session, optionally naming the agent preset it should run.
pub fn create_blank_session(
    transport: &mut RpcTransport,
    cwd: &str,
    agent_preset: Option<&str>,
) -> PagerResult<SessionCreateValue> {
    let mut params = json!({ "cwd": cwd });
    if let Some(preset) = agent_preset
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        params["agentPreset"] = json!(preset);
    }
    api_call(transport, "session.create", params)
}

/// List the deployment's agent-preset roster.
pub fn list_agent_presets(transport: &mut RpcTransport) -> PagerResult<AgentPresetListValue> {
    api_call(transport, "agentPreset.list", json!({}))
}

/// Read the effective official DSH slash-command directory for one session agent.
pub fn list_commands(
    transport: &mut RpcTransport,
    session_id: &str,
) -> PagerResult<Vec<CommandDescriptor>> {
    api_call(transport, "commands/list", json!({ "agentId": session_id }))
}

/// Execute one official DSH slash command without submitting it to the model.
pub fn execute_command(
    transport: &mut RpcTransport,
    session_id: &str,
    line: &str,
) -> PagerResult<CommandExecuteValue> {
    api_call(
        transport,
        "commands/execute",
        json!({
            "agentId": session_id,
            "line": line,
            "images": [],
        }),
    )
}

/// Recompose a blank session onto another roster preset.
pub fn select_agent_preset(
    transport: &mut RpcTransport,
    session_id: &str,
    agent_preset: &str,
) -> PagerResult<AgentPresetSelectValue> {
    api_call(
        transport,
        "agentPreset.select",
        json!({
            "sessionId": session_id,
            "agentPreset": agent_preset,
        }),
    )
}

/// Load the session-scoped model catalog and current selection.
pub fn session_models(
    transport: &mut RpcTransport,
    session_id: &str,
) -> PagerResult<SessionModelsValue> {
    api_call(
        transport,
        "session.models",
        json!({ "sessionId": session_id }),
    )
}

/// Assign the model used at the next prompt-assembly boundary.
pub fn select_session_model(
    transport: &mut RpcTransport,
    session_id: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> PagerResult<SessionSelectModelValue> {
    let mut params = json!({
        "sessionId": session_id,
        "provider": provider,
        "model": model,
    });
    if let Some(effort) = reasoning_effort
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        params["reasoningEffort"] = json!(effort);
    }
    api_call(transport, "session.selectModel", params)
}

fn fetch_tail(transport: &mut RpcTransport, session_id: &str) -> PagerResult<SessionHistoryValue> {
    api_call(
        transport,
        "session.history",
        json!({
            "sessionId": session_id,
            "maxMessages": PAGE_MESSAGES,
        }),
    )
}

fn api_call<T: DeserializeOwned>(
    transport: &mut RpcTransport,
    method: &str,
    params: Value,
) -> PagerResult<T> {
    let raw = transport.call_value(method, params)?;
    let control_value = raw.clone();
    let envelope: ApiResult<T> = serde_json::from_value(raw)?;
    if envelope.ok
        && method == "workspace.list"
        && let Some(value) = control_value.get("value")
    {
        transport
            .control_plane_mut()
            .store
            .seed_workspace_list(value)?;
    }
    envelope.into_result().map_err(PagerError::from)
}
