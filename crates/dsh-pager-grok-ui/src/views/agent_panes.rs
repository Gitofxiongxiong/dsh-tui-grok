//! Grok-derived task/subagent pane backed by DSH-owned agent snapshots.
//!
//! The controller deliberately stores stable IDs instead of a row index. A
//! refreshed task catalog can reorder or remove rows without turning an
//! interrupt key into an action for a different subagent.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};

use crate::{
    host_adapter::{AgentSnapshot, FeatureStatus, SubagentRow},
    render::line_utils::truncate_str,
    theme::Theme,
};

/// Stable selection identity across task/subagent snapshot refreshes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentItemId {
    Task(String),
    Subagent(String),
}

#[derive(Debug, Default, Clone)]
pub struct AgentPaneController {
    selected: Option<AgentItemId>,
    order: Vec<AgentItemId>,
}

impl AgentPaneController {
    pub fn clear(&mut self) {
        self.selected = None;
        self.order.clear();
    }

    pub fn sync(&mut self, snapshot: &AgentSnapshot) {
        self.order.clear();
        self.order.extend(
            snapshot
                .tasks
                .iter()
                .map(|task| AgentItemId::Task(task.id.clone())),
        );
        self.order.extend(
            snapshot
                .subagents
                .iter()
                .map(|agent| AgentItemId::Subagent(agent.id.clone())),
        );
        if self
            .selected
            .as_ref()
            .is_none_or(|selected| !self.order.contains(selected))
        {
            self.selected = self.order.first().cloned();
        }
    }

    pub fn selected(&self) -> Option<&AgentItemId> {
        self.selected.as_ref()
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.order.is_empty() {
            self.selected = None;
            return;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|selected| self.order.iter().position(|item| item == selected))
            .unwrap_or(0);
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta as usize)
                .min(self.order.len().saturating_sub(1))
        };
        self.selected = self.order.get(next).cloned();
    }

    pub fn selected_subagent<'a>(&self, snapshot: &'a AgentSnapshot) -> Option<&'a SubagentRow> {
        let AgentItemId::Subagent(id) = self.selected.as_ref()? else {
            return None;
        };
        snapshot.subagents.iter().find(|row| row.id == *id)
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected
            .as_ref()
            .and_then(|selected| self.order.iter().position(|item| item == selected))
    }
}

pub fn render_agent_tasks_content(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &AgentSnapshot,
    controller: &AgentPaneController,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    match snapshot.status {
        FeatureStatus::Unsupported => put(
            buffer,
            area,
            "Agent task/subagent state unavailable",
            theme.warning,
            theme,
        ),
        FeatureStatus::Pending => put(
            buffer,
            area,
            "Waiting for authoritative agent task snapshot...",
            theme.gray,
            theme,
        ),
        FeatureStatus::Available => {
            let mut row = 0u16;
            if !snapshot.tasks.is_empty() {
                put_at(buffer, area, row, "Tasks", theme.gray_bright, theme, true);
                row = row.saturating_add(1);
            }
            for task in &snapshot.tasks {
                if row >= area.height.saturating_sub(2) {
                    break;
                }
                let selected = controller.selected() == Some(&AgentItemId::Task(task.id.clone()));
                let marker = if selected { "▸" } else { " " };
                let line = format!(
                    "{marker} {} [{}] {}{}",
                    task.label,
                    task.status,
                    task.kind,
                    task.detail
                        .as_deref()
                        .map(|detail| format!(" · {detail}"))
                        .unwrap_or_default()
                );
                put_at(
                    buffer,
                    area,
                    row,
                    &line,
                    if selected {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    },
                    theme,
                    selected,
                );
                row = row.saturating_add(1);
            }
            if !snapshot.subagents.is_empty() && row < area.height.saturating_sub(2) {
                put_at(
                    buffer,
                    area,
                    row,
                    "Subagents",
                    theme.gray_bright,
                    theme,
                    true,
                );
                row = row.saturating_add(1);
            }
            for subagent in &snapshot.subagents {
                if row >= area.height.saturating_sub(2) {
                    break;
                }
                let selected =
                    controller.selected() == Some(&AgentItemId::Subagent(subagent.id.clone()));
                let marker = if selected { "▸" } else { " " };
                let mode = subagent.mode.as_deref().unwrap_or("unknown");
                let status = subagent.status.as_deref().unwrap_or("unknown");
                let line = format!(
                    "{marker} {} [{status}] {mode} · {}",
                    subagent.label, subagent.id
                );
                put_at(
                    buffer,
                    area,
                    row,
                    &line,
                    if selected {
                        theme.accent_assistant
                    } else {
                        theme.gray
                    },
                    theme,
                    selected,
                );
                row = row.saturating_add(1);
            }
            if row == 0 {
                put_at(
                    buffer,
                    area,
                    0,
                    "No active tasks or subagents",
                    theme.gray,
                    theme,
                    false,
                );
            }
        }
    }
    put_at(
        buffer,
        area,
        area.height.saturating_sub(1),
        "↑/↓ select · x interrupt selected subagent · Esc close",
        theme.gray_dim,
        theme,
        false,
    );
}

/// Draw the task and subagent slices allocated by the shared AgentView layout.
/// Heights and ordering come from that layout snapshot; this renderer adds no
/// competing geometry or hidden placeholder rows.
pub fn render_inline_agent_panes(
    buffer: &mut Buffer,
    tasks_area: Rect,
    catalog_area: Rect,
    snapshot: &AgentSnapshot,
    theme: &Theme,
) {
    if tasks_area.height > 0 {
        put_at(
            buffer,
            tasks_area,
            0,
            "Tasks",
            theme.gray_bright,
            theme,
            true,
        );
        for (index, task) in snapshot.tasks.iter().enumerate() {
            let row = index as u16 + 1;
            if row >= tasks_area.height {
                break;
            }
            put_at(
                buffer,
                tasks_area,
                row,
                &format!("{} [{}] {}", task.status, task.id, task.label),
                theme.text_secondary,
                theme,
                false,
            );
        }
    }
    if catalog_area.height > 0 {
        put_at(
            buffer,
            catalog_area,
            0,
            "Subagents",
            theme.gray_bright,
            theme,
            true,
        );
        for (index, subagent) in snapshot.subagents.iter().enumerate() {
            let row = index as u16 + 1;
            if row >= catalog_area.height {
                break;
            }
            put_at(
                buffer,
                catalog_area,
                row,
                &format!(
                    "{} [{}] {}",
                    subagent.status.as_deref().unwrap_or("unknown"),
                    subagent.id,
                    subagent.label
                ),
                theme.accent_assistant,
                theme,
                false,
            );
        }
    }
}

fn put(buffer: &mut Buffer, area: Rect, text: &str, color: ratatui::style::Color, theme: &Theme) {
    put_at(buffer, area, 0, text, color, theme, false);
}

fn put_at(
    buffer: &mut Buffer,
    area: Rect,
    row: u16,
    text: &str,
    color: ratatui::style::Color,
    theme: &Theme,
    bold: bool,
) {
    let y = area.y.saturating_add(row);
    if y >= area.bottom() {
        return;
    }
    let mut style = Style::default().fg(color).bg(theme.bg_base);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    buffer.set_string(area.x, y, truncate_str(text, area.width as usize), style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_adapter::{AgentSnapshot, FeatureStatus, SubagentRow, TaskRow};
    use ratatui::{buffer::Buffer, layout::Rect};

    fn snapshot() -> AgentSnapshot {
        AgentSnapshot {
            status: FeatureStatus::Available,
            tasks: vec![TaskRow {
                id: "task-a".into(),
                kind: "job".into(),
                label: "build".into(),
                status: "running".into(),
                detail: None,
            }],
            subagents: vec![SubagentRow {
                id: "child-a".into(),
                parent_id: "parent".into(),
                label: "research".into(),
                mode: Some("continuable".into()),
                status: Some("running".into()),
            }],
        }
    }

    #[test]
    fn selection_follows_stable_id_after_reorder_and_removal() {
        let mut controller = AgentPaneController::default();
        let mut state = snapshot();
        controller.sync(&state);
        controller.move_selection(1);
        assert_eq!(
            controller.selected(),
            Some(&AgentItemId::Subagent("child-a".into()))
        );
        state.tasks[0].id = "task-b".into();
        controller.sync(&state);
        assert_eq!(
            controller.selected(),
            Some(&AgentItemId::Subagent("child-a".into()))
        );
        state.subagents.clear();
        controller.sync(&state);
        assert_eq!(
            controller.selected(),
            Some(&AgentItemId::Task("task-b".into()))
        );
    }

    #[test]
    fn renderer_keeps_status_fallback_explicit() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 8));
        let snapshot = AgentSnapshot {
            status: FeatureStatus::Pending,
            ..Default::default()
        };
        render_agent_tasks_content(
            &mut buffer,
            Rect::new(1, 1, 78, 6),
            &snapshot,
            &AgentPaneController::default(),
            Theme::current(),
        );
        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Waiting for authoritative"));
    }
}
