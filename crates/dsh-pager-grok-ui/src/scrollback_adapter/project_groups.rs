//! DSH entry slice to Grok-derived group spans and per-entry annotations.

use std::collections::{HashMap, HashSet};

use dsh_pager::{
    DshRenderBlock, DshRenderEntry, DshRenderEntryId, DshRenderFinish, DshRenderKind,
    DshRenderVisibility,
};

use crate::{
    Theme,
    scrollback::{
        groups::{GroupKind, is_claimed_member, scan},
        types::DisplayMode,
        verb_group::{VerbGroupEntry, VerbGroupEntryKind, verb_group_header_label},
    },
    scrollback_adapter::project_tool::project_tool,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupProjection {
    pub anchor: DshRenderEntryId,
    pub header: bool,
    pub hidden: bool,
    pub expanded: bool,
    pub last_visible: bool,
    pub label: Option<String>,
    pub running: bool,
    pub failed: bool,
}

pub fn project_groups(
    entries: &[DshRenderEntry],
    display_modes: &HashMap<DshRenderEntryId, DisplayMode>,
    expanded_groups: &HashSet<DshRenderEntryId>,
    pending_entry: Option<DshRenderEntryId>,
    theme: Theme,
) -> HashMap<DshRenderEntryId, GroupProjection> {
    let neutral = entries
        .iter()
        .map(|entry| project_group_entry(entry, display_modes, pending_entry))
        .collect::<Vec<_>>();
    let expanded_starts = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| expanded_groups.contains(&entry.id).then_some(index))
        .collect::<HashSet<_>>();
    let spans = scan(&neutral, &expanded_starts, true);
    let mut projections = HashMap::new();
    for span in spans {
        let start = span.range.start;
        let anchor = entries[start].id;
        let (label, running, failed) = match span.kind {
            GroupKind::VerbRun { .. } => {
                let label = verb_group_header_label(&neutral, span.range.clone(), &theme);
                (label.text, label.running, label.failed)
            }
            GroupKind::Context { members } => (
                format!("Context · {members} injected messages"),
                false,
                false,
            ),
        };
        let visible_last = if span.expanded {
            span.range.end - 1
        } else {
            span.range
                .clone()
                .rev()
                .find(|index| *index == start || !is_claimed_member(&neutral, &span, *index, true))
                .unwrap_or(start)
        };
        for index in span.range.clone() {
            let claimed = is_claimed_member(&neutral, &span, index, true);
            projections.insert(
                entries[index].id,
                GroupProjection {
                    anchor,
                    header: index == start,
                    hidden: !span.expanded && claimed && index != start,
                    expanded: span.expanded,
                    last_visible: index == visible_last,
                    label: (index == start).then(|| label.clone()),
                    running,
                    failed,
                },
            );
        }
    }
    projections
}

fn project_group_entry(
    entry: &DshRenderEntry,
    display_modes: &HashMap<DshRenderEntryId, DisplayMode>,
    pending_entry: Option<DshRenderEntryId>,
) -> VerbGroupEntry {
    let display_mode = display_modes
        .get(&entry.id)
        .copied()
        .unwrap_or(DisplayMode::Expanded);
    let pending_user_input = pending_entry == Some(entry.id);
    if entry.visibility == DshRenderVisibility::Hidden {
        return neutral_entry(
            VerbGroupEntryKind::Transparent,
            display_mode,
            entry,
            pending_user_input,
        );
    }
    if matches!(
        entry.kind,
        DshRenderKind::AgentContext | DshRenderKind::Context | DshRenderKind::Compaction
    ) {
        return neutral_entry(
            VerbGroupEntryKind::Context,
            display_mode,
            entry,
            pending_user_input,
        );
    }
    if entry.kind == DshRenderKind::Thinking {
        return neutral_entry(
            VerbGroupEntryKind::Thinking,
            display_mode,
            entry,
            pending_user_input,
        );
    }
    let Some(block) = entry
        .content
        .blocks
        .iter()
        .find(|block| matches!(block, DshRenderBlock::ToolCall { .. }))
    else {
        return neutral_entry(
            VerbGroupEntryKind::Break,
            display_mode,
            entry,
            pending_user_input,
        );
    };
    let is_subagent = matches!(
        block,
        DshRenderBlock::ToolCall { name, .. }
            if matches!(name.as_str(), "task" | "subagent" | "spawn_agent")
    );
    if is_subagent {
        let mut neutral = neutral_entry(
            VerbGroupEntryKind::Subagent,
            display_mode,
            entry,
            pending_user_input,
        );
        if let DshRenderBlock::ToolCall {
            call_id: Some(call_id),
            ..
        } = block
        {
            neutral.sources.push(call_id.clone());
        }
        return neutral;
    }
    let Some(tool) = project_tool(block) else {
        return neutral_entry(
            VerbGroupEntryKind::Break,
            display_mode,
            entry,
            pending_user_input,
        );
    };
    let Some(kind) = tool.verb_group_kind() else {
        return neutral_entry(
            VerbGroupEntryKind::Break,
            display_mode,
            entry,
            pending_user_input,
        );
    };
    let mut neutral = neutral_entry(
        VerbGroupEntryKind::Tool(kind),
        display_mode,
        entry,
        pending_user_input,
    );
    neutral.failed |= tool.is_failed();
    neutral.sources.extend_from_slice(tool.distinct_sources());
    neutral
}

fn neutral_entry(
    kind: VerbGroupEntryKind,
    display_mode: DisplayMode,
    entry: &DshRenderEntry,
    pending_user_input: bool,
) -> VerbGroupEntry {
    VerbGroupEntry {
        kind,
        display_mode,
        running: entry.finish == DshRenderFinish::Running,
        failed: entry.finish == DshRenderFinish::Failed,
        pending_user_input,
        sources: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::{DshRenderContent, DshToolCallView, DshToolKind};

    fn tool(seq: i64, name: &str, kind: DshToolKind) -> DshRenderEntry {
        let mut entry = DshRenderEntry::plain(
            DshRenderEntryId::Event { seq },
            seq,
            DshRenderKind::ToolCall,
            name,
        );
        entry.content = DshRenderContent {
            blocks: vec![DshRenderBlock::ToolCall {
                name: name.into(),
                call_id: Some(format!("call-{seq}")),
                arguments: "{}".into(),
                edit: None,
                view: Some(DshToolCallView::Generic {
                    title: name.into(),
                    kind,
                    raw_input: None,
                    content: Vec::new(),
                    locations: Vec::new(),
                }),
                result: None,
            }],
            fallback: name.into(),
        };
        entry.finish = DshRenderFinish::Completed;
        entry
    }

    #[test]
    fn adapter_projects_read_thought_search_subagent_run_with_stable_anchor() {
        let read = tool(1, "read", DshToolKind::Read);
        let thought = DshRenderEntry::plain(
            DshRenderEntryId::Event { seq: 2 },
            2,
            DshRenderKind::Thinking,
            "consider",
        );
        let search = tool(3, "grep", DshToolKind::Search);
        let subagent = tool(4, "task", DshToolKind::Other);
        let entries = vec![read, thought, search, subagent];
        let modes = entries
            .iter()
            .map(|entry| (entry.id, DisplayMode::Collapsed))
            .collect();
        let projected = project_groups(&entries, &modes, &HashSet::new(), None, *Theme::current());
        let anchor = entries[0].id;
        assert!(projected[&anchor].header);
        assert_eq!(projected[&entries[3].id].anchor, anchor);
        assert_eq!(
            projected[&anchor].label.as_deref(),
            Some("Read 1 file, Searched 1 pattern, Ran 1 subagent")
        );
        assert!(projected[&entries[1].id].hidden);
    }

    #[test]
    fn execute_breaks_runs_and_pending_member_stays_standalone() {
        let entries = vec![
            tool(1, "read", DshToolKind::Read),
            tool(2, "bash", DshToolKind::Execute),
            tool(3, "grep", DshToolKind::Search),
        ];
        let modes = entries
            .iter()
            .map(|entry| (entry.id, DisplayMode::Collapsed))
            .collect();
        let projected = project_groups(
            &entries,
            &modes,
            &HashSet::new(),
            Some(entries[2].id),
            *Theme::current(),
        );
        assert!(projected[&entries[0].id].header);
        assert!(!projected.contains_key(&entries[1].id));
        assert!(!projected.contains_key(&entries[2].id));
    }
}
