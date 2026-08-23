//! JSON-RPC 2.0 line types shared with `@deepseek-ai/dsh-tui-protocol`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;

/// Wire-stable protocol integer. Handshake rejects any other value.
pub const TUI_PROTOCOL_VERSION: u32 = 1;

/// Wire-stable `serverInfo.name` returned by `tui.hello`.
pub const TUI_SERVER_INFO_NAME: &str = "deepseek-harness-tui";

/// How the server classified this connection's event recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResumeClass {
    ResumeAccepted,
    BaselineRequired,
}

/// Client process kind advertised at hello.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiClientType {
    Tui,
    Test,
}

/// Optional operator/observer/image bits reserved for later lease rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuiClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<bool>,
}

/// Identity fields used to refuse a mismatched shared backend.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiClientIdentity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
}

/// `tui.hello` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiHelloParams {
    pub protocol_version: u32,
    pub client_type: TuiClientType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<TuiClientCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<TuiClientIdentity>,
}

/// `tui.hello` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiHelloResult {
    pub protocol_version: u32,
    pub client_id: String,
    pub generation: u64,
    pub resume_class: ResumeClass,
    pub server_info: TuiServerInfo,
}

/// Server identity returned by hello.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiServerInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_digest: Option<String>,
}

/// `tui.attach` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiAttachParams {
    pub session_id: String,
    pub generation: u64,
}

/// `tui.detach` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiDetachParams {
    pub session_id: String,
    pub generation: u64,
}

/// Scope of a control-plane subscription. A session scope preserves the
/// original attached-session form; `all` is the Dashboard/control-plane form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiSubscribeScope {
    Session,
    ControlPlane,
    All,
}

/// `tui.subscribe` parameters. An omitted session id is valid only for an
/// explicit `all`/`control-plane` scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiSubscribeParams {
    pub generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<TuiSubscribeScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
}

/// `tui.subscribe` receipt. Older gateways may return only `{}`; callers that
/// use this type should keep fields optional at the wire boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TuiSubscribeResult {
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub resume_class: Option<ResumeClass>,
    #[serde(default)]
    pub scope: Option<TuiSubscribeScope>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub watermarks: std::collections::BTreeMap<String, i64>,
}

/// Session role granted by `tui.attach`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiAttachRole {
    Driver,
    Subscriber,
}

/// `tui.attach` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuiAttachResult {
    pub attached: bool,
    pub role: TuiAttachRole,
}

/// The mode used when submitting a prompt to a live session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptMode {
    Queue,
    Steer,
}

/// One prompt content part accepted by the host prompt API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PromptContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        #[serde(rename = "mediaType")]
        media_type: String,
        data: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

/// Parameters for the forwarded `session.prompt` method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptParams {
    pub session_id: String,
    pub mode: PromptMode,
    pub content: Vec<PromptContentPart>,
}

/// Result returned by a successful `session.prompt` call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptResult {
    pub accepted: bool,
    #[serde(default)]
    pub command: Option<Value>,
}

/// Answer sent to a pending approval or question interaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TuiInteractionResponse {
    #[serde(rename = "approval")]
    Approval {
        #[serde(rename = "approvalId")]
        approval_id: String,
        outcome: String,
    },
    #[serde(rename = "question")]
    Question { answers: Value },
}

/// Parameters for the TUI-owned `tui.respond` control method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiRespondParams {
    pub session_id: String,
    pub generation: u64,
    pub request_id: String,
    pub interaction: TuiInteractionResponse,
}

/// Receipt returned by `tui.respond`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuiRespondResult {
    pub accepted: bool,
}

/// One client-requested mutation of a pending queue item.
///
/// The queue message schema is owned by the host. `content` therefore remains
/// value-backed here instead of introducing a second Rust copy of the core
/// `ContentBlock` union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum QueueAction {
    #[serde(rename = "edit")]
    Edit { content: Vec<Value> },
    #[serde(rename = "remove")]
    Remove,
    #[serde(rename = "steer")]
    Steer,
}

/// Parameters for `session.updateQueue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateQueueParams {
    pub session_id: String,
    pub item_id: String,
    pub action: QueueAction,
}

/// Parameters for `session.cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCancelParams {
    pub session_id: String,
}

/// Parameters for the host-owned durable session title mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRenameParams {
    pub session_id: String,
    pub title: String,
}

/// Receipt returned by `session.rename`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRenameResult {
    pub title: String,
    pub seq: i64,
}

/// Parameters for forking a completed-turn session prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionForkParams {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_seq: Option<i64>,
}

/// Receipt returned by `session.fork`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionForkResult {
    pub session_id: String,
}

/// Parameters for the host-owned workspace archive mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceArchiveSessionParams {
    pub session_id: String,
}

/// Complete archive set returned after `workspace.archiveSession`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceArchiveSessionValue {
    pub archived_session_ids: Vec<String>,
}

/// Parameters for moving a workspace in the host-owned display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInsertBeforeParams {
    pub workspace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_workspace_id: Option<String>,
}

/// Complete workspace order returned after `workspace.insertBefore`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOrderValue {
    pub workspace_ids: Vec<String>,
}

/// Parameters for moving one accounted session inside a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInsertSessionBeforeParams {
    pub workspace_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_session_id: Option<String>,
}

/// Workspace view returned after a session-order mutation.  The workspace
/// schema is host-owned and remains value-backed for forward compatibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInsertSessionBeforeValue {
    pub workspace: Value,
}

/// Receipt returned by queue/cancel mutations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedResult {
    pub accepted: bool,
}

/// One ApiProxy business error carried inside a successful JSON-RPC response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Value,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// ApiProxy's `{ ok, value | error }` result nested in JSON-RPC `result`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiResult<T> {
    pub ok: bool,
    #[serde(default = "default_none")]
    pub value: Option<T>,
    #[serde(default = "default_none")]
    pub error: Option<ApiError>,
}

fn default_none<T>() -> Option<T> {
    None
}

impl<T> ApiResult<T> {
    pub fn into_result(self) -> Result<T, ApiError> {
        if self.ok {
            self.value.ok_or_else(|| ApiError {
                code: "invalid-response".into(),
                message: "successful ApiProxy response omitted value".into(),
                details: Value::Null,
            })
        } else {
            Err(self.error.unwrap_or_else(|| ApiError {
                code: "invalid-response".into(),
                message: "failed ApiProxy response omitted error".into(),
                details: Value::Null,
            }))
        }
    }
}

/// One row returned by `session.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: String,
    pub updated_at: f64,
    pub running: bool,
    pub blank: bool,
    #[serde(default)]
    pub parent_session_id: Option<String>,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub agent_preset: Option<String>,
    #[serde(default)]
    pub projections: Option<SessionProjectionsBlock>,
}

/// Value returned by `session.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionListValue {
    pub items: Vec<SessionSummary>,
}

/// One content-search result returned by `session.search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchItem {
    pub session_id: String,
    pub snippet: String,
}

/// Value returned by `session.search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchValue {
    pub items: Vec<SessionSearchItem>,
    #[serde(default)]
    pub has_more: bool,
}

/// One path-only file reference candidate returned by the host provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReferenceCandidate {
    pub path: String,
    pub kind: String,
}

/// Value returned by `fileReferences.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReferencesListValue {
    pub items: Vec<FileReferenceCandidate>,
}

/// Value returned by `session.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateValue {
    pub session_id: String,
    #[serde(default)]
    pub agent_preset: Option<String>,
}

/// Placement of one pending inbox item in the authoritative queue snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueuePlacement {
    Queued,
    Steering,
    Context,
}

/// One pending queue item. The complete message remains host-owned JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionQueueItem {
    pub id: String,
    pub placement: QueuePlacement,
    pub message: Value,
}

/// A value-backed Session event. Domain plugins retain ownership of `data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub seq: i64,
    pub time: f64,
    pub data: Value,
    #[serde(default)]
    pub source_event_seqs: Option<Vec<i64>>,
    #[serde(default)]
    pub surface_op: Option<Value>,
    #[serde(default)]
    pub ignorable: Option<bool>,
}

/// One history row, including optional host-computed tool presentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub event: SessionEvent,
    #[serde(default)]
    pub view: Option<Value>,
}

/// Projection baseline carried by the history tail page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjectionsBlock {
    pub as_of_seq: i64,
    pub values: Map<String, Value>,
}

/// Value returned by `session.history`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHistoryValue {
    pub events: Vec<HistoryEntry>,
    pub has_more: bool,
    #[serde(default)]
    pub projections: Option<SessionProjectionsBlock>,
}

/// Subagent child mode owned by the Harness subagent catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubagentMode {
    OneShot,
    Continuable,
}

/// Stable parent/child address used by subagent history and actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentAddress {
    pub parent_session_id: String,
    pub child_session_id: String,
    pub mode: SubagentMode,
}

/// Value-backed child row. Unknown future fields remain in `raw`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentListEntry {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub activity: Option<String>,
    #[serde(default)]
    pub mode: Option<SubagentMode>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub has_children: Option<bool>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub raw: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentListValue {
    pub entries: Vec<SubagentListEntry>,
    pub parent_available: bool,
}

/// Subagent history has the same event/page shape as session.history.
pub type SubagentHistoryValue = SessionHistoryValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentPromptParams {
    pub parent_session_id: String,
    pub child_session_id: String,
    pub mode: SubagentMode,
    pub content: Vec<PromptContentPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_time_zone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentPromptResult {
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInterruptParams {
    pub parent_session_id: String,
    pub child_session_id: String,
    pub mode: SubagentMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentInterruptResult {
    pub accepted: bool,
}

/// JSON-RPC request envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC success response envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcSuccess {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Value,
}

/// JSON-RPC failure response envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcFailure {
    pub jsonrpc: String,
    pub id: Value,
    pub error: Value,
}

/// JSON-RPC notification envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// One decoded JSON-RPC line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcLine {
    Request(JsonRpcRequest),
    Success(JsonRpcSuccess),
    Failure(JsonRpcFailure),
    Notification(JsonRpcNotification),
}

/// Parse one newline-stripped JSON-RPC request line.
pub fn parse_request_line(line: &str) -> Result<JsonRpcRequest, serde_json::Error> {
    serde_json::from_str(line)
}

/// Parse one newline-stripped JSON-RPC success line.
pub fn parse_success_line(line: &str) -> Result<JsonRpcSuccess, serde_json::Error> {
    serde_json::from_str(line)
}

/// Parse one newline-stripped JSON-RPC line of any kind.
pub fn parse_line(line: &str) -> Result<JsonRpcLine, serde_json::Error> {
    serde_json::from_str(line)
}

/// Serialize one JSON-RPC request as a newline-terminated frame.
pub fn encode_request_line(request: &JsonRpcRequest) -> Result<String, serde_json::Error> {
    Ok(format!("{}\n", serde_json::to_string(request)?))
}

/// Build a `tui.hello` request.
pub fn hello_request(
    id: impl Into<Value>,
    params: &TuiHelloParams,
) -> Result<JsonRpcRequest, serde_json::Error> {
    Ok(JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: id.into(),
        method: "tui.hello".into(),
        params: Some(serde_json::to_value(params)?),
    })
}

/// Build any JSON-RPC request using the line protocol envelope.
pub fn rpc_request(
    id: impl Into<Value>,
    method: impl Into<String>,
    params: Option<Value>,
) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: id.into(),
        method: method.into(),
        params,
    }
}

/// Embedded hello identity: profile `tui-embedded` plus the process cwd.
pub fn embedded_hello_params(cwd: String) -> TuiHelloParams {
    TuiHelloParams {
        protocol_version: TUI_PROTOCOL_VERSION,
        client_type: TuiClientType::Tui,
        capabilities: Some(TuiClientCapabilities {
            operator: Some(true),
            observer: Some(false),
            images: Some(true),
        }),
        client_id: None,
        identity: Some(TuiClientIdentity {
            profile: Some("tui-embedded".into()),
            cwd: Some(cwd),
            plugin_digest: None,
            sandbox: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn hello_request_fixture_round_trip() {
        let raw = fixture("hello-request.json");
        let request = parse_request_line(raw.trim()).expect("request json");
        assert_eq!(request.method, "tui.hello");
        let params: TuiHelloParams =
            serde_json::from_value(request.params.expect("params")).expect("hello params");
        assert_eq!(params.protocol_version, TUI_PROTOCOL_VERSION);
        assert_eq!(params.client_type, TuiClientType::Tui);
        assert_eq!(
            params.identity.as_ref().and_then(|i| i.cwd.as_deref()),
            Some("/work")
        );
        let line = parse_line(raw.trim()).expect("classified request");
        match line {
            JsonRpcLine::Request(parsed) => assert_eq!(parsed.method, "tui.hello"),
            other => panic!("expected request, got {other:?}"),
        }
    }

    #[test]
    fn hello_result_fixture_round_trip() {
        let raw = fixture("hello-result.json");
        let success = parse_success_line(raw.trim()).expect("result json");
        let result: TuiHelloResult = serde_json::from_value(success.result).expect("hello result");
        assert_eq!(result.protocol_version, TUI_PROTOCOL_VERSION);
        assert_eq!(result.client_id, "client-1");
        assert_eq!(result.resume_class, ResumeClass::BaselineRequired);
        assert_eq!(result.server_info.name, TUI_SERVER_INFO_NAME);
        let line = parse_line(raw.trim()).expect("classified success");
        match line {
            JsonRpcLine::Success(parsed) => assert_eq!(parsed.id, success.id),
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[test]
    fn encode_hello_request_round_trip() {
        let params = embedded_hello_params("/work".into());
        let request = hello_request(1u64, &params).expect("request");
        let line = encode_request_line(&request).expect("encode");
        assert!(line.ends_with('\n'));
        let parsed = parse_request_line(line.trim()).expect("parse");
        assert_eq!(parsed.method, "tui.hello");
        let decoded: TuiHelloParams =
            serde_json::from_value(parsed.params.expect("params")).expect("params");
        assert_eq!(
            decoded.identity.unwrap().profile.as_deref(),
            Some("tui-embedded")
        );
    }

    #[test]
    fn parse_server_ready_notification() {
        let line =
            parse_line(r#"{"jsonrpc":"2.0","method":"tui.serverReady"}"#).expect("notification");
        match line {
            JsonRpcLine::Notification(note) => assert_eq!(note.method, "tui.serverReady"),
            other => panic!("expected notification, got {other:?}"),
        }
    }

    #[test]
    fn api_result_accepts_success_and_failure_without_the_other_slot() {
        let success: ApiResult<String> =
            serde_json::from_str(r#"{"ok":true,"value":"loaded"}"#).expect("success result");
        assert_eq!(success.into_result().expect("value"), "loaded");

        let failure: ApiResult<String> = serde_json::from_str(
            r#"{"ok":false,"error":{"code":"session-not-found","message":"missing","details":{}}}"#,
        )
        .expect("failure result");
        assert_eq!(
            failure.into_result().expect_err("error").code,
            "session-not-found"
        );
    }

    #[test]
    fn prompt_and_response_payloads_use_host_wire_names() {
        let prompt = SessionPromptParams {
            session_id: "s-1".into(),
            mode: PromptMode::Steer,
            content: vec![PromptContentPart::Text {
                text: "你好".into(),
            }],
        };
        let value = serde_json::to_value(&prompt).expect("prompt json");
        assert_eq!(value["sessionId"], "s-1");
        assert_eq!(value["mode"], "steer");
        assert_eq!(value["content"][0]["type"], "text");
        let decoded: SessionPromptParams = serde_json::from_value(value).expect("prompt decode");
        assert_eq!(decoded, prompt);

        let response = TuiRespondParams {
            session_id: "s-1".into(),
            generation: 7,
            request_id: "rpc-9".into(),
            interaction: TuiInteractionResponse::Question {
                answers: serde_json::json!({ "answers": [] }),
            },
        };
        let value = serde_json::to_value(&response).expect("response json");
        assert_eq!(value["requestId"], "rpc-9");
        assert_eq!(value["interaction"]["type"], "question");
        assert_eq!(
            value["interaction"]["answers"]["answers"],
            serde_json::json!([])
        );
        assert_eq!(
            serde_json::from_value::<TuiRespondParams>(value).unwrap(),
            response
        );
    }

    #[test]
    fn subscribe_scope_and_receipt_are_backward_compatible() {
        let params = TuiSubscribeParams {
            generation: 9,
            session_id: None,
            scope: Some(TuiSubscribeScope::All),
            since: Some(12),
        };
        let value = serde_json::to_value(&params).expect("subscribe params");
        assert_eq!(value["generation"], 9);
        assert_eq!(value["scope"], "all");
        assert_eq!(value["since"], 12);
        let decoded: TuiSubscribeParams = serde_json::from_value(value).expect("decode params");
        assert_eq!(decoded, params);

        let old: TuiSubscribeResult =
            serde_json::from_value(serde_json::json!({})).expect("old empty subscribe receipt");
        assert_eq!(old, TuiSubscribeResult::default());
        let current: TuiSubscribeResult = serde_json::from_value(serde_json::json!({
            "generation": 9,
            "resumeClass": "resume-accepted",
            "scope": "all",
            "watermarks": {"s": 12}
        }))
        .expect("current subscribe receipt");
        assert_eq!(current.resume_class, Some(ResumeClass::ResumeAccepted));
        assert_eq!(current.watermarks.get("s"), Some(&12));
    }

    #[test]
    fn queue_actions_round_trip_without_reinterpreting_content() {
        let action = QueueAction::Edit {
            content: vec![serde_json::json!({ "type": "text", "text": "next" })],
        };
        let value = serde_json::to_value(&action).expect("queue action json");
        assert_eq!(value["kind"], "edit");
        assert_eq!(
            serde_json::from_value::<QueueAction>(value).unwrap(),
            action
        );
        let params = SessionUpdateQueueParams {
            session_id: "s".into(),
            item_id: "m".into(),
            action: QueueAction::Remove,
        };
        let value = serde_json::to_value(&params).expect("queue params json");
        assert_eq!(value["itemId"], "m");
        assert_eq!(
            serde_json::from_value::<SessionUpdateQueueParams>(value).unwrap(),
            params
        );

        let detach = TuiDetachParams {
            session_id: "s".into(),
            generation: 4,
        };
        let value = serde_json::to_value(&detach).expect("detach params json");
        assert_eq!(
            value,
            serde_json::json!({ "sessionId": "s", "generation": 4 })
        );
    }

    #[test]
    fn lifecycle_payloads_use_exact_host_wire_names() {
        let rename = SessionRenameParams {
            session_id: "s".into(),
            title: "A title".into(),
        };
        assert_eq!(
            serde_json::to_value(&rename).unwrap(),
            serde_json::json!({ "sessionId": "s", "title": "A title" })
        );
        let fork = SessionForkParams {
            session_id: "s".into(),
            at_seq: Some(12),
        };
        assert_eq!(
            serde_json::to_value(&fork).unwrap(),
            serde_json::json!({ "sessionId": "s", "atSeq": 12 })
        );
        assert_eq!(
            serde_json::from_value::<SessionForkResult>(serde_json::json!({
                "sessionId": "child"
            }))
            .unwrap()
            .session_id,
            "child"
        );
    }

    #[test]
    fn subagent_payloads_preserve_the_typed_parent_child_address() {
        let address = SubagentAddress {
            parent_session_id: "parent".into(),
            child_session_id: "child".into(),
            mode: SubagentMode::Continuable,
        };
        assert_eq!(
            serde_json::to_value(&address).unwrap(),
            serde_json::json!({
                "parentSessionId": "parent",
                "childSessionId": "child",
                "mode": "continuable"
            })
        );
        let catalog: SubagentListValue = serde_json::from_value(serde_json::json!({
            "entries": [
                {
                    "kind": "child",
                    "id": "child",
                    "mode": "one-shot",
                    "activity": "inactive",
                    "hasChildren": false,
                    "future": 1
                }
            ],
            "parentAvailable": true
        }))
        .unwrap();
        assert_eq!(catalog.entries[0].mode, Some(SubagentMode::OneShot));
        assert_eq!(
            catalog.entries[0].raw.get("future"),
            Some(&serde_json::json!(1))
        );
    }
}
