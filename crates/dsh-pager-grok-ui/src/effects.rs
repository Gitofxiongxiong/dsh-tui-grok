//! Host-independent effects emitted by Grok UI interactions.

use std::collections::HashSet;

use dsh_pager::{
    DshGeneration, DshQueueItemId, DshRequestId, DshSeq, DshSessionId, PagerResult, RpcTransport,
    SessionState,
};
use dsh_pager_protocol::{PromptMode, QueueAction, TuiInteractionResponse};
use serde::{Deserialize, Serialize};

/// Semantic user intent emitted by a Grok view. It has no transport or
/// renderer references and can be reduced in a host-neutral test harness.
#[derive(Debug, Clone, PartialEq)]
pub enum UiIntent {
    SubmitPrompt {
        text: String,
        mode: PromptMode,
    },
    AttachSession {
        session_id: DshSessionId,
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

/// DSH-neutral effect after intent compilation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiEffect {
    SubmitPrompt {
        operation: OperationKey,
        text: String,
        mode: PromptMode,
    },
    AttachSession {
        operation: OperationKey,
        session_id: DshSessionId,
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
}

/// Boundary a non-DSH host (for example Codex CLI) can implement later.
pub trait UiEffectSink {
    fn submit(&mut self, intent: UiIntent, context: &UiContext) -> PagerResult<UiEffectReceipt>;
}

/// DSH's concrete effect sink. All RPC knowledge stays here instead of in
/// copied Grok view modules.
pub struct DshEffectSink<'a> {
    pub transport: &'a mut RpcTransport,
    pub next_request: u64,
    completed: HashSet<OperationKey>,
}

impl DshEffectSink<'_> {
    pub fn new(transport: &mut RpcTransport) -> DshEffectSink<'_> {
        DshEffectSink {
            transport,
            next_request: 1,
            completed: HashSet::new(),
        }
    }
}

impl UiEffectSink for DshEffectSink<'_> {
    fn submit(&mut self, intent: UiIntent, context: &UiContext) -> PagerResult<UiEffectReceipt> {
        self.dispatch_effect(compile_intent(intent, context))
    }
}

/// Compile semantic intent into a transport-free effect. Request ids are
/// deliberately `pending` until the concrete sink admits the operation.
pub fn compile_intent(intent: UiIntent, context: &UiContext) -> UiEffect {
    let target_session_id = match &intent {
        UiIntent::AttachSession { session_id } => session_id.clone(),
        _ => context.session_id.clone(),
    };
    let action_name = match &intent {
        UiIntent::SubmitPrompt { .. } => "submit",
        UiIntent::AttachSession { .. } => "attach",
        UiIntent::QueueMutation { .. } => "queue-mutation",
        UiIntent::RespondInteraction { .. } => "respond-interaction",
        UiIntent::RenameSession { .. } => "rename",
        UiIntent::ForkSession { .. } => "fork",
        UiIntent::ArchiveSession => "archive",
    };
    let dedupe_key = match &intent {
        UiIntent::SubmitPrompt { text, mode } => {
            format!(
                "{action_name}:{}:{}",
                prompt_digest(text),
                mode_label(*mode)
            )
        }
        UiIntent::AttachSession { session_id } => format!("{action_name}:{session_id}"),
        UiIntent::QueueMutation { item_id, action } => {
            format!("{action_name}:{item_id}:{action:?}")
        }
        UiIntent::RespondInteraction { request_id, .. } => format!("{action_name}:{request_id}"),
        UiIntent::RenameSession { title } => format!("{action_name}:{}", prompt_digest(title)),
        UiIntent::ForkSession { at_seq } => format!("{action_name}:{at_seq:?}"),
        UiIntent::ArchiveSession => action_name.to_string(),
    };
    let request_id = context.request_id.clone();
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
        UiIntent::SubmitPrompt { text, mode } => UiEffect::SubmitPrompt {
            operation,
            text,
            mode,
        },
        UiIntent::AttachSession { session_id } => UiEffect::AttachSession {
            operation,
            session_id,
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
    }
}

impl DshEffectSink<'_> {
    pub fn dispatch_effect(&mut self, effect: UiEffect) -> PagerResult<UiEffectReceipt> {
        let (operation, result) = match effect {
            UiEffect::SubmitPrompt {
                mut operation,
                text,
                mode,
            } => {
                self.prepare_operation(&mut operation);
                if self.completed.contains(&operation) {
                    return Ok(self.duplicate_receipt(operation));
                }
                let result = dsh_pager::submit_prompt_for_session(
                    self.transport,
                    operation.session_id.as_str(),
                    text,
                    mode,
                )?;
                (operation, Ok(result.accepted))
            }
            UiEffect::QueueMutation {
                mut operation,
                item_id,
                action,
            } => {
                self.prepare_operation(&mut operation);
                if self.completed.contains(&operation) {
                    return Ok(self.duplicate_receipt(operation));
                }
                let session =
                    SessionState::new(operation.session_id.to_string(), operation.generation.get());
                let result =
                    dsh_pager::update_queue(self.transport, &session, item_id.to_string(), action)?;
                (operation, Ok(result.accepted))
            }
            UiEffect::RespondInteraction {
                mut operation,
                request_id,
                interaction,
            } => {
                self.prepare_operation(&mut operation);
                if self.completed.contains(&operation) {
                    return Ok(self.duplicate_receipt(operation));
                }
                let session =
                    SessionState::new(operation.session_id.to_string(), operation.generation.get());
                let result = dsh_pager::respond(
                    self.transport,
                    &session,
                    request_id.to_string(),
                    interaction,
                )?;
                (operation, Ok(result.accepted))
            }
            UiEffect::RenameSession {
                mut operation,
                title,
            } => {
                self.prepare_operation(&mut operation);
                if self.completed.contains(&operation) {
                    return Ok(self.duplicate_receipt(operation));
                }
                dsh_pager::rename_session_id(self.transport, operation.session_id.as_str(), title)?;
                (operation, Ok(true))
            }
            UiEffect::ForkSession {
                mut operation,
                at_seq,
            } => {
                self.prepare_operation(&mut operation);
                if self.completed.contains(&operation) {
                    return Ok(self.duplicate_receipt(operation));
                }
                dsh_pager::fork_session_id(
                    self.transport,
                    operation.session_id.as_str(),
                    at_seq.map(DshSeq::get),
                )?;
                (operation, Ok(true))
            }
            UiEffect::ArchiveSession { mut operation } => {
                self.prepare_operation(&mut operation);
                if self.completed.contains(&operation) {
                    return Ok(self.duplicate_receipt(operation));
                }
                dsh_pager::archive_session(self.transport, operation.session_id.as_str())?;
                (operation, Ok(true))
            }
            UiEffect::AttachSession { mut operation, .. } => {
                self.prepare_operation(&mut operation);
                if self.completed.contains(&operation) {
                    return Ok(self.duplicate_receipt(operation));
                }
                (
                    operation,
                    Err(dsh_pager::PagerError::new(
                        "attach requires loader/session swap",
                    )),
                )
            }
        };
        match result {
            Ok(true) => {
                self.completed.insert(operation.clone());
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
                status: UiEffectStatus::Failed,
                operation,
                diagnostic: Some(error.to_string()),
                retryable: Some(true),
            }),
        }
    }

    fn prepare_operation(&mut self, operation: &mut OperationKey) {
        if operation.request_id.as_str() == "pending" {
            operation.request_id = DshRequestId::new(format!("ui-{}", self.next_request));
            self.next_request = self.next_request.saturating_add(1);
            operation.dedupe_key = format!("{}:{}", operation.action, operation.request_id);
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
    use dsh_pager_protocol::PromptMode;
    use serde_json::json;

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
        assert_eq!(first.request_id, second.request_id);
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
}
