//! Grok AppView/AgentView shell state and event routing.
//!
//! This module deliberately contains no transport or ratatui `Frame` access.
//! It is the small, replayable state machine between terminal events and the
//! existing Grok-derived widgets used by the runtime.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOwner {
    Prompt,
    Transcript,
    Picker,
    Queue,
    Interaction,
    FileSearch,
    ImagePreview,
    AgentTasks,
    Modal,
    Dashboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Picker,
    Queue,
    Interaction,
    FileSearch,
    ImagePreview,
    AgentTasks,
    Modal,
    Dashboard,
}

impl KeyOwner {
    pub const fn priority(self) -> u8 {
        match self {
            Self::Interaction => 0,
            Self::FileSearch => 2,
            Self::ImagePreview => 3,
            Self::AgentTasks => 4,
            Self::Modal => 1,
            Self::Picker => 5,
            Self::Queue => 6,
            Self::Dashboard => 7,
            Self::Prompt => 8,
            Self::Transcript => 9,
        }
    }
}

impl Overlay {
    pub const fn owner(self) -> KeyOwner {
        match self {
            Self::None => KeyOwner::Transcript,
            Self::Picker => KeyOwner::Picker,
            Self::Queue => KeyOwner::Queue,
            Self::Interaction => KeyOwner::Interaction,
            Self::FileSearch => KeyOwner::FileSearch,
            Self::ImagePreview => KeyOwner::ImagePreview,
            Self::AgentTasks => KeyOwner::AgentTasks,
            Self::Modal => KeyOwner::Modal,
            Self::Dashboard => KeyOwner::Dashboard,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize { width: u16, height: u16 },
    Tick,
    Notification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellAction {
    None,
    Quit,
    OpenPicker,
    CloseOverlay,
    ClearPrompt,
    ScrollUp(u16),
    ScrollDown(u16),
    SubmitPrompt,
    PromptNewline,
    TogglePromptMode,
    OpenQueue,
    OpenInteraction,
    OpenFileSearch,
    OpenImagePreview,
    OpenAgentTasks,
    OpenDashboard,
    PromptKey(KeyEvent),
    PickerKey(KeyEvent),
    PickerMouse(MouseEvent),
    QueueKey(KeyEvent),
    QueueMouse(MouseEvent),
    InteractionKey(KeyEvent),
    InteractionMouse(MouseEvent),
    PromptPaste(String),
    PickerPaste(String),
    QueuePaste(String),
    InteractionPaste(String),
    FileSearchKey(KeyEvent),
    FileSearchMouse(MouseEvent),
    FileSearchPaste(String),
    ImagePreviewKey(KeyEvent),
    ImagePreviewMouse(MouseEvent),
    AgentTasksKey(KeyEvent),
    AgentTasksMouse(MouseEvent),
    DashboardKey(KeyEvent),
    DashboardMouse(MouseEvent),
    DashboardPaste(String),
    TranscriptMouse(MouseEvent),
    Resized(Rect),
    Redraw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplacementEntry {
    pub legacy: &'static str,
    pub replacement: &'static str,
    pub default_path: bool,
}

/// M3 replacement map.  Legacy modules remain explicit behavior oracles; the
/// shell and Grok-derived primitives are the only default path.
pub const REPLACEMENT_MAP: &[ReplacementEntry] = &[
    ReplacementEntry {
        legacy: "runtime::manual_layout",
        replacement: "AppShell::layout + AgentView render",
        default_path: true,
    },
    ReplacementEntry {
        legacy: "runtime::key_match",
        replacement: "AppShell::dispatch",
        default_path: true,
    },
    ReplacementEntry {
        legacy: "runtime::picker_open_bool",
        replacement: "AppShell::overlay",
        default_path: true,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellLayout {
    pub header: Rect,
    pub body: Rect,
    pub prompt: Rect,
    pub footer: Rect,
}

/// Stable semantic state used by focus/overlay golden tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSnapshot {
    pub owner: KeyOwner,
    pub overlay: Overlay,
    pub dim_layer: bool,
    pub z_order: Vec<Overlay>,
    pub cursor_owner: KeyOwner,
    pub layout_revision: u64,
}

impl ShellLayout {
    pub fn for_area(area: Rect) -> Self {
        if area.height < 4 {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(area);
            return Self {
                header: rows[0],
                body: rows[1],
                prompt: Rect::new(
                    rows[1].x,
                    rows[1].bottom().saturating_sub(1),
                    rows[1].width,
                    0,
                ),
                footer: rows[2],
            };
        }
        let prompt_height = if area.height >= 8 { 3 } else { 2 };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(prompt_height),
                Constraint::Length(1),
            ])
            .split(area);
        Self {
            header: rows[0],
            body: rows[1],
            prompt: rows[2],
            footer: rows[3],
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppShell {
    owner: KeyOwner,
    overlay: Overlay,
    previous_owner: KeyOwner,
    layout: Option<ShellLayout>,
    layout_revision: u64,
}

impl Default for AppShell {
    fn default() -> Self {
        Self {
            owner: KeyOwner::Transcript,
            overlay: Overlay::None,
            previous_owner: KeyOwner::Transcript,
            layout: None,
            layout_revision: 0,
        }
    }
}

impl AppShell {
    pub fn owner(&self) -> KeyOwner {
        self.owner
    }

    pub fn overlay(&self) -> Overlay {
        self.overlay
    }

    pub fn focus_prompt(&mut self) {
        if self.overlay == Overlay::None {
            self.owner = KeyOwner::Prompt;
        }
    }

    pub fn snapshot(&self) -> ShellSnapshot {
        ShellSnapshot {
            owner: self.owner,
            overlay: self.overlay,
            dim_layer: self.overlay != Overlay::None,
            z_order: if self.overlay == Overlay::None {
                Vec::new()
            } else {
                vec![self.overlay]
            },
            cursor_owner: self.owner,
            layout_revision: self.layout_revision,
        }
    }

    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }

    pub fn invalidate_content(&mut self) {
        self.layout_revision = self.layout_revision.wrapping_add(1);
        self.layout = None;
    }

    pub fn layout(&mut self, area: Rect) -> ShellLayout {
        let next = ShellLayout::for_area(area);
        if self.layout != Some(next) {
            self.layout = Some(next);
            self.layout_revision = self.layout_revision.wrapping_add(1);
        }
        next
    }

    pub fn open_picker(&mut self) {
        self.previous_owner = self.owner;
        self.overlay = Overlay::Picker;
        self.owner = KeyOwner::Picker;
    }

    pub fn open_queue(&mut self) {
        self.previous_owner = self.owner;
        self.overlay = Overlay::Queue;
        self.owner = KeyOwner::Queue;
    }

    pub fn open_interaction(&mut self) {
        self.previous_owner = self.owner;
        self.overlay = Overlay::Interaction;
        self.owner = KeyOwner::Interaction;
    }

    pub fn open_file_search(&mut self) {
        self.previous_owner = self.owner;
        self.overlay = Overlay::FileSearch;
        self.owner = KeyOwner::FileSearch;
    }

    pub fn open_image_preview(&mut self) {
        self.previous_owner = self.owner;
        self.overlay = Overlay::ImagePreview;
        self.owner = KeyOwner::ImagePreview;
    }

    pub fn open_agent_tasks(&mut self) {
        self.previous_owner = self.owner;
        self.overlay = Overlay::AgentTasks;
        self.owner = KeyOwner::AgentTasks;
    }

    pub fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
        self.owner = self.previous_owner;
    }

    pub fn dispatch(&mut self, event: ShellEvent, prompt_empty: bool) -> ShellAction {
        match event {
            ShellEvent::Resize { width, height } => {
                let area = Rect::new(0, 0, width, height);
                self.layout = None;
                self.layout_revision = self.layout_revision.wrapping_add(1);
                ShellAction::Resized(area)
            }
            ShellEvent::Tick | ShellEvent::Notification => ShellAction::Redraw,
            ShellEvent::Paste(text) => match self.owner {
                KeyOwner::Picker => ShellAction::PickerPaste(text),
                KeyOwner::Queue => ShellAction::QueuePaste(text),
                KeyOwner::Interaction => ShellAction::InteractionPaste(text),
                KeyOwner::FileSearch => ShellAction::FileSearchPaste(text),
                KeyOwner::Dashboard => ShellAction::DashboardPaste(text),
                _ => ShellAction::PromptPaste(text),
            },
            ShellEvent::Mouse(mouse) => {
                if self.overlay == Overlay::Picker {
                    return ShellAction::PickerMouse(mouse);
                }
                if self.overlay == Overlay::Queue {
                    return ShellAction::QueueMouse(mouse);
                }
                if self.overlay == Overlay::Interaction {
                    return ShellAction::InteractionMouse(mouse);
                }
                if self.overlay == Overlay::FileSearch {
                    return ShellAction::FileSearchMouse(mouse);
                }
                if self.overlay == Overlay::ImagePreview {
                    return ShellAction::ImagePreviewMouse(mouse);
                }
                if self.overlay == Overlay::AgentTasks {
                    return ShellAction::AgentTasksMouse(mouse);
                }
                if self.overlay == Overlay::Dashboard {
                    return ShellAction::DashboardMouse(mouse);
                }
                match mouse.kind {
                    MouseEventKind::ScrollUp => ShellAction::ScrollUp(3),
                    MouseEventKind::ScrollDown => ShellAction::ScrollDown(3),
                    _ => ShellAction::TranscriptMouse(mouse),
                }
            }
            ShellEvent::Key(key) => self.dispatch_key(key, prompt_empty),
        }
    }

    fn dispatch_key(&mut self, key: KeyEvent, prompt_empty: bool) -> ShellAction {
        if self.overlay != Overlay::None {
            if self.overlay == Overlay::Picker {
                // The copied Grok picker owns its own Esc ladder: first leave
                // search/selection state, then request overlay close.
                return ShellAction::PickerKey(key);
            }
            if self.overlay == Overlay::Queue {
                return ShellAction::QueueKey(key);
            }
            if self.overlay == Overlay::Interaction {
                return ShellAction::InteractionKey(key);
            }
            if self.overlay == Overlay::FileSearch {
                return ShellAction::FileSearchKey(key);
            }
            if self.overlay == Overlay::ImagePreview {
                return ShellAction::ImagePreviewKey(key);
            }
            if self.overlay == Overlay::AgentTasks {
                return ShellAction::AgentTasksKey(key);
            }
            if self.overlay == Overlay::Dashboard {
                return ShellAction::DashboardKey(key);
            }
            if key.code == KeyCode::Esc {
                self.close_overlay();
                return ShellAction::CloseOverlay;
            }
            return match self.owner {
                KeyOwner::Picker => ShellAction::PickerKey(key),
                _ => ShellAction::PromptKey(key),
            };
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return ShellAction::Quit;
        }
        if key.code == KeyCode::Esc {
            return if prompt_empty {
                ShellAction::Quit
            } else {
                ShellAction::ClearPrompt
            };
        }
        if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
            return ShellAction::PromptNewline;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(
                key.code,
                KeyCode::Char('p') | KeyCode::Char('n') | KeyCode::Char('x')
            )
        {
            self.owner = KeyOwner::Prompt;
            return ShellAction::PromptKey(key);
        }
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::ALT) {
            self.owner = KeyOwner::Prompt;
            return ShellAction::TogglePromptMode;
        }
        if prompt_empty {
            match key.code {
                KeyCode::Char('p') if key.modifiers.is_empty() => {
                    self.open_picker();
                    return ShellAction::OpenPicker;
                }
                KeyCode::Char('q') => {
                    self.open_queue();
                    return ShellAction::OpenQueue;
                }
                KeyCode::Char('i') if key.modifiers.is_empty() => {
                    self.open_interaction();
                    return ShellAction::OpenInteraction;
                }
                KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.open_file_search();
                    return ShellAction::OpenFileSearch;
                }
                KeyCode::Char('i') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.open_image_preview();
                    return ShellAction::OpenImagePreview;
                }
                KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.open_agent_tasks();
                    return ShellAction::OpenAgentTasks;
                }
                KeyCode::Char('d') => {
                    self.previous_owner = self.owner;
                    self.overlay = Overlay::Dashboard;
                    self.owner = KeyOwner::Dashboard;
                    return ShellAction::OpenDashboard;
                }
                KeyCode::Up | KeyCode::Char('k') => return ShellAction::ScrollUp(1),
                KeyCode::Down | KeyCode::Char('j') => return ShellAction::ScrollDown(1),
                KeyCode::PageUp => return ShellAction::ScrollUp(8),
                KeyCode::PageDown => return ShellAction::ScrollDown(8),
                _ => {}
            }
        }
        if key.code == KeyCode::Enter && !prompt_empty {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                return ShellAction::PromptNewline;
            }
            return ShellAction::SubmitPrompt;
        }
        self.owner = KeyOwner::Prompt;
        ShellAction::PromptKey(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn owner_priority_and_esc_ladder_are_deterministic() {
        let mut shell = AppShell::default();
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Char('p'))), true),
            ShellAction::OpenPicker
        );
        assert_eq!(shell.owner(), KeyOwner::Picker);
        assert!(matches!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), true),
            ShellAction::PickerKey(_)
        ));
        shell.close_overlay();
        assert_eq!(shell.owner(), KeyOwner::Transcript);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), false),
            ShellAction::ClearPrompt
        );
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), true),
            ShellAction::Quit
        );
    }

    #[test]
    fn all_overlay_events_have_one_owner() {
        let mut shell = AppShell::default();
        shell.open_picker();
        assert_eq!(shell.owner(), shell.overlay().owner());
        assert!(matches!(
            shell.dispatch(ShellEvent::Paste("x".into()), true),
            ShellAction::PickerPaste(_)
        ));
        assert!(matches!(
            shell.dispatch(
                ShellEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: 1,
                    row: 1,
                    modifiers: KeyModifiers::NONE
                }),
                true
            ),
            ShellAction::PickerMouse(_)
        ));
        shell.close_overlay();
        shell.open_queue();
        assert!(matches!(
            shell.dispatch(
                ShellEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    column: 1,
                    row: 1,
                    modifiers: KeyModifiers::NONE
                }),
                true
            ),
            ShellAction::QueueMouse(_)
        ));
        shell.open_interaction();
        assert!(matches!(
            shell.dispatch(
                ShellEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: 1,
                    row: 1,
                    modifiers: KeyModifiers::NONE
                }),
                true
            ),
            ShellAction::InteractionMouse(_)
        ));
    }

    #[test]
    fn dashboard_and_prompt_history_shortcuts_have_distinct_owners() {
        let mut shell = AppShell::default();
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Char('d'))), true),
            ShellAction::OpenDashboard
        );
        assert_eq!(shell.owner(), KeyOwner::Dashboard);
        shell.close_overlay();
        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert!(matches!(
            shell.dispatch(ShellEvent::Key(ctrl_p), true),
            ShellAction::PromptKey(_)
        ));
    }

    #[test]
    fn transcript_mouse_clicks_are_not_dropped_by_shell_dispatch() {
        let mut shell = AppShell::default();
        let action = shell.dispatch(
            ShellEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: 4,
                row: 3,
                modifiers: KeyModifiers::NONE,
            }),
            true,
        );
        assert!(matches!(action, ShellAction::TranscriptMouse(_)));
    }

    #[test]
    fn alt_s_toggles_prompt_mode_without_stealing_plain_s() {
        let mut shell = AppShell::default();
        let alt_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(alt_s), true),
            ShellAction::TogglePromptMode
        );
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Char('s'))), true),
            ShellAction::PromptKey(key(KeyCode::Char('s')))
        );
    }

    #[test]
    fn semantic_snapshot_captures_overlay_z_order_and_cursor_owner() {
        let mut shell = AppShell::default();
        let base = shell.snapshot();
        assert!(!base.dim_layer);
        shell.open_picker();
        let overlay = shell.snapshot();
        assert!(overlay.dim_layer);
        assert_eq!(overlay.z_order, vec![Overlay::Picker]);
        assert_eq!(overlay.cursor_owner, KeyOwner::Picker);
        assert!(KeyOwner::Interaction.priority() < KeyOwner::Prompt.priority());
    }

    #[test]
    fn file_search_has_a_distinct_owner_and_input_route() {
        let mut shell = AppShell::default();
        assert_eq!(
            shell.dispatch(
                ShellEvent::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)),
                true,
            ),
            ShellAction::OpenFileSearch
        );
        assert_eq!(shell.overlay(), Overlay::FileSearch);
        assert_eq!(shell.owner(), KeyOwner::FileSearch);
        assert!(matches!(
            shell.dispatch(ShellEvent::Paste("src/".into()), true),
            ShellAction::FileSearchPaste(_)
        ));
        assert!(matches!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), true),
            ShellAction::FileSearchKey(_)
        ));
        shell.close_overlay();
        assert!(matches!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Char('f'))), true),
            ShellAction::PromptKey(_)
        ));
    }

    #[test]
    fn media_and_agent_task_overlays_have_explicit_ctrl_shortcuts() {
        let mut shell = AppShell::default();
        let ctrl_i = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(ctrl_i), true),
            ShellAction::OpenImagePreview
        );
        assert_eq!(shell.owner(), KeyOwner::ImagePreview);
        shell.close_overlay();
        let ctrl_t = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(ctrl_t), true),
            ShellAction::OpenAgentTasks
        );
        assert_eq!(shell.owner(), KeyOwner::AgentTasks);
    }

    #[test]
    fn resize_invalidates_layout_and_adapts_prompt_height() {
        let mut shell = AppShell::default();
        let first = shell.layout(Rect::new(0, 0, 80, 24));
        let revision = shell.layout_revision();
        let second = shell.layout(Rect::new(0, 0, 40, 12));
        assert!(second.body.height < first.body.height);
        assert!(shell.layout_revision() > revision);
        assert!(matches!(
            shell.dispatch(
                ShellEvent::Resize {
                    width: 100,
                    height: 30
                },
                true
            ),
            ShellAction::Resized(_)
        ));
    }
}
