//! Verb-group aggregation and run classification.
//!
//! The walk, transparent/thought semantics, first-appearance buckets and
//! tense/plural vocabulary follow Grok Build's `state/verb_group.rs`. The
//! input is value-only so DSH remains the history authority.

use std::{collections::HashSet, ops::Range};

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{
    scrollback::{tool::VerbGroupKind, types::DisplayMode},
    theme::Theme,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerbGroupEntryKind {
    Tool(VerbGroupKind),
    Subagent,
    Thinking,
    Context,
    Transparent,
    Break,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerbGroupEntry {
    pub kind: VerbGroupEntryKind,
    pub display_mode: DisplayMode,
    pub running: bool,
    pub failed: bool,
    pub pending_user_input: bool,
    /// Distinct-count override (web citations or child session ids).
    pub sources: Vec<String>,
}

impl VerbGroupEntry {
    pub fn tool(kind: VerbGroupKind) -> Self {
        Self {
            kind: VerbGroupEntryKind::Tool(kind),
            display_mode: DisplayMode::Collapsed,
            running: false,
            failed: false,
            pending_user_input: false,
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStep {
    Member(VerbGroupKind),
    ThoughtMember,
    Transparent,
    Break,
}

/// Single classification shared by scan, range projection and label walk.
pub fn run_step(entry: &VerbGroupEntry, show_thinking: bool) -> RunStep {
    match entry.kind {
        VerbGroupEntryKind::Tool(kind) if !entry.pending_user_input => {
            if entry.display_mode == DisplayMode::Collapsed {
                RunStep::Member(kind)
            } else {
                RunStep::Transparent
            }
        }
        VerbGroupEntryKind::Subagent if !entry.pending_user_input => {
            if entry.display_mode == DisplayMode::Collapsed {
                RunStep::Member(VerbGroupKind::Subagent)
            } else {
                RunStep::Transparent
            }
        }
        VerbGroupEntryKind::Subagent => RunStep::Break,
        VerbGroupEntryKind::Thinking => {
            if show_thinking
                && !entry.running
                && !entry.pending_user_input
                && entry.display_mode == DisplayMode::Collapsed
            {
                RunStep::ThoughtMember
            } else {
                RunStep::Transparent
            }
        }
        VerbGroupEntryKind::Transparent => RunStep::Transparent,
        VerbGroupEntryKind::Tool(_) | VerbGroupEntryKind::Context | VerbGroupEntryKind::Break => {
            RunStep::Break
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunScan {
    pub members: usize,
    pub end: usize,
    pub stop: usize,
}

impl RunScan {
    pub fn folds(self) -> bool {
        self.members >= 1
    }
}

pub fn scan_run_forward(
    entries: &[VerbGroupEntry],
    start: usize,
    show_thinking: bool,
) -> Option<RunScan> {
    match run_step(entries.get(start)?, show_thinking) {
        RunStep::Member(_) | RunStep::ThoughtMember => {}
        RunStep::Transparent | RunStep::Break => return None,
    }
    let mut members = 0usize;
    let mut end = start;
    let mut index = start;
    while let Some(entry) = entries.get(index) {
        match run_step(entry, show_thinking) {
            RunStep::Member(_) => {
                members += 1;
                end = index + 1;
            }
            RunStep::ThoughtMember => end = index + 1,
            RunStep::Transparent => {}
            RunStep::Break => break,
        }
        index += 1;
    }
    Some(RunScan {
        members,
        end,
        stop: index,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerbGroupHeaderLabel {
    pub line: Line<'static>,
    pub text: String,
    pub running: bool,
    pub failed: bool,
}

#[derive(Debug)]
struct Bucket<'a> {
    kind: VerbGroupKind,
    calls: usize,
    sources: HashSet<&'a str>,
}

/// Build `Read 2 files, Searched 1 pattern` from the exact span claimed by
/// the group scan. Finished thoughts and transparent rows are never labeled.
pub fn verb_group_header_label(
    entries: &[VerbGroupEntry],
    range: Range<usize>,
    theme: &Theme,
) -> VerbGroupHeaderLabel {
    let end = range.end.min(entries.len());
    let mut buckets: Vec<Bucket<'_>> = Vec::new();
    let mut running = false;
    let mut failed_count = 0usize;
    for entry in &entries[range.start.min(end)..end] {
        let kind = match run_step(entry, true) {
            RunStep::Member(kind) => kind,
            RunStep::ThoughtMember | RunStep::Transparent => continue,
            RunStep::Break => break,
        };
        let position = buckets.iter().position(|bucket| bucket.kind == kind);
        let position = position.unwrap_or_else(|| {
            buckets.push(Bucket {
                kind,
                calls: 0,
                sources: HashSet::new(),
            });
            buckets.len() - 1
        });
        let bucket = &mut buckets[position];
        bucket.calls += 1;
        bucket
            .sources
            .extend(entry.sources.iter().map(String::as_str));
        running |= entry.running;
        failed_count += usize::from(entry.failed);
    }

    let text_style = Style::default()
        .fg(theme.gray_bright)
        .add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let mut text = String::new();
    for (index, bucket) in buckets.iter().enumerate() {
        let count = if bucket.sources.is_empty() {
            bucket.calls
        } else {
            bucket.sources.len()
        };
        let segment = format!(
            "{}{} {count} {}",
            if index == 0 { "" } else { ", " },
            bucket.kind.verb(running),
            bucket.kind.noun(count)
        );
        text.push_str(&segment);
        spans.push(Span::styled(segment, text_style));
    }
    if failed_count > 0 {
        let suffix = format!(" · {failed_count} failed");
        text.push_str(&suffix);
        spans.push(Span::styled(
            suffix,
            Style::default().fg(theme.accent_error),
        ));
    }
    VerbGroupHeaderLabel {
        line: Line::from(spans),
        text,
        running,
        failed: failed_count > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thought(running: bool) -> VerbGroupEntry {
        VerbGroupEntry {
            kind: VerbGroupEntryKind::Thinking,
            display_mode: DisplayMode::Collapsed,
            running,
            failed: false,
            pending_user_input: false,
            sources: Vec::new(),
        }
    }

    #[test]
    fn read_thought_search_subagent_is_one_claimed_run() {
        let entries = vec![
            VerbGroupEntry::tool(VerbGroupKind::File),
            thought(false),
            VerbGroupEntry::tool(VerbGroupKind::Search),
            VerbGroupEntry {
                kind: VerbGroupEntryKind::Subagent,
                ..VerbGroupEntry::tool(VerbGroupKind::Subagent)
            },
        ];
        let scan = scan_run_forward(&entries, 0, true).expect("run");
        assert_eq!(scan.members, 3);
        assert_eq!(scan.end, 4);
        assert!(scan.folds());
        let label = verb_group_header_label(&entries, 0..scan.end, Theme::current());
        assert_eq!(
            label.text,
            "Read 1 file, Searched 1 pattern, Ran 1 subagent"
        );
    }

    #[test]
    fn running_thought_and_opened_or_pending_members_stay_visible() {
        let mut opened = VerbGroupEntry::tool(VerbGroupKind::File);
        opened.display_mode = DisplayMode::Expanded;
        let mut pending = VerbGroupEntry::tool(VerbGroupKind::Search);
        pending.pending_user_input = true;
        assert_eq!(run_step(&thought(true), true), RunStep::Transparent);
        assert_eq!(run_step(&opened, true), RunStep::Transparent);
        assert_eq!(run_step(&pending, true), RunStep::Break);
    }

    #[test]
    fn action_breaks_run_and_label_tracks_running_failed_and_distinct_sources() {
        let mut first = VerbGroupEntry::tool(VerbGroupKind::WebSearch);
        first.sources = vec!["https://a.example".into(), "https://b.example".into()];
        let mut second = VerbGroupEntry::tool(VerbGroupKind::WebSearch);
        second.sources = vec!["https://b.example".into()];
        second.running = true;
        second.failed = true;
        let entries = vec![
            first,
            second,
            VerbGroupEntry {
                kind: VerbGroupEntryKind::Break,
                ..VerbGroupEntry::tool(VerbGroupKind::Command)
            },
            VerbGroupEntry::tool(VerbGroupKind::File),
        ];
        let scan = scan_run_forward(&entries, 0, true).expect("run");
        assert_eq!(scan.end, 2);
        let label = verb_group_header_label(&entries, 0..scan.end, Theme::current());
        assert_eq!(label.text, "Searching 2 websites · 1 failed");
        assert!(label.running);
        assert!(label.failed);
    }
}
