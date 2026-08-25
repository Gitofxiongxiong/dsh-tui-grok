//! Typed DSH DTO projection into renderer-neutral Grok block inputs.

use dsh_pager::{DshRenderBlock, DshRenderEntry, DshRenderEntryId, DshRenderFinish, DshRenderKind};
use ratatui::{style::Color, text::Line};

use crate::{
    Theme,
    scrollback::{
        block_renderer::{BlockRenderSpec, BlockRenderer},
        types::{AccentStyle, DisplayMode, RenderedBlock},
    },
};

#[derive(Debug, Clone)]
pub struct ProjectedLine {
    pub line: Line<'static>,
    pub block_index: Option<usize>,
    pub rail: bool,
    pub header: bool,
    pub selectable: bool,
    pub accent: Option<AccentStyle>,
    pub bullet: Option<AccentStyle>,
    pub background: Option<Color>,
    pub accent_background: bool,
    pub joiner: Option<String>,
}

pub fn materialize_block(
    block: RenderedBlock,
    width: usize,
    theme: Theme,
    block_index: Option<usize>,
) -> Vec<ProjectedLine> {
    let rendered = BlockRenderer::render(
        block,
        BlockRenderSpec {
            width: width.max(1),
            base_background: theme.bg_base,
        },
    );
    let accent = rendered.accent;
    let bullet = rendered.bullet;
    let background = rendered.background;
    let accent_background = rendered.accent_background;
    rendered
        .output
        .lines
        .into_iter()
        .map(|line| ProjectedLine {
            rail: accent.is_some(),
            header: line.header,
            selectable: line.selectable,
            background: line.background.or(background),
            line: line.content,
            block_index,
            accent,
            bullet,
            accent_background,
            joiner: line.joiner,
        })
        .collect()
}

/// Stable renderer-neutral entry projected once at the DSH/Grok boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedScrollbackEntry<'a> {
    pub id: ProjectedEntryId,
    pub blocks: Vec<ProjectedRenderBlock<'a>>,
    pub display_mode: DisplayMode,
    pub is_running: bool,
    pub is_finished: bool,
    pub created_at_ms: Option<u64>,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
}

/// Exact round-trip identity; never derived from the entry's array position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectedEntryId(pub DshRenderEntryId);

impl ProjectedEntryId {
    pub fn into_dsh(self) -> DshRenderEntryId {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedRenderBlock<'a> {
    /// Original typed-block index used by fold/hit/copy. `None` denotes an
    /// entry-level fallback synthesized from `entry.text`.
    pub source_index: Option<usize>,
    pub block: ProjectedBlock<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectedBlock<'a> {
    User {
        text: &'a str,
    },
    Thinking {
        text: &'a str,
    },
    AgentMarkdown {
        block: Option<&'a DshRenderBlock>,
        fallback: &'a str,
    },
    Tool {
        block: &'a DshRenderBlock,
    },
    Generic {
        block: Option<&'a DshRenderBlock>,
        fallback: &'a str,
    },
    Unsupported {
        label: &'static str,
    },
}

pub fn project_entry(
    entry: &DshRenderEntry,
    display_mode: DisplayMode,
) -> ProjectedScrollbackEntry<'_> {
    let blocks = project_blocks(entry);
    ProjectedScrollbackEntry {
        id: ProjectedEntryId(entry.id),
        blocks,
        display_mode,
        is_running: entry.finish == DshRenderFinish::Running,
        is_finished: matches!(
            entry.finish,
            DshRenderFinish::Completed | DshRenderFinish::Failed
        ),
        created_at_ms: entry.created_at_ms,
        started_at_ms: entry.started_at_ms,
        finished_at_ms: entry.finished_at_ms,
    }
}

fn project_blocks(entry: &DshRenderEntry) -> Vec<ProjectedRenderBlock<'_>> {
    match entry.kind {
        DshRenderKind::User => vec![ProjectedRenderBlock {
            source_index: None,
            block: ProjectedBlock::User { text: &entry.text },
        }],
        DshRenderKind::Thinking => vec![ProjectedRenderBlock {
            source_index: entry
                .content
                .blocks
                .iter()
                .position(|block| matches!(block, DshRenderBlock::Reasoning { .. })),
            block: ProjectedBlock::Thinking {
                text: reasoning_text(entry).unwrap_or(&entry.text),
            },
        }],
        DshRenderKind::Assistant => project_assistant_blocks(entry),
        DshRenderKind::ToolCall | DshRenderKind::ToolResult => entry
            .content
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| ProjectedRenderBlock {
                source_index: Some(index),
                block: ProjectedBlock::Tool { block },
            })
            .collect(),
        _ => project_generic_blocks(entry),
    }
}

fn project_assistant_blocks(entry: &DshRenderEntry) -> Vec<ProjectedRenderBlock<'_>> {
    if entry.content.blocks.is_empty() {
        return vec![ProjectedRenderBlock {
            source_index: None,
            block: ProjectedBlock::AgentMarkdown {
                block: None,
                fallback: &entry.text,
            },
        }];
    }
    entry
        .content
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| ProjectedRenderBlock {
            source_index: Some(index),
            block: match block {
                DshRenderBlock::Reasoning { text } => ProjectedBlock::Thinking { text },
                DshRenderBlock::Markdown { .. } | DshRenderBlock::Plain { .. } => {
                    ProjectedBlock::AgentMarkdown {
                        block: Some(block),
                        fallback: &entry.text,
                    }
                }
                DshRenderBlock::ToolCall { .. }
                | DshRenderBlock::ToolResult { .. }
                | DshRenderBlock::Diff { .. } => ProjectedBlock::Tool { block },
                DshRenderBlock::Image { .. } => ProjectedBlock::Unsupported {
                    label: "[unsupported image block]",
                },
                _ => ProjectedBlock::Generic {
                    block: Some(block),
                    fallback: &entry.text,
                },
            },
        })
        .collect()
}

fn project_generic_blocks(entry: &DshRenderEntry) -> Vec<ProjectedRenderBlock<'_>> {
    if entry.content.blocks.is_empty() {
        return vec![ProjectedRenderBlock {
            source_index: None,
            block: ProjectedBlock::Generic {
                block: None,
                fallback: &entry.text,
            },
        }];
    }
    entry
        .content
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| ProjectedRenderBlock {
            source_index: Some(index),
            block: ProjectedBlock::Generic {
                block: Some(block),
                fallback: &entry.text,
            },
        })
        .collect()
}

pub fn reasoning_text(entry: &DshRenderEntry) -> Option<&str> {
    entry.content.blocks.iter().find_map(|block| match block {
        DshRenderBlock::Reasoning { text } => Some(text.as_str()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant_entry() -> DshRenderEntry {
        let mut entry = DshRenderEntry::plain(
            DshRenderEntryId::Partial {
                turn: 7,
                step: 3,
                surface: 1,
            },
            42,
            DshRenderKind::Assistant,
            "fallback",
        );
        entry.created_at_ms = Some(10);
        entry.started_at_ms = Some(11);
        entry.finished_at_ms = Some(20);
        entry.content.blocks = vec![
            DshRenderBlock::Reasoning {
                text: "reason".into(),
            },
            DshRenderBlock::Markdown {
                text: "answer".into(),
            },
            DshRenderBlock::Image {
                attachment_id: Some("asset-1".into()),
                media_type: Some("image/png".into()),
                name: Some("plot.png".into()),
                raw: "opaque".into(),
            },
        ];
        entry
    }

    #[test]
    fn projection_keeps_stable_identity_mode_finish_and_times() {
        let entry = assistant_entry();
        let projected = project_entry(&entry, DisplayMode::Expanded);

        assert_eq!(projected.id.into_dsh(), entry.id);
        assert_eq!(projected.display_mode, DisplayMode::Expanded);
        assert!(!projected.is_running);
        assert!(projected.is_finished);
        assert_eq!(projected.created_at_ms, Some(10));
        assert_eq!(projected.started_at_ms, Some(11));
        assert_eq!(projected.finished_at_ms, Some(20));
    }

    #[test]
    fn assistant_projection_preserves_typed_block_order_and_indices() {
        let entry = assistant_entry();
        let projected = project_entry(&entry, DisplayMode::Truncated);

        assert_eq!(
            projected
                .blocks
                .iter()
                .map(|block| block.source_index)
                .collect::<Vec<_>>(),
            vec![Some(0), Some(1), Some(2)]
        );
        assert!(matches!(
            projected.blocks[0].block,
            ProjectedBlock::Thinking { text: "reason" }
        ));
        assert!(matches!(
            projected.blocks[1].block,
            ProjectedBlock::AgentMarkdown { block: Some(_), .. }
        ));
        assert!(matches!(
            projected.blocks[2].block,
            ProjectedBlock::Unsupported {
                label: "[unsupported image block]"
            }
        ));
    }

    #[test]
    fn running_projection_is_not_finished() {
        let mut entry = assistant_entry();
        entry.finish = DshRenderFinish::Running;
        entry.finished_at_ms = None;

        let projected = project_entry(&entry, DisplayMode::Truncated);
        assert!(projected.is_running);
        assert!(!projected.is_finished);
    }

    #[test]
    fn empty_assistant_uses_entry_fallback_without_inventing_block_index() {
        let mut entry = assistant_entry();
        entry.content.blocks.clear();

        let projected = project_entry(&entry, DisplayMode::Collapsed);
        assert!(matches!(
            projected.blocks.as_slice(),
            [ProjectedRenderBlock {
                source_index: None,
                block: ProjectedBlock::AgentMarkdown {
                    block: None,
                    fallback: "fallback"
                }
            }]
        ));
    }
}
