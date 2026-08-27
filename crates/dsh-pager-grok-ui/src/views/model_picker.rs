//! `/model` ArgPicker — Grok's type-to-find model + chained effort menu.
//!
//! Copied from Grok `app/modals.rs` ArgPicker handling (`handle_arg_picker_input`,
//! `try_arg_picker_step_back_from_effort`, ArgPicker render). Chrome is the
//! vendored picker + modal_window; catalog rows come from `ModelCommand`.

use crossterm::event::{Event, MouseEvent};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::model_state::ModelState;
use crate::slash::{ArgItem, ModelCommand};
use crate::theme::Theme;
use crate::views::modal_window::{
    ModalSizing, ModalWindowConfig, ModalWindowOutcome, Shortcut, handle_modal_mouse,
    render_modal_window,
};
use crate::views::picker::{
    PickerConfig, PickerEntry, PickerHitAreas, PickerOutcome, PickerRow, PickerState,
    handle_picker_input, picker_shortcuts, render_picker_in_modal,
};

const MODEL: ModelCommand = ModelCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelPickerOutcome {
    Closed,
    /// Synthesize `/model <insert_text>` like Grok `SendSlashCommandPreservingDraft`.
    Submit(String),
    Changed,
    Unchanged,
}

#[derive(Debug)]
pub struct ModelPickerState {
    picker: PickerState,
    window: crate::modal_window_state::ModalWindowState,
    command: String,
    args_query: String,
    items: Vec<ArgItem>,
    original_items: Vec<ArgItem>,
    loading: bool,
    error: Option<String>,
    list_revision: u64,
}

impl Default for ModelPickerState {
    fn default() -> Self {
        Self {
            picker: PickerState::input_active(),
            window: crate::modal_window_state::ModalWindowState::new(),
            command: "model".into(),
            args_query: String::new(),
            items: Vec::new(),
            original_items: Vec::new(),
            loading: false,
            error: None,
            list_revision: 0,
        }
    }
}

impl ModelPickerState {
    pub fn open(&mut self, models: &ModelState) -> u64 {
        self.list_revision = self.list_revision.saturating_add(1);
        self.picker = PickerState::input_active();
        self.window = crate::modal_window_state::ModalWindowState::new();
        self.command = "model".into();
        self.args_query.clear();
        self.error = None;
        if models.is_empty() {
            self.loading = true;
            self.items.clear();
            self.original_items.clear();
        } else {
            self.loading = false;
            self.apply_models(models);
        }
        self.list_revision
    }

    pub fn close(&mut self) {
        self.loading = false;
        self.error = None;
        self.items.clear();
        self.original_items.clear();
        self.args_query.clear();
    }

    pub fn apply_models(&mut self, models: &ModelState) -> bool {
        let Some(items) = MODEL.suggest_args(models, &self.args_query) else {
            if models.is_empty() {
                self.loading = true;
                return true;
            }
            self.loading = false;
            self.error = Some("No models available".into());
            self.items.clear();
            self.original_items.clear();
            return true;
        };
        self.loading = false;
        self.error = None;
        self.original_items = items;
        self.refilter();
        true
    }

    pub fn apply_catalog(&mut self, revision: u64, models: &ModelState) -> bool {
        if revision != self.list_revision {
            return false;
        }
        self.apply_models(models)
    }

    pub fn fail_entries(&mut self, revision: u64, message: impl Into<String>) -> bool {
        if revision != self.list_revision {
            return false;
        }
        self.loading = false;
        self.error = Some(message.into());
        true
    }

    /// `suggest_args` falls back to model rows when the query is not in effort
    /// phase. Model-phase reasoning rows use a trailing space in `insert_text`;
    /// effort rows do not. Require a non-empty list with no trailing-space
    /// rows before treating the picker as effort phase.
    fn arg_items_look_like_effort_phase(items: &[ArgItem]) -> bool {
        !items.is_empty()
            && items
                .iter()
                .all(|item| !item.insert_text.ends_with(char::is_whitespace))
    }

    fn in_effort_phase(&self) -> bool {
        !self.args_query.is_empty()
    }

    fn try_step_back_from_effort(&mut self, models: &ModelState) -> bool {
        if self.args_query.is_empty() || !matches!(self.command.as_str(), "model" | "m") {
            return false;
        }
        let Some(model_items) = MODEL.suggest_args(models, "") else {
            return false;
        };
        if model_items.is_empty() {
            return false;
        }
        self.args_query.clear();
        self.original_items = model_items;
        self.picker = PickerState::input_active();
        self.refilter();
        true
    }

    fn refilter(&mut self) {
        let q = self.picker.query().to_lowercase();
        self.items = self
            .original_items
            .iter()
            .filter(|item| {
                q.is_empty()
                    || item.match_text.to_lowercase().contains(&q)
                    || item.display.to_lowercase().contains(&q)
                    || item.description.to_lowercase().contains(&q)
            })
            .cloned()
            .collect();
        self.picker.selected = self.picker.selected.min(self.items.len().saturating_sub(1));
    }

    pub fn handle_event(&mut self, event: Event, models: &ModelState) -> ModelPickerOutcome {
        if let Event::Mouse(MouseEvent {
            kind, column, row, ..
        }) = &event
        {
            match handle_modal_mouse(&mut self.window, *kind, *column, *row) {
                ModalWindowOutcome::CloseRequested => return ModelPickerOutcome::Closed,
                ModalWindowOutcome::ShortcutActivated(2) => return ModelPickerOutcome::Closed,
                ModalWindowOutcome::ShortcutActivated(1) => {
                    if let Some(item) = self.items.get(self.picker.selected).cloned() {
                        return self.submit_or_chain(item, models);
                    }
                    return ModelPickerOutcome::Changed;
                }
                ModalWindowOutcome::Handled
                | ModalWindowOutcome::TabChanged(_)
                | ModalWindowOutcome::ShortcutActivated(_)
                | ModalWindowOutcome::CollapseGroup
                | ModalWindowOutcome::ExpandGroup
                | ModalWindowOutcome::CollapseDetails
                | ModalWindowOutcome::ExpandDetails
                | ModalWindowOutcome::JumpToParent(_) => return ModelPickerOutcome::Changed,
                ModalWindowOutcome::Unhandled => {}
            }
        }

        let entry_count = if self.loading || self.error.is_some() {
            1
        } else {
            self.items.len()
        };
        let config = PickerConfig {
            title: None,
            show_search_hint: false,
            expandable: false,
            esc_clears_query: false,
            shortcuts: Some(picker_shortcuts()),
            pending_hint: None,
            non_selectable: &[],
            non_selectable_clickable: &[],
            shortcuts_area: None,
            tabs: None,
            active_tab: 0,
            filter_label: None,
            filter_key_hint: None,
            filter_active: false,
            header_note: None,
            action_keys: &[],
            disable_search: false,
            compact_bottom_bar: false,
            search_only_on_slash: false,
            vim_normal_first: false,
        };
        let step = match handle_picker_input(&event, &mut self.picker, entry_count, &config) {
            PickerOutcome::Selected(i) => match self.items.get(i).cloned() {
                Some(item) => SelectedOrFilter::Selected(item),
                None => return ModelPickerOutcome::Changed,
            },
            PickerOutcome::Closed => SelectedOrFilter::Closed,
            PickerOutcome::QueryChanged => SelectedOrFilter::FilterChanged,
            PickerOutcome::Changed => return ModelPickerOutcome::Changed,
            PickerOutcome::Unchanged => return ModelPickerOutcome::Unchanged,
            _ => return ModelPickerOutcome::Changed,
        };

        match step {
            SelectedOrFilter::FilterChanged => {
                self.refilter();
                ModelPickerOutcome::Changed
            }
            SelectedOrFilter::Closed => {
                if self.in_effort_phase() && self.try_step_back_from_effort(models) {
                    return ModelPickerOutcome::Changed;
                }
                ModelPickerOutcome::Closed
            }
            SelectedOrFilter::Selected(item) => self.submit_or_chain(item, models),
        }
    }

    fn submit_or_chain(&mut self, item: ArgItem, models: &ModelState) -> ModelPickerOutcome {
        let chains_to_effort = matches!(self.command.as_str(), "model" | "m")
            && item.insert_text.ends_with(char::is_whitespace);
        if chains_to_effort {
            let next_query = item.insert_text.clone();
            if let Some(effort_items) = MODEL.suggest_args(models, &next_query)
                && Self::arg_items_look_like_effort_phase(&effort_items)
            {
                self.args_query = next_query;
                self.original_items = effort_items;
                self.picker = PickerState::input_active();
                self.refilter();
                return ModelPickerOutcome::Changed;
            }
        }
        let full = format!("/{} {}", self.command, item.insert_text.trim_end());
        ModelPickerOutcome::Submit(full)
    }

    pub fn render(&mut self, buf: &mut Buffer, area: Rect, theme: &Theme, compact: bool) {
        let title = match self.command.as_str() {
            "model" | "m" if !self.args_query.is_empty() => "Pick reasoning effort",
            "model" | "m" => "Pick model",
            _ => "Pick option",
        };
        let shortcuts = [
            Shortcut {
                label: "Enter select",
                clickable: true,
                id: 1,
            },
            Shortcut {
                label: "Esc close",
                clickable: true,
                id: 2,
            },
        ];
        let config = ModalWindowConfig {
            title,
            tabs: None,
            shortcuts: &shortcuts,
            sizing: ModalSizing {
                width_pct: 0.50,
                max_width: 80,
                min_width: 44,
                v_margin: 4,
                h_pad: 2,
                v_pad: 1,
                footer_lines: 2,
            }
            .with_compact(compact),
            fold_info: None,
        };
        let Some(content) = render_modal_window(buf, area, &mut self.window, &config, theme) else {
            return;
        };

        if self.loading {
            let loading = vec![PickerEntry::Row(PickerRow {
                label: "Loading…",
                right_label: "",
                selected: false,
                expanded: false,
                fields: &[],
                description_lines: &[],
                summary_lines: &[],
                dimmed: true,
                indent: 0,
                badge: "",
                badge_color: None,
                collapsible: false,
                underline_last_desc: false,
            })];
            render_picker_in_modal(
                buf,
                content.content,
                content.inner_x,
                content.inner_width,
                theme,
                &mut self.picker,
                &loading,
                &[true],
                true,
            );
            return;
        }
        if let Some(error) = self.error.as_ref() {
            let row = PickerEntry::Row(PickerRow {
                label: error,
                right_label: "",
                selected: false,
                expanded: false,
                fields: &[],
                description_lines: &[],
                summary_lines: &[],
                dimmed: true,
                indent: 0,
                badge: "",
                badge_color: None,
                collapsible: false,
                underline_last_desc: false,
            });
            render_picker_in_modal(
                buf,
                content.content,
                content.inner_x,
                content.inner_width,
                theme,
                &mut self.picker,
                &[row],
                &[true],
                false,
            );
            return;
        }

        let picker_entries: Vec<PickerEntry<'_>> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                PickerEntry::Row(PickerRow {
                    label: &item.display,
                    right_label: &item.description,
                    selected: self.picker.hovered == Some(i)
                        || (self.picker.hovered.is_none() && i == self.picker.selected),
                    expanded: false,
                    fields: &[],
                    description_lines: &[],
                    summary_lines: &[],
                    dimmed: false,
                    indent: 0,
                    badge: "",
                    badge_color: None,
                    collapsible: false,
                    underline_last_desc: false,
                })
            })
            .collect();
        render_picker_in_modal(
            buf,
            content.content,
            content.inner_x,
            content.inner_width,
            theme,
            &mut self.picker,
            &picker_entries,
            &[],
            false,
        );
        if let Some(hit) = self.picker.hit_areas.as_mut() {
            hit.close_button = self.window.close_button_rect.unwrap_or_default();
        } else {
            self.picker.hit_areas = Some(PickerHitAreas {
                close_button: self.window.close_button_rect.unwrap_or_default(),
                search_bar: Rect::default(),
                item_rects: Vec::new(),
                entry_indices: Vec::new(),
                tab_rects: Vec::new(),
                filter_rect: None,
            });
        }
    }
}

enum SelectedOrFilter {
    Selected(ArgItem),
    Closed,
    FilterChanged,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_state::{ModelId, ModelInfo, ReasoningEffortOption};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn catalog() -> ModelState {
        let mut state = ModelState::default();
        state.available.push(ModelInfo {
            id: ModelId::new("deepseek-official", "deepseek-chat"),
            name: "DeepSeek Chat".into(),
            description: None,
            reasoning: None,
            default_effort: None,
        });
        state.available.push(ModelInfo {
            id: ModelId::new("deepseek-official", "deepseek-reasoner"),
            name: "DeepSeek Reasoner".into(),
            description: None,
            reasoning: Some(vec![ReasoningEffortOption {
                id: "high".into(),
                name: "high".into(),
                description: None,
            }]),
            default_effort: Some("high".into()),
        });
        state.current = Some(ModelId::new("deepseek-official", "deepseek-chat"));
        state
    }

    #[test]
    fn selecting_plain_model_submits_slash() {
        let models = catalog();
        let mut picker = ModelPickerState::default();
        picker.open(&models);
        assert!(matches!(
            picker.handle_event(key(KeyCode::Enter), &models),
            ModelPickerOutcome::Submit(cmd) if cmd == "/model DeepSeek Chat"
        ));
    }

    #[test]
    fn reasoning_model_enter_chains_to_effort_then_submits() {
        let models = catalog();
        let mut picker = ModelPickerState::default();
        picker.open(&models);
        picker.picker.selected = 1;
        assert!(matches!(
            picker.handle_event(key(KeyCode::Enter), &models),
            ModelPickerOutcome::Changed
        ));
        assert!(!picker.args_query.is_empty());
        assert!(matches!(
            picker.handle_event(key(KeyCode::Enter), &models),
            ModelPickerOutcome::Submit(cmd) if cmd.starts_with("/model DeepSeek Reasoner")
        ));
    }

    #[test]
    fn esc_from_effort_returns_to_model_list() {
        let models = catalog();
        let mut picker = ModelPickerState::default();
        picker.open(&models);
        picker.picker.selected = 1;
        let _ = picker.handle_event(key(KeyCode::Enter), &models);
        assert!(picker.in_effort_phase());
        assert!(matches!(
            picker.handle_event(key(KeyCode::Esc), &models),
            ModelPickerOutcome::Changed
        ));
        assert!(!picker.in_effort_phase());
    }
}
