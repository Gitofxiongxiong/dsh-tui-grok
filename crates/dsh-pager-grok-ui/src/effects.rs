//! Host-independent effects emitted by Grok UI interactions.

use dsh_pager::{PagerResult, RpcTransport, SessionState};
use dsh_pager_protocol::PromptMode;

/// Commands understood by a harness adapter. Keep this enum small and
/// semantic; it is deliberately not a mirror of JSON-RPC method names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEffect {
    SubmitPrompt { text: String, mode: PromptMode },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiEffectResult {
    pub accepted: bool,
}

/// Boundary a non-DSH host (for example Codex CLI) can implement later.
pub trait UiEffectSink {
    fn dispatch(&mut self, effect: UiEffect, session: &SessionState)
    -> PagerResult<UiEffectResult>;
}

/// DSH's concrete effect sink. All RPC knowledge stays here instead of in
/// copied Grok view modules.
pub struct DshEffectSink<'a> {
    pub transport: &'a mut RpcTransport,
}

impl UiEffectSink for DshEffectSink<'_> {
    fn dispatch(
        &mut self,
        effect: UiEffect,
        session: &SessionState,
    ) -> PagerResult<UiEffectResult> {
        match effect {
            UiEffect::SubmitPrompt { text, mode } => {
                let receipt = dsh_pager::submit_prompt(self.transport, session, text, mode)?;
                Ok(UiEffectResult {
                    accepted: receipt.accepted,
                })
            }
        }
    }
}
