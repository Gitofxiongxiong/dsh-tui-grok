//! Shortcuts-bar hint builder extracted from Grok `views/agent.rs`.
//!
//! Derived from Grok Build at SOURCE_REV
//! `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`.
//!
//! Local structural adaptation: layout/scrollbar painting stays in
//! `src/views/agent.rs`. This file keeps Grok's `ActivePane`,
//! `prompt_focus_hint`, and `build_hints` product logic. `PromptWidget` here
//! is a DSH seam with the same method names `build_hints` calls; the full
//! Grok composer/runtime is not imported.

use crate::actions::{ActionId, ActionRegistry, When};
use crate::views::shortcuts_bar::HintItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePane {
    #[default]
    Scrollback,
    Todo,
    Queue,
    Prompt,
    Tasks,
    Catalog,
}

/// DSH composer projection for `build_hints`.
///
/// Method names match Grok's PromptWidget so the hint builder stays a
/// straight extract. `textarea.insert_str` is the grok test API.
#[derive(Debug, Clone, Default)]
pub struct PromptWidget {
    pub textarea: ComposerText,
    pub history_search: HistorySearchState,
    paste_at_cursor: bool,
    file_ref_near_cursor: bool,
    suggestion_visible: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ComposerText {
    text: String,
    cursor: usize,
}

impl ComposerText {
    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }
}

#[derive(Debug, Clone, Default)]
pub struct HistorySearchState {
    active: bool,
}

impl HistorySearchState {
    pub fn is_active(&self) -> bool {
        self.active
    }

    #[cfg(test)]
    pub fn activate(&mut self) {
        self.active = true;
    }
}

impl PromptWidget {
    pub fn from_composer(text: &str, cursor: usize, suggestion_visible: bool) -> Self {
        Self {
            textarea: ComposerText {
                text: text.to_string(),
                cursor,
            },
            suggestion_visible,
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        self.textarea.text()
    }

    pub fn can_send(&self) -> bool {
        let text = self.textarea.text();
        if text.trim().is_empty() {
            return false;
        }
        let cursor = self.textarea.cursor();
        if cursor > 0 && text.as_bytes().get(cursor - 1) == Some(&b'\\') {
            return false;
        }
        true
    }

    pub fn paste_element_at_cursor(&self) -> Option<&str> {
        self.paste_at_cursor.then_some("")
    }

    pub fn file_ref_near_cursor(&self) -> bool {
        self.file_ref_near_cursor
    }

    pub fn prompt_suggestion_visible(&self) -> bool {
        self.suggestion_visible
    }
}

/// Stub for Grok scrollback search until that controller is vendored.
#[derive(Debug, Clone, Default)]
pub struct ScrollbackSearchState {
    composing: bool,
}

impl ScrollbackSearchState {
    pub fn open() -> Self {
        Self { composing: true }
    }

    pub fn accept(&mut self) {
        self.composing = false;
    }

    pub fn is_composing(&self) -> bool {
        self.composing
    }
}

/// The scrollback's default focus hint: `Space` leaves for the prompt. A
/// parked blocking card replaces it with its own (pinned) route back.
pub fn prompt_focus_hint() -> HintItem {
    use crate::input::key::KeyShortcut;
    use crossterm::event::{KeyCode, KeyModifiers};
    HintItem {
        keys: vec![KeyShortcut::new(KeyCode::Char(' '), KeyModifiers::NONE)],
        label: "prompt".into(),
        custom_display: Some("Space"),
        description: None,
        pinned: false,
    }
}
/// Build the hints list for the shortcuts bar based on current state.
///
/// Each pane contributes its own hints dynamically. The registry provides
/// the key bindings; the view decides which ones are visible.
///
/// `fold_label` is the dynamic label for the fold action based on selected
/// entry state: "expand", "collapse", or "fold" (no foldable entry selected).
///
/// `group_header_label` ("expand"/"collapse") marks a selected group header;
/// it replaces the fold and Enter:open hints with a single Enter toggle hint.
///
/// `focus_hint` is how the scrollback says the keyboard can leave it —
/// [`prompt_focus_hint`], or a caller-supplied replacement. A pinned one
/// leads the bar and is offered once; an unpinned one is offered only in the
/// selection states where moving on is the useful next step.
#[allow(clippy::too_many_arguments)]
pub fn build_hints(
    active_pane: ActivePane,
    focus_hint: HintItem,
    prompt: &PromptWidget,
    registry: &ActionRegistry,
    is_editing_queued: bool,
    fold_label: Option<&'static str>,
    group_header_label: Option<&'static str>,
    thinking_label: &'static str,
    show_done: bool,
    selected_supports_copy: bool,
    selected_meta_label: Option<&'static str>,
    selected_supports_fullscreen: bool,
    can_demote: bool,
    selected_can_kill: bool,
    multiline_mode: bool,
    vim_mode: bool,
    is_subagent_view: bool,
    is_turn_running: bool,
    esc_would_cancel_turn: bool,
    has_queued_follow_up: bool,
    selected_is_user_prompt: bool,
    selected_is_agent_message: bool,
    selected_is_credit_limit: bool,
    shift_enter_unavailable: bool,
    scrollback_search: Option<&ScrollbackSearchState>,
) -> Vec<HintItem> {
    let mut hints = match active_pane {
        ActivePane::Todo => {
            let mut hints = Vec::new();
            hints.push(HintItem::new(
                crate::key!('h'),
                if show_done { "hide done" } else { "show done" },
            ));
            hints
        }
        ActivePane::Queue => {
            let mut hints = vec![
                HintItem::new(crate::key!('x'), "delete row"),
                HintItem::new(crate::key!('e'), "edit"),
                HintItem::paired(crate::key!('J'), crate::key!('K'), "reorder"),
                HintItem::new(crate::key!('y'), "copy"),
            ];
            if is_turn_running && let Some(def) = registry.find(ActionId::InterjectPrompt) {
                hints.push(def.hint());
            }
            hints
        }
        ActivePane::Prompt if is_editing_queued => {
            let mut hints = Vec::new();
            if prompt.can_send() {
                hints.push(HintItem::new(crate::key!(Enter), "save"));
            }
            hints.push(HintItem::new(crate::key!(Esc), "cancel"));
            hints
        }
        ActivePane::Prompt if prompt.history_search.is_active() => {
            use crate::input::key::KeyShortcut;
            use crossterm::event::KeyCode;
            vec![
                HintItem::paired(
                    KeyShortcut::key(KeyCode::Up),
                    KeyShortcut::key(KeyCode::Down),
                    "nav",
                ),
                HintItem::paired(
                    KeyShortcut::key(KeyCode::PageUp),
                    KeyShortcut::key(KeyCode::PageDown),
                    "page",
                ),
                HintItem::new(KeyShortcut::key(KeyCode::Enter), "select"),
                HintItem::new(KeyShortcut::key(KeyCode::Esc), "cancel"),
            ]
        }
        ActivePane::Prompt => {
            let mut hints = Vec::new();
            let newline_key = if shift_enter_unavailable {
                crate::key!(Enter, ALT)
            } else {
                crate::key!(Enter, SHIFT)
            };
            let submit_label = if is_turn_running { "queue" } else { "send" };
            if let Some(key) = registry.key_for(ActionId::SendPrompt) {
                if prompt.paste_element_at_cursor().is_some() {
                    hints.push(HintItem::new(key, "expand"));
                } else if multiline_mode && prompt.can_send() {
                    hints.push(HintItem::new(newline_key, submit_label));
                } else if prompt.can_send() {
                    hints.push(HintItem::new(key, submit_label));
                } else if is_turn_running && has_queued_follow_up {
                    hints.push(HintItem::new(key, "send now"));
                }
            }
            if shift_enter_unavailable && !multiline_mode && prompt.can_send() {
                hints.push(HintItem::new(crate::key!(Enter, ALT), "newline"));
            }
            if prompt.file_ref_near_cursor() {
                hints.push(HintItem::new(crate::key!(':'), "lines"));
            }
            if prompt.prompt_suggestion_visible() {
                hints.push(
                    HintItem::paired(crate::key!(Tab), crate::key!(Right), "accept suggestion")
                        .pinned(),
                );
            }
            hints.push(HintItem::new(crate::key!(BackTab), "mode"));
            for def in registry.hints(&[When::PromptFocused, When::AgentScreen, When::Always]) {
                if def.id == ActionId::SendPrompt
                    || def.id == ActionId::CommandPalette
                    || def.id == ActionId::Quit
                {
                    continue;
                }
                if def.id == ActionId::EnableVoiceMode || def.id == ActionId::VoiceToggle {
                    continue;
                }
                hints.push(def.hint());
            }
            hints
        }
        ActivePane::Tasks => {
            let mut hints = Vec::new();
            if selected_supports_fullscreen {
                hints.push(HintItem::new(crate::key!(Enter), "view"));
            }
            if selected_supports_copy {
                hints.push(HintItem::new(crate::key!('y'), "copy output"));
            }
            if selected_can_kill {
                hints.push(HintItem::new(crate::key!('x'), "kill"));
            }
            hints.push(HintItem::new(
                crate::key!('h'),
                if show_done { "hide done" } else { "show done" },
            ));
            hints
        }
        ActivePane::Catalog => vec![],
        ActivePane::Scrollback if scrollback_search.is_some() => {
            let mut hints = Vec::new();
            if vim_mode {
                if scrollback_search.is_some_and(|s| s.is_composing()) {
                    hints.push(HintItem::new(crate::key!(Enter), "go"));
                } else {
                    hints.push(HintItem::paired(
                        crate::key!('n'),
                        crate::key!('N'),
                        "next/prev",
                    ));
                }
            } else {
                use crate::input::key::KeyShortcut;
                use crossterm::event::KeyCode;
                hints.push(HintItem::paired(
                    KeyShortcut::key(KeyCode::Down),
                    KeyShortcut::key(KeyCode::Up),
                    "next/prev",
                ));
            }
            hints.push(HintItem::new(crate::key!(Esc), "cancel"));
            hints
        }
        ActivePane::Scrollback => {
            let mut hints = Vec::new();
            if focus_hint.pinned {
                hints.push(focus_hint.clone());
            }
            let offer_focus_hint = |hints: &mut Vec<HintItem>| {
                if !focus_hint.pinned {
                    hints.push(focus_hint.clone());
                }
            };
            let nothing_special = !selected_is_agent_message
                && !selected_is_user_prompt
                && !selected_is_credit_limit
                && fold_label.is_none()
                && group_header_label.is_none()
                && !selected_supports_fullscreen;
            if nothing_special {
                offer_focus_hint(&mut hints);
            }
            if selected_is_credit_limit {
                if let Some(key) = registry.key_for(ActionId::OpenBlockViewer) {
                    hints.push(HintItem::new(key, "open"));
                }
                offer_focus_hint(&mut hints);
            }
            if selected_is_agent_message {
                if vim_mode
                    && selected_supports_copy
                    && let Some(key) = registry.key_for(ActionId::CopyBlockContent)
                {
                    hints.push(HintItem::new(key, "copy"));
                }
                offer_focus_hint(&mut hints);
            }
            if selected_is_user_prompt {
                let user_collapsed = fold_label == Some("expand");
                if user_collapsed {
                    let key = registry
                        .key_for_mode(ActionId::ToggleFold, vim_mode)
                        .or_else(|| registry.key_for_mode(ActionId::Expand, vim_mode));
                    if let Some(key) = key {
                        hints.push(HintItem::new(key, "expand"));
                    }
                }
                if let Some(key) = registry.key_for(ActionId::ExpandAllThinking) {
                    hints.push(HintItem::new(key, thinking_label));
                }
                if !user_collapsed {
                    offer_focus_hint(&mut hints);
                }
            }
            let user_collapsed_already_pushed =
                selected_is_user_prompt && fold_label == Some("expand");
            if let Some(label) = group_header_label {
                if let Some(key) = registry.key_for(ActionId::OpenBlockViewer) {
                    hints.push(HintItem::new(key, label));
                }
            } else if !user_collapsed_already_pushed && let Some(label) = fold_label {
                let directional = if label == "expand" {
                    ActionId::Expand
                } else {
                    ActionId::Collapse
                };
                let key = registry
                    .key_for_mode(ActionId::ToggleFold, vim_mode)
                    .or_else(|| registry.key_for_mode(directional, vim_mode));
                if let Some(key) = key {
                    hints.push(HintItem::new(key, label));
                }
            }
            if group_header_label.is_none()
                && selected_supports_fullscreen
                && let Some(key) = registry.key_for(ActionId::OpenBlockViewer)
            {
                hints.push(HintItem::new(key, "open"));
            }
            if vim_mode
                && let (Some(j), Some(k)) = (
                    registry.key_for(ActionId::SelectNext),
                    registry.key_for(ActionId::SelectPrev),
                )
            {
                hints.push(HintItem::paired(j, k, "nav").pinned());
            }
            if vim_mode
                && let (Some(h), Some(l)) = (
                    registry.key_for(ActionId::PrevTurn),
                    registry.key_for(ActionId::NextTurn),
                )
            {
                hints.push(HintItem::paired(l, h, "turn").pinned());
            }
            if !selected_is_user_prompt
                && let Some(key) = registry.key_for(ActionId::ExpandAllThinking)
            {
                hints.push(HintItem::new(key, thinking_label));
            }
            if vim_mode
                && let (Some(g), Some(bg)) = (
                    registry.key_for(ActionId::GotoTop),
                    registry.key_for(ActionId::GotoBottom),
                )
            {
                hints.push(HintItem::paired(g, bg, "top/btm"));
            }
            if vim_mode
                && !selected_is_agent_message
                && selected_supports_copy
                && let Some(key) = registry.key_for(ActionId::CopyBlockContent)
            {
                hints.push(HintItem::new(key, "copy"));
            }
            if vim_mode
                && let Some(label) = selected_meta_label
                && let Some(key) = registry.key_for(ActionId::CopyBlockMeta)
            {
                hints.push(HintItem::new(key, label));
            }
            if selected_can_kill {
                hints.push(HintItem::new(crate::key!('x'), "kill"));
            }
            if is_subagent_view {
                hints.push(HintItem::paired(crate::key!('q'), crate::key!(Esc), "back"));
            }
            hints
        }
    };
    if is_turn_running && let Some(def) = registry.find(ActionId::CancelTurn) {
        let mut hint = def.hint();
        if esc_would_cancel_turn {
            hint.keys = vec![crate::key!(Esc)];
        }
        hints.push(hint);
    }
    let has_composer_payload = !prompt.text().trim().is_empty() || is_editing_queued;
    if matches!(active_pane, ActivePane::Prompt)
        && ActionRegistry::interjection_possible(is_turn_running, has_composer_payload)
        && let Some(def) = registry.find(ActionId::InterjectPrompt)
    {
        hints.push(def.hint());
    }
    if can_demote
        && !is_subagent_view
        && let Some(key) = registry.key_for(ActionId::SendToBackground)
    {
        hints.push(HintItem::new(key, "send to bg"));
    }
    hints
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionRegistry;
    /// Convenience: build hints for the Scrollback pane with sensible defaults.
    /// Override only what each test cares about.
    #[allow(clippy::too_many_arguments)]
    fn scrollback_hints(
        registry: &ActionRegistry,
        fold_label: Option<&'static str>,
        selected_supports_copy: bool,
        selected_supports_fullscreen: bool,
        selected_is_user_prompt: bool,
        selected_is_agent_message: bool,
    ) -> Vec<HintItem> {
        scrollback_hints_with_vim_mode(
            registry,
            fold_label,
            selected_supports_copy,
            selected_supports_fullscreen,
            selected_is_user_prompt,
            selected_is_agent_message,
            true,
        )
    }
    fn scrollback_hints_with_vim_mode(
        registry: &ActionRegistry,
        fold_label: Option<&'static str>,
        selected_supports_copy: bool,
        selected_supports_fullscreen: bool,
        selected_is_user_prompt: bool,
        selected_is_agent_message: bool,
        vim_mode: bool,
    ) -> Vec<HintItem> {
        build_hints(
            ActivePane::Scrollback,
            prompt_focus_hint(),
            &PromptWidget::default(),
            registry,
            false,
            fold_label,
            None,
            "expand thinking",
            false,
            selected_supports_copy,
            None,
            selected_supports_fullscreen,
            false,
            false,
            false,
            vim_mode,
            false,
            false,
            false,
            false,
            selected_is_user_prompt,
            selected_is_agent_message,
            false,
            false,
            None,
        )
    }
    fn first_two_labels(hints: &[HintItem]) -> Vec<&str> {
        hints.iter().take(2).map(|h| h.label.as_ref()).collect()
    }
    #[test]
    fn demotion_hint_uses_registered_ctrl_b_binding() {
        let registry = ActionRegistry::defaults();
        let hints = build_hints(
            ActivePane::Scrollback,
            prompt_focus_hint(),
            &PromptWidget::default(),
            &registry,
            false,
            None,
            None,
            "expand thinking",
            false,
            false,
            None,
            false,
            true,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
        );
        let hint = hints
            .iter()
            .find(|hint| hint.label == "send to bg")
            .expect("running Execute should advertise demotion");
        assert_eq!(hint.keys, vec![crate::key!('b', CONTROL)]);
    }
    #[test]
    fn group_header_shows_enter_toggle_hint_instead_of_open_and_fold() {
        let registry = ActionRegistry::defaults();
        let hints = build_hints(
            ActivePane::Scrollback,
            prompt_focus_hint(),
            &PromptWidget::default(),
            &registry,
            false,
            Some("expand"),
            Some("expand"),
            "expand thinking",
            false,
            false,
            None,
            true,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
        );
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            !labels.contains(&"open"),
            "no Enter:open hint on a group header, got {labels:?}"
        );
        let expand_hints: Vec<&HintItem> = hints.iter().filter(|h| h.label == "expand").collect();
        assert_eq!(
            expand_hints.len(),
            1,
            "exactly one expand hint (Enter toggle, no separate fold hint), got {labels:?}"
        );
        let enter_key = registry
            .key_for(crate::actions::ActionId::OpenBlockViewer)
            .expect("OpenBlockViewer has a default key");
        assert_eq!(
            expand_hints[0].keys,
            vec![enter_key],
            "the header expand hint must be bound to Enter (OpenBlockViewer)"
        );
        assert_eq!(
            labels.first(),
            Some(&"expand"),
            "the Enter toggle takes the first compact slot, got {labels:?}"
        );
    }
    #[test]
    fn scrollback_user_prompt_collapsed_hoists_expand_then_thinking() {
        let registry = ActionRegistry::defaults();
        let hints = scrollback_hints(&registry, Some("expand"), false, false, true, false);
        assert_eq!(first_two_labels(&hints), vec!["expand", "expand thinking"]);
    }
    #[test]
    fn scrollback_user_prompt_expanded_hoists_thinking_then_space() {
        let registry = ActionRegistry::defaults();
        let hints = scrollback_hints(&registry, Some("collapse"), false, false, true, false);
        assert_eq!(first_two_labels(&hints), vec!["expand thinking", "prompt"]);
    }
    #[test]
    fn scrollback_agent_message_hoists_copy_then_space() {
        let registry = ActionRegistry::defaults();
        let hints = scrollback_hints(&registry, None, true, false, false, true);
        assert_eq!(first_two_labels(&hints), vec!["copy", "prompt"]);
    }
    #[test]
    fn scrollback_agent_message_no_duplicate_y_copy_in_full_list() {
        let registry = ActionRegistry::defaults();
        let hints = scrollback_hints(&registry, None, true, false, false, true);
        let copy_count = hints.iter().filter(|h| h.label == "copy").count();
        assert_eq!(copy_count, 1, "copy should appear exactly once");
    }
    #[test]
    fn scrollback_default_block_shows_prompt_first() {
        let registry = ActionRegistry::defaults();
        let hints = scrollback_hints(&registry, None, false, false, false, false);
        assert_eq!(first_two_labels(&hints), vec!["prompt", "nav"]);
    }
    #[test]
    fn scrollback_foldable_non_user_block_hoists_fold_then_open() {
        let registry = ActionRegistry::defaults();
        let hints = scrollback_hints(&registry, Some("fold"), false, true, false, false);
        assert_eq!(first_two_labels(&hints), vec!["fold", "open"]);
    }
    #[test]
    fn scrollback_vim_mode_on_shows_nav_and_turn_hints() {
        let registry = ActionRegistry::defaults();
        let hints =
            scrollback_hints_with_vim_mode(&registry, None, false, false, false, false, true);
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            labels.contains(&"nav"),
            "vim mode should show j/k nav hint; got {labels:?}"
        );
        assert!(
            labels.contains(&"turn"),
            "vim mode should show Shift+l/h turn hint; got {labels:?}"
        );
        assert!(
            labels.contains(&"top/btm"),
            "vim mode should show g/G top/btm hint; got {labels:?}"
        );
    }
    #[test]
    fn scrollback_vim_mode_off_hides_nav_turn_and_topbtm() {
        let registry = ActionRegistry::defaults();
        let hints =
            scrollback_hints_with_vim_mode(&registry, None, false, false, false, false, false);
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            !labels.contains(&"nav"),
            "vim-off must hide j/k nav hint; got {labels:?}"
        );
        assert!(
            !labels.contains(&"turn"),
            "vim-off must hide Shift+l/h turn hint; got {labels:?}"
        );
        assert!(
            !labels.contains(&"top/btm"),
            "vim-off must hide g/G top/btm hint; got {labels:?}"
        );
    }
    #[test]
    fn scrollback_vim_mode_off_hides_y_copy_on_agent_message() {
        let registry = ActionRegistry::defaults();
        let hints =
            scrollback_hints_with_vim_mode(&registry, None, true, false, false, true, false);
        assert!(
            !hints.iter().any(|h| h.label == "copy"),
            "vim-off must hide y:copy hint on agent message"
        );
    }
    #[test]
    fn scrollback_never_shows_rewind_hint_on_user_prompt() {
        let registry = ActionRegistry::defaults();
        for vim_mode in [true, false] {
            let hints = scrollback_hints_with_vim_mode(
                &registry,
                Some("expand"),
                false,
                false,
                true,
                false,
                vim_mode,
            );
            assert!(
                !hints.iter().any(|h| h.label == "rewind"),
                "rewind is slash-command only (vim_mode={vim_mode})"
            );
        }
    }
    /// Build scrollback hints with an open search session in the given phase.
    fn scrollback_search_hints(
        registry: &ActionRegistry,
        vim_mode: bool,
        composing: bool,
    ) -> Vec<HintItem> {
        let mut search = ScrollbackSearchState::open();
        if !composing {
            search.accept();
        }
        build_hints(
            ActivePane::Scrollback,
            prompt_focus_hint(),
            &PromptWidget::default(),
            registry,
            false,
            None,
            None,
            "expand thinking",
            false,
            false,
            None,
            false,
            false,
            false,
            false,
            vim_mode,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            Some(&search),
        )
    }
    #[test]
    fn scrollback_search_hint_not_in_bottom_bar() {
        let registry = ActionRegistry::defaults();
        for vim_mode in [true, false] {
            let hints = scrollback_hints_with_vim_mode(
                &registry, None, false, false, false, false, vim_mode,
            );
            assert!(
                !hints.iter().any(|h| h.label == "search"),
                "bottom bar must not advertise / search (vim_mode={vim_mode})"
            );
        }
    }
    #[test]
    fn scrollback_search_composing_shows_go_and_cancel_only() {
        let registry = ActionRegistry::defaults();
        let labels: Vec<String> = scrollback_search_hints(&registry, true, true)
            .iter()
            .map(|h| h.label.to_string())
            .collect();
        assert!(labels.contains(&"go".to_string()), "got {labels:?}");
        assert!(labels.contains(&"cancel".to_string()), "got {labels:?}");
        assert!(!labels.contains(&"next/prev".to_string()), "got {labels:?}");
        assert!(!labels.contains(&"nav".to_string()), "got {labels:?}");
        assert!(!labels.contains(&"search".to_string()), "got {labels:?}");
    }
    #[test]
    fn scrollback_search_browsing_shows_next_prev_and_cancel() {
        let registry = ActionRegistry::defaults();
        let labels: Vec<String> = scrollback_search_hints(&registry, true, false)
            .iter()
            .map(|h| h.label.to_string())
            .collect();
        assert!(labels.contains(&"next/prev".to_string()), "got {labels:?}");
        assert!(labels.contains(&"cancel".to_string()), "got {labels:?}");
        assert!(!labels.contains(&"go".to_string()), "got {labels:?}");
        assert!(!labels.contains(&"nav".to_string()), "got {labels:?}");
    }
    /// The keys behind the `next/prev` hint for a search in the given phase.
    fn next_prev_keys(
        registry: &ActionRegistry,
        vim_mode: bool,
        composing: bool,
    ) -> Vec<crate::input::key::KeyShortcut> {
        scrollback_search_hints(registry, vim_mode, composing)
            .into_iter()
            .find(|h| h.label == "next/prev")
            .map(|h| h.keys)
            .unwrap_or_default()
    }
    #[test]
    fn scrollback_search_vim_browsing_uses_n_keys() {
        use crossterm::event::KeyCode;
        let registry = ActionRegistry::defaults();
        let keys = next_prev_keys(&registry, true, false);
        let codes: Vec<KeyCode> = keys.iter().map(|k| k.code).collect();
        assert_eq!(codes, vec![KeyCode::Char('n'), KeyCode::Char('N')]);
    }
    #[test]
    fn scrollback_search_simple_mode_uses_arrow_keys_in_both_phases() {
        use crossterm::event::KeyCode;
        let registry = ActionRegistry::defaults();
        for composing in [true, false] {
            let codes: Vec<KeyCode> = next_prev_keys(&registry, false, composing)
                .iter()
                .map(|k| k.code)
                .collect();
            assert_eq!(
                codes,
                vec![KeyCode::Down, KeyCode::Up],
                "simple mode next/prev should be arrows (composing={composing})"
            );
        }
    }
    #[test]
    fn prompt_branch_excludes_exit_session_home_hint() {
        let registry = ActionRegistry::defaults();
        let hints = build_hints(
            ActivePane::Prompt,
            prompt_focus_hint(),
            &PromptWidget::default(),
            &registry,
            false,
            None,
            None,
            "expand thinking",
            false,
            false,
            None,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
        );
        assert!(
            !hints.iter().any(|h| h.label == "home"),
            "ExitSession (home) must not appear in prompt-focused bar"
        );
    }
    fn prompt_hints_with_text(
        multiline_mode: bool,
        shift_enter_unavailable: bool,
    ) -> Vec<HintItem> {
        prompt_hints_with_text_and_turn(multiline_mode, shift_enter_unavailable, false)
    }
    fn prompt_hints_with_text_and_turn(
        multiline_mode: bool,
        shift_enter_unavailable: bool,
        is_turn_running: bool,
    ) -> Vec<HintItem> {
        let mut prompt = PromptWidget::default();
        prompt.textarea.insert_str("hello");
        let registry = ActionRegistry::defaults();
        build_hints(
            ActivePane::Prompt,
            prompt_focus_hint(),
            &prompt,
            &registry,
            false,
            None,
            None,
            "expand thinking",
            false,
            false,
            None,
            false,
            false,
            false,
            multiline_mode,
            true,
            false,
            is_turn_running,
            false,
            false,
            false,
            false,
            false,
            shift_enter_unavailable,
            None,
        )
    }
    #[test]
    fn prompt_idle_submit_hint_is_send() {
        let hints = prompt_hints_with_text_and_turn(false, false, false);
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            labels.contains(&"send") && !labels.contains(&"queue"),
            "idle prompt must advertise Enter:send; got {labels:?}"
        );
    }
    #[test]
    fn prompt_running_submit_hint_is_queue_and_send_now() {
        let hints = prompt_hints_with_text_and_turn(false, false, true);
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            labels.contains(&"queue"),
            "mid-turn follow-up must advertise Enter:queue (not send); got {labels:?}"
        );
        assert!(
            !labels.contains(&"send"),
            "mid-turn must not mislabel Enter as send; got {labels:?}"
        );
        assert!(
            labels.contains(&"send now"),
            "mid-turn with composer text must advertise the send-now (interject) chord; got {labels:?}"
        );
    }
    /// Empty composer + mid-turn queue: bare Enter is send-now in both normal
    /// and multiline modes (multiline only inserts newline when there is text).
    #[test]
    fn prompt_empty_mid_turn_queue_advertises_send_now_including_multiline() {
        for multiline in [false, true] {
            let prompt = PromptWidget::default();
            let registry = ActionRegistry::defaults();
            let hints = build_hints(
                ActivePane::Prompt,
                prompt_focus_hint(),
                &prompt,
                &registry,
                false,
                None,
                None,
                "expand thinking",
                false,
                false,
                None,
                false,
                false,
                false,
                multiline,
                true,
                false,
                true,
                false,
                true,
                false,
                false,
                false,
                false,
                None,
            );
            let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
            assert!(
                labels.contains(&"send now"),
                "empty composer mid-turn with queue must advertise Enter:send now \
                 (multiline={multiline}); got {labels:?}"
            );
        }
    }
    /// Running-turn cancel hint key tracks `esc_would_cancel_turn` — the
    /// input-routing predicate computed by the caller: Esc when a bare press
    /// would reach the policy's mid-turn cancel, the registry Ctrl+C binding
    /// otherwise. (The predicate itself — gate, panes, and higher-priority
    /// Esc consumers — is pinned by `esc_would_cancel_turn_tests` in
    /// `agent_view::input`.)
    #[test]
    fn running_turn_cancel_hint_key_tracks_esc_predicate() {
        let prompt = PromptWidget::default();
        let registry = ActionRegistry::defaults();
        for (esc_would_cancel_turn, expected) in
            [(true, crate::key!(Esc)), (false, crate::key!('c', CONTROL))]
        {
            let hints = build_hints(
                ActivePane::Prompt,
                prompt_focus_hint(),
                &prompt,
                &registry,
                false,
                None,
                None,
                "expand thinking",
                false,
                false,
                None,
                false,
                false,
                false,
                false,
                true,
                false,
                true,
                esc_would_cancel_turn,
                false,
                false,
                false,
                false,
                false,
                None,
            );
            let cancel = hints
                .iter()
                .find(|h| h.label == "cancel")
                .expect("running turn must surface the cancel hint");
            assert_eq!(
                cancel.keys,
                vec![expected],
                "cancel hint key for esc_would_cancel_turn={esc_would_cancel_turn}"
            );
        }
    }
    /// Running turn + open scrollback search: the search's own `Esc cancel`
    /// hint stays the ONLY Esc hint — the CancelTurn hint keeps Ctrl+C (the
    /// caller's predicate is false while the search would steal Esc), so the
    /// bar never shows two different `Esc cancel` meanings at once.
    #[test]
    fn running_turn_with_scrollback_search_keeps_ctrl_c_cancel_hint() {
        let registry = ActionRegistry::defaults();
        let search = ScrollbackSearchState::open();
        let hints = build_hints(
            ActivePane::Scrollback,
            prompt_focus_hint(),
            &PromptWidget::default(),
            &registry,
            false,
            None,
            None,
            "expand thinking",
            false,
            false,
            None,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            Some(&search),
        );
        let esc_cancels: Vec<&HintItem> = hints
            .iter()
            .filter(|h| h.label == "cancel" && h.keys == vec![crate::key!(Esc)])
            .collect();
        assert_eq!(
            esc_cancels.len(),
            1,
            "exactly one Esc:cancel hint (the search's own dismiss)"
        );
        assert!(
            hints
                .iter()
                .any(|h| h.label == "cancel" && h.keys == vec![crate::key!('c', CONTROL)]),
            "CancelTurn hint must stay on Ctrl+C while the search owns Esc"
        );
    }
    /// Running turn + editing a queued prompt: the edit's own `Esc cancel`
    /// (discard) hint is the ONLY Esc-keyed row — the CancelTurn hint keeps
    /// Ctrl+C (the caller's predicate is false while the edit owns Esc), so
    /// the bar never shows two contradictory `Esc cancel` rows.
    #[test]
    fn running_turn_editing_queued_keeps_ctrl_c_cancel_hint() {
        let registry = ActionRegistry::defaults();
        let mut prompt = PromptWidget::default();
        prompt.textarea.insert_str("edited row");
        let hints = build_hints(
            ActivePane::Prompt,
            prompt_focus_hint(),
            &prompt,
            &registry,
            true,
            None,
            None,
            "expand thinking",
            false,
            false,
            None,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
        );
        let esc_rows: Vec<&HintItem> = hints
            .iter()
            .filter(|h| h.keys.contains(&crate::key!(Esc)))
            .collect();
        assert_eq!(
            esc_rows.len(),
            1,
            "exactly one Esc-keyed hint (the edit's discard), got {:?}",
            hints.iter().map(|h| h.label.as_ref()).collect::<Vec<_>>()
        );
        assert_eq!(esc_rows[0].label, "cancel");
        assert!(
            hints
                .iter()
                .any(|h| h.label == "cancel" && h.keys == vec![crate::key!('c', CONTROL)]),
            "CancelTurn hint must stay on Ctrl+C while the edit owns Esc"
        );
    }
    #[test]
    fn prompt_legacy_vte_adds_alt_enter_newline_hint() {
        let hints = prompt_hints_with_text(false, true);
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            labels.contains(&"newline"),
            "legacy VTE must surface an explicit Alt+Enter newline hint; \
             got {labels:?}"
        );
    }
    #[test]
    fn prompt_modern_terminal_no_newline_hint() {
        let hints = prompt_hints_with_text(false, false);
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            !labels.contains(&"newline"),
            "modern terminals must not show the legacy-VTE newline hint \
             (Shift+Enter works natively); got {labels:?}"
        );
    }
    #[test]
    fn prompt_multiline_mode_no_extra_newline_hint() {
        let hints = prompt_hints_with_text(true, true);
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            !labels.contains(&"newline"),
            "multiline mode must not show the legacy-VTE newline hint \
             (Enter already inserts a newline); got {labels:?}"
        );
    }
    #[test]
    fn prompt_multiline_send_hint_uses_alt_on_legacy_vte() {
        use crossterm::event::KeyModifiers;
        let hints = prompt_hints_with_text(true, true);
        let send_hint = hints
            .iter()
            .find(|h| h.label == "send")
            .expect("multiline mode with text must show a send hint");
        let key = send_hint
            .keys
            .first()
            .expect("send hint must have at least one key");
        assert!(
            key.modifiers.contains(KeyModifiers::ALT),
            "legacy VTE multiline send hint must advertise Alt+Enter, \
             got modifiers {:?}",
            key.modifiers
        );
        assert!(
            !key.modifiers.contains(KeyModifiers::SHIFT),
            "legacy VTE multiline send hint must NOT advertise Shift+Enter, \
             got modifiers {:?}",
            key.modifiers
        );
    }
    #[test]
    fn prompt_multiline_send_hint_uses_shift_on_modern_terminal() {
        use crossterm::event::KeyModifiers;
        let hints = prompt_hints_with_text(true, false);
        let send_hint = hints
            .iter()
            .find(|h| h.label == "send")
            .expect("multiline mode with text must show a send hint");
        let key = send_hint
            .keys
            .first()
            .expect("send hint must have at least one key");
        assert!(
            key.modifiers.contains(KeyModifiers::SHIFT),
            "modern terminal multiline send hint must advertise Shift+Enter, \
             got modifiers {:?}",
            key.modifiers
        );
    }

    fn pane_compact_labels(hints: &[HintItem]) -> Vec<String> {
        use crate::views::shortcuts_bar::{CompactConfig, compute_effective_hints};
        let registry = ActionRegistry::defaults();
        let help = registry
            .find(crate::actions::ActionId::ShortcutsHelp)
            .map(|def| def.hint())
            .expect("ShortcutsHelp");
        let cfg = CompactConfig {
            max_visible: 5,
            help_hint: Some(help),
        };
        compute_effective_hints(hints, Some(&cfg))
            .into_iter()
            .map(|h| h.label.to_string())
            .collect()
    }
    #[test]
    fn prompt_empty_compact_bar_shows_mode_and_shortcuts() {
        let registry = ActionRegistry::defaults();
        let hints = build_hints(
            ActivePane::Prompt,
            prompt_focus_hint(),
            &PromptWidget::default(),
            &registry,
            false,
            None,
            None,
            "expand thinking",
            false,
            false,
            None,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
        );
        let labels: Vec<&str> = hints.iter().map(|h| h.label.as_ref()).collect();
        assert!(
            !labels.contains(&"send"),
            "empty composer must not advertise Enter:send; got {labels:?}"
        );
        assert_eq!(
            labels,
            vec!["mode"],
            "idle empty prompt bar body is mode-only; got {labels:?}"
        );
        let painted = pane_compact_labels(&hints);
        assert_eq!(
            painted,
            vec!["mode".to_string(), "shortcuts".to_string()],
            "Grok compact(5, help) paints two chips when the composer is empty; got {painted:?}"
        );
    }
    #[test]
    fn prompt_filled_legacy_vte_compact_bar_shows_four_chips() {
        let hints = prompt_hints_with_text(false, true);
        let painted = pane_compact_labels(&hints);
        assert_eq!(
            painted,
            vec![
                "send".to_string(),
                "newline".to_string(),
                "mode".to_string(),
                "shortcuts".to_string()
            ],
            "Grok compact(5, help) paints four chips with text on Shift+Enter-unavailable hosts; got {painted:?}"
        );
    }
}
