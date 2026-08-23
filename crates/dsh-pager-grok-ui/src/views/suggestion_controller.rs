//! DSH-neutral suggestion/history controller derived from Grok's state rules.
//!
//! Candidates are host-owned. The controller owns only generation, filtering,
//! viewport selection and the Esc/Enter accept contract; it cannot issue RPC.

use crossterm::event::KeyCode;

use crate::host_adapter::{FeatureStatus, SuggestionSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionOutcome {
    Unhandled,
    Handled,
    Accepted,
    Dismissed,
}

#[derive(Debug, Default)]
pub struct SuggestionController {
    selected: usize,
    dismissed: bool,
    generation: u64,
}

impl SuggestionController {
    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn dismissed(&self) -> bool {
        self.dismissed
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn reset(&mut self) {
        self.selected = 0;
        self.dismissed = false;
        self.generation = self.generation.saturating_add(1);
    }

    /// Any edit invalidates old candidate selection and clears an Esc
    /// dismissal. This is the generation fence used by Grok's controller.
    pub fn text_changed(&mut self) {
        self.selected = 0;
        self.dismissed = false;
        self.generation = self.generation.saturating_add(1);
    }

    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }

    pub fn visible_items<'a>(
        &self,
        snapshot: &'a SuggestionSnapshot,
        prompt: &str,
    ) -> Option<Vec<&'a str>> {
        if self.dismissed
            || !snapshot.active
            || snapshot.status != FeatureStatus::Available
            || !prompt.starts_with('/')
        {
            return None;
        }
        let items = snapshot
            .items
            .iter()
            .filter(|item| item.starts_with(prompt))
            .map(String::as_str)
            .collect::<Vec<_>>();
        (!items.is_empty()).then_some(items)
    }

    pub fn handle_key(
        &mut self,
        key: KeyCode,
        snapshot: &SuggestionSnapshot,
        prompt: &str,
    ) -> SuggestionOutcome {
        let Some(items) = self.visible_items(snapshot, prompt) else {
            return SuggestionOutcome::Unhandled;
        };
        match key {
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                SuggestionOutcome::Handled
            }
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(items.len().saturating_sub(1));
                SuggestionOutcome::Handled
            }
            KeyCode::Tab | KeyCode::Enter => {
                self.selected = self.selected.min(items.len().saturating_sub(1));
                SuggestionOutcome::Accepted
            }
            KeyCode::Esc => {
                self.dismissed = true;
                SuggestionOutcome::Dismissed
            }
            _ => SuggestionOutcome::Unhandled,
        }
    }

    pub fn accepted_item<'a>(
        &self,
        snapshot: &'a SuggestionSnapshot,
        prompt: &str,
    ) -> Option<&'a str> {
        let items = self.visible_items(snapshot, prompt)?;
        items
            .get(self.selected.min(items.len().saturating_sub(1)))
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> SuggestionSnapshot {
        SuggestionSnapshot {
            status: FeatureStatus::Available,
            active: true,
            selected: None,
            items: vec!["/help".into(), "/history".into(), "/model".into()],
        }
    }

    #[test]
    fn controller_filters_and_accepts_stable_host_candidates() {
        let mut controller = SuggestionController::default();
        let snapshot = snapshot();
        assert_eq!(
            controller.visible_items(&snapshot, "/h"),
            Some(vec!["/help", "/history"])
        );
        assert_eq!(
            controller.handle_key(KeyCode::Down, &snapshot, "/h"),
            SuggestionOutcome::Handled
        );
        assert_eq!(controller.accepted_item(&snapshot, "/h"), Some("/history"));
        assert_eq!(
            controller.handle_key(KeyCode::Tab, &snapshot, "/h"),
            SuggestionOutcome::Accepted
        );
    }

    #[test]
    fn dismissal_is_local_until_the_next_text_generation() {
        let mut controller = SuggestionController::default();
        let snapshot = snapshot();
        assert_eq!(
            controller.handle_key(KeyCode::Esc, &snapshot, "/"),
            SuggestionOutcome::Dismissed
        );
        assert!(controller.visible_items(&snapshot, "/").is_none());
        controller.text_changed();
        assert!(controller.visible_items(&snapshot, "/").is_some());
    }

    #[test]
    fn unsupported_or_inactive_host_never_opens_dropdown() {
        let controller = SuggestionController::default();
        let mut snapshot = snapshot();
        snapshot.status = FeatureStatus::Pending;
        assert!(controller.visible_items(&snapshot, "/").is_none());
        snapshot.status = FeatureStatus::Available;
        snapshot.active = false;
        assert!(controller.visible_items(&snapshot, "/").is_none());
    }
}
