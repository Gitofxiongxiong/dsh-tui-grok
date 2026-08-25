//! Grok-derived transcript block projection.
//!
//! The host adapter keeps typed DSH blocks intact. This module owns the
//! user-visible role, indentation and copy projection so the runtime never
//! needs to inspect protocol JSON or flatten a tool result itself.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone};
use dsh_pager::scrollback::{Scrollback, compute_paint_window};
use dsh_pager::{
    DshRenderBlock, DshRenderContent, DshRenderEntry, DshRenderEntryId, DshRenderFinish,
    DshRenderKind, DshRenderVisibility, DshToolCallView, DshToolDiff, DshToolKind, DshToolResult,
    DshToolResultView, ScrollAnchor,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::views::execute_tool::{
    DisplayMode as ExecuteDisplayMode, ExecuteBlockContext, ExecuteBlockLine,
};
use crate::views::execute_tool_adapter::project_execute_tool;
use crate::{
    geometry::HitTarget, glyphs, host_adapter::TranscriptRow,
    render::wrapping::word_wrap_line, theme::Theme,
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
    /// Whether the left rail is in Grok's running wave state.
    pub rail_animated: bool,
    /// Whether the left rail is in Grok's short post-finish accent flash.
    pub rail_flash: bool,
    /// Base accent used by the running wave and finish flash.
    pub rail_accent: Option<Color>,
    /// Logical row phase inside the entry's rail.
    pub rail_wave_row: u16,
    /// Number of terminal rows in this contiguous animated rail segment.
    pub rail_wave_len: u16,
    pub selectable: bool,
    pub background: Option<Color>,
    pub copy_text: String,
    /// Presentation columns painted before `copy_text`.
    ///
    /// Grok reserves one accent column and two left-pad columns for every
    /// entry, including plain Markdown. Keeping the offset in the paint line
    /// keeps hit-testing aligned with the visible text.
    pub content_offset: u16,
    /// Timestamp painted as a non-selectable right-side overlay. The
    /// transcript copy/selection geometry intentionally keeps it separate.
    pub timestamp: Option<TimestampLabel>,
    pub screen_y: u16,
    pub line: Line<'static>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampLabel {
    pub short: String,
    pub hover: String,
}

pub fn timestamp_label(created_at_ms: Option<u64>) -> Option<TimestampLabel> {
    let millis = i64::try_from(created_at_ms?).ok()?;
    let local = Local.timestamp_millis_opt(millis).single()?;
    Some(TimestampLabel {
        short: local.format("%-I:%M %p").to_string(),
        hover: local.format("%H:%M:%S | %b %d").to_string(),
    })
}

#[derive(Debug, Clone)]
struct RichEntry {
    id: DshRenderEntryId,
    lines: Vec<RichPaintLine>,
    start_y: usize,
}

/// Width-specific transcript projection used by the production AgentView.
/// Scrollback remains the host-owned source of stable entries; this layer
/// only supplies Grok semantic block lines and matching viewport geometry.
#[derive(Debug, Clone)]
pub struct RichTranscript {
    width: usize,
    entries: Vec<RichEntry>,
    total_height: usize,
}

#[derive(Debug, Clone)]
struct CachedPaneEntry {
    entry: DshRenderEntry,
    projection: ProjectionInfo,
    lines: Vec<RichPaintLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayMode {
    Collapsed,
    Truncated,
    Expanded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    VerbRun,
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolVerb {
    Read,
    Search,
    WebFetch,
    WebSearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct VerbCounts {
    read: usize,
    search: usize,
    web_fetch: usize,
    web_search: usize,
}

impl VerbCounts {
    fn add(&mut self, verb: ToolVerb) {
        match verb {
            ToolVerb::Read => self.read += 1,
            ToolVerb::Search => self.search += 1,
            ToolVerb::WebFetch => self.web_fetch += 1,
            ToolVerb::WebSearch => self.web_search += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectionInfo {
    mode: DisplayMode,
    group_anchor: Option<DshRenderEntryId>,
    group_header: bool,
    group_hidden: bool,
    group_expanded: bool,
    group_last_visible: bool,
    group_kind: Option<GroupKind>,
    group_count: usize,
    verb_counts: VerbCounts,
    group_running: bool,
    group_failed: bool,
    rail: bool,
    background: Option<Color>,
}

impl ProjectionInfo {
    fn plain(entry: &DshRenderEntry, width: usize, theme: Theme) -> Self {
        Self {
            mode: default_display_mode(entry, width, theme),
            group_anchor: None,
            group_header: false,
            group_hidden: false,
            group_expanded: false,
            group_last_visible: false,
            group_kind: None,
            group_count: 0,
            verb_counts: VerbCounts::default(),
            group_running: false,
            group_failed: false,
            rail: entry.kind == DshRenderKind::ToolCall,
            background: (entry.kind == DshRenderKind::User).then_some(theme.bg_light),
        }
    }
}

/// Production scrollback adapter.
///
/// DSH `Scrollback` owns entry identity, partial replacement, height indexing
/// and anchor restoration.  This cache owns only the Grok semantic block lines
/// for entries that are currently known.  It reports those actual heights back
/// to the DSH index and then paints through the shared `ScrollbackLayout`.
#[derive(Debug, Default)]
pub struct ScrollbackPane {
    width: usize,
    show_timestamps: bool,
    wave_elapsed_ms: u64,
    entries: HashMap<DshRenderEntryId, CachedPaneEntry>,
    expanded_entries: HashSet<DshRenderEntryId>,
    expanded_blocks: HashSet<(DshRenderEntryId, usize)>,
    expanded_groups: HashSet<DshRenderEntryId>,
    projections: HashMap<DshRenderEntryId, ProjectionInfo>,
    selected_target: Option<HitTarget>,
}

impl ScrollbackPane {
    pub fn clear(&mut self) {
        self.width = 0;
        self.show_timestamps = true;
        self.wave_elapsed_ms = 0;
        self.entries.clear();
        self.expanded_entries.clear();
        self.expanded_blocks.clear();
        self.expanded_groups.clear();
        self.projections.clear();
        self.selected_target = None;
    }

    pub fn sync(&mut self, scrollback: &mut Scrollback, width: usize, theme: Theme) {
        self.sync_with_options(scrollback, width, theme, true);
    }

    /// Set monotonic animation time without invalidating the semantic-line
    /// cache. Running accents are recolored when visible lines materialize.
    pub fn set_wave_elapsed(&mut self, elapsed: Duration) {
        self.wave_elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    }

    /// Pass the runtime's current transcript hit target into the projection.
    /// Grok uses the same selection state to lift a muted block back to its
    /// primary (white) foreground; keeping it here also makes the paint path
    /// independent of the runtime's hit-map implementation.
    pub fn set_selected_target(&mut self, target: Option<HitTarget>) {
        self.selected_target = target;
    }

    pub fn sync_with_options(
        &mut self,
        scrollback: &mut Scrollback,
        width: usize,
        theme: Theme,
        show_timestamps: bool,
    ) {
        let width = width.max(1);
        if self.width != width || self.show_timestamps != show_timestamps {
            self.entries.clear();
        }
        self.width = width;
        self.show_timestamps = show_timestamps;
        let entries = scrollback.render_entries();
        self.prune_local_state(&entries);
        self.projections = build_projection(
            &entries,
            width,
            theme,
            &self.expanded_entries,
            &self.expanded_blocks,
            &self.expanded_groups,
        );
        let mut live = HashMap::with_capacity(entries.len());
        for (entry_idx, entry) in entries.into_iter().enumerate() {
            let projection = self
                .projections
                .get(&entry.id)
                .copied()
                .unwrap_or_else(|| ProjectionInfo::plain(&entry, width, theme));
            let cached = self.entries.remove(&entry.id);
            let cached = match cached {
                Some(cached) if cached.entry == entry && cached.projection == projection => cached,
                _ => CachedPaneEntry {
                    lines: semantic_lines(
                        &entry,
                        width,
                        theme,
                        projection,
                        &self.expanded_blocks,
                        show_timestamps,
                    ),
                    entry: entry.clone(),
                    projection,
                },
            };
            let height =
                if entry.visibility == DshRenderVisibility::Hidden || projection.group_hidden {
                    0
                } else {
                    cached
                        .lines
                        .len()
                        .saturating_add(if projection.group_anchor.is_some() {
                            usize::from(projection.group_last_visible)
                        } else {
                            1
                        })
                };
            scrollback.set_projected_height(width, entry_idx, height);
            live.insert(entry.id, cached);
        }
        self.entries = live;
    }

    fn prune_local_state(&mut self, entries: &[DshRenderEntry]) {
        let live = entries.iter().map(|entry| entry.id).collect::<HashSet<_>>();
        self.expanded_entries.retain(|id| live.contains(id));
        self.expanded_groups.retain(|id| live.contains(id));
        self.expanded_blocks.retain(|(id, index)| {
            live.contains(id)
                && entries
                    .iter()
                    .find(|entry| entry.id == *id)
                    .and_then(|entry| entry.content.blocks.get(*index))
                    .is_some_and(is_local_foldable_block)
        });
    }

    /// Handle a transcript click after the runtime has resolved it through the
    /// render-time hit map. Single clicks remain selection-only; this method is
    /// called only for the second click in the same short gesture window.
    pub fn toggle_fold_or_group(&mut self, entry_id: DshRenderEntryId) -> bool {
        self.toggle_fold_or_group_at(entry_id, None)
    }

    /// Toggle a structured block inside an assistant surface. Grok's native
    /// scrollback routes this gesture to the concrete `ThinkingBlock` or
    /// `ToolCallBlock`; this keeps the same behavior while the host retains
    /// one stable `(turn, step)` surface id.
    pub fn toggle_fold_or_group_at(
        &mut self,
        entry_id: DshRenderEntryId,
        block_index: Option<usize>,
    ) -> bool {
        if let Some(block_index) = block_index {
            let foldable = self
                .entries
                .get(&entry_id)
                .and_then(|entry| entry.entry.content.blocks.get(block_index))
                .is_some_and(is_local_foldable_block);
            if !foldable {
                return false;
            }
            let key = (entry_id, block_index);
            if !self.expanded_blocks.insert(key) {
                self.expanded_blocks.remove(&key);
            }
            // The block projection is width-specific just like the upstream
            // EntryRenderer cache. Rebuild it on the next frame.
            self.entries.clear();
            return true;
        }
        let Some(projection) = self.projections.get(&entry_id).copied() else {
            return false;
        };
        if let Some(anchor) = projection.group_anchor
            && (projection.group_header || anchor == entry_id)
        {
            if !self.expanded_groups.insert(anchor) {
                self.expanded_groups.remove(&anchor);
            }
            self.entries.clear();
            return true;
        }
        if !is_foldable_kind(self.entries.get(&entry_id).map(|entry| &entry.entry)) {
            return false;
        }
        if !self.expanded_entries.insert(entry_id) {
            self.expanded_entries.remove(&entry_id);
        }
        self.entries.clear();
        true
    }

    pub fn is_group_header(&self, entry_id: DshRenderEntryId) -> bool {
        self.projections
            .get(&entry_id)
            .is_some_and(|projection| projection.group_header)
    }

    pub fn is_empty(&self) -> bool {
        self.entries
            .values()
            .all(|entry| entry.entry.visibility == DshRenderVisibility::Hidden)
    }

    pub fn is_animating(&self) -> bool {
        let now = now_epoch_ms();
        self.entries.values().any(|entry| {
            entry.lines.iter().any(|line| {
                line.rail_animated
                    || (line.rail_flash && finish_flash_active(entry.entry.finished_at_ms, now))
            })
        })
    }

    pub fn total_height(&mut self, scrollback: &mut Scrollback) -> usize {
        scrollback.total_height(self.width.max(1))
    }

    pub fn anchor_at(
        &mut self,
        scrollback: &mut Scrollback,
        scroll_top: usize,
    ) -> Option<ScrollAnchor> {
        let (total_height, entries) = {
            let layout = scrollback.layout(self.width.max(1));
            (layout.total_height, layout.entries.to_vec())
        };
        let top = scroll_top.min(total_height.checked_sub(1)?);
        let item = entries
            .iter()
            .rev()
            .find(|item| item.height > 0 && item.start_y <= top)?;
        let entry = scrollback.entries().get(item.entry_idx)?;
        Some(ScrollAnchor {
            entry_id: entry.id,
            intra_row: top
                .saturating_sub(item.start_y)
                .min(item.height.saturating_sub(1)),
        })
    }

    pub fn scroll_for_anchor(
        &mut self,
        scrollback: &mut Scrollback,
        anchor: ScrollAnchor,
    ) -> Option<usize> {
        let entry_idx = scrollback.entry_index(anchor.entry_id)?;
        let item = {
            let layout = scrollback.layout(self.width.max(1));
            *layout.entries.get(entry_idx)?
        };
        Some(
            item.start_y
                .saturating_add(anchor.intra_row.min(item.height.saturating_sub(1))),
        )
    }

    pub fn visible_lines(
        &mut self,
        scrollback: &mut Scrollback,
        scroll_top: usize,
        viewport_height: u16,
    ) -> Vec<RichPaintLine> {
        if viewport_height == 0 {
            return Vec::new();
        }
        let (total_height, entries) = {
            let layout = scrollback.layout(self.width.max(1));
            (layout.total_height, layout.entries.to_vec())
        };
        let top = scroll_top.min(total_height);
        let range = compute_paint_window(&entries, top, viewport_height as usize);
        let mut painted = Vec::new();
        for entry_idx in range {
            let Some(entry) = scrollback.entries().get(entry_idx) else {
                continue;
            };
            if entries
                .get(entry_idx)
                .is_none_or(|layout| layout.height == 0)
            {
                continue;
            }
            let Some(cached) = self.entries.get(&entry.id) else {
                continue;
            };
            for line in &cached.lines {
                let virtual_y = entries[entry_idx].start_y.saturating_add(line.line_index);
                if virtual_y < top {
                    continue;
                }
                let screen_y = virtual_y.saturating_sub(top);
                if screen_y >= viewport_height as usize {
                    break;
                }
                let mut line = line.clone();
                line.screen_y = screen_y as u16;
                let selected = self
                    .selected_target
                    .as_ref()
                    .is_some_and(|target| line_matches_target(&line, target));
                apply_dynamic_accent(
                    &mut line,
                    self.wave_elapsed_ms,
                    *Theme::current(),
                    cached.entry.finished_at_ms,
                    now_epoch_ms(),
                    selected,
                );
                painted.push(line);
            }
        }
        painted
    }
}

fn semantic_lines(
    entry: &DshRenderEntry,
    width: usize,
    theme: Theme,
    projection: ProjectionInfo,
    expanded_blocks: &HashSet<(DshRenderEntryId, usize)>,
    show_timestamps: bool,
) -> Vec<RichPaintLine> {
    if projection.group_hidden {
        return Vec::new();
    }
    let Some(row) = projected_row(entry, projection.mode, width) else {
        return Vec::new();
    };
    let semantic = if entry.kind == DshRenderKind::Thinking
        && projection.mode == DisplayMode::Collapsed
        && let Some(block) = entry
            .content
            .blocks
            .iter()
            .find(|block| matches!(block, DshRenderBlock::Reasoning { .. }))
    {
        vec![SemanticLine {
            line: collapsed_block_line(block, entry, theme),
            block_index: None,
            rail: true,
            header: true,
            selectable: true,
        }]
    } else if projection.mode == DisplayMode::Collapsed
        && entry.kind == DshRenderKind::ToolCall
        && let Some(tool) = tool_block(entry)
        && let Some(line) = execute_summary_line(
            tool,
            entry.finish,
            theme,
            width.saturating_sub(ENTRY_CHROME_WIDTH),
        )
    {
        vec![SemanticLine {
            line,
            block_index: None,
            rail: true,
            header: true,
            selectable: true,
        }]
    } else {
        render_semantic_lines(entry, &row, theme, width, expanded_blocks)
    };
    let mut semantic = semantic;
    if projection.group_header {
        let single_read_detail = (projection.group_count == 1 && projection.verb_counts.read == 1)
            .then(|| tool_block(entry).and_then(read_summary_detail))
            .flatten();
        let label = group_header_label(
            projection.group_kind,
            projection.group_count,
            projection.verb_counts,
            projection.group_running,
            projection.group_failed,
            projection.group_expanded,
            single_read_detail.as_deref(),
        );
        let header = Line::from(Span::styled(
            label,
            Style::default()
                .fg(if projection.group_failed {
                    theme.accent_error
                } else {
                    theme.gray
                })
                .add_modifier(Modifier::BOLD),
        ));
        if projection.group_expanded {
            semantic.insert(
                0,
                SemanticLine {
                    line: header,
                    block_index: None,
                    rail: projection.rail,
                    header: true,
                    selectable: true,
                },
            );
        } else {
            semantic = vec![SemanticLine {
                line: header,
                block_index: None,
                rail: projection.rail,
                header: true,
                selectable: true,
            }];
        }
    }
    let mut lines = Vec::new();
    let timestamp = if show_timestamps
        && matches!(entry.kind, DshRenderKind::User | DshRenderKind::Assistant)
        && !projection.group_header
    {
        timestamp_label(entry.created_at_ms)
    } else {
        None
    };
    let mut timestamp_pending = timestamp;
    let rail_accent = if projection.group_failed || entry.finish == DshRenderFinish::Failed {
        theme.accent_error
    } else if !projection.group_header
        && let Some(tool) = tool_block(entry)
    {
        tool_accent(tool, entry.finish, theme)
    } else {
        theme.gray
    };
    let rail_animated = projection.group_running || entry.finish == DshRenderFinish::Running;
    let rail_flash = entry.finished_at_ms.is_some()
        && !rail_animated
        && matches!(
            entry.kind,
            DshRenderKind::Thinking | DshRenderKind::ToolCall
        );
    let collapsed_rail = !rail_animated
        && ((projection.group_header && !projection.group_expanded)
            || (entry.kind == DshRenderKind::ToolCall
                && projection.mode == DisplayMode::Collapsed));
    for semantic_line in semantic {
        let line_rail = projection.rail || semantic_line.rail;
        let reserve_timestamp = timestamp_pending.is_some();
        let wrap_width = width
            .saturating_sub(ENTRY_CHROME_WIDTH)
            .saturating_sub(usize::from(reserve_timestamp) * TIMESTAMP_RESERVED_WIDTH)
            .max(1);
        for wrapped_line in word_wrap_line(&semantic_line.line, wrap_width) {
            let copy_text = wrapped_line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();
            let line = decorate_line(
                wrapped_line,
                width,
                line_rail,
                collapsed_rail,
                rail_accent,
                projection.background,
                theme,
            );
            lines.push(RichPaintLine {
                entry_id: entry.id,
                block_index: semantic_line.block_index,
                line_index: lines.len(),
                header: semantic_line.header,
                group_header: projection.group_header && semantic_line.header,
                rail: line_rail,
                rail_animated: line_rail && rail_animated,
                rail_flash: line_rail && rail_flash,
                rail_accent: line_rail.then_some(rail_accent),
                rail_wave_row: 0,
                rail_wave_len: 0,
                selectable: semantic_line.selectable,
                background: projection.background,
                copy_text,
                content_offset: if semantic_line.selectable {
                    ENTRY_CHROME_WIDTH as u16
                } else {
                    0
                },
                timestamp: timestamp_pending.take(),
                screen_y: 0,
                line,
            });
        }
    }
    if projection.background.is_some() {
        let blank = Line::from(Span::styled(
            " ".repeat(width),
            Style::default().bg(projection.background.unwrap_or(theme.bg_base)),
        ));
        let content_len = lines.len();
        for line in &mut lines {
            line.line_index = line.line_index.saturating_add(1);
        }
        lines.insert(
            0,
            RichPaintLine {
                entry_id: entry.id,
                block_index: None,
                line_index: 0,
                header: false,
                group_header: false,
                rail: false,
                rail_animated: false,
                rail_flash: false,
                rail_accent: None,
                rail_wave_row: 0,
                rail_wave_len: 0,
                selectable: false,
                background: projection.background,
                copy_text: String::new(),
                content_offset: 0,
                timestamp: None,
                screen_y: 0,
                line: blank.clone(),
            },
        );
        lines.push(RichPaintLine {
            entry_id: entry.id,
            block_index: None,
            line_index: content_len.saturating_add(1),
            header: false,
            group_header: false,
            rail: false,
            rail_animated: false,
            rail_flash: false,
            rail_accent: None,
            rail_wave_row: 0,
            rail_wave_len: 0,
            selectable: false,
            background: projection.background,
            copy_text: String::new(),
            content_offset: 0,
            timestamp: None,
            screen_y: 0,
            line: blank,
        });
    }
    assign_rail_wave_geometry(&mut lines);
    lines
}

fn assign_rail_wave_geometry(lines: &mut [RichPaintLine]) {
    let mut start = 0;
    while start < lines.len() {
        if !lines[start].rail_animated {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < lines.len() && lines[end].rail_animated {
            end += 1;
        }
        let len = (end - start).min(u16::MAX as usize) as u16;
        for (row, line) in lines[start..end].iter_mut().enumerate() {
            line.rail_wave_row = row.min(u16::MAX as usize) as u16;
            line.rail_wave_len = len;
        }
        start = end;
    }
}

#[derive(Debug, Clone)]
struct SemanticLine {
    line: Line<'static>,
    block_index: Option<usize>,
    rail: bool,
    header: bool,
    selectable: bool,
}

/// Upstream Grok's horizontal entry chrome: accent column + two left-pad
/// columns. The local pane has its own outer border, so these three columns
/// are painted as either `│  ` for operational rows or `   ` for plain text.
const ENTRY_CHROME_WIDTH: usize = 3;
const TIMESTAMP_RESERVED_WIDTH: usize = 10;
fn is_local_foldable_block(block: &DshRenderBlock) -> bool {
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

fn collapsed_block_line(
    block: &DshRenderBlock,
    entry: &DshRenderEntry,
    theme: Theme,
) -> Line<'static> {
    match block {
        DshRenderBlock::Reasoning { .. } => {
            let label = if entry.finish == DshRenderFinish::Running {
                "Thinking…".to_string()
            } else if let Some(elapsed) = thinking_elapsed_ms(entry) {
                format!("Thought for {}", format_elapsed_ms(elapsed))
            } else {
                "Thought".to_string()
            };
            Line::from(Span::styled(
                format!("{} {label}", crate::glyphs::diamond_filled()),
                Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
            ))
        }
        DshRenderBlock::ToolCall { .. } => execute_summary_line(block, entry.finish, theme, 120)
            .unwrap_or_else(|| {
                tool_summary_line(
                    "",
                    "›",
                    &tool_header_text(block),
                    tool_accent(block, entry.finish, theme),
                    theme,
                )
            }),
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

fn now_epoch_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn finish_flash_active(finished_at_ms: Option<u64>, now_ms: Option<u64>) -> bool {
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

fn thinking_elapsed_ms(entry: &DshRenderEntry) -> Option<u64> {
    let started = entry.started_at_ms?;
    let ended = if entry.finish == DshRenderFinish::Running {
        now_epoch_ms()?
    } else {
        entry.finished_at_ms?
    };
    ended.checked_sub(started)
}

fn format_elapsed_ms(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms as f64 / 1_000.0;
    if seconds < 60.0 {
        format!("{seconds:.1}s")
    } else {
        let minutes = (seconds / 60.0).floor() as u64;
        let remaining = (seconds - minutes as f64 * 60.0).round() as u64;
        format!("{minutes}m{remaining}s")
    }
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
    expanded_blocks: &HashSet<(DshRenderEntryId, usize)>,
) -> Vec<SemanticLine> {
    let has_foldable_blocks = row.kind == DshRenderKind::Assistant
        && (entry.group_key.is_some() || row.content.blocks.len() > 1)
        && row.content.blocks.iter().any(is_local_foldable_block);
    if !has_foldable_blocks {
        if row.kind == DshRenderKind::Thinking
            && entry.finish == DshRenderFinish::Running
            && let Some(block) = row
                .content
                .blocks
                .iter()
                .find(|block| matches!(block, DshRenderBlock::Reasoning { .. }))
        {
            return thinking_running_lines(entry, block, theme, width);
        }
        return render_row_at_width(row, theme, width.saturating_sub(ENTRY_CHROME_WIDTH))
            .into_iter()
            .enumerate()
            .map(|(index, line)| SemanticLine {
                line,
                block_index: None,
                rail: false,
                header: index == 0,
                selectable: true,
            })
            .collect();
    }

    let mut semantic = Vec::new();
    let blocks = &row.content.blocks;
    for (index, block) in blocks.iter().enumerate() {
        let expanded = expanded_blocks.contains(&(entry.id, index));
        if index > 0 {
            let previous = &blocks[index - 1];
            let previous_expanded = expanded_blocks.contains(&(entry.id, index - 1));
            let dense_operational_run = is_local_foldable_block(previous)
                && is_local_foldable_block(block)
                && !previous_expanded
                && !expanded;
            if !dense_operational_run {
                semantic.push(SemanticLine {
                    line: Line::from(""),
                    block_index: None,
                    rail: false,
                    header: false,
                    selectable: false,
                });
            }
        }
        let lines = if matches!(block, DshRenderBlock::Reasoning { .. })
            && entry.finish == DshRenderFinish::Running
            && !expanded
        {
            thinking_running_lines(entry, block, theme, width)
                .into_iter()
                .map(|line| line.line)
                .collect::<Vec<_>>()
        } else if is_local_foldable_block(block) && !expanded {
            vec![collapsed_block_line(block, entry, theme)]
        } else {
            let mut rendered = Vec::new();
            render_block(
                &mut rendered,
                block,
                theme,
                0,
                width.saturating_sub(ENTRY_CHROME_WIDTH),
            );
            rendered
        };
        let rail = is_operational_block(block);
        for (line_index, line) in lines.into_iter().enumerate() {
            semantic.push(SemanticLine {
                line,
                block_index: Some(index),
                rail,
                header: line_index == 0,
                selectable: true,
            });
        }
    }
    semantic
}

fn thinking_running_lines(
    entry: &DshRenderEntry,
    block: &DshRenderBlock,
    theme: Theme,
    width: usize,
) -> Vec<SemanticLine> {
    let mut body = Vec::new();
    render_block(
        &mut body,
        block,
        theme,
        0,
        width.saturating_sub(ENTRY_CHROME_WIDTH),
    );
    let mut lines = vec![SemanticLine {
        line: collapsed_block_line(block, entry, theme),
        block_index: None,
        rail: true,
        header: true,
        selectable: true,
    }];
    let max_body = 3usize;
    if body.len() > max_body {
        lines.push(SemanticLine {
            line: Line::from(Span::styled("…", theme.gray)),
            block_index: None,
            rail: true,
            header: false,
            selectable: true,
        });
        body = body.split_off(body.len().saturating_sub(max_body));
    }
    lines.extend(body.into_iter().map(|line| SemanticLine {
        line,
        block_index: None,
        rail: true,
        header: false,
        selectable: true,
    }));
    lines
}

impl RichTranscript {
    pub fn new(entries: &[DshRenderEntry], width: usize, theme: Theme) -> Self {
        let width = width.max(1);
        let mut projected = Vec::with_capacity(entries.len());
        let mut start_y = 0usize;
        for entry in entries {
            let projection = ProjectionInfo::plain(entry, width, theme);
            let lines = semantic_lines(entry, width, theme, projection, &HashSet::new(), true);
            if lines.is_empty() {
                continue;
            }
            // Every entry keeps one non-selectable spacer row, matching the
            // existing scrollback rhythm without inventing a fake identity.
            let height = lines.len().saturating_add(1);
            projected.push(RichEntry {
                id: entry.id,
                lines,
                start_y,
            });
            start_y = start_y.saturating_add(height);
        }
        Self {
            width,
            entries: projected,
            total_height: start_y,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn total_height(&self) -> usize {
        self.total_height
    }

    pub fn anchor_at(&self, scroll_top: usize) -> Option<ScrollAnchor> {
        let top = scroll_top.min(self.total_height.checked_sub(1)?);
        let entry = self
            .entries
            .iter()
            .rev()
            .find(|entry| entry.start_y <= top)?;
        let intra = top.saturating_sub(entry.start_y);
        Some(ScrollAnchor {
            entry_id: entry.id,
            intra_row: intra.min(entry.lines.len().saturating_sub(1)),
        })
    }

    pub fn scroll_for_anchor(&self, anchor: ScrollAnchor) -> Option<usize> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.id == anchor.entry_id)?;
        Some(
            entry
                .start_y
                .saturating_add(anchor.intra_row.min(entry.lines.len().saturating_sub(1))),
        )
    }

    pub fn visible_lines(&self, scroll_top: usize, viewport_height: u16) -> Vec<RichPaintLine> {
        let top = scroll_top.min(self.total_height);
        let bottom = top.saturating_add(viewport_height as usize);
        let mut painted = Vec::new();
        for entry in &self.entries {
            for line in &entry.lines {
                let virtual_y = entry.start_y.saturating_add(line.line_index);
                if virtual_y < top {
                    continue;
                }
                if virtual_y >= bottom {
                    break;
                }
                let mut line = line.clone();
                line.screen_y = (virtual_y.saturating_sub(top)) as u16;
                painted.push(line);
            }
            if entry.start_y >= bottom {
                break;
            }
        }
        painted
    }

    pub fn line_y(&self, entry_id: DshRenderEntryId, line_index: usize) -> Option<usize> {
        let entry = self.entries.iter().find(|entry| entry.id == entry_id)?;
        (line_index < entry.lines.len()).then_some(entry.start_y.saturating_add(line_index))
    }
}

/// Paint a materialized scrollback line. Scrollback owns wrapping and line
/// identity; this helper only applies Grok's semantic role colors.
pub fn style_for_paint(kind: DshRenderKind, header: bool, text: &str, theme: Theme) -> Style {
    if header {
        return Style::default()
            .fg(color_for_kind(kind, theme))
            .add_modifier(Modifier::BOLD);
    }
    let color = if text.starts_with("▸ ")
        || text.starts_with("› ")
        || text.starts_with(glyphs::disclosure_open())
        || text.starts_with("◆ ")
        || text.starts_with('✓')
    {
        theme.gray_bright
    } else if text.starts_with('✗') || text.starts_with("[unsupported") {
        theme.accent_user
    } else if text.starts_with("diff ") {
        theme.diff_equal_fg
    } else if text.starts_with('+') {
        theme.diff_insert_fg
    } else if text.starts_with('-') {
        theme.diff_delete_fg
    } else {
        color_for_kind(kind, theme)
    };
    Style::default().fg(color)
}

/// Render one transcript row using the same role hierarchy as the imported
/// Grok block widgets. User and assistant messages render their content
/// directly; operational rows retain a stable semantic header.
pub fn render_row(row: &TranscriptRow, theme: Theme) -> Vec<Line<'static>> {
    render_row_at_width(row, theme, 120)
}

fn render_row_at_width(row: &TranscriptRow, theme: Theme, width: usize) -> Vec<Line<'static>> {
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
        return vec![tool_summary_line("", "›", header, color, theme)];
    }

    let mut lines = if matches!(row.kind, DshRenderKind::User | DshRenderKind::Assistant) {
        Vec::new()
    } else {
        vec![Line::from(Span::styled(
            row.label.clone(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))]
    };

    if row.kind == DshRenderKind::User {
        push_plain_lines(
            &mut lines,
            &row.text,
            Style::default().fg(theme.text_primary),
            "",
        );
        for (index, line) in lines.iter_mut().enumerate() {
            line.spans.insert(
                0,
                Span::styled(
                    if index == 0 { "❯ " } else { "  " },
                    Style::default()
                        .fg(theme.accent_user)
                        .add_modifier(Modifier::BOLD),
                ),
            );
        }
        return lines;
    }

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
        if let Some(tool) = entry
            .content
            .blocks
            .iter()
            .find(|block| matches!(block, DshRenderBlock::ToolCall { .. }))
        {
            row.label = format!("› {}", tool_header_text(tool));
            row.text.clear();
            row.content = DshRenderContent::default();
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

fn is_foldable_kind(entry: Option<&DshRenderEntry>) -> bool {
    entry.is_some_and(|entry| {
        matches!(
            entry.kind,
            DshRenderKind::User
                | DshRenderKind::Thinking
                | DshRenderKind::ToolCall
                | DshRenderKind::ToolResult
                | DshRenderKind::Error
        ) && !entry.text.trim().is_empty()
    })
}

fn default_display_mode(entry: &DshRenderEntry, width: usize, theme: Theme) -> DisplayMode {
    if entry.visibility == DshRenderVisibility::Collapsed {
        return DisplayMode::Collapsed;
    }
    match entry.kind {
        DshRenderKind::User => {
            let lines = render_row(&TranscriptRow::from(entry.clone()), theme)
                .iter()
                .map(|line| word_wrap_line(line, width))
                .map(|lines| lines.len())
                .sum::<usize>();
            if lines > 3 {
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
        | DshRenderKind::Error => DisplayMode::Collapsed,
        DshRenderKind::AgentContext | DshRenderKind::Context | DshRenderKind::Compaction => {
            DisplayMode::Collapsed
        }
        _ => DisplayMode::Expanded,
    }
}

fn group_header_label(
    kind: Option<GroupKind>,
    count: usize,
    counts: VerbCounts,
    running: bool,
    failed: bool,
    expanded: bool,
    single_read_detail: Option<&str>,
) -> String {
    let count = count.max(1);
    match (kind, expanded) {
        (Some(GroupKind::Context), false) => format!("◆ Context · {count} injected messages"),
        (Some(GroupKind::Context), true) => format!("▾ Context · {count} injected messages"),
        (Some(GroupKind::VerbRun), false)
            if count == 1 && counts.read == 1 && single_read_detail.is_some() =>
        {
            format!("◆ Read · {}", single_read_detail.unwrap_or_default())
        }
        (Some(GroupKind::VerbRun), _) => {
            let mut parts = Vec::new();
            let mut push = |calls: usize, done: &str, doing: &str, one: &str, many: &str| {
                if calls > 0 {
                    parts.push(format!(
                        "{} {calls} {}",
                        if running { doing } else { done },
                        if calls == 1 { one } else { many }
                    ));
                }
            };
            push(counts.read, "Read", "Reading", "file", "files");
            push(
                counts.search,
                "Searched",
                "Searching",
                "pattern",
                "patterns",
            );
            push(counts.web_fetch, "Fetched", "Fetching", "page", "pages");
            push(
                counts.web_search,
                "Searched",
                "Searching",
                "web query",
                "web queries",
            );
            let mut label = parts.join(", ");
            if failed {
                label.push_str(" · failed");
            }
            format!("{} {label}", if expanded { "▾" } else { "◆" })
        }
        (None, false) => format!("◆ {count} more"),
        (None, true) => format!("▾ {count} messages"),
    }
}

fn decorate_line(
    mut line: Line<'static>,
    width: usize,
    rail: bool,
    collapsed_rail: bool,
    rail_accent: Color,
    background: Option<Color>,
    theme: Theme,
) -> Line<'static> {
    // EntryRenderer always reserves accent + left padding.  Operational rows
    // use the accent column as a continuous rail; plain Markdown keeps those
    // columns as spaces so its first glyph lands exactly where a tool diamond
    // would land.
    let prefix = if !rail {
        "   "
    } else if collapsed_rail {
        "❙  "
    } else {
        "┃  "
    };
    line.spans.insert(
        0,
        Span::styled(
            prefix,
            Style::default()
                .fg(rail_accent)
                .bg(background.unwrap_or(theme.bg_base)),
        ),
    );
    if let Some(background) = background {
        for span in &mut line.spans {
            span.style = span.style.bg(background);
        }
        let used = line.width();
        if used < width {
            line.spans.push(Span::styled(
                " ".repeat(width.saturating_sub(used)),
                Style::default().bg(background),
            ));
        }
    }
    line
}

const WAVE_ROWS_PER_SECOND: f64 = 4.0;
const WAVE_BAND_ROWS: f64 = 4.0;
const WAVE_GAP_ROWS: f64 = 6.0;
const FINISH_FLASH_DURATION_MS: u64 = 400;

fn wave_cycle_seconds(rail_len: u16) -> f64 {
    (f64::from(rail_len.max(1)) + WAVE_BAND_ROWS + WAVE_GAP_ROWS) / WAVE_ROWS_PER_SECOND
}

fn wave_brightness(elapsed_ms: u64, row: u16, rail_len: u16) -> f32 {
    use std::f64::consts::FRAC_PI_2;

    let rail_len = rail_len.max(row.saturating_add(1)).max(1);
    let travel_rows = wave_cycle_seconds(rail_len) * WAVE_ROWS_PER_SECOND;
    let traveled_rows = (elapsed_ms as f64 / 1_000.0 * WAVE_ROWS_PER_SECOND) % travel_rows;
    let half_band = WAVE_BAND_ROWS / 2.0;
    let band_center = traveled_rows - half_band;
    let row_center = f64::from(row) + 0.5;
    let normalized_distance = (row_center - band_center).abs() / half_band;
    if normalized_distance >= 1.0 {
        return 0.0;
    }
    let envelope = (normalized_distance * FRAC_PI_2).cos();
    (envelope * envelope) as f32
}

fn blend_color(base: Color, foreground: Color, opacity: f32) -> Option<Color> {
    let (Color::Rgb(br, bg, bb), Color::Rgb(fr, fg, fb)) = (base, foreground) else {
        return None;
    };
    let opacity = opacity.clamp(0.0, 1.0);
    let channel = |base: u8, foreground: u8| {
        (base as f32 + (foreground as f32 - base as f32) * opacity).round() as u8
    };
    Some(Color::Rgb(
        channel(br, fr),
        channel(bg, fg),
        channel(bb, fb),
    ))
}

fn line_matches_target(line: &RichPaintLine, target: &HitTarget) -> bool {
    match target {
        HitTarget::TranscriptEntry(entry_id) => line.entry_id == *entry_id,
        HitTarget::TranscriptBlock {
            entry_id,
            block_index,
        } => {
            line.entry_id == *entry_id
                && (line.block_index == Some(*block_index)
                    // A standalone running Thinking surface has no local
                    // block index; its entry target is still the block's
                    // visible selection target.
                    || (line.block_index.is_none() && line.header))
        }
        _ => false,
    }
}

fn apply_dynamic_accent(
    line: &mut RichPaintLine,
    wave_elapsed_ms: u64,
    theme: Theme,
    finished_at_ms: Option<u64>,
    now_ms: Option<u64>,
    selected: bool,
) {
    if !line.rail || (!line.rail_animated && !line.rail_flash && !selected) {
        return;
    }
    let accent = line.rail_accent.unwrap_or(theme.gray);
    let error_accent = accent == theme.accent_error;
    let background = line.background.unwrap_or(theme.bg_base);
    let flashing = line.rail_flash && finish_flash_active(finished_at_ms, now_ms);
    if !line.rail_animated && !flashing && !selected {
        return;
    }
    let rail_color = if flashing {
        accent
    } else if selected {
        if error_accent {
            accent
        } else {
            theme.text_primary
        }
    } else {
        blend_color(
            background,
            theme.text_primary,
            wave_brightness(wave_elapsed_ms, line.rail_wave_row, line.rail_wave_len),
        )
        .unwrap_or(theme.text_primary)
    };
    let text_color = if error_accent {
        accent
    } else if selected || line.rail_animated {
        theme.text_primary
    } else {
        theme.gray
    };

    if let Some(prefix) = line.line.spans.first_mut() {
        // Only the left rail receives the animated color.
        prefix.style = prefix.style.fg(rail_color);
    }

    // Right-side content is a static role: muted gray when settled, primary
    // white when selected or running. It must not inherit the rail's wave
    // phase.
    if selected || line.rail_animated {
        for span in line.line.spans.iter_mut().skip(1) {
            span.style = span.style.fg(text_color);
        }
    }
}

fn build_projection(
    entries: &[DshRenderEntry],
    width: usize,
    theme: Theme,
    expanded_entries: &HashSet<DshRenderEntryId>,
    _expanded_blocks: &HashSet<(DshRenderEntryId, usize)>,
    expanded_groups: &HashSet<DshRenderEntryId>,
) -> HashMap<DshRenderEntryId, ProjectionInfo> {
    let mut projections = entries
        .iter()
        .map(|entry| {
            let mut projection = ProjectionInfo::plain(entry, width, theme);
            if expanded_entries.contains(&entry.id) && is_foldable_kind(Some(entry)) {
                projection.mode = DisplayMode::Expanded;
            }
            (entry.id, projection)
        })
        .collect::<HashMap<_, _>>();

    let mut index = 0usize;
    while index < entries.len() {
        if entries[index].visibility == DshRenderVisibility::Hidden {
            index += 1;
            continue;
        }
        let context = matches!(
            entries[index].kind,
            DshRenderKind::AgentContext | DshRenderKind::Context | DshRenderKind::Compaction
        );
        let verb_member = tool_verb(&entries[index]).is_some();
        if !context && !verb_member {
            index += 1;
            continue;
        }
        let kind = if context {
            GroupKind::Context
        } else {
            GroupKind::VerbRun
        };
        let start = index;
        index += 1;
        while index < entries.len() && entries[index].visibility != DshRenderVisibility::Hidden {
            let joins = match kind {
                GroupKind::Context => matches!(
                    entries[index].kind,
                    DshRenderKind::AgentContext
                        | DshRenderKind::Context
                        | DshRenderKind::Compaction
                ),
                GroupKind::VerbRun => tool_verb(&entries[index]).is_some(),
            };
            if !joins {
                break;
            }
            index += 1;
        }
        let count = index.saturating_sub(start);
        if kind == GroupKind::Context && count < 2 {
            continue;
        }
        let mut verb_counts = VerbCounts::default();
        let mut group_running = false;
        let mut group_failed = false;
        if kind == GroupKind::VerbRun {
            for entry in &entries[start..index] {
                if let Some(verb) = tool_verb(entry) {
                    verb_counts.add(verb);
                }
                group_running |= entry.finish == DshRenderFinish::Running;
                group_failed |= entry.finish == DshRenderFinish::Failed;
            }
        }
        let anchor = entries[start].id;
        let expanded = expanded_groups.contains(&anchor);
        for (offset, entry) in entries[start..index].iter().enumerate() {
            let projection = projections
                .get_mut(&entry.id)
                .expect("projection exists for canonical entry");
            projection.group_anchor = Some(anchor);
            projection.group_header = offset == 0;
            projection.group_hidden = !expanded && offset > 0;
            projection.group_expanded = expanded;
            projection.group_last_visible = if expanded {
                offset + 1 == count
            } else {
                offset == 0
            };
            projection.group_kind = Some(kind);
            projection.group_count = count;
            projection.verb_counts = verb_counts;
            projection.group_running = group_running;
            projection.group_failed = group_failed;
            projection.rail = true;
        }
    }
    projections
}

/// Reconstruct a stable copy payload from typed blocks. This intentionally
/// preserves newlines and does not trim user-visible content.
pub fn copy_row(row: &TranscriptRow) -> String {
    copy_content(&row.content, &row.text)
}

fn copy_content(content: &DshRenderContent, fallback: &str) -> String {
    if content.blocks.is_empty() {
        fallback.to_string()
    } else {
        content.display_text()
    }
}

fn tool_block(entry: &DshRenderEntry) -> Option<&DshRenderBlock> {
    entry
        .content
        .blocks
        .iter()
        .find(|block| matches!(block, DshRenderBlock::ToolCall { .. }))
}

fn tool_verb(entry: &DshRenderEntry) -> Option<ToolVerb> {
    let DshRenderBlock::ToolCall {
        name, view, result, ..
    } = tool_block(entry)?
    else {
        return None;
    };
    if let Some(result_view) = result.as_ref().and_then(|result| result.view.as_ref()) {
        match result_view {
            DshToolResultView::Read { .. } => return Some(ToolVerb::Read),
            DshToolResultView::SearchMatches { .. } | DshToolResultView::SearchPaths { .. } => {
                return Some(ToolVerb::Search);
            }
            DshToolResultView::WebSearch { .. } => return Some(ToolVerb::WebSearch),
            DshToolResultView::WebFetch { .. } => return Some(ToolVerb::WebFetch),
            _ => {}
        }
    }
    match view.as_ref().map(DshToolCallView::kind) {
        Some(DshToolKind::Read) => Some(ToolVerb::Read),
        Some(DshToolKind::Search) => Some(ToolVerb::Search),
        Some(DshToolKind::Fetch) => Some(ToolVerb::WebFetch),
        _ if name == "read" => Some(ToolVerb::Read),
        _ => None,
    }
}

fn tool_diffs<'a>(
    view: Option<&'a DshToolCallView>,
    result: Option<&'a DshToolResult>,
) -> Option<&'a [DshToolDiff]> {
    if let Some(DshToolResultView::Diff { diffs, .. }) =
        result.and_then(|result| result.view.as_ref())
    {
        return Some(diffs);
    }
    if let Some(DshToolCallView::Diff { diffs, .. }) = view {
        return Some(diffs);
    }
    None
}

fn diffstat(diffs: &[DshToolDiff]) -> (usize, usize) {
    diffs.iter().fold((0, 0), |(added, removed), diff| {
        (
            added + diff.new_text.lines().count(),
            removed
                + diff
                    .old_text
                    .as_deref()
                    .map(str::lines)
                    .map(Iterator::count)
                    .unwrap_or(0),
        )
    })
}

fn tool_header_text(block: &DshRenderBlock) -> String {
    if let Some(execute) = project_execute_tool(block) {
        let theme = Theme::current();
        let is_running = matches!(block, DshRenderBlock::ToolCall { result: None, .. });
        let output = execute.output(&ExecuteBlockContext::new(
            ExecuteDisplayMode::Collapsed,
            is_running,
            4096,
            theme,
        ));
        return output
            .lines
            .first()
            .map(|line| line.content.to_string())
            .unwrap_or_else(|| "Run …".to_string());
    }
    let DshRenderBlock::ToolCall {
        name, view, result, ..
    } = block
    else {
        return String::new();
    };
    let completed_title = result
        .as_ref()
        .and_then(|result| result.view.as_ref())
        .and_then(DshToolResultView::title);
    let title = completed_title
        .or_else(|| view.as_ref().map(DshToolCallView::title))
        .map(str::to_string)
        .or_else(|| read_summary_detail(block).map(|detail| format!("Read {detail}")))
        .unwrap_or_else(|| name.to_string());
    let mut label = title;
    if let Some(diffs) = tool_diffs(view.as_ref(), result.as_deref()) {
        let (added, removed) = diffstat(diffs);
        if added > 0 || removed > 0 {
            label.push_str(&format!(" +{added}/-{removed}"));
        }
    }
    label
}

/// DSH Web uses the call arguments as the stable one-line summary for read
/// tools. The presenter title/result path remain replay-safe fallbacks for
/// events recorded before those arguments were available to the renderer.
fn read_summary_detail(block: &DshRenderBlock) -> Option<String> {
    let DshRenderBlock::ToolCall {
        name,
        arguments,
        view,
        result,
        ..
    } = block
    else {
        return None;
    };
    let is_read = name == "read"
        || view
            .as_ref()
            .is_some_and(|view| view.kind() == DshToolKind::Read)
        || matches!(
            result.as_ref().and_then(|result| result.view.as_ref()),
            Some(DshToolResultView::Read { .. })
        );
    if !is_read {
        return None;
    }

    if let Ok(arguments) = serde_json::from_str::<serde_json::Value>(arguments)
        && let Some(summary) = ["path", "file_path", "url"].iter().find_map(|key| {
            arguments
                .get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(nonempty_first_line)
        })
    {
        return Some(summary.to_string());
    }
    if let Some(DshToolResultView::Read { path, .. }) =
        result.as_ref().and_then(|result| result.view.as_ref())
        && let Some(path) = nonempty_first_line(path)
    {
        return Some(path.to_string());
    }
    view.as_ref()
        .map(DshToolCallView::title)
        .and_then(|title| title.strip_prefix("Read "))
        .and_then(nonempty_first_line)
        .map(str::to_string)
}

fn nonempty_first_line(value: &str) -> Option<&str> {
    value
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn tool_accent(block: &DshRenderBlock, finish: DshRenderFinish, theme: Theme) -> Color {
    let failed = match block {
        DshRenderBlock::ToolCall { result, .. } => {
            result.as_deref().is_some_and(|result| result.is_error)
        }
        _ => false,
    } || finish == DshRenderFinish::Failed;
    if failed {
        theme.accent_error
    } else {
        theme.gray
    }
}

fn execute_summary_line(
    block: &DshRenderBlock,
    finish: DshRenderFinish,
    theme: Theme,
    width: usize,
) -> Option<Line<'static>> {
    let execute = project_execute_tool(block)?;
    let context = ExecuteBlockContext::new(
        ExecuteDisplayMode::Collapsed,
        finish == DshRenderFinish::Running,
        width.saturating_sub(2).max(1),
        &theme,
    );
    let mut context = context;
    // Grok's collapsed Execute header uses the muted role; the selected or
    // running paint pass lifts it to primary/white.
    context.muted_command_collapsed = true;
    let accent = if finish == DshRenderFinish::Failed {
        theme.accent_error
    } else {
        theme.gray
    };
    let mut line = execute.output(&context).lines.into_iter().next()?.content;
    line.spans.insert(
        0,
        Span::styled(
            "› ",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
    );
    Some(line)
}

fn tool_summary_line(
    prefix: &str,
    marker: &str,
    header: &str,
    accent: Color,
    theme: Theme,
) -> Line<'static> {
    let mut spans = Vec::with_capacity(4);
    if !prefix.is_empty() {
        spans.push(Span::raw(prefix.to_string()));
    }
    spans.push(Span::styled(
        format!("{marker} "),
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    ));
    if let Some(description) = header.strip_prefix("Run ") {
        spans.push(Span::styled(
            "Run ",
            Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            description.to_string(),
            Style::default().fg(theme.gray),
        ));
    } else if let Some(command) = header.strip_prefix("$ ") {
        spans.push(Span::styled("$ ", Style::default().fg(theme.gray)));
        spans.push(Span::styled(
            command.to_string(),
            Style::default().fg(theme.gray),
        ));
    } else {
        spans.push(Span::styled(
            header.to_string(),
            Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn push_panel_lines(
    lines: &mut Vec<Line<'static>>,
    text: &str,
    prefix: &str,
    foreground: Color,
    theme: Theme,
) {
    for line in text.split('\n') {
        lines.push(Line::from(Span::styled(
            format!("{prefix}{line}"),
            Style::default().fg(foreground).bg(theme.bg_terminal),
        )));
    }
}

fn render_tool_children(
    lines: &mut Vec<Line<'static>>,
    blocks: &[DshRenderBlock],
    theme: Theme,
    indent: usize,
    width: usize,
) {
    for child in blocks {
        render_block(lines, child, theme, indent, width);
    }
}

fn paint_execute_component_line(
    mut component_line: ExecuteBlockLine,
    prefix: &str,
    first: bool,
    accent: Color,
    width: usize,
    theme: Theme,
) -> Line<'static> {
    let mut spans = Vec::with_capacity(component_line.content.spans.len() + 3);
    if !prefix.is_empty() {
        spans.push(Span::raw(prefix.to_string()));
    }
    if first {
        spans.push(Span::styled(
            format!("{} ", glyphs::disclosure_open()),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    } else if let Some(background) = component_line.panel_background {
        spans.push(Span::styled("  ", Style::default().bg(background)));
    } else {
        spans.push(Span::raw("  "));
    }
    if let Some(background) = component_line.panel_background {
        for span in &mut component_line.content.spans {
            span.style = span.style.bg(background);
        }
    }
    spans.extend(component_line.content.spans);
    let mut line = Line::from(spans);
    if let Some(background) = component_line.panel_background {
        let used = line.width();
        if used < width {
            line.spans.push(Span::styled(
                " ".repeat(width - used),
                Style::default().bg(background),
            ));
        }
    } else if !first && line.spans.len() == usize::from(!prefix.is_empty()) + 1 {
        line.spans
            .push(Span::styled("", Style::default().fg(theme.text_primary)));
    }
    line
}

fn render_execute_tool_call(
    lines: &mut Vec<Line<'static>>,
    block: &DshRenderBlock,
    theme: Theme,
    indent: usize,
    width: usize,
) -> bool {
    let Some(execute) = project_execute_tool(block) else {
        return false;
    };
    let finish = match block {
        DshRenderBlock::ToolCall {
            result: Some(result),
            ..
        } if result.is_error => DshRenderFinish::Failed,
        DshRenderBlock::ToolCall {
            result: Some(_), ..
        } => DshRenderFinish::Completed,
        _ => DshRenderFinish::Running,
    };
    let prefix = " ".repeat(indent.saturating_mul(2));
    let component_width = width.saturating_sub(prefix.len()).saturating_sub(2).max(1);
    let context = ExecuteBlockContext::new(
        ExecuteDisplayMode::Expanded,
        finish == DshRenderFinish::Running,
        component_width,
        &theme,
    );
    let accent = if finish == DshRenderFinish::Failed {
        theme.accent_error
    } else {
        theme.gray
    };
    for (index, component_line) in execute.output(&context).lines.into_iter().enumerate() {
        lines.push(paint_execute_component_line(
            component_line,
            &prefix,
            index == 0,
            accent,
            width,
            theme,
        ));
    }
    true
}

fn render_tool_call(
    lines: &mut Vec<Line<'static>>,
    block: &DshRenderBlock,
    theme: Theme,
    indent: usize,
    width: usize,
) {
    if render_execute_tool_call(lines, block, theme, indent, width) {
        return;
    }
    let DshRenderBlock::ToolCall {
        arguments,
        edit,
        view,
        result,
        ..
    } = block
    else {
        return;
    };
    let prefix = " ".repeat(indent.saturating_mul(2));
    let finish = match result {
        Some(result) if result.is_error => DshRenderFinish::Failed,
        Some(_) => DshRenderFinish::Completed,
        None => DshRenderFinish::Running,
    };
    lines.push(tool_summary_line(
        &prefix,
        glyphs::disclosure_open(),
        &tool_header_text(block),
        tool_accent(block, finish, theme),
        theme,
    ));

    match view {
        // Terminal cards are always projected into ExecuteToolCallBlock by
        // `render_execute_tool_call` above. Keeping this arm data-only makes a
        // future enum extension exhaustive without reintroducing a second
        // terminal renderer here.
        Some(DshToolCallView::Terminal { .. }) => {}
        Some(DshToolCallView::Diff { .. }) => {
            if let Some(diffs) = tool_diffs(view.as_ref(), result.as_deref()) {
                for diff in diffs {
                    render_diff(
                        lines,
                        Some(&diff.path),
                        diff.old_text.as_deref().unwrap_or(""),
                        &diff.new_text,
                        theme,
                        indent + 1,
                    );
                }
            }
        }
        Some(DshToolCallView::Generic {
            raw_input, content, ..
        }) => {
            if let Some(raw_input) = raw_input {
                let raw = raw_input.as_str().map(str::to_string).unwrap_or_else(|| {
                    serde_json::to_string_pretty(raw_input)
                        .unwrap_or_else(|_| raw_input.to_string())
                });
                push_panel_lines(
                    lines,
                    &raw,
                    &format!("{prefix}  "),
                    theme.text_secondary,
                    theme,
                );
            }
            render_tool_children(lines, content, theme, indent + 1, width);
            match result.as_ref().and_then(|result| result.view.as_ref()) {
                Some(DshToolResultView::Generic { content, .. }) => {
                    render_tool_children(lines, content, theme, indent + 1, width)
                }
                Some(DshToolResultView::Read {
                    path,
                    lines: read_lines,
                    total_lines,
                    ..
                }) => {
                    let number_width = read_lines
                        .last()
                        .map(|line| line.number.to_string().len())
                        .unwrap_or(1);
                    for line in read_lines {
                        push_panel_lines(
                            lines,
                            &format!(
                                "{:>width$} │ {}",
                                line.number,
                                line.text,
                                width = number_width
                            ),
                            &format!("{prefix}  "),
                            theme.text_secondary,
                            theme,
                        );
                    }
                    lines.push(Line::from(Span::styled(
                        format!(
                            "{prefix}  {path} · {} of {total_lines} lines",
                            read_lines.len()
                        ),
                        Style::default().fg(theme.gray_dim),
                    )));
                }
                Some(DshToolResultView::SearchMatches {
                    files,
                    truncated,
                    total,
                    ..
                }) => {
                    for file in files {
                        lines.push(Line::from(Span::styled(
                            format!("{prefix}  {}", file.path),
                            Style::default().fg(theme.path).add_modifier(Modifier::BOLD),
                        )));
                        for matched in &file.matches {
                            push_panel_lines(
                                lines,
                                &format!("{} │ {}", matched.line_number, matched.line),
                                &format!("{prefix}    "),
                                theme.text_secondary,
                                theme,
                            );
                        }
                    }
                    if *truncated {
                        lines.push(Line::from(Span::styled(
                            format!("{prefix}  showing capped results · {total} total"),
                            Style::default().fg(theme.warning),
                        )));
                    }
                }
                Some(DshToolResultView::SearchPaths {
                    paths,
                    truncated,
                    total,
                    ..
                }) => {
                    for path in paths {
                        lines.push(Line::from(Span::styled(
                            format!("{prefix}  {path}"),
                            Style::default().fg(theme.path),
                        )));
                    }
                    if *truncated {
                        lines.push(Line::from(Span::styled(
                            format!("{prefix}  showing {} of {total} paths", paths.len()),
                            Style::default().fg(theme.warning),
                        )));
                    }
                }
                Some(DshToolResultView::WebSearch {
                    sources,
                    answer,
                    truncated,
                    ..
                }) => {
                    if let Some(answer) = answer {
                        push_plain_lines(
                            lines,
                            answer,
                            Style::default().fg(theme.text_secondary),
                            &format!("{prefix}  "),
                        );
                    }
                    for (index, source) in sources.iter().enumerate() {
                        let title = source.title.as_deref().unwrap_or(&source.url);
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{prefix}  [{}] ", index + 1),
                                Style::default().fg(theme.gray_dim),
                            ),
                            Span::styled(title.to_string(), Style::default().fg(theme.link_fg)),
                        ]));
                        if source.title.is_some() {
                            lines.push(Line::from(Span::styled(
                                format!("{prefix}      {}", source.url),
                                Style::default().fg(theme.gray_dim),
                            )));
                        }
                    }
                    if *truncated {
                        lines.push(Line::from(Span::styled(
                            format!("{prefix}  source list truncated"),
                            Style::default().fg(theme.warning),
                        )));
                    }
                }
                Some(DshToolResultView::WebFetch {
                    url,
                    status_code,
                    truncated,
                    ..
                }) => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{prefix}  HTTP {status_code} · "),
                            Style::default().fg(theme.gray_dim),
                        ),
                        Span::styled(url.clone(), Style::default().fg(theme.link_fg)),
                        Span::styled(
                            if *truncated { " · truncated" } else { "" },
                            Style::default().fg(theme.warning),
                        ),
                    ]));
                    if let Some(result) = result {
                        render_tool_children(lines, &result.blocks, theme, indent + 1, width);
                    }
                }
                Some(DshToolResultView::Diff { diffs, .. }) => {
                    for diff in diffs {
                        render_diff(
                            lines,
                            Some(&diff.path),
                            diff.old_text.as_deref().unwrap_or(""),
                            &diff.new_text,
                            theme,
                            indent + 1,
                        );
                    }
                }
                Some(DshToolResultView::Terminal { .. }) => {}
                None => {
                    if let Some(result) = result {
                        render_tool_children(lines, &result.blocks, theme, indent + 1, width);
                    }
                }
            }
        }
        None => {
            if !arguments.is_empty() {
                push_panel_lines(lines, arguments, &format!("{prefix}  "), theme.gray, theme);
            }
            if let Some(edit) = edit {
                render_diff(
                    lines,
                    edit.path.as_deref(),
                    &edit.old_text,
                    &edit.new_text,
                    theme,
                    indent + 1,
                );
            }
            if let Some(result) = result {
                render_tool_children(lines, &result.blocks, theme, indent + 1, width);
            }
        }
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
        DshRenderBlock::ToolCall { .. } => render_tool_call(lines, block, theme, indent, width),
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
    use dsh_pager::{
        DshRenderBlock, DshRenderContent, DshRenderEntryId, DshRenderFinish, DshRenderVisibility,
    };
    use dsh_pager_protocol::{HistoryEntry, SessionEvent};
    use serde_json::json;

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
        let rendered = render_row(&row, *Theme::current())
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("src/lib.rs"));
        assert!(rendered.contains("- old"));
        assert!(rendered.contains("+ new"));
        assert!(copy_row(&row).contains("diff src/lib.rs"));
    }

    #[test]
    fn materialized_tool_lines_keep_semantic_roles() {
        let theme = *Theme::current();
        assert_eq!(
            style_for_paint(DshRenderKind::ToolCall, false, "▸ edit", theme).fg,
            Some(theme.gray_bright)
        );
        assert_eq!(
            style_for_paint(DshRenderKind::Error, false, "✗ result", theme).fg,
            Some(theme.accent_user)
        );
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
        let rich = RichTranscript::new(std::slice::from_ref(&user), 80, theme);
        let lines = rich.visible_lines(0, 20);
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
        let tool_lines = RichTranscript::new(&[tool], 80, theme).visible_lines(0, 20);
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
        let rendered = render_row(&assistant, theme);
        let text = rendered.iter().map(Line::to_string).collect::<Vec<_>>();
        assert_eq!(text[0], "Title");
        assert!(!text.iter().any(|line| line == "Assistant"));
        assert!(text.iter().any(|line| line == "Title"));
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
        let user_lines = render_row(&user, theme);
        assert_eq!(user_lines.len(), 1);
        assert_eq!(user_lines[0].to_string(), "❯ hello");
        assert!(!user_lines.iter().any(|line| line.to_string() == "You"));
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
        let rich = RichTranscript::new(std::slice::from_ref(&entry), 80, theme);
        let initial = rich.visible_lines(0, 20);
        assert!(initial.iter().any(|line| line.copy_text == "◆ Thinking…"));
        assert!(initial.iter().any(|line| line.copy_text == "› shell"));
        assert!(initial.iter().any(|line| line.copy_text == "final answer"));
        assert!(
            initial
                .iter()
                .any(|line| line.copy_text.contains("hidden thought"))
        );
        assert!(!initial.iter().any(|line| line.copy_text.contains("cmd")));
        assert!(
            initial
                .iter()
                .filter(|line| line.block_index.is_some())
                .filter(|line| {
                    line.copy_text == "◆ Thinking…" || line.copy_text == "› shell"
                })
                .all(|line| line.rail)
        );
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
            header.line.spans.first().and_then(|span| span.style.fg),
            Some(Theme::current().gray)
        );
        assert!(
            header
                .line
                .spans
                .iter()
                .skip(1)
                .all(|span| { span.style.fg == Some(Theme::current().gray) })
        );
    }

    #[test]
    fn rail_wave_cycle_scales_with_length_at_fixed_row_speed() {
        assert!((wave_cycle_seconds(1) - 2.75).abs() < f64::EPSILON);
        assert!((wave_cycle_seconds(4) - 3.5).abs() < f64::EPSILON);
        assert!((wave_cycle_seconds(12) - 5.5).abs() < f64::EPSILON);
        assert!((wave_cycle_seconds(24) - 8.5).abs() < f64::EPSILON);

        let row_zero_peak = wave_brightness(625, 0, 24);
        let row_eight_peak_two_seconds_later = wave_brightness(2_625, 8, 24);
        assert!(row_zero_peak > 0.999);
        assert!(row_eight_peak_two_seconds_later > 0.999);
    }

    #[test]
    fn running_rail_geometry_uses_its_contiguous_rendered_length() {
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
            .filter(|line| line.rail_animated)
            .collect::<Vec<_>>();
        let expected_len = animated.len().min(u16::MAX as usize) as u16;
        assert!(
            expected_len >= 4,
            "header and reasoning rows should animate"
        );
        assert!(
            animated
                .iter()
                .all(|line| line.rail_wave_len == expected_len)
        );
        assert_eq!(
            animated
                .iter()
                .map(|line| line.rail_wave_row)
                .collect::<Vec<_>>(),
            (0..expected_len).collect::<Vec<_>>()
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
        pane.set_wave_elapsed(Duration::ZERO);
        let first = pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .find(|line| line.rail_animated)
            .expect("running rail");
        pane.set_wave_elapsed(Duration::from_millis(313));
        let second = pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .find(|line| line.rail_animated)
            .expect("running rail after elapsed time");
        assert_eq!(first.rail_wave_row, 0);
        assert_eq!(first.rail_wave_len, 1);
        assert_ne!(
            first.line.spans.first().and_then(|span| span.style.fg),
            second.line.spans.first().and_then(|span| span.style.fg),
            "running rail should be recolored by monotonic elapsed time"
        );
        assert_ne!(
            first.line.spans.first().and_then(|span| span.style.fg),
            Some(theme.accent_running),
            "running tools use a neutral gray-to-white wave, not the legacy purple accent"
        );
        assert_eq!(
            first
                .line
                .spans
                .iter()
                .skip(1)
                .map(|span| span.style.fg)
                .collect::<Vec<_>>(),
            second
                .line
                .spans
                .iter()
                .skip(1)
                .map(|span| span.style.fg)
                .collect::<Vec<_>>(),
            "running right-side text must not inherit the rail wave phase"
        );
        assert!(
            first
                .line
                .spans
                .iter()
                .skip(1)
                .all(|span| { span.style.fg == Some(theme.text_primary) })
        );

        pane.set_selected_target(Some(HitTarget::TranscriptEntry(DshRenderEntryId::Event {
            seq: 40,
        })));
        let selected = pane
            .visible_lines(&mut scrollback, 0, 20)
            .into_iter()
            .find(|line| line.rail_animated && line.header)
            .expect("selected running header");
        assert_eq!(
            selected.line.spans.first().and_then(|span| span.style.fg),
            Some(theme.text_primary)
        );
        assert!(
            selected
                .line
                .spans
                .iter()
                .skip(1)
                .all(|span| { span.style.fg == Some(theme.text_primary) })
        );

        let mut failed = selected.clone();
        failed.rail_animated = false;
        failed.rail_flash = false;
        failed.rail_accent = Some(theme.accent_error);
        apply_dynamic_accent(&mut failed, 0, theme, None, None, true);
        assert!(
            failed
                .line
                .spans
                .iter()
                .all(|span| { span.style.fg == Some(theme.accent_error) })
        );
    }

    #[test]
    fn finish_flash_is_bounded_to_grok_window() {
        assert!(finish_flash_active(Some(1_000), Some(1_399)));
        assert!(!finish_flash_active(Some(1_000), Some(1_400)));
        assert!(!finish_flash_active(Some(1_000), Some(900)));
    }

    #[test]
    fn rich_transcript_uses_structured_blocks_and_stable_identity() {
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
        let rich = RichTranscript::new(&[entry], 80, *Theme::current());
        let lines = rich.visible_lines(0, 20);
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
        let anchor = rich.anchor_at(1).expect("anchor");
        assert_eq!(rich.scroll_for_anchor(anchor), Some(1));
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
        let rich = RichTranscript::new(&[entry], 24, *Theme::current());
        let lines = rich.visible_lines(0, 20);

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
    fn rich_transcript_wraps_unicode_without_losing_lines() {
        let mut row = row();
        row.content = DshRenderContent {
            blocks: vec![DshRenderBlock::Markdown {
                text: "中文abcdef".into(),
            }],
            fallback: "中文abcdef".into(),
        };
        let rich = RichTranscript::new(
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
        assert!(rich.total_height() > 1);
        let visible_text = rich
            .visible_lines(0, 20)
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
        let rich = RichTranscript::new(&entries, 80, *Theme::current());
        let rendered = rich
            .visible_lines(0, 20)
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!rendered.contains("secret system prompt"));
        assert!(rendered.contains("repository files"));
        assert_eq!(rich.entries.len(), 1);
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
        let rendered = render_row(&row, *Theme::current())
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "markdown",
            "thinking",
            "▾ shell",
            "✗ result",
            "output",
            "[image: image/png]",
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
            render_row(&row, theme)
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
        assert!(terminal_text.contains("▾ Run show workspace"));
        assert!(!terminal_text.contains('⌄'));
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
        assert!(read_text.contains("7 │ fn main() {}"));
        assert!(read_text.contains("1 of 42 lines"));

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
        assert!(search_text.contains("9 │ // TODO"));
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
        let rich = RichTranscript::new(&entries, 80, theme);
        let user_lines = rich
            .visible_lines(0, 20)
            .into_iter()
            .filter(|line| line.entry_id == DshRenderEntryId::Event { seq: 10 })
            .collect::<Vec<_>>();
        let agent_lines = rich
            .visible_lines(0, 20)
            .into_iter()
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
        assert!(
            !collapsed
                .iter()
                .any(|line| line.copy_text.contains("Search TODO"))
        );
        assert_eq!(scrollback.layout(80).entries[1].height, 0);

        assert!(pane.toggle_fold_or_group(DshRenderEntryId::Event { seq: 20 }));
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
                .any(|line| line.copy_text.contains("Search TODO"))
        );
        assert!(expanded.iter().all(|line| line.rail));
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
                .any(|line| line.copy_text.contains("Read · a.rs"))
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
            "› Run retry jobs and inspect recent worker logs"
        );
        assert!(!summary.copy_text.contains("node worker.js"));
        assert!(summary.line.to_string().starts_with("┃  › Run "));
        assert!(summary.rail_animated);
        assert!(
            summary
                .line
                .spans
                .iter()
                .any(|span| { span.content.contains('›') && span.style.fg.is_some() })
        );
        assert!(summary.line.spans.iter().any(|span| {
            span.content == "Run " && span.style.add_modifier.contains(Modifier::BOLD)
        }));
        assert!(summary.line.spans.iter().any(|span| {
            span.content == "retry jobs and inspect recent worker logs"
                && !span.style.add_modifier.contains(Modifier::BOLD)
        }));

        assert!(pane.toggle_fold_or_group(id));
        pane.sync(&mut scrollback, 80, theme);
        let expanded = pane.visible_lines(&mut scrollback, 0, 20);
        let expanded_text = expanded
            .iter()
            .map(|line| line.copy_text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(expanded_text.contains("▾ Run retry jobs and inspect recent worker logs"));
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
                .any(|line| { line.group_header && line.copy_text.contains("Read · src/lib.rs") })
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
}
