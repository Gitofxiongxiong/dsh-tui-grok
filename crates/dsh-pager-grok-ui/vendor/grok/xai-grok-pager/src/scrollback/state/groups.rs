//! Derived group spans for view-time folds.
//!
//! Verb runs claim first, preserving Grok's run semantics. DSH's existing
//! injected-context fold is represented as a second non-overlapping family.

use std::{collections::HashSet, ops::Range};

use super::verb_group::{RunStep, VerbGroupEntry, VerbGroupEntryKind, run_step, scan_run_forward};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSpan {
    pub range: Range<usize>,
    pub kind: GroupKind,
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    VerbRun { members: usize },
    Context { members: usize },
}

pub fn span_containing(spans: &[GroupSpan], index: usize) -> Option<&GroupSpan> {
    let position = spans.partition_point(|span| span.range.end <= index);
    spans
        .get(position)
        .filter(|span| span.range.contains(&index))
}

pub fn scan(
    entries: &[VerbGroupEntry],
    expanded_starts: &HashSet<usize>,
    show_thinking: bool,
) -> Vec<GroupSpan> {
    let mut spans = scan_verb_runs(entries, expanded_starts, show_thinking);
    let claimed = spans
        .iter()
        .flat_map(|span| span.range.clone())
        .collect::<HashSet<_>>();
    spans.extend(scan_context_runs(entries, expanded_starts, &claimed));
    spans.sort_unstable_by_key(|span| span.range.start);
    spans
}

fn scan_verb_runs(
    entries: &[VerbGroupEntry],
    expanded_starts: &HashSet<usize>,
    show_thinking: bool,
) -> Vec<GroupSpan> {
    let mut spans = Vec::new();
    let mut index = 0usize;
    while index < entries.len() {
        let Some(run) = scan_run_forward(entries, index, show_thinking) else {
            index += 1;
            continue;
        };
        if run.folds() {
            spans.push(GroupSpan {
                range: index..run.end,
                kind: GroupKind::VerbRun {
                    members: run.members,
                },
                expanded: expanded_starts.contains(&index),
            });
        }
        index = run.stop.max(index + 1);
    }
    spans
}

fn scan_context_runs(
    entries: &[VerbGroupEntry],
    expanded_starts: &HashSet<usize>,
    claimed: &HashSet<usize>,
) -> Vec<GroupSpan> {
    let mut spans = Vec::new();
    let mut index = 0usize;
    while index < entries.len() {
        if claimed.contains(&index) || !matches!(entries[index].kind, VerbGroupEntryKind::Context) {
            index += 1;
            continue;
        }
        let start = index;
        while index < entries.len()
            && !claimed.contains(&index)
            && matches!(entries[index].kind, VerbGroupEntryKind::Context)
        {
            index += 1;
        }
        let members = index - start;
        if members >= 2 {
            spans.push(GroupSpan {
                range: start..index,
                kind: GroupKind::Context { members },
                expanded: expanded_starts.contains(&start),
            });
        }
    }
    spans
}

/// Whether this index is a claimed member rather than a transparent visible
/// row inside a verb span.
pub fn is_claimed_member(
    entries: &[VerbGroupEntry],
    span: &GroupSpan,
    index: usize,
    show_thinking: bool,
) -> bool {
    if !span.range.contains(&index) {
        return false;
    }
    match span.kind {
        GroupKind::Context { .. } => true,
        GroupKind::VerbRun { .. } => matches!(
            run_step(&entries[index], show_thinking),
            RunStep::Member(_) | RunStep::ThoughtMember
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scrollback::{tool::VerbGroupKind, types::DisplayMode};

    fn entry(kind: VerbGroupEntryKind) -> VerbGroupEntry {
        VerbGroupEntry {
            kind,
            display_mode: DisplayMode::Collapsed,
            running: false,
            failed: false,
            pending_user_input: false,
            sources: Vec::new(),
        }
    }

    #[test]
    fn verb_runs_precede_context_runs_and_spans_are_disjoint() {
        let entries = vec![
            entry(VerbGroupEntryKind::Tool(VerbGroupKind::File)),
            entry(VerbGroupEntryKind::Thinking),
            entry(VerbGroupEntryKind::Tool(VerbGroupKind::Search)),
            entry(VerbGroupEntryKind::Break),
            entry(VerbGroupEntryKind::Context),
            entry(VerbGroupEntryKind::Context),
        ];
        let spans = scan(&entries, &HashSet::new(), true);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].range, 0..3);
        assert_eq!(spans[1].range, 4..6);
        assert!(matches!(spans[0].kind, GroupKind::VerbRun { members: 2 }));
        assert!(matches!(spans[1].kind, GroupKind::Context { members: 2 }));
        assert_eq!(span_containing(&spans, 2), Some(&spans[0]));
        assert_eq!(span_containing(&spans, 3), None);
    }

    #[test]
    fn transparent_running_thought_keeps_rows_but_does_not_break_run() {
        let mut live = entry(VerbGroupEntryKind::Thinking);
        live.running = true;
        let entries = vec![
            entry(VerbGroupEntryKind::Tool(VerbGroupKind::File)),
            live,
            entry(VerbGroupEntryKind::Tool(VerbGroupKind::Search)),
        ];
        let spans = scan(&entries, &HashSet::new(), true);
        assert_eq!(spans[0].range, 0..3);
        assert!(!is_claimed_member(&entries, &spans[0], 1, true));
    }
}
