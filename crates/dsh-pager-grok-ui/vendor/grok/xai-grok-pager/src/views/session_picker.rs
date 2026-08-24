//! Native-only `/resume` session picker.
//!
//! This is a B adaptation of Grok Build's shared session-picker helpers and
//! `ActiveModal::SessionPicker` input/render branches. The session data and
//! search completions are host-neutral owned DTOs; foreign-session filters,
//! deletion, worktrees and direct UUID loading are deliberately excluded.

use std::collections::{BTreeMap, HashSet};

use crossterm::event::{Event, MouseEvent};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::theme::Theme;
use crate::views::modal_window::{
    ModalSizing, ModalWindowConfig, ModalWindowOutcome, Shortcut, handle_modal_mouse,
    render_modal_window,
};
use crate::views::picker::{
    PickerConfig, PickerEntry, PickerField, PickerHitAreas, PickerOutcome, PickerRow, PickerState,
    clamp_picker_selection, handle_picker_input, render_divider,
    render_picker_content_with_scrollbar_x, render_picker_search_bar,
};

pub const CONTENT_EXPAND_OFFSET: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPickerEntry {
    pub id: String,
    pub summary: String,
    pub updated_at_ms: u64,
    pub cwd: String,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSearchHit {
    pub session_id: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PickerItem {
    Session { original_index: usize },
    Content { hit_index: usize },
}

impl PickerItem {
    fn expansion_key(&self) -> usize {
        match self {
            Self::Session { original_index } => *original_index,
            Self::Content { hit_index } => CONTENT_EXPAND_OFFSET + hit_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RenderItem {
    Header(String),
    Row(RenderRow),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderRow {
    target: PickerItem,
    label: String,
    right_label: String,
    fields: Vec<(String, String)>,
    summary_lines: Vec<String>,
    indent: u8,
    expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumePickerOutcome {
    Closed,
    Selected(String),
    QueryChanged { query: String, revision: u64 },
    Changed,
    Unchanged,
}

#[derive(Debug)]
pub struct ResumePickerState {
    picker: PickerState,
    window: crate::modal_window_state::ModalWindowState,
    entries: Option<Vec<SessionPickerEntry>>,
    loading: bool,
    error: Option<String>,
    content_results: Vec<SessionSearchHit>,
    content_loading: bool,
    list_revision: u64,
    search_revision: u64,
    current_repo: String,
    selected_session_id: Option<String>,
}

impl Default for ResumePickerState {
    fn default() -> Self {
        Self {
            picker: PickerState::default(),
            window: crate::modal_window_state::ModalWindowState::new(),
            entries: None,
            loading: false,
            error: None,
            content_results: Vec::new(),
            content_loading: false,
            list_revision: 0,
            search_revision: 0,
            current_repo: "unknown".to_string(),
            selected_session_id: None,
        }
    }
}

impl ResumePickerState {
    pub fn open(&mut self, current_session_id: &str, current_cwd: &str) -> u64 {
        self.list_revision = self.list_revision.saturating_add(1);
        self.search_revision = self.search_revision.saturating_add(1);
        self.picker = PickerState::default();
        self.window = crate::modal_window_state::ModalWindowState::new();
        self.entries = None;
        self.loading = true;
        self.error = None;
        self.content_results.clear();
        self.content_loading = false;
        self.current_repo = repo_name_from_cwd(current_cwd);
        self.selected_session_id = Some(current_session_id.to_string());
        self.list_revision
    }

    pub fn close(&mut self) {
        self.list_revision = self.list_revision.saturating_add(1);
        self.search_revision = self.search_revision.saturating_add(1);
        self.loading = false;
        self.content_loading = false;
        self.picker.reset();
        self.window = crate::modal_window_state::ModalWindowState::new();
    }

    pub fn list_revision(&self) -> u64 {
        self.list_revision
    }

    pub fn search_revision(&self) -> u64 {
        self.search_revision
    }

    pub fn query(&self) -> &str {
        self.picker.query()
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn entries(&self) -> Option<&[SessionPickerEntry]> {
        self.entries.as_deref()
    }

    pub fn apply_entries(&mut self, revision: u64, mut entries: Vec<SessionPickerEntry>) -> bool {
        if revision != self.list_revision || !self.loading {
            return false;
        }
        entries.sort_by(|left, right| {
            right
                .updated_at_ms
                .cmp(&left.updated_at_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        self.entries = Some(entries);
        self.loading = false;
        self.error = None;
        self.reanchor_selection();
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

    pub fn apply_search(&mut self, revision: u64, results: Vec<SessionSearchHit>) -> bool {
        if revision != self.search_revision || !self.content_loading {
            return false;
        }
        let known_ids = self
            .entries
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<HashSet<_>>();
        self.content_results = results
            .into_iter()
            .filter(|hit| known_ids.contains(hit.session_id.as_str()))
            .collect();
        self.content_loading = false;
        self.reanchor_selection();
        true
    }

    pub fn fail_search(&mut self, revision: u64, message: impl Into<String>) -> bool {
        if revision != self.search_revision || !self.content_loading {
            return false;
        }
        self.content_loading = false;
        self.error = Some(message.into());
        true
    }

    fn begin_search(&mut self) -> u64 {
        self.search_revision = self.search_revision.saturating_add(1);
        self.content_results.clear();
        self.content_loading = !self.picker.query().trim().is_empty();
        self.search_revision
    }

    fn reanchor_selection(&mut self) {
        let items = self.build_render_items(0);
        let non_selectable = non_selectable_mask(&items);
        if let Some(selected) = self.selected_session_id.as_deref()
            && let Some(index) = items.iter().position(|item| {
                matches!(item, RenderItem::Row(row) if self.target_id(&row.target) == Some(selected))
            })
        {
            self.picker.selected = index;
        }
        clamp_picker_selection(&mut self.picker, items.len(), &non_selectable);
        self.capture_selected_id(&items);
    }

    fn capture_selected_id(&mut self, items: &[RenderItem]) {
        if let Some(RenderItem::Row(row)) = items.get(self.picker.selected)
            && let Some(id) = self.target_id(&row.target)
        {
            self.selected_session_id = Some(id.to_string());
        }
    }

    fn target_id<'a>(&'a self, target: &'a PickerItem) -> Option<&'a str> {
        match target {
            PickerItem::Session { original_index } => self
                .entries
                .as_deref()?
                .get(*original_index)
                .map(|entry| entry.id.as_str()),
            PickerItem::Content { hit_index } => self
                .content_results
                .get(*hit_index)
                .map(|hit| hit.session_id.as_str()),
        }
    }

    pub fn handle_event(&mut self, event: Event) -> ResumePickerOutcome {
        if let Event::Mouse(MouseEvent {
            kind, column, row, ..
        }) = &event
        {
            match handle_modal_mouse(&mut self.window, *kind, *column, *row) {
                ModalWindowOutcome::CloseRequested => return ResumePickerOutcome::Closed,
                ModalWindowOutcome::Handled
                | ModalWindowOutcome::TabChanged(_)
                | ModalWindowOutcome::ShortcutActivated(_)
                | ModalWindowOutcome::CollapseGroup
                | ModalWindowOutcome::ExpandGroup
                | ModalWindowOutcome::CollapseDetails
                | ModalWindowOutcome::ExpandDetails
                | ModalWindowOutcome::JumpToParent(_) => return ResumePickerOutcome::Changed,
                ModalWindowOutcome::Unhandled => {}
            }
        }

        let items_before = self.build_render_items(0);
        let selected_before = self.selected_session_id.clone();
        let non_selectable = non_selectable_mask(&items_before);
        let config = PickerConfig {
            title: Some("Resume session"),
            show_search_hint: true,
            expandable: true,
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
            search_only_on_slash: false,
            vim_normal_first: false,
        };
        let outcome = handle_picker_input(&event, &mut self.picker, items_before.len(), &config);
        match outcome {
            PickerOutcome::Closed => ResumePickerOutcome::Closed,
            PickerOutcome::Selected(index) => {
                let Some(RenderItem::Row(row)) = items_before.get(index) else {
                    return ResumePickerOutcome::Changed;
                };
                let Some(id) = self.target_id(&row.target).map(str::to_string) else {
                    return ResumePickerOutcome::Changed;
                };
                self.selected_session_id = Some(id.clone());
                ResumePickerOutcome::Selected(id)
            }
            PickerOutcome::Expand(index) => {
                if let Some(RenderItem::Row(row)) = items_before.get(index) {
                    let key = row.target.expansion_key();
                    if !self.picker.expanded.insert(key) {
                        self.picker.expanded.remove(&key);
                    }
                }
                ResumePickerOutcome::Changed
            }
            PickerOutcome::Collapse(index) => {
                if let Some(RenderItem::Row(row)) = items_before.get(index) {
                    self.picker.expanded.remove(&row.target.expansion_key());
                }
                ResumePickerOutcome::Changed
            }
            PickerOutcome::QueryChanged => {
                self.selected_session_id = selected_before;
                let revision = self.begin_search();
                self.reanchor_selection();
                ResumePickerOutcome::QueryChanged {
                    query: self.picker.query().to_string(),
                    revision,
                }
            }
            PickerOutcome::Changed => {
                let items = self.build_render_items(0);
                self.capture_selected_id(&items);
                ResumePickerOutcome::Changed
            }
            PickerOutcome::Unchanged | PickerOutcome::SubmitQuery | PickerOutcome::Copy(_) => {
                ResumePickerOutcome::Unchanged
            }
            PickerOutcome::TabChanged(_)
            | PickerOutcome::FilterCycled
            | PickerOutcome::Action(_)
            | PickerOutcome::NonSelectableClick(_) => ResumePickerOutcome::Changed,
        }
    }

    pub fn render(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        theme: &Theme,
        compact: bool,
        tick: u64,
        now_ms: u64,
    ) {
        let shortcuts = [
            Shortcut {
                label: "↑↓ nav",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "e expand",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "/ search",
                clickable: false,
                id: 0,
            },
        ];
        let config = ModalWindowConfig {
            title: "Resume session",
            tabs: None,
            shortcuts: &shortcuts,
            sizing: ModalSizing {
                width_pct: 0.65,
                max_width: 120,
                min_width: 48,
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

        render_picker_search_bar(
            buf,
            content.content.x,
            content.content.y,
            content.content.width,
            theme,
            &self.picker,
            self.picker.search_active,
            true,
            Some(theme.bg_base),
        );
        let sep_y = content.content.y + 1;
        if sep_y < content.content.bottom() {
            render_divider(
                buf,
                content.inner_x,
                sep_y,
                content.inner_width,
                theme,
                Some(theme.bg_base),
            );
        }

        let items = self.build_render_items_at(tick, now_ms);
        let non_selectable = non_selectable_mask(&items);
        clamp_picker_selection(&mut self.picker, items.len(), &non_selectable);

        let field_data = items
            .iter()
            .map(|item| match item {
                RenderItem::Header(_) => Vec::new(),
                RenderItem::Row(row) => row
                    .fields
                    .iter()
                    .map(|(label, value)| PickerField {
                        label: label.as_str(),
                        value: value.as_str(),
                    })
                    .collect::<Vec<_>>(),
            })
            .collect::<Vec<_>>();
        let summary_data = items
            .iter()
            .map(|item| match item {
                RenderItem::Header(_) => Vec::new(),
                RenderItem::Row(row) => row
                    .summary_lines
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            })
            .collect::<Vec<_>>();
        let picker_entries = items
            .iter()
            .enumerate()
            .map(|(index, item)| match item {
                RenderItem::Header(label) => PickerEntry::Header { label },
                RenderItem::Row(row) => PickerEntry::Row(PickerRow {
                    label: &row.label,
                    right_label: &row.right_label,
                    selected: !self.picker.selection_hidden && index == self.picker.selected,
                    expanded: row.expanded,
                    fields: &field_data[index],
                    description_lines: &[],
                    summary_lines: &summary_data[index],
                    dimmed: false,
                    indent: row.indent,
                    badge: "",
                    badge_color: None,
                    collapsible: true,
                    underline_last_desc: false,
                }),
            })
            .collect::<Vec<_>>();
        let entries_y = sep_y.saturating_add(1);
        let entries_area = Rect::new(
            content.content.x,
            entries_y,
            content.content.width,
            content.content.bottom().saturating_sub(entries_y),
        );
        let hit = render_picker_content_with_scrollbar_x(
            buf,
            entries_area,
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
            search_bar: Rect::new(
                content.content.x,
                content.content.y,
                content.content.width,
                1,
            ),
            item_rects: hit.item_rects,
            entry_indices: hit.entry_indices,
            tab_rects: Vec::new(),
            filter_rect: None,
        });
    }

    fn build_render_items(&self, tick: u64) -> Vec<RenderItem> {
        self.build_render_items_at(tick, now_epoch_ms())
    }

    fn build_render_items_at(&self, tick: u64, now_ms: u64) -> Vec<RenderItem> {
        let query = self.picker.query().trim().to_lowercase();
        let entries = self.entries.as_deref().unwrap_or(&[]);
        let filtered = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                query.is_empty()
                    || entry.id.to_lowercase().contains(&query)
                    || entry.summary.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();

        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for original_index in &filtered {
            let repo = repo_name_from_cwd(&entries[*original_index].cwd);
            groups.entry(repo).or_default().push(*original_index);
        }
        let mut ordered = groups.into_iter().collect::<Vec<_>>();
        if let Some(position) = ordered
            .iter()
            .position(|(repo, _)| repo == &self.current_repo)
        {
            let current = ordered.remove(position);
            ordered.insert(0, current);
        }

        let mut result = Vec::new();
        for (repo, member_indices) in ordered {
            result.push(RenderItem::Header(repo));
            for original_index in member_indices {
                let entry = &entries[original_index];
                let expanded = self.picker.expanded.contains(&original_index);
                let mut fields = Vec::new();
                if expanded {
                    fields.push(("ID".to_string(), entry.id.clone()));
                    fields.push(("CWD".to_string(), entry.cwd.clone()));
                    if let Some(model) = entry.model_id.as_deref() {
                        fields.push(("Model".to_string(), model.to_string()));
                    }
                    fields.push((
                        "Updated".to_string(),
                        format_time_ago_at(entry.updated_at_ms, now_ms)
                            .trim()
                            .to_string(),
                    ));
                    fields.push(("Source".to_string(), "DSH".to_string()));
                }
                result.push(RenderItem::Row(RenderRow {
                    target: PickerItem::Session { original_index },
                    label: if entry.summary.trim().is_empty() {
                        "(no prompt)".to_string()
                    } else {
                        entry.summary.clone()
                    },
                    right_label: format_time_ago_at(entry.updated_at_ms, now_ms),
                    fields,
                    summary_lines: Vec::new(),
                    indent: 1,
                    expanded,
                }));
            }
        }

        let fuzzy_ids = filtered
            .iter()
            .filter_map(|index| entries.get(*index).map(|entry| entry.id.as_str()))
            .collect::<HashSet<_>>();
        let content_indices = self
            .content_results
            .iter()
            .enumerate()
            .filter(|(_, hit)| !fuzzy_ids.contains(hit.session_id.as_str()))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if self.content_loading || !content_indices.is_empty() {
            let label = if self.content_loading {
                let frames = crate::glyphs::dot_spinner_frames();
                format!(
                    "{} Searching session content…",
                    frames[(tick / 4) as usize % frames.len()]
                )
            } else {
                "Extended search results".to_string()
            };
            result.push(RenderItem::Header(label));
        }
        for hit_index in content_indices {
            let hit = &self.content_results[hit_index];
            let known = entries.iter().find(|entry| entry.id == hit.session_id);
            let expanded = self
                .picker
                .expanded
                .contains(&(CONTENT_EXPAND_OFFSET + hit_index));
            let snippet = first_non_empty_line(&hit.snippet);
            let mut fields = Vec::new();
            if expanded {
                fields.push(("ID".to_string(), hit.session_id.clone()));
                if let Some(entry) = known {
                    fields.push(("CWD".to_string(), entry.cwd.clone()));
                    if let Some(model) = entry.model_id.as_deref() {
                        fields.push(("Model".to_string(), model.to_string()));
                    }
                }
                fields.push(("Source".to_string(), "DSH".to_string()));
            }
            result.push(RenderItem::Row(RenderRow {
                target: PickerItem::Content { hit_index },
                label: known
                    .map(|entry| entry.summary.clone())
                    .filter(|summary| !summary.trim().is_empty())
                    .unwrap_or_else(|| format!("Session {}", hit.session_id)),
                right_label: known
                    .map(|entry| format_time_ago_at(entry.updated_at_ms, now_ms))
                    .unwrap_or_default(),
                fields,
                summary_lines: (!snippet.is_empty())
                    .then_some(snippet)
                    .into_iter()
                    .collect(),
                indent: 0,
                expanded,
            }));
        }

        if result.is_empty()
            && let Some(error) = self.error.as_deref()
        {
            result.push(RenderItem::Header(format!(
                "Unable to load sessions: {error}"
            )));
        }
        result
    }
}

fn non_selectable_mask(items: &[RenderItem]) -> Vec<bool> {
    items
        .iter()
        .map(|item| matches!(item, RenderItem::Header(_)))
        .collect()
}

fn first_non_empty_line(value: &str) -> String {
    let line = value
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim();
    if line.chars().count() > 80 {
        format!("{}...", line.chars().take(77).collect::<String>())
    } else {
        line.to_string()
    }
}

pub fn repo_name_from_cwd(cwd: &str) -> String {
    let path = std::path::Path::new(cwd);
    let components = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        return if cwd.is_empty() {
            "unknown".to_string()
        } else {
            cwd.to_string()
        };
    }
    components[components.len().saturating_sub(2)..].join("-")
}

pub fn format_time_ago_at(updated_at_ms: u64, now_ms: u64) -> String {
    let elapsed_minutes = now_ms.saturating_sub(updated_at_ms) / 60_000;
    let raw = if elapsed_minutes < 1 {
        "just now".to_string()
    } else if elapsed_minutes < 60 {
        format!("{elapsed_minutes}m ago")
    } else if elapsed_minutes < 60 * 24 {
        format!("{}h ago", elapsed_minutes / 60)
    } else if elapsed_minutes < 60 * 24 * 30 {
        format!("{}d ago", elapsed_minutes / (60 * 24))
    } else {
        format!("{}mo ago", elapsed_minutes / (60 * 24 * 30))
    };
    format!("{raw:>8}")
}

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn entry(id: &str, summary: &str, cwd: &str, updated_at_ms: u64) -> SessionPickerEntry {
        SessionPickerEntry {
            id: id.to_string(),
            summary: summary.to_string(),
            updated_at_ms,
            cwd: cwd.to_string(),
            model_id: Some("deepseek-chat".to_string()),
        }
    }

    #[test]
    fn repo_name_matches_grok_last_two_component_rule() {
        assert_eq!(repo_name_from_cwd("/home/user/fw/1"), "fw-1");
        assert_eq!(repo_name_from_cwd("/xai"), "xai");
        assert_eq!(repo_name_from_cwd("/"), "/");
        assert_eq!(repo_name_from_cwd(""), "unknown");
    }

    #[test]
    fn current_repo_is_pinned_and_headers_are_not_selectable() {
        let mut picker = ResumePickerState::default();
        let revision = picker.open("b", "/work/current/repo");
        assert!(picker.apply_entries(
            revision,
            vec![
                entry("a", "Other", "/work/alpha/repo", 300),
                entry("b", "Current", "/work/current/repo", 200),
            ],
        ));
        let items = picker.build_render_items_at(0, 1_000);
        assert!(matches!(&items[0], RenderItem::Header(label) if label == "current-repo"));
        assert!(matches!(&items[1], RenderItem::Row(row) if row.label == "Current"));
        assert_eq!(picker.picker.selected, 1);
    }

    #[test]
    fn stale_list_and_search_completions_are_ignored() {
        let mut picker = ResumePickerState::default();
        let old = picker.open("a", "/repo");
        let current = picker.open("a", "/repo");
        assert!(!picker.apply_entries(old, vec![entry("old", "Old", "/repo", 1)]));
        assert!(picker.apply_entries(current, vec![entry("a", "A", "/repo", 2)]));

        let outcome = picker.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('/'),
            KeyModifiers::NONE,
        )));
        assert_eq!(outcome, ResumePickerOutcome::Changed);
        let outcome = picker.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
        )));
        let ResumePickerOutcome::QueryChanged { revision, .. } = outcome else {
            panic!("query change");
        };
        assert!(!picker.apply_search(
            revision.saturating_sub(1),
            vec![SessionSearchHit {
                session_id: "old".to_string(),
                snippet: "old".to_string(),
            }]
        ));
        assert!(picker.apply_search(
            revision,
            vec![SessionSearchHit {
                session_id: "b".to_string(),
                snippet: "matching content".to_string(),
            }]
        ));
    }

    #[test]
    fn modal_uses_grok_resume_title_and_geometry() {
        let mut picker = ResumePickerState::default();
        let revision = picker.open("a", "/work/current/repo");
        picker.apply_entries(
            revision,
            vec![entry("a", "Current session", "/work/current/repo", 1_000)],
        );
        let area = Rect::new(0, 0, 100, 30);
        let mut buffer = Buffer::empty(area);
        picker.render(&mut buffer, area, Theme::current(), false, 0, 61_000);
        let popup = picker.window.popup_area.expect("popup");
        assert_eq!(popup, Rect::new(17, 4, 65, 22));
        let screen = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("Resume session"));
        assert!(screen.contains("Current session"));
        assert!(screen.contains("↑↓ nav"));
        assert!(screen.contains("e expand"));
        assert!(screen.contains("/ search"));
    }

    #[test]
    fn reference_terminal_sizes_keep_resume_modal_inside_the_buffer() {
        for (width, height, compact) in [
            (40, 12, true),
            (60, 20, false),
            (80, 24, false),
            (120, 40, false),
            (160, 50, false),
        ] {
            let mut picker = ResumePickerState::default();
            let revision = picker.open("a", "/work/current/repo");
            picker.apply_entries(
                revision,
                vec![entry("a", "Current session", "/work/current/repo", 1_000)],
            );
            let area = Rect::new(0, 0, width, height);
            let mut buffer = Buffer::empty(area);
            picker.render(&mut buffer, area, Theme::current(), compact, 0, 61_000);
            let popup = picker.window.popup_area.expect("popup");
            assert!(popup.x >= area.x && popup.y >= area.y);
            assert!(popup.right() <= area.right() && popup.bottom() <= area.bottom());
        }
    }

    #[test]
    fn expand_and_content_search_render_only_real_native_fields() {
        let mut picker = ResumePickerState::default();
        let revision = picker.open("a", "/work/current/repo");
        picker.apply_entries(
            revision,
            vec![
                entry("a", "Current session", "/work/current/repo", 1_000),
                entry("b", "Second session", "/work/other/repo", 500),
            ],
        );
        assert_eq!(
            picker.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('e'),
                KeyModifiers::NONE,
            ))),
            ResumePickerOutcome::Changed
        );
        let expanded = picker.build_render_items_at(0, 61_000);
        let fields = expanded
            .iter()
            .find_map(|item| match item {
                RenderItem::Row(row) if row.label == "Current session" => Some(&row.fields),
                _ => None,
            })
            .expect("expanded row");
        assert_eq!(
            fields
                .iter()
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>(),
            vec!["ID", "CWD", "Model", "Updated", "Source"]
        );
        assert_eq!(
            picker.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('/'),
                KeyModifiers::NONE,
            ))),
            ResumePickerOutcome::Changed
        );
        let ResumePickerOutcome::QueryChanged { revision, .. } = picker.handle_event(Event::Key(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        )) else {
            panic!("query revision");
        };
        assert!(picker.apply_search(
            revision,
            vec![SessionSearchHit {
                session_id: "b".to_string(),
                snippet: "content x match".to_string(),
            }],
        ));
        let items = picker.build_render_items_at(0, 61_000);
        assert!(matches!(
            items.first(),
            Some(RenderItem::Header(label)) if label == "Extended search results"
        ));
        assert!(matches!(
            items.get(1),
            Some(RenderItem::Row(row))
                if row.label == "Second session"
                    && row.summary_lines == ["content x match"]
        ));
    }

    #[test]
    fn time_ago_matches_grok_fixed_width_units() {
        assert_eq!(format_time_ago_at(60_000, 60_000), "just now");
        assert_eq!(format_time_ago_at(0, 8 * 60_000), "  8m ago");
        assert_eq!(format_time_ago_at(0, 3 * 60 * 60_000), "  3h ago");
    }
}
