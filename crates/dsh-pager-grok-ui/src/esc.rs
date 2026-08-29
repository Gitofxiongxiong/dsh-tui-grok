//! Grok-compatible bare-Esc pending-action policy.
//!
//! Structurally adapted from Grok Build's `AgentView::try_handle_esc_policy`
//! and `AppView::PendingAction`. Host effects stay outside this module.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

pub const ESC_DOUBLE_PRESS_TTL: Duration = Duration::from_millis(800);
pub const ESC_CANCEL_REWIND_GRACE: Duration = Duration::from_millis(1_000);
const ESC_DOUBLE_PRESS_TEST_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingEscAction {
    ClearPrompt,
    ShowRewindPicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscOutcome {
    CancelTurn,
    ClearPrompt,
    ShowRewindPicker,
    ArmedClear,
    ArmedRewind,
    Swallowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EscContext {
    pub prompt_has_content: bool,
    pub prompt_owns_keys: bool,
    pub has_rewindable_turns: bool,
    pub turn_running: bool,
    pub cancel_pending: bool,
    pub blocking_input_pending: bool,
    pub normal_prompt_mode: bool,
    pub history_search_active: bool,
}

impl EscContext {
    pub const fn idle(prompt_has_content: bool) -> Self {
        Self {
            prompt_has_content,
            prompt_owns_keys: true,
            has_rewindable_turns: false,
            turn_running: false,
            cancel_pending: false,
            blocking_input_pending: false,
            normal_prompt_mode: true,
            history_search_active: false,
        }
    }

    const fn busy(self) -> bool {
        self.turn_running || self.cancel_pending
    }
}

#[derive(Debug, Clone)]
struct PendingEsc {
    action: PendingEscAction,
    expires_at: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct EscPolicy {
    pending: Option<PendingEsc>,
    rewind_suppress_deadline: Option<Instant>,
}

impl EscPolicy {
    pub fn pending_action(&self, now: Instant) -> Option<PendingEscAction> {
        self.pending
            .as_ref()
            .filter(|pending| now < pending.expires_at)
            .map(|pending| pending.action)
    }

    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    pub fn expire_at(&mut self, now: Instant) -> bool {
        let expired = self
            .pending
            .as_ref()
            .is_some_and(|pending| now >= pending.expires_at);
        if expired {
            self.pending = None;
        }
        if self
            .rewind_suppress_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.rewind_suppress_deadline = None;
        }
        expired
    }

    /// Pending matching happens before overlays, matching Grok AppView. If no
    /// pending action fires, the caller should let higher-priority surfaces
    /// consume the key and only then call [`Self::handle_unclaimed_at`].
    pub fn try_fire_pending_at(
        &mut self,
        key: &KeyEvent,
        context: EscContext,
        now: Instant,
    ) -> Option<EscOutcome> {
        // Grok AppView does not classify release reports as key events, so a
        // terminal that reports releases must not retire the armed press.
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let pending = self.pending.take()?;
        let stale_idle_arm_while_busy = context.busy();
        if !stale_idle_arm_while_busy && now < pending.expires_at && is_bare_esc_press(key) {
            return Some(match pending.action {
                PendingEscAction::ClearPrompt => EscOutcome::ClearPrompt,
                PendingEscAction::ShowRewindPicker => EscOutcome::ShowRewindPicker,
            });
        }
        None
    }

    /// Handle a key after dropdowns, search, selections, and overlays decline
    /// it. `None` means it is not a bare Esc press.
    pub fn handle_unclaimed_at(
        &mut self,
        key: &KeyEvent,
        context: EscContext,
        now: Instant,
    ) -> Option<EscOutcome> {
        if !is_bare_esc_press(key) {
            return None;
        }

        if context.blocking_input_pending {
            return Some(EscOutcome::Swallowed);
        }

        if context.busy() {
            self.rewind_suppress_deadline = Some(now + ESC_CANCEL_REWIND_GRACE);
            return Some(EscOutcome::CancelTurn);
        }

        if context.prompt_has_content && context.prompt_owns_keys {
            self.pending = Some(PendingEsc {
                action: PendingEscAction::ClearPrompt,
                expires_at: now + esc_double_press_ttl(),
            });
            return Some(EscOutcome::ArmedClear);
        }

        let rewind_suppressed = self
            .rewind_suppress_deadline
            .is_some_and(|deadline| now < deadline);
        if !context.prompt_has_content
            && context.has_rewindable_turns
            && context.normal_prompt_mode
            && !context.blocking_input_pending
            && !context.history_search_active
            && !rewind_suppressed
        {
            self.pending = Some(PendingEsc {
                action: PendingEscAction::ShowRewindPicker,
                expires_at: now + esc_double_press_ttl(),
            });
            return Some(EscOutcome::ArmedRewind);
        }

        Some(EscOutcome::Swallowed)
    }
}

pub fn is_bare_esc_press(key: &KeyEvent) -> bool {
    key.kind != KeyEventKind::Release && key.code == KeyCode::Esc && key.modifiers.is_empty()
}

pub fn esc_double_press_ttl() -> Duration {
    static TTL: OnceLock<Duration> = OnceLock::new();
    *TTL.get_or_init(|| parse_esc_ttl(std::env::var("GROK_ESC_DOUBLE_PRESS_MS").ok()))
}

fn parse_esc_ttl(raw: Option<String>) -> Duration {
    raw.and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|milliseconds| *milliseconds > 0)
        .map(|milliseconds| Duration::from_millis(milliseconds.min(ESC_DOUBLE_PRESS_TEST_MS)))
        .unwrap_or(ESC_DOUBLE_PRESS_TTL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventState, KeyModifiers};

    fn esc() -> KeyEvent {
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    fn fire(policy: &mut EscPolicy, key: &KeyEvent, ctx: EscContext, now: Instant) -> EscOutcome {
        policy
            .try_fire_pending_at(key, ctx, now)
            .or_else(|| policy.handle_unclaimed_at(key, ctx, now))
            .expect("Esc policy should own bare Esc")
    }

    #[test]
    fn idle_draft_arms_then_clears_within_800ms() {
        let now = Instant::now();
        let mut policy = EscPolicy::default();
        let ctx = EscContext::idle(true);
        assert_eq!(fire(&mut policy, &esc(), ctx, now), EscOutcome::ArmedClear);
        assert_eq!(
            policy.pending_action(now),
            Some(PendingEscAction::ClearPrompt)
        );
        assert_eq!(
            fire(&mut policy, &esc(), ctx, now + Duration::from_millis(799)),
            EscOutcome::ClearPrompt
        );
    }

    #[test]
    fn expired_second_esc_rearms_instead_of_clearing() {
        let now = Instant::now();
        let mut policy = EscPolicy::default();
        let ctx = EscContext::idle(true);
        assert_eq!(fire(&mut policy, &esc(), ctx, now), EscOutcome::ArmedClear);
        assert_eq!(
            fire(&mut policy, &esc(), ctx, now + Duration::from_millis(801)),
            EscOutcome::ArmedClear
        );
    }

    #[test]
    fn idle_empty_history_uses_silent_arm_then_picker() {
        let now = Instant::now();
        let mut policy = EscPolicy::default();
        let ctx = EscContext {
            has_rewindable_turns: true,
            ..EscContext::idle(false)
        };
        assert_eq!(fire(&mut policy, &esc(), ctx, now), EscOutcome::ArmedRewind);
        assert_eq!(
            policy.pending_action(now),
            Some(PendingEscAction::ShowRewindPicker)
        );
        assert_eq!(
            fire(&mut policy, &esc(), ctx, now + Duration::from_millis(1)),
            EscOutcome::ShowRewindPicker
        );
    }

    #[test]
    fn idle_empty_without_history_and_scrollback_draft_are_swallowed() {
        let now = Instant::now();
        let mut policy = EscPolicy::default();
        assert_eq!(
            fire(&mut policy, &esc(), EscContext::idle(false), now),
            EscOutcome::Swallowed
        );
        let ctx = EscContext {
            prompt_owns_keys: false,
            ..EscContext::idle(true)
        };
        assert_eq!(fire(&mut policy, &esc(), ctx, now), EscOutcome::Swallowed);
    }

    #[test]
    fn running_and_cancel_pending_retry_and_suppress_rewind_arm() {
        let now = Instant::now();
        let mut policy = EscPolicy::default();
        let running = EscContext {
            turn_running: true,
            ..EscContext::idle(true)
        };
        assert_eq!(
            fire(&mut policy, &esc(), running, now),
            EscOutcome::CancelTurn
        );
        let idle = EscContext {
            has_rewindable_turns: true,
            ..EscContext::idle(false)
        };
        assert_eq!(
            fire(&mut policy, &esc(), idle, now + Duration::from_millis(999)),
            EscOutcome::Swallowed
        );
        assert_eq!(
            fire(
                &mut policy,
                &esc(),
                idle,
                now + Duration::from_millis(1_000)
            ),
            EscOutcome::ArmedRewind
        );

        let cancelling = EscContext {
            cancel_pending: true,
            ..EscContext::idle(false)
        };
        assert_eq!(
            fire(
                &mut policy,
                &esc(),
                cancelling,
                now + Duration::from_millis(1_001)
            ),
            EscOutcome::CancelTurn
        );
    }

    #[test]
    fn other_key_or_modified_esc_retires_pending_but_release_does_not() {
        let now = Instant::now();
        let mut policy = EscPolicy::default();
        let ctx = EscContext::idle(true);
        assert_eq!(fire(&mut policy, &esc(), ctx, now), EscOutcome::ArmedClear);
        let modified = KeyEvent::new(KeyCode::Esc, KeyModifiers::ALT);
        assert_eq!(policy.try_fire_pending_at(&modified, ctx, now), None);
        assert_eq!(policy.handle_unclaimed_at(&modified, ctx, now), None);
        assert_eq!(policy.pending_action(now), None);

        assert_eq!(fire(&mut policy, &esc(), ctx, now), EscOutcome::ArmedClear);
        let release = KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        };
        assert_eq!(policy.try_fire_pending_at(&release, ctx, now), None);
        assert_eq!(
            policy.pending_action(now),
            Some(PendingEscAction::ClearPrompt)
        );
    }

    #[test]
    fn ttl_override_parser_matches_grok_bounds() {
        assert_eq!(parse_esc_ttl(None), ESC_DOUBLE_PRESS_TTL);
        assert_eq!(parse_esc_ttl(Some("0".into())), ESC_DOUBLE_PRESS_TTL);
        assert_eq!(parse_esc_ttl(Some("bad".into())), ESC_DOUBLE_PRESS_TTL);
        assert_eq!(
            parse_esc_ttl(Some("1200".into())),
            Duration::from_millis(1_200)
        );
        assert_eq!(
            parse_esc_ttl(Some("999999".into())),
            Duration::from_millis(ESC_DOUBLE_PRESS_TEST_MS)
        );
    }
}
