//! Grok-derived task/subagent surfaces backed by DSH-owned snapshots.
//!
//! The top strip is intentionally a compact watcher cue.  It expands into a
//! single shared list and a detail view, so mouse and keyboard selection keep
//! the same stable `AgentItemId` even when the host refreshes its snapshot.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

use crate::{
    glyphs,
    host_adapter::{AgentSnapshot, ChildTranscriptView, FeatureStatus, SubagentRow, TaskRow},
    render::line_utils::truncate_str,
    theme::Theme,
};

/// Show each watcher pulse frame for this many animation ticks.
/// Copied from Grok's `MONITOR_PULSE_DIVISOR`: at ~30fps, 8 ticks ≈ 267ms
/// per frame, so `○ ◎ ◉ ◎` completes in about 1.07s.
pub const MONITOR_PULSE_DIVISOR: u64 = 8;

/// Stable selection identity across task/subagent snapshot refreshes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentItemId {
    Task(String),
    Subagent(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPaneRowKind {
    Header,
    Item,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPaneRow {
    pub id: AgentItemId,
    pub rect: Rect,
    pub kind: AgentPaneRowKind,
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
        let mut tasks = snapshot.tasks.iter().collect::<Vec<_>>();
        tasks.sort_by(task_ordering);
        self.order.extend(
            tasks
                .into_iter()
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

    pub fn select(&mut self, id: AgentItemId) {
        if self.order.contains(&id) {
            self.selected = Some(id);
        }
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

/// Height needed for the compact top strip.  A running task/subagent gets a
/// single line; the strip is hidden when the host has no active work.
pub fn inline_agent_pane_height(snapshot: &AgentSnapshot, view_height: u16) -> u16 {
    if snapshot.status != FeatureStatus::Available {
        return 0;
    }
    let tasks = snapshot.tasks.iter().filter(|task| task.is_live()).count();
    let subagents = snapshot
        .subagents
        .iter()
        .filter(|agent| agent.is_running())
        .count();
    let groups = u16::from(tasks > 0) + u16::from(subagents > 0);
    let rows = tasks.saturating_add(subagents) as u16;
    if rows == 0 || groups == 0 {
        return 0;
    }
    let cap = if view_height >= 12 {
        (view_height / 6).max(3)
    } else {
        3
    };
    (rows.saturating_add(groups)).min(cap).min(8)
}

/// Render the expanded list used by `Ctrl+G`/`Ctrl+T`.
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
                put_at(
                    buffer,
                    area,
                    row,
                    &format!("Tasks ({})", snapshot.tasks.len()),
                    theme.gray_bright,
                    theme,
                    true,
                );
                row = row.saturating_add(1);
            }
            let mut tasks = snapshot.tasks.iter().collect::<Vec<_>>();
            tasks.sort_by(task_ordering);
            for task in tasks {
                if row >= area.height.saturating_sub(2) {
                    break;
                }
                let id = AgentItemId::Task(task.id.clone());
                let selected = controller.selected() == Some(&id);
                put_at(
                    buffer,
                    area,
                    row,
                    &task_line(task, selected, true),
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
                    &format!("Subagents ({})", snapshot.subagents.len()),
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
                let id = AgentItemId::Subagent(subagent.id.clone());
                let selected = controller.selected() == Some(&id);
                put_at(
                    buffer,
                    area,
                    row,
                    &subagent_line(subagent, selected),
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
                    "No tasks or subagents",
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
        "↑/↓ select · Enter open · x interrupt subagent · Esc close",
        theme.gray_dim,
        theme,
        false,
    );
}

/// Draw the compact top strip and return item hit rectangles for mouse input.
pub fn render_inline_agent_panes(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &AgentSnapshot,
    controller: &AgentPaneController,
    theme: &Theme,
) -> Vec<AgentPaneRow> {
    let mut hits = Vec::new();
    if area.width == 0 || area.height == 0 {
        return hits;
    }
    let mut row = 0u16;
    let groups = [
        (
            "Tasks",
            snapshot.tasks.iter().filter(|task| task.is_live()).count(),
        ),
        (
            "Subagents",
            snapshot
                .subagents
                .iter()
                .filter(|agent| agent.is_running())
                .count(),
        ),
    ];
    for (group, count) in groups {
        if count == 0 || row >= area.height {
            continue;
        }
        let header = Rect::new(area.x, area.y.saturating_add(row), area.width, 1);
        put_at(
            buffer,
            area,
            row,
            &format!("▾ {group} {count}"),
            theme.gray_bright,
            theme,
            true,
        );
        row = row.saturating_add(1);
        let _ = header;
        match group {
            "Tasks" => {
                let mut tasks = snapshot
                    .tasks
                    .iter()
                    .filter(|task| task.is_live())
                    .collect::<Vec<_>>();
                tasks.sort_by(task_ordering);
                for task in tasks {
                    if row >= area.height {
                        break;
                    }
                    let rect = Rect::new(area.x, area.y.saturating_add(row), area.width, 1);
                    let id = AgentItemId::Task(task.id.clone());
                    put_at(
                        buffer,
                        area,
                        row,
                        &task_line(task, controller.selected() == Some(&id), false),
                        if controller.selected() == Some(&id) {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        },
                        theme,
                        controller.selected() == Some(&id),
                    );
                    hits.push(AgentPaneRow {
                        id,
                        rect,
                        kind: AgentPaneRowKind::Item,
                    });
                    row = row.saturating_add(1);
                }
            }
            "Subagents" => {
                for subagent in snapshot.subagents.iter().filter(|agent| agent.is_running()) {
                    if row >= area.height {
                        break;
                    }
                    let rect = Rect::new(area.x, area.y.saturating_add(row), area.width, 1);
                    let id = AgentItemId::Subagent(subagent.id.clone());
                    put_at(
                        buffer,
                        area,
                        row,
                        &subagent_line(subagent, controller.selected() == Some(&id)),
                        if controller.selected() == Some(&id) {
                            theme.accent_assistant
                        } else {
                            theme.gray
                        },
                        theme,
                        controller.selected() == Some(&id),
                    );
                    hits.push(AgentPaneRow {
                        id,
                        rect,
                        kind: AgentPaneRowKind::Item,
                    });
                    row = row.saturating_add(1);
                }
            }
            _ => {}
        }
    }
    hits
}

pub fn render_agent_detail_content(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &AgentSnapshot,
    id: &AgentItemId,
    child: Option<&ChildTranscriptView>,
    theme: &Theme,
) -> Rect {
    render_agent_detail_chrome(
        buffer,
        area,
        snapshot,
        id,
        child.and_then(|view| view.error.as_deref()),
        child.is_none(),
        theme,
    )
}

/// Paint detail chrome and return the inner body rect reserved for the child
/// Grok scrollback (or empty when a status line occupies the body).
pub fn render_agent_detail_chrome(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &AgentSnapshot,
    id: &AgentItemId,
    error: Option<&str>,
    loading: bool,
    theme: &Theme,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }
    let mut row = 0u16;
    match id {
        AgentItemId::Task(id) => {
            let Some(task) = snapshot.tasks.iter().find(|task| task.id == *id) else {
                put(buffer, area, "Task no longer exists", theme.warning, theme);
                return Rect::new(area.x, area.y, area.width, 0);
            };
            put_at(
                buffer,
                area,
                row,
                &format!(
                    "{} {}",
                    status_icon(&task.status, task.is_running()),
                    task.label
                ),
                theme.text_primary,
                theme,
                true,
            );
            row += 1;
            put_at(
                buffer,
                area,
                row,
                &format!(
                    "id: {} · kind: {} · status: {}",
                    task.id, task.kind, task.status
                ),
                theme.gray_bright,
                theme,
                false,
            );
            row += 1;
            if let Some(detail) = task.detail.as_deref() {
                put_at(
                    buffer,
                    area,
                    row,
                    &format!("detail: {detail}"),
                    theme.text_secondary,
                    theme,
                    false,
                );
                row = row.saturating_add(1);
            }
        }
        AgentItemId::Subagent(id) => {
            let Some(agent) = snapshot.subagents.iter().find(|agent| agent.id == *id) else {
                put(
                    buffer,
                    area,
                    "Subagent no longer exists",
                    theme.warning,
                    theme,
                );
                return Rect::new(area.x, area.y, area.width, 0);
            };
            put_at(
                buffer,
                area,
                row,
                &format!(
                    "{} {}",
                    status_icon(
                        agent.status.as_deref().unwrap_or("unknown"),
                        agent.is_running()
                    ),
                    agent.label
                ),
                theme.accent_assistant,
                theme,
                true,
            );
            row += 1;
            put_at(
                buffer,
                area,
                row,
                &format!(
                    "id: {} · mode: {} · status: {}",
                    agent.id,
                    agent.mode.as_deref().unwrap_or("unknown"),
                    agent.status.as_deref().unwrap_or("unknown")
                ),
                theme.gray_bright,
                theme,
                false,
            );
            row += 1;
            if let Some(activity) = agent.activity.as_deref() {
                put_at(
                    buffer,
                    area,
                    row,
                    &format!("activity: {activity}"),
                    theme.text_secondary,
                    theme,
                    false,
                );
                row += 1;
            }
            if let Some(model) = agent.model.as_deref() {
                put_at(
                    buffer,
                    area,
                    row,
                    &format!("model: {model}"),
                    theme.text_secondary,
                    theme,
                    false,
                );
                row = row.saturating_add(1);
            }
            row = row.saturating_add(1);
            if let Some(error) = error {
                put_at(
                    buffer,
                    area,
                    row,
                    &format!("history unavailable: {error}"),
                    theme.warning,
                    theme,
                    false,
                );
                row = row.saturating_add(1);
            } else if loading {
                put_at(
                    buffer,
                    area,
                    row,
                    "Loading child transcript…",
                    theme.gray,
                    theme,
                    false,
                );
                row = row.saturating_add(1);
            }
        }
    }
    put_at(
        buffer,
        area,
        area.height.saturating_sub(1),
        "q/Esc back · does not cancel the child",
        theme.gray_dim,
        theme,
        false,
    );
    let body_top = area
        .y
        .saturating_add(row.min(area.height.saturating_sub(1)));
    let body_bottom = area.bottom().saturating_sub(1);
    if body_top >= body_bottom || error.is_some() || loading || matches!(id, AgentItemId::Task(_)) {
        Rect::new(area.x, body_top, area.width, 0)
    } else {
        Rect::new(
            area.x,
            body_top,
            area.width,
            body_bottom.saturating_sub(body_top),
        )
    }
}

/// A watcher cue shown next to the normal turn status when background work is
/// running but no foreground turn is active.
pub fn render_watcher_cue(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &AgentSnapshot,
    tick: u64,
    theme: &Theme,
) -> Option<Rect> {
    let label = watcher_label(snapshot)?;
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let frame = watcher_pulse_frame(tick);
    put_at(
        buffer,
        area,
        0,
        &format!("{frame} {label} · Ctrl+G details"),
        theme.accent_assistant,
        theme,
        false,
    );
    Some(area)
}

pub fn watcher_pulse_frame(tick: u64) -> &'static str {
    let frames = glyphs::monitor_icon_frames();
    frames[(tick / MONITOR_PULSE_DIVISOR) as usize % frames.len()]
}

pub fn watcher_label(snapshot: &AgentSnapshot) -> Option<String> {
    let tasks = snapshot
        .tasks
        .iter()
        .filter(|task| task.is_running())
        .count();
    let subagents = snapshot
        .subagents
        .iter()
        .filter(|agent| agent.is_running())
        .count();
    if tasks == 0 && subagents == 0 {
        return None;
    }
    let mut parts = Vec::new();
    if tasks > 0 {
        parts.push(format!(
            "{tasks} background task{}",
            if tasks == 1 { "" } else { "s" }
        ));
    }
    if subagents > 0 {
        parts.push(format!(
            "{subagents} subagent{}",
            if subagents == 1 { "" } else { "s" }
        ));
    }
    Some(format!("{} still running", parts.join(" · ")))
}

fn task_line(task: &TaskRow, selected: bool, detailed: bool) -> String {
    let marker = if selected { "▸" } else { " " };
    let detail = task.detail.as_deref().unwrap_or("");
    let elapsed = elapsed_label(task.started_at_ms, task.finished_at_ms, task.is_live());
    let mut line = format!(
        "{marker} {} {} · {}",
        status_icon(&task.status, task.is_running()),
        task.label,
        task.status
    );
    if !detail.is_empty() {
        line.push_str(&format!(" · {detail}"));
    }
    if !elapsed.is_empty() {
        line.push_str(&format!(" · {elapsed}"));
    }
    if detailed && !task.id.is_empty() {
        line.push_str(&format!(" · {}", task.id));
    }
    line
}

fn subagent_line(agent: &SubagentRow, selected: bool) -> String {
    let marker = if selected { "▸" } else { " " };
    let activity = agent
        .activity
        .as_deref()
        .or(agent.status.as_deref())
        .unwrap_or("unknown");
    let mode = agent.mode.as_deref().unwrap_or("unknown");
    let elapsed = elapsed_label(
        agent.started_at_ms,
        agent.finished_at_ms,
        agent.is_running(),
    );
    let mut line = format!(
        "{marker} {} {} · {} · {}",
        status_icon(activity, agent.is_running()),
        agent.label,
        activity,
        mode
    );
    if !elapsed.is_empty() {
        line.push_str(&format!(" · {elapsed}"));
    }
    line
}

fn status_icon(status: &str, running: bool) -> &'static str {
    match status.to_ascii_lowercase().as_str() {
        "running" if running => "⠴",
        "stopping" => "!",
        "completed" | "done" | "success" | "succeeded" => "✓",
        "failed" | "error" | "cancelled" | "canceled" => "✗",
        _ if running => "⠴",
        _ => "·",
    }
}

fn elapsed_label(started_at_ms: Option<u64>, finished_at_ms: Option<u64>, live: bool) -> String {
    let Some(started_at_ms) = started_at_ms else {
        return String::new();
    };
    let end = if live {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(started_at_ms)
    } else {
        finished_at_ms.unwrap_or(started_at_ms)
    };
    let seconds = end.saturating_sub(started_at_ms) / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    }
}

fn task_ordering(left: &&TaskRow, right: &&TaskRow) -> std::cmp::Ordering {
    let left_live = left.is_live();
    let right_live = right.is_live();
    if left_live != right_live {
        return right_live.cmp(&left_live);
    }
    if left_live {
        return left
            .started_at_ms
            .unwrap_or_default()
            .cmp(&right.started_at_ms.unwrap_or_default())
            .then_with(|| left.id.cmp(&right.id));
    }
    right
        .finished_at_ms
        .or(right.started_at_ms)
        .unwrap_or_default()
        .cmp(
            &left
                .finished_at_ms
                .or(left.started_at_ms)
                .unwrap_or_default(),
        )
        .then_with(|| {
            left.started_at_ms
                .unwrap_or_default()
                .cmp(&right.started_at_ms.unwrap_or_default())
        })
        .then_with(|| left.id.cmp(&right.id))
}

fn put(buffer: &mut Buffer, area: Rect, text: &str, color: Color, theme: &Theme) {
    put_at(buffer, area, 0, text, color, theme, false);
}

fn put_at(
    buffer: &mut Buffer,
    area: Rect,
    row: u16,
    text: &str,
    color: Color,
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
                ..Default::default()
            }],
            subagents: vec![SubagentRow {
                id: "child-a".into(),
                parent_id: "parent".into(),
                label: "research".into(),
                mode: Some("continuable".into()),
                status: Some("running".into()),
                running: true,
                ..Default::default()
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
            &Theme::current(),
        );
        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Waiting for authoritative"));
    }

    #[test]
    fn watcher_counts_only_live_entries() {
        let snapshot = snapshot();
        assert_eq!(
            watcher_label(&snapshot).as_deref(),
            Some("1 background task · 1 subagent still running")
        );
        let settled = AgentSnapshot {
            status: FeatureStatus::Available,
            tasks: vec![TaskRow {
                id: "done".into(),
                kind: "bash".into(),
                label: "done".into(),
                status: "completed".into(),
                ..Default::default()
            }],
            subagents: vec![SubagentRow {
                id: "child-b".into(),
                parent_id: "parent".into(),
                label: "settled".into(),
                mode: Some("one-shot".into()),
                status: Some("inactive".into()),
                activity: Some("inactive".into()),
                running: false,
                ..Default::default()
            }],
        };
        assert_eq!(watcher_label(&settled), None);
    }

    #[test]
    fn subagent_detail_chrome_reserves_body_for_child_scrollback() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 12));
        let snapshot = snapshot();
        let child = ChildTranscriptView {
            child_id: "child-a".into(),
            error: None,
        };
        let body = render_agent_detail_content(
            &mut buffer,
            Rect::new(0, 0, 80, 12),
            &snapshot,
            &AgentItemId::Subagent("child-a".into()),
            Some(&child),
            &Theme::current(),
        );
        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("research"), "{text}");
        assert!(text.contains("does not cancel"), "{text}");
        assert!(body.height > 0, "{body:?}");
        assert!(body.y > 0, "{body:?}");
    }

    #[test]
    fn watcher_pulse_holds_each_frame_for_the_grok_divisor() {
        let frames = crate::glyphs::monitor_icon_frames();
        assert_eq!(watcher_pulse_frame(0), frames[0]);
        assert_eq!(
            watcher_pulse_frame(MONITOR_PULSE_DIVISOR.saturating_sub(1)),
            frames[0]
        );
        assert_eq!(watcher_pulse_frame(MONITOR_PULSE_DIVISOR), frames[1]);
        assert_ne!(
            watcher_pulse_frame(0),
            watcher_pulse_frame(MONITOR_PULSE_DIVISOR)
        );
    }

    #[test]
    fn task_order_matches_dsh_live_then_newest_settled() {
        let tasks = [
            TaskRow {
                id: "done-old".into(),
                kind: "bash".into(),
                label: "old".into(),
                status: "completed".into(),
                started_at_ms: Some(10),
                finished_at_ms: Some(20),
                ..Default::default()
            },
            TaskRow {
                id: "live-late".into(),
                kind: "bash".into(),
                label: "late".into(),
                status: "running".into(),
                started_at_ms: Some(30),
                ..Default::default()
            },
            TaskRow {
                id: "done-new".into(),
                kind: "bash".into(),
                label: "new".into(),
                status: "killed".into(),
                started_at_ms: Some(40),
                finished_at_ms: Some(50),
                ..Default::default()
            },
            TaskRow {
                id: "live-early".into(),
                kind: "bash".into(),
                label: "early".into(),
                status: "stopping".into(),
                started_at_ms: Some(5),
                ..Default::default()
            },
        ];
        let mut refs = tasks.iter().collect::<Vec<_>>();
        refs.sort_by(task_ordering);
        assert_eq!(
            refs.into_iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["live-early", "live-late", "done-new", "done-old"]
        );
    }
}
