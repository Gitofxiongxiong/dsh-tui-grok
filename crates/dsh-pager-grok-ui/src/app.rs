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
    Permission,
    Interaction,
    FileSearch,
    ImagePreview,
    AgentTasks,
    Modal,
    Dashboard,
}

/// Whether the pager uses the alternate screen (fullscreen) or stays inline.
///
/// Copied from Grok `crate::app::ScreenMode` so vendored `ActionRegistry`
/// defaults keep their mode-specific Ctrl+G / dashboard / scrollback set.
/// Inline/Minimal are constructed by registry tests and future mode switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ScreenMode {
    Fullscreen,
    Inline,
    Minimal,
}

impl ScreenMode {
    pub(crate) fn is_minimal(self) -> bool {
        matches!(self, Self::Minimal)
    }
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
            Self::Permission => KeyOwner::Interaction,
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

/// Homepage (no overlay) inputs for the Esc / Ctrl+C policy.
///
/// Mirrors Grok `AgentView::try_handle_esc_policy` plus the Ctrl+C
/// CancelTurn ladder: overlays still steal keys first; this only applies
/// once the agent surface owns the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeKeyState {
    pub prompt_empty: bool,
    pub turn_running: bool,
    pub cancel_pending: bool,
}

impl HomeKeyState {
    pub const fn idle_prompt(prompt_empty: bool) -> Self {
        Self {
            prompt_empty,
            turn_running: false,
            cancel_pending: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellAction {
    None,
    Quit,
    /// Interrupt the in-flight turn without leaving the session.
    CancelTurn,
    CloseOverlay,
    ClearPrompt,
    ScrollUp(u16),
    ScrollDown(u16),
    SubmitPrompt,
    PromptNewline,
    CycleSessionMode,
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
            dim_layer: !matches!(
                self.overlay,
                Overlay::None | Overlay::Permission | Overlay::Interaction
            ),
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

    pub fn open_permission(&mut self) {
        self.previous_owner = self.owner;
        self.overlay = Overlay::Permission;
        self.owner = KeyOwner::Interaction;
    }

    /// Match Grok's permission-card Esc rung: keep the blocking card visible
    /// while handing keyboard ownership to scrollback.
    pub fn park_permission(&mut self) {
        if matches!(self.overlay, Overlay::Permission | Overlay::Interaction) {
            self.owner = KeyOwner::Transcript;
        }
    }

    pub fn focus_permission(&mut self) {
        if matches!(self.overlay, Overlay::Permission | Overlay::Interaction) {
            self.owner = KeyOwner::Interaction;
        }
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
        self.dispatch_home(event, HomeKeyState::idle_prompt(prompt_empty))
    }

    pub fn dispatch_home(&mut self, event: ShellEvent, home: HomeKeyState) -> ShellAction {
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
                if self.overlay == Overlay::Permission {
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
            ShellEvent::Key(key) => self.dispatch_key(key, home),
        }
    }

    fn dispatch_key(&mut self, key: KeyEvent, home: HomeKeyState) -> ShellAction {
        let prompt_empty = home.prompt_empty;
        if self.overlay != Overlay::None {
            if self.overlay == Overlay::Picker {
                // The copied Grok picker owns its own Esc ladder: first leave
                // search/selection state, then request overlay close.
                return ShellAction::PickerKey(key);
            }
            if self.overlay == Overlay::Queue {
                return ShellAction::QueueKey(key);
            }
            if matches!(self.overlay, Overlay::Interaction | Overlay::Permission) {
                if self.owner == KeyOwner::Interaction {
                    return ShellAction::InteractionKey(key);
                }
                return match key.code {
                    KeyCode::Char('i') | KeyCode::Enter | KeyCode::Tab => {
                        self.focus_permission();
                        ShellAction::OpenInteraction
                    }
                    KeyCode::Up | KeyCode::Char('k') => ShellAction::ScrollUp(1),
                    KeyCode::Down | KeyCode::Char('j') => ShellAction::ScrollDown(1),
                    KeyCode::PageUp => ShellAction::ScrollUp(8),
                    KeyCode::PageDown => ShellAction::ScrollDown(8),
                    _ => ShellAction::None,
                };
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
            // Grok CancelTurn / Ctrl+C ladder: a draft is cleared first and
            // the turn keeps running; an empty running prompt cancels the
            // turn; idle empty or already-cancelling empty quits. Esc is
            // the one that retries cancel instead of escalating to quit.
            if !prompt_empty {
                return ShellAction::ClearPrompt;
            }
            if home.turn_running && !home.cancel_pending {
                return ShellAction::CancelTurn;
            }
            return ShellAction::Quit;
        }
        if key.code == KeyCode::Esc {
            // Grok `try_handle_esc_policy` (non-vim homepage): running or
            // cancelling → cancel/retry immediately, even with a draft;
            // idle draft → clear; idle empty → swallow. Esc never quits.
            if home.turn_running || home.cancel_pending {
                return ShellAction::CancelTurn;
            }
            return if prompt_empty {
                ShellAction::None
            } else {
                ShellAction::ClearPrompt
            };
        }
        if key.code == KeyCode::Enter
            && key
                .modifiers
                .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT)
        {
            return ShellAction::PromptNewline;
        }
        if key.code == KeyCode::BackTab
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            self.owner = KeyOwner::Prompt;
            return ShellAction::CycleSessionMode;
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
        if prompt_empty {
            match key.code {
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
                KeyCode::Char('t') | KeyCode::Char('g')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
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
        assert!(matches!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Char('p'))), true),
            ShellAction::PromptKey(_)
        ));
        assert_eq!(shell.overlay(), Overlay::None);
        shell.open_picker();
        assert_eq!(shell.owner(), KeyOwner::Picker);
        assert!(matches!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), true),
            ShellAction::PickerKey(_)
        ));
        shell.close_overlay();
        assert_eq!(shell.owner(), KeyOwner::Prompt);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), false),
            ShellAction::ClearPrompt
        );
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), true),
            ShellAction::None
        );
    }

    fn ctrl_c() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    }

    fn running(prompt_empty: bool) -> HomeKeyState {
        HomeKeyState {
            prompt_empty,
            turn_running: true,
            cancel_pending: false,
        }
    }

    fn cancelling(prompt_empty: bool) -> HomeKeyState {
        HomeKeyState {
            prompt_empty,
            turn_running: false,
            cancel_pending: true,
        }
    }

    #[test]
    fn homepage_esc_cancels_a_running_turn_instead_of_quitting() {
        let mut shell = AppShell::default();
        assert_eq!(
            shell.dispatch_home(ShellEvent::Key(key(KeyCode::Esc)), running(true)),
            ShellAction::CancelTurn
        );
        assert_eq!(
            shell.dispatch_home(ShellEvent::Key(key(KeyCode::Esc)), running(false)),
            ShellAction::CancelTurn,
            "mid-turn Esc keeps the draft and cancels, matching Grok non-vim"
        );
        assert_eq!(
            shell.dispatch_home(ShellEvent::Key(key(KeyCode::Esc)), cancelling(true)),
            ShellAction::CancelTurn,
            "Esc while cancelling retries cancel instead of quitting"
        );
    }

    #[test]
    fn homepage_esc_idle_empty_is_swallowed() {
        let mut shell = AppShell::default();
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), true),
            ShellAction::None
        );
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), false),
            ShellAction::ClearPrompt
        );
    }

    #[test]
    fn homepage_ctrl_c_matches_grok_cancel_then_quit_ladder() {
        let mut shell = AppShell::default();
        assert_eq!(
            shell.dispatch_home(ShellEvent::Key(ctrl_c()), running(true)),
            ShellAction::CancelTurn
        );
        assert_eq!(
            shell.dispatch_home(ShellEvent::Key(ctrl_c()), running(false)),
            ShellAction::ClearPrompt,
            "Ctrl+C with a draft clears first and leaves the turn running"
        );
        assert_eq!(
            shell.dispatch(ShellEvent::Key(ctrl_c()), false),
            ShellAction::ClearPrompt
        );
        assert_eq!(
            shell.dispatch(ShellEvent::Key(ctrl_c()), true),
            ShellAction::Quit
        );
        assert_eq!(
            shell.dispatch_home(ShellEvent::Key(ctrl_c()), cancelling(true)),
            ShellAction::Quit,
            "Ctrl+C while cancelling escalates to quit"
        );
        assert_eq!(
            shell.dispatch_home(
                ShellEvent::Key(ctrl_c()),
                HomeKeyState {
                    prompt_empty: true,
                    turn_running: true,
                    cancel_pending: true,
                }
            ),
            ShellAction::Quit,
            "host still running plus a pending cancel is the cancelling state"
        );
    }

    #[test]
    fn overlay_still_owns_esc_while_a_turn_is_running() {
        let mut shell = AppShell::default();
        shell.open_file_search();
        assert!(matches!(
            shell.dispatch_home(ShellEvent::Key(key(KeyCode::Esc)), running(true)),
            ShellAction::FileSearchKey(_)
        ));
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
    fn grok_mode_and_newline_chords_reach_the_same_actions() {
        let mut shell = AppShell::default();
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::BackTab)), true),
            ShellAction::CycleSessionMode
        );
        let shift_tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(shift_tab), true),
            ShellAction::CycleSessionMode
        );
        let alt_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(alt_enter), false),
            ShellAction::PromptNewline
        );
        let shift_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(shift_enter), false),
            ShellAction::PromptNewline
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
    fn permission_card_parks_focus_without_closing_or_dimming() {
        let mut shell = AppShell::default();
        shell.open_permission();
        let focused = shell.snapshot();
        assert_eq!(focused.overlay, Overlay::Permission);
        assert_eq!(focused.cursor_owner, KeyOwner::Interaction);
        assert!(!focused.dim_layer);

        shell.park_permission();
        assert_eq!(shell.overlay(), Overlay::Permission);
        assert_eq!(shell.owner(), KeyOwner::Transcript);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Down)), true),
            ShellAction::ScrollDown(1)
        );
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Enter)), true),
            ShellAction::OpenInteraction
        );
        assert_eq!(shell.owner(), KeyOwner::Interaction);
    }

    #[test]
    fn question_card_parks_focus_without_closing_or_dimming() {
        let mut shell = AppShell::default();
        shell.open_interaction();
        let focused = shell.snapshot();
        assert_eq!(focused.overlay, Overlay::Interaction);
        assert_eq!(focused.cursor_owner, KeyOwner::Interaction);
        assert!(!focused.dim_layer);

        shell.park_permission();
        assert_eq!(shell.overlay(), Overlay::Interaction);
        assert_eq!(shell.owner(), KeyOwner::Transcript);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Down)), true),
            ShellAction::ScrollDown(1)
        );
        assert_eq!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Enter)), true),
            ShellAction::OpenInteraction
        );
        assert_eq!(shell.owner(), KeyOwner::Interaction);
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
    fn agent_tasks_overlay_owns_esc_q_and_mouse() {
        let mut shell = AppShell::default();
        shell.open_agent_tasks();
        assert_eq!(shell.overlay(), Overlay::AgentTasks);
        assert!(matches!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Esc)), true),
            ShellAction::AgentTasksKey(_)
        ));
        assert!(matches!(
            shell.dispatch(ShellEvent::Key(key(KeyCode::Char('q'))), true),
            ShellAction::AgentTasksKey(_)
        ));
        assert!(matches!(
            shell.dispatch(
                ShellEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
                    column: 80,
                    row: 4,
                    modifiers: KeyModifiers::NONE
                }),
                true
            ),
            ShellAction::AgentTasksMouse(_)
        ));
        shell.close_overlay();
        assert_eq!(shell.overlay(), Overlay::None);
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
        shell.close_overlay();
        let ctrl_g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        assert_eq!(
            shell.dispatch(ShellEvent::Key(ctrl_g), true),
            ShellAction::OpenAgentTasks
        );
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
