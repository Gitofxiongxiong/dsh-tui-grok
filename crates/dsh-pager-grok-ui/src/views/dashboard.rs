//! Dashboard/workspace surface composed from Grok modal primitives.
//!
//! The model and row identities are owned by DSH. This module only paints the
//! deterministic hierarchy and the optional non-attaching peek result.

use dsh_pager::dashboard::{DashboardModel, DashboardStatus};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};

use crate::host_adapter::{FeatureStatus, WorkspaceSnapshot};
use crate::theme::Theme;
use crate::views::workspace::WorkspaceTreeController;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardPeek {
    pub session_id: String,
    pub title: String,
    pub lines: Vec<String>,
}

pub struct DashboardRenderState<'a> {
    pub model: &'a DashboardModel,
    pub peek: Option<&'a DashboardPeek>,
    pub query_active: bool,
    pub query: &'a str,
    pub workspace: &'a WorkspaceSnapshot,
    pub workspace_tree: &'a WorkspaceTreeController,
    pub theme: &'a Theme,
}

pub fn render_dashboard_content(buffer: &mut Buffer, area: Rect, state: DashboardRenderState<'_>) {
    let DashboardRenderState {
        model,
        peek,
        query_active,
        query,
        workspace,
        workspace_tree,
        theme,
    } = state;
    if area.width == 0 || area.height == 0 {
        return;
    }
    if let Some(peek) = peek {
        render_peek(buffer, area, peek, theme);
        return;
    }
    let heading = if query_active {
        format!("Dashboard · search: {query}")
    } else {
        format!(
            "Dashboard · {}{}",
            if model.group_by_workspace() {
                "workspaces"
            } else {
                "sessions"
            },
            if model.show_archived() {
                " · archived"
            } else {
                ""
            }
        )
    };
    let heading = workspace_tree
        .focused_workspace_id()
        .map(|id| format!("{heading} · focus {id}"))
        .unwrap_or(heading);
    let heading = match workspace.status {
        FeatureStatus::Available if workspace.actions_supported => heading,
        FeatureStatus::Available => format!("{heading} · read-only"),
        FeatureStatus::Pending => format!("{heading} · pending"),
        FeatureStatus::Unsupported => format!("{heading} · unavailable"),
    };
    put(
        buffer,
        area,
        (area.x, area.y),
        &heading,
        theme.gray_bright,
        theme,
        true,
    );
    let rows = model.rows_window(0, area.height.saturating_sub(4) as usize);
    if rows.is_empty() {
        put(
            buffer,
            area,
            (area.x, area.y.saturating_add(2)),
            if query_active {
                "No matching sessions"
            } else {
                "No sessions in control plane"
            },
            theme.gray,
            theme,
            false,
        );
    } else {
        let selected = model.selected_id();
        for (index, row) in rows.iter().enumerate() {
            let y = area.y.saturating_add(2 + index as u16);
            if y >= area.bottom().saturating_sub(1) {
                break;
            }
            let marker = if selected == Some(row.session_id.as_str()) {
                "▸"
            } else {
                " "
            };
            let status = status_label(row.status);
            let indent = "  ".repeat(row.depth.min(8));
            let workspace = row
                .workspace_title
                .as_deref()
                .map(|title| format!(" · {title}"))
                .unwrap_or_default();
            let detail = if row.pending_interactions > 0 {
                format!("{} · input", row.pending_interactions)
            } else if row.jobs > 0 {
                format!("{} job{}", row.jobs, if row.jobs == 1 { "" } else { "s" })
            } else {
                row.session_id.clone()
            };
            let line = format!(
                "{marker} {indent}{} [{status}] {detail}{workspace}",
                row.title
            );
            let style = if selected == Some(row.session_id.as_str()) {
                Style::default()
                    .fg(theme.text_primary)
                    .bg(theme.bg_highlight)
            } else if row.removed || row.archived {
                Style::default().fg(theme.gray_dim)
            } else {
                Style::default().fg(theme.gray)
            };
            buffer.set_string(area.x, y, line, style);
        }
    }
    let footer = if query_active {
        "Enter apply search · Esc search off · Backspace edit"
    } else if !workspace.actions_supported {
        "↑/↓ select · Enter attach · v peek · g group · a archived · / search · workspace mutations unavailable"
    } else {
        "↑/↓ select · Shift+↑/↓ reorder · Enter attach · x archive · v peek · g group · a archived · / search · Esc back"
    };
    put(
        buffer,
        area,
        (area.x, area.bottom().saturating_sub(1)),
        footer,
        theme.gray_dim,
        theme,
        false,
    );
}

fn render_peek(buffer: &mut Buffer, area: Rect, peek: &DashboardPeek, theme: &Theme) {
    put(
        buffer,
        area,
        (area.x, area.y),
        &format!("Peek · {}", peek.title),
        theme.gray_bright,
        theme,
        true,
    );
    put(
        buffer,
        area,
        (area.x, area.y.saturating_add(1)),
        &peek.session_id,
        theme.gray_dim,
        theme,
        false,
    );
    for (index, line) in peek
        .lines
        .iter()
        .take(area.height.saturating_sub(4) as usize)
        .enumerate()
    {
        put(
            buffer,
            area,
            (area.x, area.y.saturating_add(3 + index as u16)),
            line,
            theme.text_primary,
            theme,
            false,
        );
    }
    put(
        buffer,
        area,
        (area.x, area.bottom().saturating_sub(1)),
        "Enter attach · Esc back to dashboard",
        theme.gray_dim,
        theme,
        false,
    );
}

fn put(
    buffer: &mut Buffer,
    area: Rect,
    (x, y): (u16, u16),
    text: &str,
    fg: ratatui::style::Color,
    theme: &Theme,
    bold: bool,
) {
    if y >= area.bottom() {
        return;
    }
    let style = if bold {
        Style::default()
            .fg(fg)
            .bg(theme.bg_base)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg).bg(theme.bg_base)
    };
    buffer.set_string(x, y, text, style);
}

fn status_label(status: DashboardStatus) -> &'static str {
    match status {
        DashboardStatus::NeedsInput => "input",
        DashboardStatus::Failed => "error",
        DashboardStatus::Running => "run",
        DashboardStatus::Idle => "idle",
        DashboardStatus::Blank => "blank",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::dashboard::DashboardModel;
    use ratatui::{buffer::Buffer, layout::Rect};

    #[test]
    fn empty_dashboard_has_stable_controls() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 12));
        render_dashboard_content(
            &mut buffer,
            Rect::new(0, 0, 80, 12),
            DashboardRenderState {
                model: &DashboardModel::default(),
                peek: None,
                query_active: false,
                query: "",
                workspace: &WorkspaceSnapshot {
                    status: FeatureStatus::Available,
                    actions_supported: true,
                    ..Default::default()
                },
                workspace_tree: &WorkspaceTreeController::default(),
                theme: Theme::current(),
            },
        );
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "D"));
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "v"));
    }
}
