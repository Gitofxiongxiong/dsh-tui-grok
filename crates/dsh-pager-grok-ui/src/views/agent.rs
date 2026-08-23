//! Grok-derived AppView/AgentView composition boundary.
//!
//! The shell owns focus and overlay state; this module owns the stable geometry
//! of the main agent surface. Keeping this boundary data-only lets semantic
//! snapshots and the terminal renderer use the same layout contract.

use dsh_pager_protocol::PromptMode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::{AppShell, ShellLayout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentViewLayout {
    pub header: Rect,
    pub transcript: Rect,
    pub rail: Rect,
    pub prompt: Rect,
    pub footer: Rect,
}

impl AgentViewLayout {
    pub fn shell_layout(self) -> ShellLayout {
        ShellLayout {
            header: self.header,
            body: self.transcript,
            prompt: self.prompt,
            footer: self.footer,
        }
    }
}

pub struct AgentView;

impl AgentView {
    /// Build the complete main-surface geometry from the shell's one layout
    /// revision. No renderer or host state is consulted here.
    pub fn layout(shell: &mut AppShell, area: Rect) -> AgentViewLayout {
        let shell_layout = shell.layout(area);
        let body = shell_layout.body;
        let (transcript, rail) = if body.width >= 6 {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1), Constraint::Length(2)])
                .split(body);
            (columns[0], columns[1])
        } else {
            (body, Rect::new(body.right(), body.y, 0, body.height))
        };
        AgentViewLayout {
            header: shell_layout.header,
            transcript,
            rail,
            prompt: shell_layout.prompt,
            footer: shell_layout.footer,
        }
    }

    pub fn prompt_label(mode: PromptMode, running: bool) -> &'static str {
        match (mode, running) {
            (PromptMode::Steer, true) => " ! ",
            (PromptMode::Steer, false) => " ~ ",
            (PromptMode::Queue, true) => " > ",
            (PromptMode::Queue, false) => " · ",
        }
    }

    pub fn mode_label(mode: PromptMode) -> &'static str {
        match mode {
            PromptMode::Queue => "queue",
            PromptMode::Steer => "steer",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppShell;

    #[test]
    fn agent_layout_keeps_rail_out_of_narrow_transcript() {
        let mut shell = AppShell::default();
        let wide = AgentView::layout(&mut shell, Rect::new(0, 0, 80, 24));
        assert_eq!(wide.rail.width, 2);
        assert_eq!(wide.transcript.right(), wide.rail.x);

        let narrow = AgentView::layout(&mut shell, Rect::new(0, 0, 5, 12));
        assert_eq!(narrow.rail.width, 0);
        assert_eq!(narrow.transcript.width, 5);
    }

    #[test]
    fn prompt_labels_expose_queue_and_steer_modes() {
        assert_eq!(AgentView::prompt_label(PromptMode::Queue, true), " > ");
        assert_eq!(AgentView::prompt_label(PromptMode::Steer, true), " ! ");
        assert_eq!(AgentView::mode_label(PromptMode::Steer), "steer");
    }
}
