//! Grapheme-aware transcript selection state and copy reconstruction.

use std::cmp::Ordering;

use dsh_pager::DshRenderEntryId;
use unicode_segmentation::UnicodeSegmentation;

use crate::geometry::{GeometryLine, HitTarget, column_for_grapheme, grapheme_at_column};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectionPoint {
    pub entry_id: DshRenderEntryId,
    pub line_index: usize,
    pub grapheme_index: usize,
}

impl SelectionPoint {
    pub fn new(entry_id: DshRenderEntryId, line_index: usize, grapheme_index: usize) -> Self {
        Self {
            entry_id,
            line_index,
            grapheme_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSelection {
    pub start: SelectionPoint,
    pub end: SelectionPoint,
}

impl ResolvedSelection {
    pub fn normalize(mut self) -> Self {
        if compare_points(&self.start, &self.end) == Ordering::Greater {
            std::mem::swap(&mut self.start, &mut self.end);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn grapheme_range_for_line(
        &self,
        entry_id: DshRenderEntryId,
        line_index: usize,
        grapheme_count: usize,
    ) -> Option<(usize, usize)> {
        let selection = self.clone().normalize();
        if compare_line(
            &entry_id,
            line_index,
            &selection.start.entry_id,
            selection.start.line_index,
        ) == Ordering::Less
            || compare_line(
                &entry_id,
                line_index,
                &selection.end.entry_id,
                selection.end.line_index,
            ) == Ordering::Greater
        {
            return None;
        }
        let start =
            if entry_id == selection.start.entry_id && line_index == selection.start.line_index {
                selection.start.grapheme_index
            } else {
                0
            };
        let end = if entry_id == selection.end.entry_id && line_index == selection.end.line_index {
            selection.end.grapheme_index.min(grapheme_count)
        } else {
            grapheme_count
        };
        (start < end).then_some((start, end))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionModel {
    anchor: Option<SelectionPoint>,
    head: Option<SelectionPoint>,
    dragging: bool,
}

impl SelectionModel {
    pub fn begin(&mut self, point: SelectionPoint) {
        self.anchor = Some(point.clone());
        self.head = Some(point);
        self.dragging = true;
    }

    pub fn extend(&mut self, point: SelectionPoint) {
        if self.anchor.is_some() {
            self.head = Some(point);
            self.dragging = true;
        }
    }

    pub fn finish(&mut self) -> Option<ResolvedSelection> {
        self.dragging = false;
        self.selection()
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn selection(&self) -> Option<ResolvedSelection> {
        Some(
            ResolvedSelection {
                start: self.anchor.clone()?,
                end: self.head.clone()?,
            }
            .normalize(),
        )
    }

    /// Resolve a pointer against a painted line. This keeps pointer math at
    /// the same grapheme/display boundary used by the renderer.
    pub fn point_for_line(
        target: &HitTarget,
        line: &GeometryLine,
        column: u16,
    ) -> Option<SelectionPoint> {
        let (HitTarget::TranscriptEntry(entry_id) | HitTarget::TranscriptBlock { entry_id, .. }) =
            target
        else {
            return None;
        };
        let relative = column.saturating_sub(line.rect.x) as usize;
        Some(SelectionPoint::new(
            *entry_id,
            line.line_index,
            grapheme_at_column(&line.text, relative),
        ))
    }

    /// Reconstruct selected text from visible lines. Newlines between rows
    /// and entries are preserved; no trim is performed on user-visible data.
    pub fn copy_lines(&self, lines: &[GeometryLine], selection: &ResolvedSelection) -> String {
        let selection = selection.clone().normalize();
        let mut output = String::new();
        let mut first = true;
        for line in lines {
            let Some(entry_id) = target_entry(&line.target) else {
                continue;
            };
            let line_order = compare_line(
                &entry_id,
                line.line_index,
                &selection.start.entry_id,
                selection.start.line_index,
            );
            let end_order = compare_line(
                &entry_id,
                line.line_index,
                &selection.end.entry_id,
                selection.end.line_index,
            );
            if line_order == Ordering::Less || end_order == Ordering::Greater {
                continue;
            }
            let start = if entry_id == selection.start.entry_id
                && line.line_index == selection.start.line_index
            {
                selection.start.grapheme_index
            } else {
                0
            };
            let end = if entry_id == selection.end.entry_id
                && line.line_index == selection.end.line_index
            {
                selection.end.grapheme_index
            } else {
                line.text.graphemes(true).count()
            };
            let text = line
                .text
                .graphemes(true)
                .skip(start)
                .take(end.saturating_sub(start))
                .collect::<String>();
            if !first {
                output.push_str(line.joiner_to_previous.as_deref().unwrap_or("\n"));
            }
            first = false;
            output.push_str(&text);
        }
        output
    }
}

fn target_entry(target: &HitTarget) -> Option<DshRenderEntryId> {
    match target {
        HitTarget::TranscriptEntry(id) | HitTarget::TranscriptBlock { entry_id: id, .. } => {
            Some(*id)
        }
        _ => None,
    }
}

fn compare_points(left: &SelectionPoint, right: &SelectionPoint) -> Ordering {
    entry_order(&left.entry_id, &right.entry_id)
        .then(left.line_index.cmp(&right.line_index))
        .then(left.grapheme_index.cmp(&right.grapheme_index))
}

fn compare_line(
    left_entry: &DshRenderEntryId,
    left_line: usize,
    right_entry: &DshRenderEntryId,
    right_line: usize,
) -> Ordering {
    entry_order(left_entry, right_entry).then(left_line.cmp(&right_line))
}

fn entry_order(left: &DshRenderEntryId, right: &DshRenderEntryId) -> Ordering {
    match (left, right) {
        (DshRenderEntryId::Event { seq: left }, DshRenderEntryId::Event { seq: right }) => {
            left.cmp(right)
        }
        (
            DshRenderEntryId::Partial {
                turn: left_turn,
                step: left_step,
                surface: left_surface,
            },
            DshRenderEntryId::Partial {
                turn: right_turn,
                step: right_step,
                surface: right_surface,
            },
        ) => left_turn
            .cmp(right_turn)
            .then(left_step.cmp(right_step))
            .then(left_surface.cmp(right_surface)),
        (DshRenderEntryId::Event { .. }, DshRenderEntryId::Partial { .. }) => Ordering::Less,
        (DshRenderEntryId::Partial { .. }, DshRenderEntryId::Event { .. }) => Ordering::Greater,
    }
}

#[allow(dead_code)]
fn _column_helper(text: &str, grapheme: usize) -> usize {
    column_for_grapheme(text, grapheme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::{Position, Rect};

    fn line(id: i64, index: usize, text: &str) -> GeometryLine {
        GeometryLine {
            target: HitTarget::TranscriptEntry(DshRenderEntryId::Event { seq: id }),
            line_index: index,
            text: text.into(),
            joiner_to_previous: None,
            rect: Rect::new(2, index as u16, 20, 1),
        }
    }

    #[test]
    fn drag_normalizes_and_copies_unicode_without_trim() {
        let mut model = SelectionModel::default();
        model.begin(SelectionPoint::new(
            DshRenderEntryId::Event { seq: 2 },
            1,
            2,
        ));
        model.extend(SelectionPoint::new(
            DshRenderEntryId::Event { seq: 1 },
            0,
            1,
        ));
        let selection = model.finish().unwrap();
        assert_eq!(selection.start.entry_id, DshRenderEntryId::Event { seq: 1 });
        let selection = ResolvedSelection {
            start: SelectionPoint::new(DshRenderEntryId::Event { seq: 1 }, 0, 1),
            end: SelectionPoint::new(DshRenderEntryId::Event { seq: 2 }, 1, 5),
        };
        let copied = model.copy_lines(&[line(1, 0, "界a"), line(2, 1, "b e\u{301}  ")], &selection);
        assert_eq!(copied, "a\nb e\u{301}  ");
    }

    #[test]
    fn point_for_line_uses_display_columns() {
        let line = line(1, 0, "A界x");
        let point = SelectionModel::point_for_line(&line.target, &line, 5).unwrap();
        assert_eq!(point.grapheme_index, 2);
        assert!(line.rect.contains(Position::new(3, 0)));
    }

    #[test]
    fn copy_reconstructs_soft_wrap_joiners_instead_of_visual_newlines() {
        let mut first = line(1, 0, "hello");
        let mut second = line(1, 1, "world");
        first.joiner_to_previous = None;
        second.joiner_to_previous = Some("  ".into());
        let selection = ResolvedSelection {
            start: SelectionPoint::new(DshRenderEntryId::Event { seq: 1 }, 0, 0),
            end: SelectionPoint::new(DshRenderEntryId::Event { seq: 1 }, 1, 5),
        };
        assert_eq!(
            SelectionModel::default().copy_lines(&[first, second], &selection),
            "hello  world"
        );
    }
}
