//! DeepSeek Harness runtime facade.
//!
//! This crate owns protocol/session state and host projections only. Visual
//! layout and terminal event handling live in a replaceable UI crate such as
//! `dsh-pager-grok-ui`.

pub mod control_plane;
pub mod dashboard;
pub mod error;
pub mod identity;
pub mod loader;
pub mod presentation;
pub mod scrollback;
pub mod session;
pub mod transport;

pub use control_plane::{
    ConnectionState, ControlPlaneApplyResult, ControlPlaneRecord, ControlPlaneRouter,
    ControlPlaneStore, ControlPlaneStoreOptions, ControlPlaneUpdate, JobView,
    SessionControlSnapshot, SessionProjection, SubagentCatalog, SubagentListEntry, WorkspaceView,
};
pub use dashboard::{
    DashboardActionKind, DashboardActionState, DashboardModel, DashboardRow, DashboardStatus,
    DashboardViewState, DashboardWorkspace,
};
pub use error::{PagerError, PagerResult};
pub use identity::{
    DshGeneration, DshInteractionId, DshQueueItemId, DshRequestId, DshSeq, DshSessionId,
};
pub use loader::{
    AttachmentPreview, DispatchSessionReceipt, SessionChoice, archive_session, cancel_session,
    cancel_session_id, create_blank_session, detach_session, dispatch_session,
    dispatch_session_with_id, drain_notifications, execute_command, fetch_attachment, fork_session,
    fork_session_id, interrupt_subagent, list_agent_presets, list_commands, list_file_references,
    list_sessions, list_subagents, list_workspaces, load_session, load_session_id,
    peek_session_tail, peek_subagent_history, prompt_subagent, reconnect_session, rename_session,
    rename_session_id, reorder_session, reorder_workspace, repair_tail, respond, search_sessions,
    select_agent_preset, select_session_model, session_models, submit_prompt,
    submit_prompt_for_session, subscribe_control_plane, update_queue,
};
pub use presentation::{
    DshEditDetail, DshInteraction, DshPresentationAdapter, DshPresentationModel, DshQueueContent,
    DshQueueItem, DshReadLine, DshRenderBlock, DshRenderContent, DshRenderEntry, DshRenderEntryId,
    DshRenderFinish, DshRenderKind, DshRenderRole, DshRenderUpdate, DshRenderVisibility,
    DshSearchFile, DshSearchMatch, DshToolCallView, DshToolDiff, DshToolKind, DshToolLocation,
    DshToolResult, DshToolResultView, DshWebSource, event_time_epoch_ms,
};
pub use scrollback::{
    DshRenderEntryRef, EntryId, EntryLayout, PaintWindow, ScrollAnchor, ScrollbackLayout,
    ScrollbackRevisionDelta,
};
pub use session::{
    ConnectionPhase, Diagnostic, DiagnosticLevel, InteractionKind, OperationToken,
    PendingInteraction, SessionState, SessionUpdate,
};
pub use transport::{RpcTransport, validate_backend_program};
