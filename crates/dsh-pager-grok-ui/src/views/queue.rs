//! Queue pane assembled from Grok modal/list primitives and DSH queue DTOs.

use dsh_pager::{DshQueueItem, DshQueueItemId};
use dsh_pager_protocol::QueuePlacement;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};

use crate::theme::Theme;

pub struct QueueRenderState<'a> {
    pub selected_id: Option<&'a str>,
    pub editing: bool,
    pub editor_text: &'a str,
    pub pending_id: Option<&'a str>,
    pub revision: u64,
}

/// Return whether an authoritative inbox item belongs on a user-facing queue
/// surface. Context injections are model-facing state: the host keeps them in
/// the same snapshot for authority/revision purposes, but its wire contract
/// requires them to remain invisible until the agent claims them.
pub fn queue_item_is_visible(item: &DshQueueItem) -> bool {
    item.placement != QueuePlacement::Context
}

/// Iterate the queue rows that the user may see and mutate.
pub fn visible_queue_items(queue: &[DshQueueItem]) -> impl Iterator<Item = &DshQueueItem> {
    queue.iter().filter(|item| queue_item_is_visible(item))
}

/// Count rows that contribute to queue geometry and shortcut visibility.
pub fn visible_queue_len(queue: &[DshQueueItem]) -> usize {
    visible_queue_items(queue).count()
}

/// Render the authoritative queue list. `selected_id` is a stable item ID;
/// the caller never persists an array index across refreshes.
pub fn render_queue_content(
    buffer: &mut Buffer,
    area: Rect,
    queue: &[DshQueueItem],
    state: QueueRenderState<'_>,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    buffer.set_string(
        area.x,
        area.y,
        format!("Queue · revision {}", state.revision),
        Style::default()
            .fg(theme.gray_bright)
            .add_modifier(Modifier::BOLD),
    );
    let mut visible_items = visible_queue_items(queue).peekable();
    if visible_items.peek().is_none() {
        buffer.set_string(
            area.x,
            area.y.saturating_add(2),
            "No queued prompts",
            Style::default().fg(theme.gray),
        );
        return;
    }
    let mut y = area.y.saturating_add(2);
    for item in visible_items {
        if y >= area.bottom() {
            break;
        }
        let selected = state.selected_id == Some(item.id.as_str());
        let pending = state.pending_id == Some(item.id.as_str());
        let marker = if selected { "▸" } else { " " };
        let pending_label = if pending { " · pending" } else { "" };
        let summary = item.content.summary.as_deref().unwrap_or("(empty content)");
        let line = format!(
            "{marker} [{}] {}{}",
            placement_label(item.placement),
            summary,
            pending_label
        );
        let style = if selected {
            Style::default()
                .fg(theme.text_primary)
                .bg(theme.bg_highlight)
        } else if pending {
            Style::default().fg(theme.fuzzy_accent)
        } else {
            Style::default().fg(theme.gray)
        };
        buffer.set_string(area.x, y, line, style);
        y = y.saturating_add(1);
        if selected && state.editing && y < area.bottom() {
            buffer.set_string(
                area.x.saturating_add(2),
                y,
                format!("edit: {}", state.editor_text),
                Style::default().fg(theme.text_primary),
            );
            y = y.saturating_add(1);
        }
        for content_line in item.content.lines.iter().skip(1) {
            if y >= area.bottom() {
                break;
            }
            buffer.set_string(
                area.x.saturating_add(4),
                y,
                content_line,
                Style::default().fg(theme.gray_dim),
            );
            y = y.saturating_add(1);
        }
    }
    if y < area.bottom() {
        buffer.set_string(
            area.x,
            area.bottom().saturating_sub(1),
            if state.editing {
                "Enter save · Esc cancel"
            } else {
                "↑/↓ select · e edit · d delete · s steer · Esc close"
            },
            Style::default().fg(theme.gray_dim),
        );
    }
}

/// Return the stable target selected by a relative movement in the current
/// authoritative snapshot.
pub fn moved_selection(
    queue: &[DshQueueItem],
    selected_id: Option<&str>,
    delta: isize,
) -> Option<DshQueueItemId> {
    let visible_len = visible_queue_len(queue);
    if visible_len == 0 {
        return None;
    }
    let current = selected_id
        .and_then(|id| visible_queue_items(queue).position(|item| item.id == id))
        .unwrap_or(0);
    let next = if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current
            .saturating_add(delta as usize)
            .min(visible_len.saturating_sub(1))
    };
    visible_queue_items(queue)
        .nth(next)
        .map(|item| DshQueueItemId::new(item.id.clone()))
}

pub fn placement_label(placement: QueuePlacement) -> &'static str {
    match placement {
        QueuePlacement::Queued => "queue",
        QueuePlacement::Steering => "steer",
        QueuePlacement::Context => "context",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::DshQueueContent;

    fn item(id: &str, placement: QueuePlacement) -> DshQueueItem {
        DshQueueItem {
            id: id.into(),
            placement,
            content: DshQueueContent {
                lines: vec![id.into()],
                summary: Some(id.into()),
                editable_text: Some(id.into()),
                block_count: 1,
            },
        }
    }

    #[test]
    fn selection_moves_by_stable_item_id() {
        let queue = vec![
            item("hidden-before", QueuePlacement::Context),
            item("a", QueuePlacement::Queued),
            item("hidden-between", QueuePlacement::Context),
            item("b", QueuePlacement::Steering),
        ];
        assert_eq!(moved_selection(&queue, Some("a"), 1).unwrap().as_str(), "b");
        assert_eq!(
            moved_selection(&queue, Some("b"), -1).unwrap().as_str(),
            "a"
        );
        assert_eq!(moved_selection(&queue, None, 0).unwrap().as_str(), "a");
    }

    #[test]
    fn context_items_are_not_rendered_on_the_queue_surface() {
        let queue = vec![
            item(
                "The approval policy changed from never to ask",
                QueuePlacement::Context,
            ),
            item("visible follow-up", QueuePlacement::Queued),
        ];
        let area = Rect::new(0, 0, 80, 8);
        let mut buffer = Buffer::empty(area);
        render_queue_content(
            &mut buffer,
            area,
            &queue,
            QueueRenderState {
                selected_id: Some("visible follow-up"),
                editing: false,
                editor_text: "",
                pending_id: None,
                revision: 4,
            },
            Theme::current(),
        );
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("visible follow-up"));
        assert!(!rendered.contains("approval policy changed"));
        assert!(!rendered.contains("[context]"));
    }
}
