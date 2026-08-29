//! DSH entry materialization for the Grok-derived scrollback renderer.
//!
//! The host adapter keeps typed DSH blocks intact. This module owns the
//! user-visible role, indentation and copy projection so production and
//! parity consume the same renderer without inspecting protocol JSON.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use dsh_pager::scrollback::Scrollback;
use dsh_pager::{
    DshRenderBlock, DshRenderContent, DshRenderEntry, DshRenderEntryId, DshRenderEntryRef,
    DshRenderFinish, DshRenderKind, DshRenderVisibility,
};
#[cfg(test)]
use dsh_pager::{DshToolCallView, DshToolDiff, DshToolKind, DshToolResult, DshToolResultView};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    appearance::{GrokAppearanceSnapshot, ScrollbackAppearance},
    geometry::HitTarget,
    glyphs,
    host_adapter::TranscriptRow,
    render::wrapping::word_wrap_line,
    scrollback::{
        agent::AgentMessageBlock,
        entry_renderer::{
            EntryLayoutSpec, EntryRenderer, EntrySourceLine, GroupHeaderSpec,
            TIMESTAMP_RESERVED_WIDTH, TimestampLabel, timestamp_label,
        },
        thinking::{ThinkingBlock, ThinkingBlockContext},
        tool::ToolBlockContext,
        types::{AccentStyle, DisplayMode},
        user::{UserPromptBlock, UserPromptContext},
    },
    scrollback_adapter::{
        host_pane::ProjectionInfo,
        project_entry::{
            ProjectedBlock, ProjectedLine as SemanticLine, materialize_block, project_entry,
        },
        project_tool::project_tool,
    },
    theme::Theme,
};

#[cfg(test)]
use crate::{
    scrollback::entry_renderer::DynamicAccentSpec,
    scrollback_adapter::{host_pane::DshScrollbackHost as ScrollbackPane, tick::GROK_WAVE_SPEED},
};

/// A rich line after semantic block rendering and terminal-width wrapping.
/// `line_index` is stable within an entry at a given width and is shared with
/// hit testing and selection; `screen_y` is filled only by the viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichPaintLine {
    pub entry_id: DshRenderEntryId,
    /// Structured block index when this line belongs to a foldable block
    /// inside an assistant streaming surface. `None` denotes entry chrome or
    /// a legacy/plain row.
    pub block_index: Option<usize>,
    pub line_index: usize,
    pub header: bool,
    pub group_header: bool,
    pub rail: bool,
    /// Grok-owned accent style for the entry rail.
    pub accent: Option<AccentStyle>,
    /// Accent used only during Grok's bounded post-finish flash.
    pub flash_accent: Option<Color>,
    /// Grok-owned accent style for the first-row bullet.
    pub bullet: Option<AccentStyle>,
    /// Whether the left rail is in Grok's short post-finish accent flash.
    pub accent_flash: bool,
    /// Logical row phase inside the entry's rail.
    pub accent_wave_row: u16,
    /// Span index of the bullet after EntryRenderer inserts its chrome.
    pub bullet_span: Option<usize>,
    /// Freeze animated chrome while this entry is waiting on user input.
    pub pending_user_input: bool,
    pub selectable: bool,
    pub background: Option<Color>,
    pub copy_text: String,
    /// Presentation columns painted before `copy_text`.
    ///
    /// Grok reserves one accent column and two left-pad columns for every
    /// entry, including plain Markdown. Keeping the offset in the paint line
    /// keeps hit-testing aligned with the visible text.
    pub content_offset: u16,
    /// Selectable content width after chrome and timestamp reservation.
    pub content_width: u16,
    /// Timestamp painted as a non-selectable right-side overlay. The
    /// transcript copy/selection geometry intentionally keeps it separate.
    pub timestamp: Option<TimestampLabel>,
    /// Exact source separator before this visual row (`None` = hard break).
    pub joiner_to_previous: Option<String>,
    pub screen_y: u16,
    pub line: Line<'static>,
}

pub(crate) struct MaterializeContext<'a> {
    expanded_blocks: &'a HashSet<(DshRenderEntryId, usize)>,
    show_timestamps: bool,
    appearance: &'a ScrollbackAppearance,
    selected_target: Option<&'a HitTarget>,
}

impl<'a> MaterializeContext<'a> {
    pub(crate) fn new(
        expanded_blocks: &'a HashSet<(DshRenderEntryId, usize)>,
        show_timestamps: bool,
        appearance: &'a ScrollbackAppearance,
        selected_target: Option<&'a HitTarget>,
    ) -> Self {
        Self {
            expanded_blocks,
            show_timestamps,
            appearance,
            selected_target,
        }
    }
}

#[cfg(test)]
fn semantic_lines(
    entry: &DshRenderEntry,
    width: usize,
    theme: Theme,
    projection: &ProjectionInfo,
    context: &MaterializeContext<'_>,
) -> Vec<RichPaintLine> {
    semantic_lines_with_markdown_cache(entry, width, theme, projection, context, None)
}

pub(crate) fn semantic_lines_with_markdown_cache(
    entry: &DshRenderEntry,
    width: usize,
    theme: Theme,
    projection: &ProjectionInfo,
    context: &MaterializeContext<'_>,
    markdown: Option<&HashMap<usize, Vec<Line<'static>>>>,
) -> Vec<RichPaintLine> {
    if projection.group_hidden {
        return Vec::new();
    }
    let effective_mode = if projection.group_expanded {
        DisplayMode::Expanded
    } else {
        projection.mode
    };
    let Some(row) = projected_row(entry, effective_mode, width) else {
        return Vec::new();
    };
    let timestamp = if context.show_timestamps
        && matches!(entry.kind, DshRenderKind::User | DshRenderKind::Assistant)
        && !projection.group_header
    {
        timestamp_label(entry.created_at_ms)
    } else {
        None
    };
    let semantic = render_semantic_lines(
        entry,
        &row,
        theme,
        width.saturating_sub(usize::from(timestamp.is_some()) * TIMESTAMP_RESERVED_WIDTH),
        effective_mode,
        context,
        markdown,
    );
    let fallback_color = if projection.group_failed || entry.finish == DshRenderFinish::Failed {
        theme.accent_error
    } else if projection.group_running || entry.finish == DshRenderFinish::Running {
        context.appearance.scrollback.blocks.execute.running_accent
    } else {
        theme.gray
    };
    let fallback_animated = (projection.group_running || entry.finish == DshRenderFinish::Running)
        && entry.finish != DshRenderFinish::Failed;
    let accent_flash = entry.finished_at_ms.is_some()
        && !fallback_animated
        && matches!(
            entry.kind,
            DshRenderKind::Thinking | DshRenderKind::ToolCall
        );
    let flash_accent = accent_flash.then_some(match entry.kind {
        DshRenderKind::Thinking => theme.accent_thinking,
        DshRenderKind::ToolCall if entry.finish == DshRenderFinish::Failed => theme.accent_error,
        DshRenderKind::ToolCall => theme.accent_success,
        _ => fallback_color,
    });
    let selected_content = !projection.group_header
        && target_selects_entry(context.selected_target, entry.id)
        && entry.kind == DshRenderKind::ToolCall;
    let collapsed_group_rail =
        projection.group_header && !projection.group_running && !projection.group_failed;
    let collapsed_tool_rail = !fallback_animated
        && entry.kind == DshRenderKind::ToolCall
        && effective_mode == DisplayMode::Collapsed;
    let collapsed_rail = !selected_content && (collapsed_group_rail || collapsed_tool_rail);
    let fallback_accent = if fallback_animated {
        AccentStyle::animated(fallback_color)
    } else {
        AccentStyle::static_color(fallback_color)
    };
    let source = semantic
        .into_iter()
        .map(|line| EntrySourceLine {
            content: line.line,
            block_index: line.block_index,
            rail: line.rail,
            header: line.header,
            selectable: line.selectable,
            accent: line.accent,
            bullet: line.bullet,
            background: line.background,
            accent_background: line.accent_background,
            joiner: line.joiner,
        })
        .collect();
    let group_header = projection.group_header.then(|| GroupHeaderSpec {
        label: projection
            .group_label
            .clone()
            .unwrap_or_else(|| "1 more".into()),
        expanded: projection.group_expanded,
        running: projection.group_running,
        failed: projection.group_failed,
        tool_accent: theme.accent_tool,
        error_accent: theme.accent_error,
        muted: theme.gray,
        text: theme.gray_bright,
    });
    EntryRenderer::render_entry(
        source,
        EntryLayoutSpec {
            width,
            layout: context.appearance.scrollback.layout,
            fallback_accent,
            collapsed_accent: collapsed_rail,
            dim_accent: context.appearance.scrollback.display.dim_accent,
            fallback_background: projection.background,
            base_background: theme.bg_base,
            flash_accent,
            accent_flash,
            timestamp,
            group_header,
        },
        0,
        None,
    )
    .into_iter()
    .map(|rendered| RichPaintLine {
        entry_id: entry.id,
        block_index: rendered.block_index,
        line_index: rendered.line_index,
        header: rendered.header,
        group_header: rendered.group_header,
        rail: rendered.accent.is_some() || rendered.flash_accent.is_some(),
        accent: rendered.accent,
        flash_accent: rendered.flash_accent,
        bullet: rendered.bullet,
        accent_flash: rendered.accent_flash,
        accent_wave_row: rendered.line_index.min(u16::MAX as usize) as u16,
        bullet_span: rendered.bullet_span,
        pending_user_input: false,
        selectable: rendered.selectable,
        background: rendered.background,
        copy_text: rendered.copy_text,
        content_offset: rendered.content_offset,
        content_width: rendered.content_width,
        timestamp: rendered.timestamp,
        joiner_to_previous: rendered.joiner_to_previous,
        screen_y: 0,
        line: rendered.line,
    })
    .collect()
}

pub(crate) fn is_local_foldable_block(block: &DshRenderBlock) -> bool {
    matches!(
        block,
        DshRenderBlock::Reasoning { .. }
            | DshRenderBlock::ToolCall { .. }
            | DshRenderBlock::ToolResult { .. }
    )
}

fn is_operational_block(block: &DshRenderBlock) -> bool {
    is_local_foldable_block(block)
}

fn collapsed_operational_line(
    block: &DshRenderBlock,
    entry: &DshRenderEntry,
    theme: Theme,
) -> Line<'static> {
    match block {
        DshRenderBlock::ToolCall { .. } => project_tool(block)
            .and_then(|tool| {
                let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
                tool.render(ToolBlockContext {
                    mode: DisplayMode::Collapsed,
                    is_running: entry.finish == DshRenderFinish::Running,
                    is_selected: false,
                    width: 120,
                    appearance: &appearance,
                    theme,
                })
                .output
                .lines
                .into_iter()
                .next()
                .map(|line| line.content)
            })
            .unwrap_or_else(|| Line::from("Tool")),
        DshRenderBlock::ToolResult { is_error, .. } => Line::from(Span::styled(
            if *is_error {
                "✗ result"
            } else {
                "✓ result"
            },
            Style::default().fg(if *is_error {
                theme.accent_error
            } else {
                theme.gray
            }),
        )),
        _ => Line::from(""),
    }
}

pub(crate) fn now_epoch_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

pub(crate) fn finish_flash_active(finished_at_ms: Option<u64>, now_ms: Option<u64>) -> bool {
    let Some(finished_at_ms) = finished_at_ms else {
        return false;
    };
    let Some(now_ms) = now_ms else {
        return false;
    };
    now_ms
        .checked_sub(finished_at_ms)
        .is_some_and(|elapsed| elapsed < FINISH_FLASH_DURATION_MS)
}

pub(crate) const FINISH_FLASH_DURATION_MS: u64 = 400;

fn thinking_elapsed_ms(entry: &DshRenderEntry) -> Option<u64> {
    let started = entry.started_at_ms?;
    let ended = if entry.finish == DshRenderFinish::Running {
        now_epoch_ms()?
    } else {
        entry.finished_at_ms?
    };
    ended.checked_sub(started)
}

/// Build the block-level projection that the upstream Grok scrollback gets
/// from `ThinkingBlock`, `ToolCallBlock` and `AgentMessageBlock`. The host
/// keeps one stable streaming surface, but its typed blocks still receive
/// independent display modes and hit targets here.
fn render_semantic_lines(
    entry: &DshRenderEntry,
    row: &TranscriptRow,
    theme: Theme,
    width: usize,
    entry_mode: DisplayMode,
    context: &MaterializeContext<'_>,
    markdown: Option<&HashMap<usize, Vec<Line<'static>>>>,
) -> Vec<SemanticLine> {
    let appearance = context.appearance;
    let expanded_blocks = context.expanded_blocks;
    let selected_target = context.selected_target;
    let content_width = width
        .saturating_sub(EntryRenderer::chrome_width(&appearance.scrollback.layout))
        .max(1);
    let projected_entry = project_entry(entry, entry_mode);
    let has_foldable_blocks = row.kind == DshRenderKind::Assistant
        && (entry.group_key.is_some() || row.content.blocks.len() > 1)
        && row.content.blocks.iter().any(is_local_foldable_block);
    if !has_foldable_blocks {
        match projected_entry.blocks.as_slice() {
            [projected_block] if matches!(projected_block.block, ProjectedBlock::User { .. }) => {
                let ProjectedBlock::User { text } = &projected_block.block else {
                    unreachable!()
                };
                let block = UserPromptBlock::new(text).render(UserPromptContext {
                    mode: projected_entry.display_mode,
                    width: content_width,
                    appearance,
                    theme,
                });
                return materialize_block(block, content_width, theme, None);
            }
            [projected_block]
                if matches!(projected_block.block, ProjectedBlock::Thinking { .. }) =>
            {
                let ProjectedBlock::Thinking { text } = &projected_block.block else {
                    unreachable!()
                };
                let body = render_markdown_body_cached(
                    text,
                    theme,
                    content_width,
                    projected_block.source_index,
                    markdown,
                );
                let block = ThinkingBlock::new(body, thinking_elapsed_ms(entry)).render(
                    ThinkingBlockContext {
                        mode: projected_entry.display_mode,
                        is_running: projected_entry.is_running,
                        appearance,
                        theme,
                    },
                );
                return materialize_block(
                    block,
                    content_width,
                    theme,
                    projected_block.source_index,
                );
            }
            [projected_block]
                if matches!(projected_block.block, ProjectedBlock::AgentMarkdown { .. }) =>
            {
                let ProjectedBlock::AgentMarkdown { block, fallback } = &projected_block.block
                else {
                    unreachable!()
                };
                let body = render_agent_projection_cached(
                    *block,
                    fallback,
                    theme,
                    content_width,
                    projected_block.source_index,
                    markdown,
                );
                return materialize_block(
                    AgentMessageBlock::new(body).render(),
                    content_width,
                    theme,
                    projected_block.source_index,
                );
            }
            [projected_block]
                if matches!(projected_block.block, ProjectedBlock::Unsupported { .. }) =>
            {
                let ProjectedBlock::Unsupported { label } = &projected_block.block else {
                    unreachable!()
                };
                return vec![SemanticLine {
                    line: Line::from(Span::styled(
                        *label,
                        Style::default().fg(theme.accent_error),
                    )),
                    block_index: projected_block.source_index,
                    rail: false,
                    header: true,
                    selectable: true,
                    accent: None,
                    bullet: None,
                    background: None,
                    accent_background: false,
                    joiner: None,
                }];
            }
            [projected_block] if matches!(projected_block.block, ProjectedBlock::Tool { .. }) => {
                let ProjectedBlock::Tool { block } = projected_block.block else {
                    unreachable!()
                };
                if let Some(tool) = project_tool(block) {
                    let rendered = tool.render(ToolBlockContext {
                        mode: projected_entry.display_mode,
                        is_running: projected_entry.is_running,
                        is_selected: target_selects_block(
                            selected_target,
                            entry.id,
                            projected_block.source_index,
                        ),
                        width: content_width,
                        appearance,
                        theme,
                    });
                    return materialize_block(
                        rendered,
                        content_width,
                        theme,
                        projected_block.source_index,
                    );
                }
            }
            _ => {}
        }
        if row.kind != DshRenderKind::Assistant {
            return render_generic_row_at_width(row, theme, content_width)
                .into_iter()
                .enumerate()
                .map(|(index, line)| SemanticLine {
                    line,
                    block_index: None,
                    rail: false,
                    header: index == 0,
                    selectable: true,
                    accent: None,
                    bullet: None,
                    background: None,
                    accent_background: false,
                    joiner: None,
                })
                .collect();
        }
    }

    let mut semantic = Vec::new();
    for (position, projected_block) in projected_entry.blocks.iter().enumerate() {
        let block_index = projected_block.source_index;
        let expanded = !has_foldable_blocks
            || block_index.is_some_and(|index| expanded_blocks.contains(&(entry.id, index)));
        if position > 0 {
            let previous_index = projected_entry.blocks[position - 1].source_index;
            let previous = previous_index.and_then(|index| row.content.blocks.get(index));
            let current = block_index.and_then(|index| row.content.blocks.get(index));
            let previous_expanded =
                previous_index.is_some_and(|index| expanded_blocks.contains(&(entry.id, index)));
            let dense_operational_run = previous.is_some_and(is_local_foldable_block)
                && current.is_some_and(is_local_foldable_block)
                && !previous_expanded
                && !expanded;
            if !dense_operational_run {
                semantic.push(SemanticLine {
                    line: Line::from(""),
                    block_index: None,
                    rail: false,
                    header: false,
                    selectable: false,
                    accent: None,
                    bullet: None,
                    background: None,
                    accent_background: false,
                    joiner: None,
                });
            }
        }
        let (lines, rail) = match &projected_block.block {
            ProjectedBlock::Thinking { text } => {
                let mode = if expanded {
                    DisplayMode::Expanded
                } else if projected_entry.is_running {
                    DisplayMode::Truncated
                } else {
                    DisplayMode::Collapsed
                };
                let body =
                    render_markdown_body_cached(text, theme, content_width, block_index, markdown);
                let rendered = ThinkingBlock::new(body, thinking_elapsed_ms(entry)).render(
                    ThinkingBlockContext {
                        mode,
                        is_running: projected_entry.is_running,
                        appearance,
                        theme,
                    },
                );
                semantic.extend(materialize_block(
                    rendered,
                    content_width,
                    theme,
                    block_index,
                ));
                continue;
            }
            ProjectedBlock::AgentMarkdown { block, fallback } => {
                let rendered = render_agent_projection_cached(
                    *block,
                    fallback,
                    theme,
                    content_width,
                    block_index,
                    markdown,
                );
                semantic.extend(materialize_block(
                    AgentMessageBlock::new(rendered).render(),
                    content_width,
                    theme,
                    block_index,
                ));
                continue;
            }
            ProjectedBlock::Unsupported { label } => (
                vec![Line::from(Span::styled(
                    *label,
                    Style::default().fg(theme.accent_error),
                ))],
                false,
            ),
            ProjectedBlock::Tool { block } => {
                if let Some(tool) = project_tool(block) {
                    let mode = if expanded {
                        DisplayMode::Expanded
                    } else {
                        DisplayMode::Collapsed
                    };
                    semantic.extend(materialize_block(
                        tool.render(ToolBlockContext {
                            mode,
                            is_running: projected_entry.is_running,
                            is_selected: target_selects_block(
                                selected_target,
                                entry.id,
                                block_index,
                            ),
                            width: content_width,
                            appearance,
                            theme,
                        }),
                        content_width,
                        theme,
                        block_index,
                    ));
                    continue;
                }
                let mut rendered = Vec::new();
                render_block(&mut rendered, block, theme, 0, content_width);
                (rendered, is_operational_block(block))
            }
            ProjectedBlock::Generic {
                block: Some(block), ..
            } => {
                let lines = if is_local_foldable_block(block) && !expanded {
                    vec![collapsed_operational_line(block, entry, theme)]
                } else {
                    let mut rendered = Vec::new();
                    render_block(&mut rendered, block, theme, 0, content_width);
                    rendered
                };
                (lines, is_operational_block(block))
            }
            ProjectedBlock::User { text } => {
                let rendered = UserPromptBlock::new(text).render(UserPromptContext {
                    mode: projected_entry.display_mode,
                    width: content_width,
                    appearance,
                    theme,
                });
                semantic.extend(materialize_block(
                    rendered,
                    content_width,
                    theme,
                    block_index,
                ));
                continue;
            }
            ProjectedBlock::Generic {
                block: None,
                fallback,
            } => (vec![Line::from((*fallback).to_string())], false),
        };
        for (line_index, line) in lines.into_iter().enumerate() {
            semantic.push(SemanticLine {
                line,
                block_index,
                rail,
                header: line_index == 0,
                selectable: true,
                accent: None,
                bullet: None,
                background: None,
                accent_background: false,
                joiner: None,
            });
        }
    }
    semantic
}

fn target_selects_block(
    target: Option<&HitTarget>,
    entry_id: DshRenderEntryId,
    block_index: Option<usize>,
) -> bool {
    target.is_some_and(|target| match target {
        HitTarget::TranscriptEntry(selected) => *selected == entry_id,
        HitTarget::TranscriptBlock {
            entry_id: selected,
            block_index: selected_block,
        } => *selected == entry_id && block_index == Some(*selected_block),
        _ => false,
    })
}

fn target_selects_entry(target: Option<&HitTarget>, entry_id: DshRenderEntryId) -> bool {
    target.is_some_and(|target| match target {
        HitTarget::TranscriptEntry(selected)
        | HitTarget::TranscriptBlock {
            entry_id: selected, ..
        } => *selected == entry_id,
        _ => false,
    })
}

fn render_markdown_body(text: &str, theme: Theme, width: usize) -> Vec<Line<'static>> {
    let mut body = Vec::new();
    render_markdown(&mut body, text, theme, "", width);
    body
}

fn render_markdown_body_cached(
    text: &str,
    theme: Theme,
    width: usize,
    block_index: Option<usize>,
    cache: Option<&HashMap<usize, Vec<Line<'static>>>>,
) -> Vec<Line<'static>> {
    block_index
        .and_then(|index| cache.and_then(|cache| cache.get(&index)))
        .cloned()
        .unwrap_or_else(|| render_markdown_body(text, theme, width))
}

fn render_agent_projection(
    block: Option<&DshRenderBlock>,
    fallback: &str,
    theme: Theme,
    width: usize,
) -> Vec<Line<'static>> {
    let Some(block) = block else {
        return render_markdown_body(fallback, theme, width);
    };
    let mut body = Vec::new();
    render_block(&mut body, block, theme, 0, width);
    body
}

fn render_agent_projection_cached(
    block: Option<&DshRenderBlock>,
    fallback: &str,
    theme: Theme,
    width: usize,
    block_index: Option<usize>,
    cache: Option<&HashMap<usize, Vec<Line<'static>>>>,
) -> Vec<Line<'static>> {
    let Some(DshRenderBlock::Markdown { text }) = block else {
        return render_agent_projection(block, fallback, theme, width);
    };
    render_markdown_body_cached(text, theme, width, block_index, cache)
}

fn render_generic_row_at_width(
    row: &TranscriptRow,
    theme: Theme,
    width: usize,
) -> Vec<Line<'static>> {
    let color = if row.kind == DshRenderKind::ToolCall {
        match row.finish {
            DshRenderFinish::Failed => theme.accent_error,
            _ => theme.gray,
        }
    } else {
        color_for_kind(row.kind, theme)
    };
    if row.kind == DshRenderKind::ToolCall && row.content.blocks.is_empty() && row.text.is_empty() {
        let header = row.label.strip_prefix("› ").unwrap_or(&row.label);
        return vec![Line::from(vec![
            Span::styled(
                format!("{} ", glyphs::diamond_filled()),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                header.to_string(),
                Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
            ),
        ])];
    }

    debug_assert!(!matches!(
        row.kind,
        DshRenderKind::User | DshRenderKind::Assistant | DshRenderKind::Thinking
    ));
    let mut lines = vec![Line::from(Span::styled(
        row.label.clone(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))];

    if row.content.blocks.is_empty() {
        push_plain_lines(&mut lines, &row.text, Style::default().fg(color), "");
    } else {
        for block in &row.content.blocks {
            render_block(&mut lines, block, theme, 0, width);
        }
    }
    lines
}

/// Project canonical entries into the default Grok-style transcript. Hidden
/// entries remain in the host scrollback but produce no layout rows. A
/// collapsed entry becomes one synthetic summary/header line; expanding it is
/// a local view concern and never mutates the history DTO.
fn projected_row(entry: &DshRenderEntry, mode: DisplayMode, width: usize) -> Option<TranscriptRow> {
    if entry.visibility == DshRenderVisibility::Hidden {
        return None;
    }
    let mut row = TranscriptRow::from(entry.clone());
    if mode == DisplayMode::Expanded {
        return Some(row);
    }
    if (mode == DisplayMode::Truncated && entry.kind != DshRenderKind::Thinking)
        || entry.kind == DshRenderKind::User
    {
        let text = truncate_visual_lines(&row.text, width, 3);
        row.text = text;
        row.content = DshRenderContent::default();
        return Some(row);
    }
    if mode == DisplayMode::Collapsed {
        if entry
            .content
            .blocks
            .iter()
            .any(|block| matches!(block, DshRenderBlock::ToolCall { .. }))
        {
            return Some(row);
        }
        let summary = entry
            .text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or(entry.kind.label());
        let summary = truncate_summary(summary, 96);
        row.label = format!("{} · {}", row.label, summary);
        row.text.clear();
        row.content = DshRenderContent::default();
    }
    Some(row)
}

fn take_lines(text: &str, max_lines: usize) -> String {
    text.lines()
        .take(max_lines.max(1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_visual_lines(text: &str, width: usize, max_lines: usize) -> String {
    let mut lines = Vec::new();
    for logical in text.split('\n') {
        for wrapped in word_wrap_line(&Line::from(logical), width.max(1)) {
            lines.push(wrapped.to_string());
            if lines.len() >= max_lines.max(1) {
                return lines.join("\n");
            }
        }
    }
    if lines.is_empty() {
        take_lines(text, max_lines)
    } else {
        lines.join("\n")
    }
}

fn truncate_summary(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let summary = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{summary}…")
    } else {
        summary
    }
}

pub(crate) fn default_display_mode_ref(entry: DshRenderEntryRef<'_>, width: usize) -> DisplayMode {
    if entry.visibility == DshRenderVisibility::Collapsed {
        return DisplayMode::Collapsed;
    }
    match entry.kind {
        DshRenderKind::User => {
            let rows = entry
                .text
                .split('\n')
                .map(|text| word_wrap_line(&Line::from(text), width.max(1)).len().max(1))
                .sum::<usize>();
            if rows > 3 {
                DisplayMode::Truncated
            } else {
                DisplayMode::Expanded
            }
        }
        DshRenderKind::Thinking if entry.finish == DshRenderFinish::Running => {
            DisplayMode::Truncated
        }
        DshRenderKind::Thinking
        | DshRenderKind::ToolCall
        | DshRenderKind::ToolResult
        | DshRenderKind::Error
        | DshRenderKind::AgentContext
        | DshRenderKind::Context
        | DshRenderKind::Compaction => DisplayMode::Collapsed,
        _ => DisplayMode::Expanded,
    }
}

fn render_projected_tool_lines(
    lines: &mut Vec<Line<'static>>,
    block: &DshRenderBlock,
    theme: Theme,
    indent: usize,
    width: usize,
) {
    let Some(tool) = project_tool(block) else {
        return;
    };
    let is_running = matches!(block, DshRenderBlock::ToolCall { result: None, .. });
    let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
    let rendered = tool.render(ToolBlockContext {
        mode: DisplayMode::Expanded,
        is_running,
        is_selected: false,
        width: width.saturating_sub(indent.saturating_mul(2)).max(1),
        appearance: &appearance,
        theme,
    });
    let prefix = " ".repeat(indent.saturating_mul(2));
    for (index, mut block_line) in rendered.output.lines.into_iter().enumerate() {
        let mut spans = Vec::with_capacity(block_line.content.spans.len() + 2);
        if !prefix.is_empty() {
            spans.push(Span::raw(prefix.clone()));
        }
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        if let Some(background) = block_line.background {
            for span in &mut block_line.content.spans {
                span.style = span.style.bg(background);
            }
        }
        spans.extend(block_line.content.spans);
        lines.push(Line::from(spans));
    }
}

fn render_block(
    lines: &mut Vec<Line<'static>>,
    block: &DshRenderBlock,
    theme: Theme,
    indent: usize,
    width: usize,
) {
    let prefix = " ".repeat(indent.saturating_mul(2));
    match block {
        DshRenderBlock::Markdown { text } => {
            render_markdown(lines, text, theme, &prefix, width);
        }
        DshRenderBlock::Plain { text } => {
            push_plain_lines(
                lines,
                text,
                Style::default().fg(theme.text_primary),
                &prefix,
            );
        }
        DshRenderBlock::Reasoning { text } => {
            push_plain_lines(lines, text, Style::default().fg(theme.gray), &prefix);
        }
        DshRenderBlock::Image {
            attachment_id,
            media_type,
            name,
            ..
        } => {
            let label = name
                .as_deref()
                .or(media_type.as_deref())
                .or(attachment_id.as_deref())
                .unwrap_or("image");
            lines.push(Line::from(Span::styled(
                format!("{prefix}[image: {label}]"),
                Style::default().fg(theme.accent_assistant),
            )));
        }
        DshRenderBlock::ToolCall { .. } => {
            render_projected_tool_lines(lines, block, theme, indent, width)
        }
        DshRenderBlock::ToolResult {
            blocks, is_error, ..
        } => {
            lines.push(Line::from(Span::styled(
                format!(
                    "{prefix}{}",
                    if *is_error {
                        "✗ result"
                    } else {
                        "✓ result"
                    }
                ),
                Style::default().fg(if *is_error {
                    theme.accent_error
                } else {
                    theme.gray_bright
                }),
            )));
            for child in blocks {
                render_block(lines, child, theme, indent + 1, width);
            }
        }
        DshRenderBlock::Diff {
            path,
            old_text,
            new_text,
        } => render_diff(lines, path.as_deref(), old_text, new_text, theme, indent),
        DshRenderBlock::Unknown { kind, raw } => {
            lines.push(Line::from(Span::styled(
                format!("{prefix}[unsupported block: {kind}]"),
                Style::default().fg(theme.accent_user),
            )));
            push_plain_lines(
                lines,
                raw,
                Style::default().fg(theme.gray),
                &format!("{prefix}  "),
            );
        }
    }
}

/// Render Markdown through the fixed Grok Build renderer.
fn render_markdown(
    lines: &mut Vec<Line<'static>>,
    text: &str,
    _theme: Theme,
    prefix: &str,
    width: usize,
) {
    let content_width = width
        .saturating_sub(unicode_width::UnicodeWidthStr::width(prefix))
        .max(1);
    lines.extend(crate::render::markdown::render(text, content_width, prefix));
}

fn render_diff(
    lines: &mut Vec<Line<'static>>,
    path: Option<&str>,
    old_text: &str,
    new_text: &str,
    theme: Theme,
    indent: usize,
) {
    let prefix = " ".repeat(indent.saturating_mul(2));
    if let Some(path) = path {
        lines.push(Line::from(Span::styled(
            format!("{prefix}diff {path}"),
            Style::default()
                .fg(theme.gray_bright)
                .add_modifier(Modifier::BOLD),
        )));
    }
    for line in old_text.lines() {
        lines.push(Line::from(Span::styled(
            format!("{prefix}- {line}"),
            Style::default()
                .fg(theme.diff_delete_fg)
                .bg(theme.diff_delete_bg),
        )));
    }
    for line in new_text.lines() {
        lines.push(Line::from(Span::styled(
            format!("{prefix}+ {line}"),
            Style::default()
                .fg(theme.diff_insert_fg)
                .bg(theme.diff_insert_bg),
        )));
    }
}

fn push_plain_lines(lines: &mut Vec<Line<'static>>, text: &str, style: Style, prefix: &str) {
    for line in text.split('\n') {
        lines.push(Line::from(Span::styled(format!("{prefix}{line}"), style)));
    }
}

fn color_for_kind(kind: DshRenderKind, theme: Theme) -> ratatui::style::Color {
    match kind {
        DshRenderKind::User => theme.accent_user,
        DshRenderKind::Assistant => theme.accent_assistant,
        DshRenderKind::Thinking | DshRenderKind::ToolCall | DshRenderKind::ToolResult => theme.gray,
        DshRenderKind::Error => theme.accent_error,
        _ => theme.gray,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scrollback_adapter::tick::DEFAULT_WAVE_ROWS, theme::wave_brightness};
    use dsh_pager::{
        DshRenderBlock, DshRenderContent, DshRenderEntryId, DshRenderFinish, DshRenderVisibility,
    };
    use dsh_pager_protocol::{HistoryEntry, SessionEvent};
    use serde_json::json;

    fn materialized_entries(
        entries: &[DshRenderEntry],
        width: usize,
        theme: Theme,
    ) -> Vec<RichPaintLine> {
        let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
        let mut lines = Vec::new();
        for entry in entries {
            let projection = ProjectionInfo::plain(entry, width.max(1), theme);
            lines.extend(semantic_lines(
                entry,
                width.max(1),
                theme,
                &projection,
                &MaterializeContext::new(&HashSet::new(), true, &appearance, None),
            ));
        }
        lines
    }

    fn entry_from_row(row: TranscriptRow) -> DshRenderEntry {
        DshRenderEntry {
            id: row.id,
            source_seq: row.source_seq,
            created_at_ms: row.created_at_ms,
            started_at_ms: row.started_at_ms,
            finished_at_ms: row.finished_at_ms,
            kind: row.kind,
            text: row.text,
            partial: false,
            visibility: row.visibility,
            finish: row.finish,
            group_key: row.group_key,
            selectable: row.selectable,
            lineage: Vec::new(),
            content: row.content,
        }
    }

    fn materialized_row(row: TranscriptRow, width: usize, theme: Theme) -> Vec<RichPaintLine> {
        materialized_entries(&[entry_from_row(row)], width, theme)
    }

    fn materialized_row_with_expanded_blocks(
        row: TranscriptRow,
        width: usize,
        theme: Theme,
    ) -> Vec<RichPaintLine> {
        let entry = entry_from_row(row);
        let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
        let mut projection = ProjectionInfo::plain(&entry, width.max(1), theme);
        projection.mode = DisplayMode::Expanded;
        let expanded_blocks = entry
            .content
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| is_local_foldable_block(block))
            .map(|(index, _)| (entry.id, index))
            .collect();
        semantic_lines(
            &entry,
            width.max(1),
            theme,
            &projection,
            &MaterializeContext::new(&expanded_blocks, true, &appearance, None),
        )
    }

    fn row() -> TranscriptRow {
        TranscriptRow {
            id: DshRenderEntryId::Event { seq: 7 },
            created_at_ms: None,
            started_at_ms: None,
            finished_at_ms: None,
            label: "Assistant".into(),
            text: "fallback".into(),
            kind: DshRenderKind::Assistant,
            visibility: DshRenderVisibility::Visible,
            finish: DshRenderFinish::Completed,
            group_key: None,
            selectable: true,
            source_seq: 7,
            seq: dsh_pager::DshSeq::new(7),
            content: DshRenderContent {
                blocks: vec![DshRenderBlock::ToolResult {
                    call_id: Some("call-1".into()),
                    blocks: vec![DshRenderBlock::Diff {
                        path: Some("src/lib.rs".into()),
                        old_text: "old".into(),
                        new_text: "new".into(),
                    }],
                    is_error: false,
                }],
                fallback: "fallback".into(),
            },
        }
    }

    #[test]
    fn structured_blocks_render_roles_and_copy_without_flattening_at_adapter() {
        let row = row();
        let copied = row.content.display_text();
        let rendered = materialized_row(row, 120, *Theme::current())
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("src/lib.rs"));
        assert!(rendered.contains("- old"));
        assert!(rendered.contains("+ new"));
        assert!(copied.contains("diff src/lib.rs"));
    }

    #[test]
    fn timestamps_are_attached_to_first_user_or_agent_line_only() {
        let theme = *Theme::current();
        let mut user = DshRenderEntry::plain(
            DshRenderEntryId::Event { seq: 7 },
            7,
            DshRenderKind::User,
            "fallback",
        );
        user.created_at_ms = Some(1_787_500_000_000);
        let lines = materialized_entries(std::slice::from_ref(&user), 80, theme);
        let timestamp_line = lines
            .iter()
            .find(|line| line.timestamp.is_some())
            .expect("user timestamp");
        assert!(timestamp_line.copy_text.contains("fallback"));
        assert!(
            timestamp_line
                .timestamp
                .as_ref()
                .is_some_and(|timestamp| timestamp.hover.contains("|"))
        );
        assert!(lines.iter().filter(|line| line.timestamp.is_some()).count() == 1);

        let tool = DshRenderEntry::plain(
            DshRenderEntryId::Event { seq: 8 },
            8,
            DshRenderKind::ToolCall,
            "pwd",
        );
        let tool_lines = materialized_entries(&[tool], 80, theme);
        assert!(tool_lines.iter().all(|line| line.timestamp.is_none()));
    }

    #[test]
    fn grok_message_chrome_and_markdown_ast_are_visible() {
        let theme = *Theme::current();
        let assistant = TranscriptRow {
            id: DshRenderEntryId::Event { seq: 12 },
            created_at_ms: None,
            started_at_ms: None,
            finished_at_ms: None,
            label: "Assistant".into(),
            text: "# Title\n\n**bold** and `code`\n\n```rust\nlet x = 1;\n```".into(),
            kind: DshRenderKind::Assistant,
            visibility: DshRenderVisibility::Visible,
            finish: DshRenderFinish::Completed,
            group_key: None,
            selectable: true,
            source_seq: 12,
            seq: dsh_pager::DshSeq::new(12),
            content: DshRenderContent {
                blocks: vec![DshRenderBlock::Markdown {
                    text: "# Title\n\n**bold** and `code`\n\n```rust\nlet x = 1;\n```".into(),
                }],
                fallback: "# Title".into(),
            },
        };
        let rendered = materialized_row(assistant.clone(), 120, theme)
            .into_iter()
            .map(|line| line.line)
            .collect::<Vec<_>>();
        let text = rendered.iter().map(Line::to_string).collect::<Vec<_>>();
        assert_eq!(text[0].trim(), "Title");
        assert!(!text.iter().any(|line| line == "Assistant"));
        assert!(text.iter().any(|line| line.trim() == "Title"));
        assert!(!text.iter().any(|line| line.contains("# Title")));
        let inline_code = rendered
            .iter()
            .flat_map(|line| &line.spans)
            .find(|span| span.content.contains("code"))
            .expect("inline code span");
        assert_eq!(inline_code.style.fg, Some(theme.md_code));
        assert!(inline_code.style.add_modifier.contains(Modifier::BOLD));
        let fenced = rendered
            .iter()
            .find(|line| line.to_string().contains("let x = 1;"))
            .expect("fenced code line");
        assert_eq!(fenced.style.bg, Some(theme.md_code_bg));
        let mut list = Vec::new();
        render_markdown(&mut list, "- one\n- two", theme, "", 80);
        assert!(
            list.iter()
                .any(|line| line.to_string().starts_with("• one"))
        );

        let user = TranscriptRow {
            label: "You".into(),
            kind: DshRenderKind::User,
            text: "hello".into(),
            content: DshRenderContent::default(),
            ..assistant
        };
        let user_lines = materialized_row(user, 120, theme);
        assert!(
            user_lines
                .iter()
                .any(|line| line.line.to_string().trim() == "❯ hello")
        );
        assert!(
            !user_lines
                .iter()
                .any(|line| line.line.to_string().trim() == "You")
        );
    }

    #[test]
    fn streaming_assistant_components_fold_independently() {
        let entry = DshRenderEntry {
            id: DshRenderEntryId::Partial {
                turn: 4,
                step: 0,
                surface: 0,
            },
            source_seq: 6,
            created_at_ms: None,
            started_at_ms: None,
            finished_at_ms: None,
            kind: DshRenderKind::Assistant,
            text: "hidden thought\n▸ shell\nfinal answer".into(),
            partial: true,
            visibility: DshRenderVisibility::Visible,
            finish: DshRenderFinish::Running,
            // History replay may contain only the final assistant/message and
            // therefore omit the streaming group key; the multi-block shape
            // must still receive the same component projection.
            group_key: None,
            selectable: true,
            lineage: vec![1, 2, 3, 4, 5, 6],
            content: DshRenderContent {
                blocks: vec![
                    DshRenderBlock::Reasoning {
                        text: "hidden thought".into(),
                    },
                    DshRenderBlock::ToolCall {
                        name: "shell".into(),
                        call_id: Some("call-1".into()),
                        arguments: "{\"cmd\":\"pwd\"}".into(),
                        edit: None,
                        view: None,
                        result: None,
                    },
                    DshRenderBlock::Markdown {
                        text: "final answer".into(),
                    },
                ],
                fallback: "hidden thought\n▸ shell\nfinal answer".into(),
            },
        };
        let theme = *Theme::current();
        let initial = materialized_entries(std::slice::from_ref(&entry), 80, theme);
        assert!(initial.iter().any(|line| line.copy_text == "◆ Thinking…"));
        assert!(initial.iter().any(|line| line.copy_text == "◆ shell"));
        assert!(initial.iter().any(|line| line.copy_text == "final answer"));
        assert!(
            initial
                .iter()
                .any(|line| line.copy_text.contains("hidden thought"))
        );
        assert!(!initial.iter().any(|line| line.copy_text.contains("cmd")));
        let thinking_header = initial
            .iter()
            .find(|line| line.copy_text == "◆ Thinking…")
            .expect("running thinking header");
        let shell_header = initial
            .iter()
            .find(|line| line.copy_text == "◆ shell")
            .expect("collapsed generic tool header");
        assert!(thinking_header.rail);
        assert!(!shell_header.rail);
        let final_line = initial
            .iter()
            .find(|line| line.copy_text == "final answer")
            .expect("final Markdown line");
        assert_eq!(final_line.content_offset, 3);
        assert!(final_line.line.to_string().starts_with("   "));
        assert!(initial.iter().any(|line| !line.selectable
            && line.copy_text.is_empty()
            && line.line_index < final_line.line_index));

        let mut pane = ScrollbackPane::default();
        let mut scrollback = Scrollback::default();
        for (seq, chunk) in [
            (
                1,
                json!({
                    "turn": 4,
                    "step": 0,
                    "chunk": {"type": "block-start", "index": 0, "blockType": "reasoning"}
                }),
            ),
            (
                2,
                json!({
                    "turn": 4,
                    "step": 0,
                    "chunk": {"type": "reasoning-delta", "index": 0, "text": "hidden thought"}
                }),
            ),
            (
                3,
                json!({
                    "turn": 4,
                    "step": 0,
                    "chunk": {"type": "block-start", "index": 1, "blockType": "tool-call"}
                }),
            ),
            (
                4,
                json!({
                    "turn": 4,
                    "step": 0,
                    "chunk": {"type": "tool-call-delta", "index": 1, "name": "shell", "id": "call-1", "argumentsDelta": r#"{"cmd":"pwd"}"#}
                }),
            ),
            (
                5,
                json!({
                    "turn": 4,
                    "step": 0,
                    "chunk": {"type": "block-start", "index": 2, "blockType": "text"}
                }),
            ),
            (
                6,
                json!({
                    "turn": 4,
                    "step": 0,
                    "chunk": {"type": "text-delta", "index": 2, "text": "final answer"}
                }),
            ),
        ] {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "assistant/chunk".into(),
                    seq,
                    time: 1.0,
                    data: chunk,
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            });
        }
        let id = DshRenderEntryId::Partial {
            turn: 4,
            step: 0,
            surface: 0,
        };
        pane.sync(&mut scrollback, 80, theme);
        assert!(pane.toggle_fold_or_group_at(id, Some(0)));
        pane.sync(&mut scrollback, 80, theme);
        assert!(
            pane.visible_lines(&mut scrollback, 0, 20)
                .iter()
                .any(|line| line.copy_text.contains("hidden thought"))
        );
        assert!(pane.toggle_fold_or_group_at(id, Some(1)));
        pane.sync(&mut scrollback, 80, theme);
        assert!(
            pane.visible_lines(&mut scrollback, 0, 20)
                .iter()
                .any(|line| line.copy_text.contains("cmd"))
        );
    }

    #[test]
    fn completed_thinking_renders_grok_elapsed_header() {
        let start = 1_787_500_000_000.0;
        let finish = start + 10_000.0;
        let mut scrollback = Scrollback::default();
        for (seq, time, chunk) in [
            (
                1,
                start,
                json!({
                    "turn": 8,
                    "step": 0,
                    "chunk": {"type": "block-start", "index": 0, "blockType": "reasoning"}
                }),
            ),
            (
                2,
                start + 100.0,
                json!({
                    "turn": 8,
                    "step": 0,
                    "chunk": {"type": "reasoning-delta", "index": 0, "text": "deep thought"}
                }),
            ),
            (
                3,
                finish,
                json!({
                    "turn": 8,
                    "step": 0,
                    "chunk": {"type": "finish", "reason": "stop"}
                }),
            ),
        ] {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "assistant/chunk".into(),
                    seq,
                    time,
                    data: chunk,
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            });
        }
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, *Theme::current());
        let lines = pane.visible_lines(&mut scrollback, 0, 20);
        assert!(
            lines
                .iter()
                .any(|line| line.copy_text == "◆ Thought for 10.0s")
        );
        let header = lines
            .iter()
            .find(|line| line.copy_text == "◆ Thought for 10.0s")
            .expect("completed thinking header");
        assert_eq!(
            header.line.spans.get(1).and_then(|span| span.style.fg),
            Some(Theme::current().gray)
        );
        assert!(
            header
                .line
                .spans
                .iter()
                .skip(2)
                .all(|span| { span.style.fg == Some(Theme::current().gray) })
        );
    }

    #[test]
    fn rail_wave_uses_the_fixed_grok_phase_contract() {
        let top = wave_brightness(0, 0, DEFAULT_WAVE_ROWS, GROK_WAVE_SPEED);
        let quarter = wave_brightness(0, 8, DEFAULT_WAVE_ROWS, GROK_WAVE_SPEED);
        let next_tick = wave_brightness(1, 0, DEFAULT_WAVE_ROWS, GROK_WAVE_SPEED);

        assert!(top.abs() < f32::EPSILON);
        assert!((quarter - 1.0).abs() < 1e-6);
        assert!(next_tick > top);
    }

    #[test]
    fn running_rail_phase_uses_logical_line_rows() {
        let mut scrollback = Scrollback::default();
        for (seq, data) in [
            (
                1,
                json!({
                    "turn": 1,
                    "step": 0,
                    "chunk": {"type": "block-start", "index": 0, "blockType": "reasoning"}
                }),
            ),
            (
                2,
                json!({
                    "turn": 1,
                    "step": 0,
                    "chunk": {
                        "type": "reasoning-delta",
                        "index": 0,
                        "text": "first line\nsecond line\nthird line"
                    }
                }),
            ),
        ] {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "assistant/chunk".into(),
                    seq,
                    time: 1_787_500_000_000.0 + seq as f64,
                    data,
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            });
        }

        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, *Theme::current());
        let animated = pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .filter(|line| line.accent.is_some_and(|accent| accent.animated))
            .collect::<Vec<_>>();
        assert!(
            animated.len() >= 3,
            "header, header gap, and reasoning body should animate; got {animated:#?}"
        );
        assert!(
            animated
                .iter()
                .all(|line| line.accent_wave_row == line.line_index.min(u16::MAX as usize) as u16),
            "wave phase must follow the stable logical row, not rail length"
        );
    }

    #[test]
    fn running_tool_rail_uses_traveling_wave_colors() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&HistoryEntry {
            event: SessionEvent {
                event_type: "tool/call".into(),
                seq: 40,
                time: 1_787_500_000_000.0,
                data: json!({
                    "name": "bash",
                    "callId": "wave-call",
                    "arguments": "{}"
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: Some(json!({
                "for": "call",
                "view": {"card": "terminal", "title": "cargo test", "description": "run tests"}
            })),
        });
        let theme = *Theme::current();
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, theme);
        pane.set_wave_tick(0);
        let first = pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .find(|line| line.accent.is_some_and(|accent| accent.animated))
            .expect("running rail");
        pane.set_wave_tick(9);
        let second = pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .find(|line| line.accent.is_some_and(|accent| accent.animated))
            .expect("running rail after elapsed time");
        assert_eq!(first.accent_wave_row, 0);
        assert_eq!(
            first.accent,
            Some(AccentStyle::animated(theme.accent_running))
        );
        assert_ne!(
            first.line.spans.first().and_then(|span| span.style.fg),
            second.line.spans.first().and_then(|span| span.style.fg),
            "running rail should be recolored by monotonic elapsed time"
        );
        assert_ne!(
            first.line.spans.first().and_then(|span| span.style.fg),
            Some(theme.accent_running),
            "the traveling wave starts between background and the configured running accent"
        );
        assert_eq!(
            first
                .line
                .spans
                .iter()
                .skip(2)
                .map(|span| span.style.fg)
                .collect::<Vec<_>>(),
            second
                .line
                .spans
                .iter()
                .skip(2)
                .map(|span| span.style.fg)
                .collect::<Vec<_>>(),
            "running text after the synchronized bullet must not inherit the wave phase"
        );
        assert_ne!(first.line.spans[1].style.fg, second.line.spans[1].style.fg);
        pane.set_selected_target(Some(HitTarget::TranscriptEntry(DshRenderEntryId::Event {
            seq: 40,
        })));
        let selected = pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .find(|line| line.accent.is_some_and(|accent| accent.animated) && line.header)
            .expect("selected running header");
        assert_eq!(
            selected.line.spans.first().and_then(|span| span.style.fg),
            Some(theme.accent_running)
        );

        let mut failed = selected.clone();
        failed.accent = Some(AccentStyle::static_color(theme.accent_error));
        failed.accent_flash = false;
        EntryRenderer::paint_dynamic(
            &mut failed.line,
            DynamicAccentSpec {
                tick: 0,
                logical_row: failed.accent_wave_row,
                wave_rows: DEFAULT_WAVE_ROWS,
                wave_speed: GROK_WAVE_SPEED,
                background: theme.bg_base,
                dim_accent: 0.5,
                accent: failed.accent,
                flash_accent: failed.flash_accent,
                bullet: None,
                bullet_span: None,
                selected: true,
                flash: false,
                pending_user_input: false,
            },
        );
        assert_eq!(failed.line.spans[0].style.fg, Some(theme.accent_error));
    }

    #[test]
    fn finish_flash_is_bounded_to_grok_window() {
        assert!(finish_flash_active(Some(1_000), Some(1_399)));
        assert!(!finish_flash_active(Some(1_000), Some(1_400)));
        assert!(!finish_flash_active(Some(1_000), Some(900)));
    }

    #[test]
    fn semantic_materializer_uses_structured_blocks_and_stable_identity() {
        let row = row();
        let entry = DshRenderEntry {
            id: row.id,
            source_seq: row.source_seq,
            created_at_ms: row.created_at_ms,
            started_at_ms: None,
            finished_at_ms: None,
            kind: row.kind,
            text: row.text,
            partial: false,
            visibility: row.visibility,
            finish: row.finish,
            group_key: row.group_key.clone(),
            selectable: row.selectable,
            lineage: Vec::new(),
            content: row.content,
        };
        let lines = materialized_entries(&[entry], 80, *Theme::current());
        let rendered = lines
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>();
        assert!(rendered.iter().any(|line| line.contains("src/lib.rs")));
        assert!(rendered.iter().any(|line| line.contains("- old")));
        assert!(rendered.iter().any(|line| line.contains("+ new")));
        assert!(lines.iter().all(|line| line.entry_id == row.id));
        assert_eq!(
            lines.iter().map(|line| line.line_index).collect::<Vec<_>>(),
            (0..lines.len()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn wrapped_markdown_does_not_inject_periodic_blank_rows() {
        let id = DshRenderEntryId::Event { seq: 31 };
        let text = "dsh-pager-grok-ui keeps one continuous paragraph while ordinary terminal wrapping produces more than four visible rows";
        let entry = DshRenderEntry {
            id,
            source_seq: 31,
            created_at_ms: None,
            started_at_ms: None,
            finished_at_ms: None,
            kind: DshRenderKind::Assistant,
            text: text.into(),
            partial: false,
            visibility: DshRenderVisibility::Visible,
            finish: DshRenderFinish::Completed,
            group_key: None,
            selectable: true,
            lineage: Vec::new(),
            content: DshRenderContent {
                blocks: vec![DshRenderBlock::Markdown { text: text.into() }],
                fallback: text.into(),
            },
        };
        let lines = materialized_entries(&[entry], 24, *Theme::current());

        assert!(
            lines.len() > 4,
            "fixture must exercise the old four-row cadence"
        );
        assert!(
            lines.iter().all(|line| line.selectable),
            "a single paragraph must not gain synthetic blank rows"
        );
        assert!(lines.iter().all(|line| !line.copy_text.is_empty()));
        assert_eq!(
            lines.iter().map(|line| line.line_index).collect::<Vec<_>>(),
            (0..lines.len()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn semantic_materializer_wraps_unicode_without_losing_lines() {
        let mut row = row();
        row.content = DshRenderContent {
            blocks: vec![DshRenderBlock::Markdown {
                text: "中文abcdef".into(),
            }],
            fallback: "中文abcdef".into(),
        };
        let lines = materialized_entries(
            &[DshRenderEntry {
                id: row.id,
                source_seq: row.source_seq,
                created_at_ms: row.created_at_ms,
                started_at_ms: None,
                finished_at_ms: None,
                kind: row.kind,
                text: row.text,
                partial: false,
                visibility: row.visibility,
                finish: row.finish,
                group_key: row.group_key.clone(),
                selectable: row.selectable,
                lineage: Vec::new(),
                content: row.content,
            }],
            4,
            *Theme::current(),
        );
        assert!(lines.len() > 1);
        let visible_text = lines
            .iter()
            .flat_map(|line| line.line.to_string().chars().collect::<Vec<_>>())
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        assert_eq!(visible_text, "中文abcdef");
    }

    #[test]
    fn default_projection_hides_instructions_and_summarizes_context() {
        let entries = vec![
            DshRenderEntry {
                id: DshRenderEntryId::Event { seq: 1 },
                source_seq: 1,
                created_at_ms: None,
                started_at_ms: None,
                finished_at_ms: None,
                kind: DshRenderKind::SystemInstruction,
                text: "secret system prompt".into(),
                partial: false,
                visibility: DshRenderVisibility::Hidden,
                finish: DshRenderFinish::Completed,
                group_key: Some("system-instructions".into()),
                selectable: false,
                lineage: vec![1],
                content: DshRenderContent {
                    blocks: vec![DshRenderBlock::Markdown {
                        text: "secret system prompt".into(),
                    }],
                    fallback: "secret system prompt".into(),
                },
            },
            DshRenderEntry {
                id: DshRenderEntryId::Event { seq: 2 },
                source_seq: 2,
                created_at_ms: None,
                started_at_ms: None,
                finished_at_ms: None,
                kind: DshRenderKind::AgentContext,
                text: "repository files and hidden instructions".into(),
                partial: false,
                visibility: DshRenderVisibility::Collapsed,
                finish: DshRenderFinish::Completed,
                group_key: Some("agent-context:repo".into()),
                selectable: false,
                lineage: vec![2],
                content: DshRenderContent::default(),
            },
        ];
        let lines = materialized_entries(&entries, 80, *Theme::current());
        let rendered = lines
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!rendered.contains("secret system prompt"));
        assert!(rendered.contains("repository files"));
        assert_eq!(
            lines
                .iter()
                .map(|line| line.entry_id)
                .collect::<HashSet<_>>()
                .len(),
            1
        );
    }

    #[test]
    fn rich_renderer_covers_all_block_fallbacks() {
        let row = TranscriptRow {
            id: DshRenderEntryId::Event { seq: 9 },
            created_at_ms: None,
            started_at_ms: None,
            finished_at_ms: None,
            label: "Assistant".into(),
            text: "fallback".into(),
            kind: DshRenderKind::Assistant,
            visibility: DshRenderVisibility::Visible,
            finish: DshRenderFinish::Completed,
            group_key: None,
            selectable: true,
            source_seq: 9,
            seq: dsh_pager::DshSeq::new(9),
            content: DshRenderContent {
                blocks: vec![
                    DshRenderBlock::Markdown {
                        text: "markdown".into(),
                    },
                    DshRenderBlock::Reasoning {
                        text: "thinking".into(),
                    },
                    DshRenderBlock::ToolCall {
                        name: "shell".into(),
                        call_id: None,
                        arguments: "{}".into(),
                        edit: None,
                        view: None,
                        result: None,
                    },
                    DshRenderBlock::ToolResult {
                        call_id: None,
                        blocks: vec![DshRenderBlock::Plain {
                            text: "output".into(),
                        }],
                        is_error: true,
                    },
                    DshRenderBlock::Image {
                        attachment_id: None,
                        media_type: Some("image/png".into()),
                        name: None,
                        raw: "{}".into(),
                    },
                    DshRenderBlock::Unknown {
                        kind: "future".into(),
                        raw: "raw payload".into(),
                    },
                ],
                fallback: "fallback".into(),
            },
        };
        let rendered = materialized_row_with_expanded_blocks(row, 120, *Theme::current())
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "markdown",
            "thinking",
            "◆ shell",
            "✗ result",
            "output",
            "[unsupported image block]",
            "[unsupported block: future]",
            "raw payload",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected} in {rendered}"
            );
        }
    }

    #[test]
    fn harness_tool_views_render_grok_terminal_diff_read_search_and_web_cards() {
        let theme = *Theme::current();
        let render = |block: DshRenderBlock| {
            let row = TranscriptRow {
                id: DshRenderEntryId::Event { seq: 90 },
                created_at_ms: None,
                started_at_ms: None,
                finished_at_ms: None,
                label: "Tool".into(),
                text: block.display_text(),
                kind: DshRenderKind::ToolCall,
                visibility: DshRenderVisibility::Visible,
                finish: DshRenderFinish::Completed,
                group_key: Some("tool:test".into()),
                selectable: true,
                source_seq: 90,
                seq: dsh_pager::DshSeq::new(90),
                content: DshRenderContent {
                    fallback: block.display_text(),
                    blocks: vec![block],
                },
            };
            materialized_row_with_expanded_blocks(row, 120, theme)
                .into_iter()
                .map(|line| line.line)
                .collect::<Vec<_>>()
        };

        let terminal = render(DshRenderBlock::ToolCall {
            name: "bash".into(),
            call_id: Some("terminal".into()),
            arguments: "{\"cmd\":\"pwd\"}".into(),
            edit: None,
            view: Some(DshToolCallView::Terminal {
                title: "pwd".into(),
                description: Some("show workspace".into()),
                cwd: Some("/work".into()),
            }),
            result: Some(Box::new(DshToolResult {
                view: Some(DshToolResultView::Terminal {
                    title: None,
                    output: Some("/work\n".into()),
                    exit_code: Some(0),
                    signal: None,
                }),
                blocks: Vec::new(),
                is_error: false,
            })),
        });
        let terminal_text = terminal
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(terminal_text.contains("◆ Run show workspace"));
        assert!(!terminal_text.contains('⌄'));
        let terminal_header = terminal
            .iter()
            .find(|line| line.to_string().contains("Run show workspace"))
            .expect("expanded terminal header");
        assert!(
            terminal_header
                .to_string()
                .ends_with("◆ Run show workspace")
        );
        assert!(
            terminal_header
                .spans
                .iter()
                .any(|span| span.content == format!("{} ", glyphs::diamond_filled()))
        );
        assert!(terminal_text.contains("$ pwd"));
        assert!(terminal_text.contains("/work"));
        assert!(!terminal_text.contains("exit 0"));
        assert!(
            terminal
                .iter()
                .flat_map(|line| &line.spans)
                .any(|span| span.style.bg == Some(theme.bg_terminal))
        );

        let diff = render(DshRenderBlock::ToolCall {
            name: "edit".into(),
            call_id: Some("diff".into()),
            arguments: "{}".into(),
            edit: None,
            view: Some(DshToolCallView::Diff {
                title: "Edit src/lib.rs".into(),
                diffs: vec![DshToolDiff {
                    path: "src/lib.rs".into(),
                    old_text: Some("old".into()),
                    new_text: "new".into(),
                }],
                locations: Vec::new(),
            }),
            result: None,
        });
        let diff_text = diff
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(diff_text.contains("Edit src/lib.rs +1/-1"));
        assert!(diff_text.contains("- old"));
        assert!(diff_text.contains("+ new"));

        let read = render(DshRenderBlock::ToolCall {
            name: "read".into(),
            call_id: Some("read".into()),
            arguments: "{}".into(),
            edit: None,
            view: Some(DshToolCallView::Generic {
                title: "Read src/lib.rs".into(),
                kind: DshToolKind::Read,
                raw_input: None,
                content: Vec::new(),
                locations: Vec::new(),
            }),
            result: Some(Box::new(DshToolResult {
                view: Some(DshToolResultView::Read {
                    title: None,
                    path: "src/lib.rs".into(),
                    offset: 7,
                    lines: vec![dsh_pager::DshReadLine {
                        number: 7,
                        text: "fn main() {}".into(),
                    }],
                    total_lines: 42,
                    lang: Some("rs".into()),
                    content: Vec::new(),
                }),
                blocks: Vec::new(),
                is_error: false,
            })),
        });
        let read_text = read
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(read_text.contains("7  fn main() {}"));
        assert!(read_text.contains("(7-7 of 42)"));

        let search = render(DshRenderBlock::ToolCall {
            name: "grep".into(),
            call_id: Some("search".into()),
            arguments: "{}".into(),
            edit: None,
            view: Some(DshToolCallView::Generic {
                title: "Search TODO".into(),
                kind: DshToolKind::Search,
                raw_input: None,
                content: Vec::new(),
                locations: Vec::new(),
            }),
            result: Some(Box::new(DshToolResult {
                view: Some(DshToolResultView::SearchMatches {
                    title: None,
                    files: vec![dsh_pager::DshSearchFile {
                        path: "src/lib.rs".into(),
                        matches: vec![dsh_pager::DshSearchMatch {
                            line_number: 9,
                            line: "// TODO".into(),
                        }],
                    }],
                    truncated: true,
                    total: 3,
                }),
                blocks: Vec::new(),
                is_error: false,
            })),
        });
        let search_text = search
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(search_text.contains("src/lib.rs"));
        assert!(search_text.contains("9  // TODO"));
        assert!(search_text.contains("3 total"));

        let web = render(DshRenderBlock::ToolCall {
            name: "web_search".into(),
            call_id: Some("web".into()),
            arguments: "{}".into(),
            edit: None,
            view: Some(DshToolCallView::Generic {
                title: "Search the web".into(),
                kind: DshToolKind::Search,
                raw_input: None,
                content: Vec::new(),
                locations: Vec::new(),
            }),
            result: Some(Box::new(DshToolResult {
                view: Some(DshToolResultView::WebSearch {
                    title: None,
                    sources: vec![dsh_pager::DshWebSource {
                        url: "https://example.com".into(),
                        title: Some("Example".into()),
                        snippet: None,
                        published_at: None,
                    }],
                    answer: Some("answer".into()),
                    truncated: false,
                }),
                blocks: Vec::new(),
                is_error: false,
            })),
        });
        let web_text = web
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(web_text.contains("answer"));
        assert!(web_text.contains("Example"));
        assert!(web_text.contains("https://example.com"));
    }

    #[test]
    fn markdown_and_diff_use_dedicated_theme_roles() {
        let theme = *Theme::current();
        let mut lines = Vec::new();
        render_markdown(
            &mut lines,
            "# heading\n```rust\nlet x = 1;\n```\n[Example](https://example.com)",
            theme,
            "",
            80,
        );
        assert_eq!(lines[0].spans[0].style.fg, Some(theme.md_heading_h1));
        let code = lines
            .iter()
            .find(|line| line.to_string().contains("let x = 1;"))
            .expect("fenced code line");
        assert_eq!(code.style.bg, Some(theme.md_code_bg));
        let link = lines
            .iter()
            .flat_map(|line| &line.spans)
            .find(|span| span.content.contains("Example"))
            .expect("link span");
        assert_eq!(link.style.fg, Some(theme.link_fg));

        let mut diff = Vec::new();
        render_diff(&mut diff, None, "old", "new", theme, 0);
        assert_eq!(diff[0].spans[0].style.bg, Some(theme.diff_delete_bg));
        assert_eq!(diff[1].spans[0].style.bg, Some(theme.diff_insert_bg));
    }

    #[test]
    fn scrollback_pane_uses_dsh_identity_and_semantic_height() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&HistoryEntry {
            event: SessionEvent {
                event_type: "assistant/message".into(),
                seq: 4,
                time: 1.0,
                data: json!({
                    "turn": 0,
                    "step": 0,
                    "message": {
                        "content": [{ "type": "text", "text": "a semantic block" }]
                    }
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        });
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, *Theme::current());
        let lines = pane.visible_lines(&mut scrollback, 0, 20);
        assert!(
            lines
                .iter()
                .any(|line| line.line.to_string().contains("semantic block"))
        );
        let anchor = pane.anchor_at(&mut scrollback, 1).expect("pane anchor");
        assert_eq!(anchor.entry_id, DshRenderEntryId::Event { seq: 4 });
        assert_eq!(pane.scroll_for_anchor(&mut scrollback, anchor), Some(1));
    }

    #[test]
    fn user_prompt_and_agent_message_use_distinct_background_contracts() {
        let theme = *Theme::current();
        let entries = vec![
            DshRenderEntry {
                id: DshRenderEntryId::Event { seq: 10 },
                source_seq: 10,
                created_at_ms: None,
                started_at_ms: None,
                finished_at_ms: None,
                kind: DshRenderKind::User,
                text: "one\ntwo\nthree\nfour".into(),
                partial: false,
                visibility: DshRenderVisibility::Visible,
                finish: DshRenderFinish::Completed,
                group_key: None,
                selectable: true,
                lineage: vec![10],
                content: DshRenderContent::default(),
            },
            DshRenderEntry {
                id: DshRenderEntryId::Event { seq: 11 },
                source_seq: 11,
                created_at_ms: None,
                started_at_ms: None,
                finished_at_ms: None,
                kind: DshRenderKind::Assistant,
                text: "agent body".into(),
                partial: false,
                visibility: DshRenderVisibility::Visible,
                finish: DshRenderFinish::Completed,
                group_key: None,
                selectable: true,
                lineage: vec![11],
                content: DshRenderContent::default(),
            },
        ];
        let lines = materialized_entries(&entries, 80, theme);
        let user_lines = lines
            .iter()
            .filter(|line| line.entry_id == DshRenderEntryId::Event { seq: 10 })
            .collect::<Vec<_>>();
        let agent_lines = lines
            .iter()
            .filter(|line| line.entry_id == DshRenderEntryId::Event { seq: 11 })
            .collect::<Vec<_>>();
        assert!(
            user_lines
                .iter()
                .all(|line| line.background == Some(theme.bg_light))
        );
        assert!(agent_lines.iter().all(|line| line.background.is_none()));
        assert_eq!(
            user_lines.len(),
            5,
            "top vpad, three-line prompt preview, bottom vpad"
        );
        assert_eq!(user_lines[1].line.to_string().trim_end(), "   ❯ one");
        assert_eq!(user_lines[2].line.to_string().trim_end(), "     two");
        assert!(!user_lines.iter().any(|line| line.line.to_string() == "You"));
        assert!(
            !agent_lines
                .iter()
                .any(|line| line.line.to_string() == "Assistant")
        );
        assert!(user_lines.iter().filter(|line| !line.selectable).count() >= 2);
    }

    #[test]
    fn semantic_tool_group_has_zero_height_members_and_expands_from_header() {
        let mut scrollback = Scrollback::default();
        for (seq, name, kind, title) in [
            (20, "read", "read", "Read src/a.rs"),
            (21, "grep", "search", "Search TODO"),
        ] {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "tool/call".into(),
                    seq,
                    time: 1.0,
                    data: json!({
                        "name": name,
                        "callId": format!("call-{seq}"),
                        "arguments": "{}"
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: Some(json!({
                    "for": "call",
                    "view": { "card": "generic", "title": title, "kind": kind }
                })),
            });
        }
        let theme = *Theme::current();
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, theme);
        let collapsed = pane.visible_lines(&mut scrollback, 0, 20);
        assert!(collapsed.iter().any(|line| line.group_header
            && line.copy_text.contains("Reading 1 file")
            && line.copy_text.contains("Searching 1 pattern")));
        let collapsed_header = collapsed
            .iter()
            .find(|line| line.group_header)
            .expect("collapsed group header");
        assert!(collapsed_header.line.spans[0].content.starts_with('┃'));
        assert!(
            collapsed_header
                .accent
                .is_some_and(|accent| accent.animated && accent.color == theme.accent_tool)
        );
        assert!(
            !collapsed
                .iter()
                .any(|line| line.copy_text.contains("Search \"TODO\""))
        );
        assert_eq!(scrollback.layout(80).entries[1].height, 0);

        assert!(pane.toggle_fold_or_group_at(DshRenderEntryId::Event { seq: 20 }, Some(0)));
        pane.sync(&mut scrollback, 80, theme);
        let expanded = pane.visible_lines(&mut scrollback, 0, 20);
        assert!(
            expanded
                .iter()
                .any(|line| line.group_header && line.copy_text.contains("Reading 1 file"))
        );
        assert!(
            expanded
                .iter()
                .any(|line| line.copy_text.contains("Search \"TODO\""))
        );
        let group_header = expanded
            .iter()
            .find(|line| line.group_header)
            .expect("expanded group header");
        let read_member = expanded
            .iter()
            .find(|line| line.copy_text.contains("Read src/a.rs"))
            .expect("expanded read member");
        let search_member = expanded
            .iter()
            .find(|line| line.copy_text.contains("Search \"TODO\""))
            .expect("expanded search member");
        assert!(group_header.rail);
        assert!(group_header.line.spans[0].content.starts_with('┃'));
        assert!(!read_member.rail);
        assert!(!search_member.rail);
        assert!(scrollback.layout(80).entries[1].height > 0);
    }

    #[test]
    fn execute_and_edit_cards_break_non_destructive_verb_runs() {
        let mut scrollback = Scrollback::default();
        for (seq, name, view) in [
            (
                40,
                "read",
                json!({ "card": "generic", "title": "Read a.rs", "kind": "read" }),
            ),
            (
                41,
                "bash",
                json!({ "card": "terminal", "title": "cargo test", "description": "run tests" }),
            ),
            (
                42,
                "grep",
                json!({ "card": "generic", "title": "Search TODO", "kind": "search" }),
            ),
        ] {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "tool/call".into(),
                    seq,
                    time: 1.0,
                    data: json!({
                        "name": name,
                        "callId": format!("call-{seq}"),
                        "arguments": "{}"
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: Some(json!({ "for": "call", "view": view })),
            });
        }
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, *Theme::current());
        let lines = pane.visible_lines(&mut scrollback, 0, 20);
        assert_eq!(lines.iter().filter(|line| line.group_header).count(), 2);
        assert!(
            lines
                .iter()
                .any(|line| line.copy_text.contains("Reading 1 file"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.copy_text.contains("Run tests"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.copy_text.contains("Searching 1 pattern"))
        );
        assert!(
            scrollback
                .layout(80)
                .entries
                .iter()
                .all(|entry| entry.height > 0)
        );
    }

    #[test]
    fn generic_execute_uses_description_until_the_card_is_expanded() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&HistoryEntry {
            event: SessionEvent {
                event_type: "tool/call".into(),
                seq: 50,
                time: 1.0,
                data: json!({
                    "name": "bash",
                    "callId": "call-50",
                    "arguments": "{\"command\":\"node worker.js\"}"
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: Some(json!({
                "for": "call",
                "view": {
                    "card": "generic",
                    "title": "node worker.js",
                    "kind": "execute",
                    "rawInput": "node worker.js",
                    "content": [{
                        "type": "text",
                        "text": "Run retry jobs and inspect recent worker logs"
                    }]
                }
            })),
        });

        let theme = *Theme::current();
        let id = DshRenderEntryId::Event { seq: 50 };
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, theme);
        let collapsed = pane.visible_lines(&mut scrollback, 0, 20);
        let summary = collapsed
            .iter()
            .find(|line| line.entry_id == id && line.selectable)
            .expect("collapsed execute summary");
        assert_eq!(
            summary.copy_text,
            "◆ Run retry jobs and inspect recent worker logs"
        );
        assert!(!summary.copy_text.contains("node worker.js"));
        assert!(summary.line.to_string().starts_with("┃  ◆ Run "));
        assert!(summary.accent.is_some_and(|accent| accent.animated));
        assert!(
            summary
                .line
                .spans
                .iter()
                .any(|span| { span.content.contains('◆') && span.style.fg.is_some() })
        );
        assert!(summary.line.spans.iter().any(|span| {
            span.content == "Run " && span.style.add_modifier.contains(Modifier::BOLD)
        }));
        assert!(summary.line.spans.iter().any(|span| {
            span.content == "retry jobs and inspect recent worker logs"
                && !span.style.add_modifier.contains(Modifier::BOLD)
        }));

        assert!(pane.toggle_fold_or_group_at(id, Some(0)));
        pane.sync(&mut scrollback, 80, theme);
        let expanded = pane.visible_lines(&mut scrollback, 0, 20);
        let expanded_text = expanded
            .iter()
            .map(|line| line.copy_text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(expanded_text.contains("◆ Run retry jobs and inspect recent worker logs"));
        assert!(!expanded_text.contains('⌄'));
        assert!(expanded_text.contains("$ node worker.js"));
        assert_eq!(
            expanded_text
                .matches("retry jobs and inspect recent worker logs")
                .count(),
            1,
            "the generic call description is promoted into the header, not repeated"
        );
    }

    #[test]
    fn bash_and_single_read_restore_dsh_argument_summaries_without_views() {
        let mut scrollback = Scrollback::default();
        for (seq, name, arguments) in [
            (
                60,
                "bash",
                r#"{"command":"cargo test","description":"Verify the workspace"}"#,
            ),
            (61, "read", r#"{"file_path":"src/lib.rs"}"#),
        ] {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "tool/call".into(),
                    seq,
                    time: 1.0,
                    data: json!({
                        "name": name,
                        "callId": format!("call-{seq}"),
                        "arguments": arguments
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            });
        }

        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, *Theme::current());
        let lines = pane.visible_lines(&mut scrollback, 0, 20);
        assert!(
            lines
                .iter()
                .any(|line| line.copy_text.contains("Run Verify the workspace"))
        );
        assert!(
            lines
                .iter()
                .any(|line| { line.group_header && line.copy_text.contains("Reading 1 file") })
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.copy_text.contains("cargo test"))
        );
    }

    #[test]
    fn long_user_prompt_toggles_between_three_line_preview_and_full_body() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&HistoryEntry {
            event: SessionEvent {
                event_type: "user/message".into(),
                seq: 30,
                time: 1.0,
                data: json!({
                    "source": { "kind": "user" },
                        "content": [{ "type": "text", "text": "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five" }]
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        });
        let theme = *Theme::current();
        let id = DshRenderEntryId::Event { seq: 30 };
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, theme);
        assert_eq!(pane.visible_lines(&mut scrollback, 0, 20).len(), 5);
        assert!(pane.toggle_fold_or_group(id));
        pane.sync(&mut scrollback, 80, theme);
        let expanded = pane.visible_lines(&mut scrollback, 0, 20);
        assert!(expanded.iter().any(|line| line.copy_text.contains("five")));
        assert!(
            expanded
                .iter()
                .all(|line| line.background == Some(theme.bg_light))
        );
    }

    #[test]
    fn fifty_thousand_entry_hot_frames_are_viewport_bounded() {
        let mut scrollback = Scrollback::default();
        for seq in 0..49_999 {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "user/message".into(),
                    seq,
                    time: 1.0,
                    data: json!({
                        "source": { "kind": "user" },
                        "content": [{ "type": "text", "text": format!("entry {seq}") }]
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            });
        }
        scrollback.apply_event(&HistoryEntry {
            event: SessionEvent {
                event_type: "assistant/chunk".into(),
                seq: 49_999,
                time: 1.0,
                data: json!({
                    "turn": 1,
                    "step": 0,
                    "chunk": {
                        "type": "text-delta",
                        "index": 0,
                        "text": "streaming tail"
                    }
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        });
        assert_eq!(scrollback.entries().len(), 50_000);

        let theme = *Theme::current();
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, theme);
        let cold = pane.stats();
        assert_eq!(cold.revision_syncs, 1);
        assert_eq!(cold.scanned_entries, 50_000);
        assert_eq!(cold.materialized_entries, 0);

        let viewport = 24;
        let first = pane.visible_lines(&mut scrollback, 0, viewport);
        assert!(!first.is_empty());
        assert!(pane.cached_entry_count() < 100);
        assert!(pane.stats().materialized_entries < 100);

        pane.reset_stats();
        for tick in 1..=8 {
            pane.set_wave_tick(tick);
            pane.sync(&mut scrollback, 80, theme);
            let lines = pane.visible_lines(&mut scrollback, 0, viewport);
            assert!(lines.len() <= viewport as usize);
        }
        let hot = pane.stats();
        assert_eq!(hot.revision_syncs, 0);
        assert_eq!(hot.scanned_entries, 0);
        assert_eq!(hot.materialized_entries, 0);
        assert!(hot.painted_lines <= viewport as usize * 8);
        assert!(pane.cached_entry_count() < 100);

        pane.reset_stats();
        for update in 1..=8 {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "assistant/chunk".into(),
                    seq: 49_999 + update,
                    time: 1.0,
                    data: json!({
                        "turn": 1,
                        "step": 0,
                        "chunk": {
                            "type": "text-delta",
                            "index": 0,
                            "text": format!(" revision {update}")
                        }
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            });
            pane.sync(&mut scrollback, 80, theme);
            let lines = pane.visible_lines(&mut scrollback, 0, viewport);
            assert!(lines.len() <= viewport as usize);
        }
        let streaming = pane.stats();
        assert_eq!(streaming.revision_syncs, 8);
        assert_eq!(streaming.scanned_entries, 8);
        assert_eq!(streaming.materialized_entries, 0);
        assert!(streaming.painted_lines <= viewport as usize * 8);
        assert!(pane.cached_entry_count() < 100);
    }

    #[test]
    fn incremental_sync_falls_back_when_group_classification_can_change() {
        let mut scrollback = Scrollback::default();
        for seq in 0..2 {
            scrollback.apply_event(&HistoryEntry {
                event: SessionEvent {
                    event_type: "user/message".into(),
                    seq,
                    time: 1.0,
                    data: json!({
                        "source": { "kind": "user" },
                        "content": [{ "type": "text", "text": format!("entry {seq}") }]
                    }),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                view: None,
            });
        }

        let theme = *Theme::current();
        let mut pane = ScrollbackPane::default();
        pane.sync(&mut scrollback, 80, theme);
        pane.reset_stats();

        scrollback.apply_event(&HistoryEntry {
            event: SessionEvent {
                event_type: "user/message".into(),
                seq: 1,
                time: 1.0,
                data: json!({
                    "source": { "kind": "system" },
                    "content": [{ "type": "text", "text": "hidden context" }]
                }),
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        });
        pane.sync(&mut scrollback, 80, theme);

        assert_eq!(pane.stats().revision_syncs, 1);
        assert_eq!(pane.stats().scanned_entries, 2);
        assert!(!pane.is_empty());
    }
}
