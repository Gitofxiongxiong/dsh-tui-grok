//! Agent-preset picker over Grok's shared picker + modal chrome.
//!
//! Roster rows are host-neutral DTOs. Selection is blank-only at the Host;
//! this surface only presents the list and returns the chosen id.

use crossterm::event::{Event, MouseEvent};
use dsh_pager_protocol::AgentPresetEntry;
use ratatui::{buffer::Buffer, layout::Rect};

use crate::agent_preset::agent_preset_label;
use crate::theme::Theme;
use crate::views::modal_window::{
    ModalSizing, ModalWindowConfig, ModalWindowOutcome, Shortcut, handle_modal_mouse,
    render_modal_window,
};
use crate::views::picker::{
    PickerConfig, PickerEntry, PickerHitAreas, PickerOutcome, PickerRow, PickerState,
    clamp_picker_selection, handle_picker_input, render_picker_content_with_scrollbar_x,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetPickerOutcome {
    Closed,
    Selected(String),
    Changed,
    Unchanged,
}

#[derive(Debug, Default)]
pub struct PresetPickerState {
    picker: PickerState,
    window: crate::modal_window_state::ModalWindowState,
    options: Vec<AgentPresetEntry>,
    loading: bool,
    error: Option<String>,
    current: Option<String>,
    list_revision: u64,
}

impl PresetPickerState {
    pub fn open(&mut self, current: Option<&str>) -> u64 {
        self.list_revision = self.list_revision.saturating_add(1);
        self.picker = PickerState::default();
        self.window = crate::modal_window_state::ModalWindowState::new();
        self.loading = true;
        self.error = None;
        self.current = current.map(str::to_string);
        self.list_revision
    }

    pub fn close(&mut self) {
        self.loading = false;
        self.error = None;
    }

    pub fn apply_entries(&mut self, revision: u64, options: Vec<AgentPresetEntry>) -> bool {
        if revision != self.list_revision {
            return false;
        }
        self.options = options;
        self.loading = false;
        self.error = None;
        if let Some(current) = self.current.as_deref()
            && let Some(index) = self.options.iter().position(|entry| entry.id == current)
        {
            self.picker.selected = index;
        }
        true
    }

    pub fn fail_entries(&mut self, revision: u64, message: impl Into<String>) -> bool {
        if revision != self.list_revision || !self.loading {
            return false;
        }
        self.loading = false;
        self.error = Some(message.into());
        true
    }

    pub fn handle_event(&mut self, event: Event) -> PresetPickerOutcome {
        if let Event::Mouse(MouseEvent {
            kind, column, row, ..
        }) = &event
        {
            match handle_modal_mouse(&mut self.window, *kind, *column, *row) {
                ModalWindowOutcome::CloseRequested => return PresetPickerOutcome::Closed,
                ModalWindowOutcome::Handled
                | ModalWindowOutcome::TabChanged(_)
                | ModalWindowOutcome::ShortcutActivated(_)
                | ModalWindowOutcome::CollapseGroup
                | ModalWindowOutcome::ExpandGroup
                | ModalWindowOutcome::CollapseDetails
                | ModalWindowOutcome::ExpandDetails
                | ModalWindowOutcome::JumpToParent(_) => return PresetPickerOutcome::Changed,
                ModalWindowOutcome::Unhandled => {}
            }
        }

        let row_count = self.row_count();
        let non_selectable = self.non_selectable_mask();
        clamp_picker_selection(&mut self.picker, row_count, &non_selectable);
        let config = PickerConfig {
            title: Some("Agent preset"),
            show_search_hint: false,
            expandable: false,
            esc_clears_query: false,
            shortcuts: None,
            pending_hint: None,
            shortcuts_area: None,
            non_selectable: &non_selectable,
            non_selectable_clickable: &[],
            tabs: None,
            active_tab: 0,
            filter_label: None,
            filter_key_hint: None,
            filter_active: false,
            header_note: None,
            action_keys: &[],
            disable_search: false,
            compact_bottom_bar: false,
            search_only_on_slash: true,
            vim_normal_first: false,
        };
        match handle_picker_input(&event, &mut self.picker, row_count, &config) {
            PickerOutcome::Closed => PresetPickerOutcome::Closed,
            PickerOutcome::Selected(index) => {
                let Some(entry) = self.options.get(index) else {
                    return PresetPickerOutcome::Changed;
                };
                if entry.broken.is_some() {
                    return PresetPickerOutcome::Changed;
                }
                PresetPickerOutcome::Selected(entry.id.clone())
            }
            PickerOutcome::Unchanged => PresetPickerOutcome::Unchanged,
            _ => PresetPickerOutcome::Changed,
        }
    }

    pub fn render(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        theme: &Theme,
        compact: bool,
        tick: u64,
    ) {
        let title = "Agent preset";
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
            sizing: ModalSizing::medium().with_compact(compact),
            fold_info: None,
        };
        let Some(content) = render_modal_window(buf, area, &mut self.window, &config, theme) else {
            return;
        };

        let labels: Vec<(String, String, bool)> = self.row_labels();
        let non_selectable = self.non_selectable_mask();
        let picker_entries: Vec<PickerEntry<'_>> = labels
            .iter()
            .enumerate()
            .map(|(index, (label, right, dimmed))| {
                PickerEntry::Row(PickerRow {
                    label,
                    right_label: right,
                    selected: !self.picker.selection_hidden && index == self.picker.selected,
                    expanded: false,
                    fields: &[],
                    description_lines: &[],
                    summary_lines: &[],
                    dimmed: *dimmed,
                    indent: 0,
                    badge: "",
                    badge_color: None,
                    collapsible: false,
                    underline_last_desc: false,
                })
            })
            .collect();
        clamp_picker_selection(&mut self.picker, picker_entries.len(), &non_selectable);
        let hit = render_picker_content_with_scrollbar_x(
            buf,
            content.content,
            theme,
            &mut self.picker,
            &picker_entries,
            &non_selectable,
            &[],
            Some(theme.bg_base),
            self.loading,
            tick,
            content.inner_x + content.inner_width.saturating_sub(1),
        );
        self.picker.hit_areas = Some(PickerHitAreas {
            close_button: self.window.close_button_rect.unwrap_or_default(),
            search_bar: Rect::default(),
            item_rects: hit.item_rects,
            entry_indices: hit.entry_indices,
            tab_rects: Vec::new(),
            filter_rect: None,
        });
    }

    fn row_count(&self) -> usize {
        if self.loading || self.error.is_some() {
            1
        } else {
            self.options.len()
        }
    }

    fn row_labels(&self) -> Vec<(String, String, bool)> {
        if self.loading {
            return vec![("Loading…".into(), String::new(), true)];
        }
        if let Some(error) = self.error.as_ref() {
            return vec![(error.clone(), String::new(), true)];
        }
        self.options
            .iter()
            .map(|entry| {
                let mut label = agent_preset_label(&entry.id, &self.options);
                if entry.is_default {
                    label.push_str("  default");
                }
                if self.current.as_deref() == Some(entry.id.as_str()) {
                    label.push_str("  current");
                }
                let description = entry
                    .broken
                    .clone()
                    .or_else(|| entry.description.clone())
                    .unwrap_or_default();
                (label, description, entry.broken.is_some())
            })
            .collect()
    }

    fn non_selectable_mask(&self) -> Vec<bool> {
        if self.loading || self.error.is_some() {
            return vec![true];
        }
        self.options
            .iter()
            .map(|entry| entry.broken.is_some())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use dsh_pager_protocol::AgentPresetTrust;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn shipped(id: &str, name: &str, broken: Option<&str>) -> AgentPresetEntry {
        AgentPresetEntry {
            id: id.to_string(),
            trust: AgentPresetTrust::System,
            is_default: id == "standard",
            name: Some(name.to_string()),
            description: Some(format!("{name} description")),
            broken: broken.map(str::to_string),
        }
    }

    #[test]
    fn enter_selects_the_current_row_and_skips_broken() {
        let mut picker = PresetPickerState::default();
        let revision = picker.open(Some("standard"));
        assert!(picker.apply_entries(
            revision,
            vec![
                shipped("standard", "标准模式", None),
                shipped("broken", "坏掉", Some("yaml invalid")),
                shipped("ptc", "PTC 模式", None),
            ],
        ));
        assert_eq!(
            picker.handle_event(key(KeyCode::Enter)),
            PresetPickerOutcome::Selected("standard".into())
        );
        picker.picker.selected = 1;
        assert_eq!(
            picker.handle_event(key(KeyCode::Enter)),
            PresetPickerOutcome::Selected("ptc".into())
        );
    }
}
