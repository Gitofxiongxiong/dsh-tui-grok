//! DSH-owned host/Fenwick adapter for the Grok-derived scrollback renderer.
//!
//! This module owns production revision synchronization, viewport materialization,
//! fold/group state and direct Buffer painting. Canonical history remains owned
//! by `dsh_pager::Scrollback`; semantic block materialization is temporarily
//! provided by `views::transcript` until the S7 oracle deletion.

use std::collections::{HashMap, HashSet};

use dsh_pager::scrollback::Scrollback;
use dsh_pager::{
    DshInteraction, DshRenderBlock, DshRenderEntry, DshRenderEntryId, DshRenderEntryRef,
    DshRenderFinish, DshRenderKind, DshRenderVisibility, ScrollAnchor,
};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use crate::{
    appearance::{GrokAppearanceSnapshot, ScrollbackAppearance},
    geometry::HitTarget,
    scrollback::{
        entry_renderer::{DynamicAccentSpec, EntryRenderer, RenderedEntryLine, TimestampPaint},
        render::RenderWindow,
        types::DisplayMode,
    },
    scrollback_adapter::{project_groups::project_groups, tick::GROK_WAVE_SPEED},
    theme::Theme,
    views::transcript::{
        RichPaintLine, default_display_mode, default_display_mode_ref, finish_flash_active,
        is_local_foldable_block, now_epoch_ms, semantic_lines,
    },
};

#[derive(Debug, Clone)]
struct CachedPaneEntry {
    entry: DshRenderEntry,
    lines: Vec<RichPaintLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionInfo {
    pub(crate) mode: DisplayMode,
    pub(crate) group_anchor: Option<DshRenderEntryId>,
    pub(crate) group_header: bool,
    pub(crate) group_hidden: bool,
    pub(crate) group_expanded: bool,
    pub(crate) group_last_visible: bool,
    pub(crate) group_label: Option<String>,
    pub(crate) group_running: bool,
    pub(crate) group_failed: bool,
    pub(crate) rail: bool,
    pub(crate) background: Option<Color>,
}

impl ProjectionInfo {
    pub(crate) fn plain(entry: &DshRenderEntry, width: usize, theme: Theme) -> Self {
        Self {
            mode: default_display_mode(entry, width, theme),
            group_anchor: None,
            group_header: false,
            group_hidden: false,
            group_expanded: false,
            group_last_visible: false,
            group_label: None,
            group_running: false,
            group_failed: false,
            rail: entry.kind == DshRenderKind::ToolCall,
            background: (entry.kind == DshRenderKind::User).then_some(theme.bg_light),
        }
    }

    fn plain_ref(entry: DshRenderEntryRef<'_>, width: usize, theme: Theme) -> Self {
        Self {
            mode: default_display_mode_ref(entry, width),
            group_anchor: None,
            group_header: false,
            group_hidden: false,
            group_expanded: false,
            group_last_visible: false,
            group_label: None,
            group_running: false,
            group_failed: false,
            rail: entry.kind == DshRenderKind::ToolCall,
            background: (entry.kind == DshRenderKind::User).then_some(theme.bg_light),
        }
    }
}

/// Instrumentation for proving the distinction between a cold semantic
/// revision scan and unchanged viewport-bounded paint frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostPaneStats {
    pub revision_syncs: usize,
    pub scanned_entries: usize,
    pub materialized_entries: usize,
    pub painted_lines: usize,
}

/// Production scrollback adapter.
///
/// DSH `Scrollback` owns entry identity, partial replacement, height indexing
/// and anchor restoration.  This cache owns only the Grok semantic block lines
/// for entries that are currently known.  It reports those actual heights back
/// to the DSH index and then paints through the shared `ScrollbackLayout`.
#[derive(Debug)]
pub struct DshScrollbackHost {
    width: usize,
    show_timestamps: bool,
    wave_tick: u64,
    appearance: ScrollbackAppearance,
    entries: HashMap<DshRenderEntryId, CachedPaneEntry>,
    expanded_entries: HashSet<DshRenderEntryId>,
    expanded_blocks: HashSet<(DshRenderEntryId, usize)>,
    foldable_entries: HashSet<DshRenderEntryId>,
    foldable_blocks: HashSet<(DshRenderEntryId, usize)>,
    expanded_groups: HashSet<DshRenderEntryId>,
    projections: HashMap<DshRenderEntryId, ProjectionInfo>,
    selected_target: Option<HitTarget>,
    pending_user_input: bool,
    pending_call_id: Option<String>,
    pending_entry: Option<DshRenderEntryId>,
    host_revision: Option<u64>,
    semantic_dirty: bool,
    has_visible_entries: bool,
    theme: Theme,
    stats: HostPaneStats,
}

impl Default for DshScrollbackHost {
    fn default() -> Self {
        let theme = Theme::default();
        Self {
            width: 0,
            show_timestamps: true,
            wave_tick: 0,
            appearance: GrokAppearanceSnapshot::default().scrollback(theme),
            entries: HashMap::new(),
            expanded_entries: HashSet::new(),
            expanded_blocks: HashSet::new(),
            foldable_entries: HashSet::new(),
            foldable_blocks: HashSet::new(),
            expanded_groups: HashSet::new(),
            projections: HashMap::new(),
            selected_target: None,
            pending_user_input: false,
            pending_call_id: None,
            pending_entry: None,
            host_revision: None,
            semantic_dirty: true,
            has_visible_entries: false,
            theme,
            stats: HostPaneStats::default(),
        }
    }
}

impl DshScrollbackHost {
    pub fn clear(&mut self) {
        self.width = 0;
        self.show_timestamps = true;
        self.wave_tick = 0;
        self.entries.clear();
        self.expanded_entries.clear();
        self.expanded_blocks.clear();
        self.foldable_entries.clear();
        self.foldable_blocks.clear();
        self.expanded_groups.clear();
        self.projections.clear();
        self.selected_target = None;
        self.pending_user_input = false;
        self.pending_call_id = None;
        self.pending_entry = None;
        self.host_revision = None;
        self.semantic_dirty = true;
        self.has_visible_entries = false;
        self.stats = HostPaneStats::default();
    }

    pub fn sync(&mut self, scrollback: &mut Scrollback, width: usize, theme: Theme) {
        self.sync_with_options(scrollback, width, theme, true);
    }

    /// Set the time-derived Grok animation tick without invalidating the
    /// semantic-line cache. Running accents are recolored when visible lines
    /// materialize.
    pub fn set_wave_tick(&mut self, tick: u64) {
        self.wave_tick = tick;
    }

    /// Pass the runtime's current transcript hit target into the projection.
    /// Grok uses the same selection state to lift a muted block back to its
    /// primary (white) foreground; keeping it here also makes the paint path
    /// independent of the runtime's hit-map implementation.
    pub fn set_selected_target(&mut self, target: Option<HitTarget>) {
        self.selected_target = target;
    }

    /// Project the host-authoritative interaction onto the exact transcript
    /// entry whose running chrome Grok freezes while waiting for the user.
    pub fn set_pending_interaction(&mut self, interaction: Option<&DshInteraction>) {
        let pending_user_input = interaction.is_some();
        let pending_call_id = interaction.and_then(|interaction| match interaction {
            DshInteraction::Approval { call_id, .. } => call_id.clone(),
            DshInteraction::Question { .. } => None,
        });
        if self.pending_user_input != pending_user_input || self.pending_call_id != pending_call_id
        {
            self.pending_user_input = pending_user_input;
            self.pending_call_id = pending_call_id;
            self.semantic_dirty = true;
        }
    }

    pub fn sync_with_options(
        &mut self,
        scrollback: &mut Scrollback,
        width: usize,
        theme: Theme,
        show_timestamps: bool,
    ) {
        let appearance = GrokAppearanceSnapshot::default().scrollback(theme);
        self.sync_with_appearance(scrollback, width, theme, show_timestamps, appearance);
    }

    pub fn sync_with_appearance(
        &mut self,
        scrollback: &mut Scrollback,
        width: usize,
        theme: Theme,
        show_timestamps: bool,
        appearance: ScrollbackAppearance,
    ) {
        let width = width.max(1);
        let options_changed = self.width != width
            || self.show_timestamps != show_timestamps
            || self.appearance != appearance;
        self.width = width;
        self.show_timestamps = show_timestamps;
        self.appearance = appearance;
        self.theme = theme;
        let revision = scrollback.revision();
        if !options_changed && !self.semantic_dirty && self.host_revision == Some(revision) {
            return;
        }
        if !options_changed
            && !self.semantic_dirty
            && let Some(known_revision) = self.host_revision
            && self.try_incremental_revision_sync(scrollback, known_revision)
        {
            return;
        }

        self.entries.clear();
        let entries = scrollback.render_entry_refs().collect::<Vec<_>>();
        self.prune_local_state(&entries);
        self.foldable_entries = entries
            .iter()
            .copied()
            .filter(|entry| is_foldable_ref(*entry))
            .map(|entry| entry.id)
            .collect();
        self.foldable_blocks = entries
            .iter()
            .filter(|entry| entry.kind == DshRenderKind::Assistant)
            .flat_map(|entry| {
                entry
                    .content
                    .blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, block)| is_local_foldable_block(block))
                    .map(|(index, _)| (entry.id, index))
            })
            .collect();
        self.pending_entry = self.resolve_pending_entry(&entries);
        self.projections = entry_projections(
            &entries,
            width,
            theme,
            &self.expanded_entries,
            &self.expanded_groups,
            self.pending_entry,
        );
        let mut heights = Vec::with_capacity(entries.len());
        self.has_visible_entries = false;
        for entry in entries.iter().copied() {
            let projection = self
                .projections
                .get(&entry.id)
                .cloned()
                .unwrap_or_else(|| ProjectionInfo::plain_ref(entry, width, theme));
            let height = estimated_projected_height(entry, &projection, width);
            self.has_visible_entries |= height > 0;
            heights.push(height);
        }
        drop(entries);
        for (entry_idx, height) in heights.into_iter().enumerate() {
            scrollback.set_projected_height(width, entry_idx, height);
        }
        self.host_revision = Some(revision);
        self.semantic_dirty = false;
        self.stats.revision_syncs = self.stats.revision_syncs.saturating_add(1);
        self.stats.scanned_entries = self
            .stats
            .scanned_entries
            .saturating_add(scrollback.entries().len());
    }

    /// Consume a host-reported in-place change without rescanning unrelated
    /// history. Adjacency-sensitive group/pending entries deliberately fall
    /// back to the complete projection path above.
    fn try_incremental_revision_sync(
        &mut self,
        scrollback: &mut Scrollback,
        known_revision: u64,
    ) -> bool {
        let Some(delta) = scrollback.content_delta_since(known_revision) else {
            return false;
        };
        if delta.topology_changed
            || delta.entries.is_empty()
            || delta.entries.end > scrollback.entries().len()
            || self.pending_user_input
        {
            return false;
        }

        for entry_idx in delta.entries.clone() {
            let Some(entry) = scrollback.render_entry_ref(entry_idx) else {
                return false;
            };
            if self
                .projections
                .get(&entry.id)
                .is_none_or(|projection| projection.group_anchor.is_some())
                || !is_group_break_ref(entry)
            {
                return false;
            }
        }

        for entry_idx in delta.entries.clone() {
            let (entry_id, projection, height, foldable, foldable_blocks) = {
                let Some(entry) = scrollback.render_entry_ref(entry_idx) else {
                    return false;
                };
                let mut projection = ProjectionInfo::plain_ref(entry, self.width, self.theme);
                let foldable = is_foldable_ref(entry);
                if foldable && self.expanded_entries.contains(&entry.id) {
                    projection.mode = DisplayMode::Expanded;
                }
                let foldable_blocks = entry
                    .content
                    .blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, block)| is_local_foldable_block(block))
                    .map(|(index, _)| index)
                    .collect::<HashSet<_>>();
                let height = estimated_projected_height(entry, &projection, self.width);
                (entry.id, projection, height, foldable, foldable_blocks)
            };

            self.entries.remove(&entry_id);
            self.projections.insert(entry_id, projection);
            if foldable {
                self.foldable_entries.insert(entry_id);
            } else {
                self.foldable_entries.remove(&entry_id);
            }
            self.foldable_blocks
                .retain(|(candidate, _)| *candidate != entry_id);
            self.expanded_blocks.retain(|(candidate, index)| {
                *candidate != entry_id || foldable_blocks.contains(index)
            });
            self.foldable_blocks
                .extend(foldable_blocks.into_iter().map(|index| (entry_id, index)));
            scrollback.set_projected_height(self.width, entry_idx, height);
            self.has_visible_entries |= height > 0;
        }

        self.host_revision = Some(delta.to_revision);
        self.stats.revision_syncs = self.stats.revision_syncs.saturating_add(1);
        self.stats.scanned_entries = self
            .stats
            .scanned_entries
            .saturating_add(delta.entries.len());
        true
    }

    fn prune_local_state(&mut self, entries: &[DshRenderEntryRef<'_>]) {
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
        let Some(projection) = self.projections.get(&entry_id).cloned() else {
            return false;
        };
        // A verb-run header is painted from the anchor's first Tool block, so
        // its hit target can carry that block index. Upstream group routing
        // wins before concrete block folding.
        if let Some(anchor) = projection.group_anchor
            && (projection.group_header || anchor == entry_id)
        {
            if !self.expanded_groups.insert(anchor) {
                self.expanded_groups.remove(&anchor);
            }
            self.entries.clear();
            self.semantic_dirty = true;
            return true;
        }
        if let Some(block_index) = block_index
            && self.foldable_blocks.contains(&(entry_id, block_index))
        {
            let key = (entry_id, block_index);
            if !self.expanded_blocks.insert(key) {
                self.expanded_blocks.remove(&key);
            }
            // The block projection is width-specific just like the upstream
            // EntryRenderer cache. Rebuild it on the next frame.
            self.entries.clear();
            self.semantic_dirty = true;
            return true;
        }
        if !self.foldable_entries.contains(&entry_id) {
            return false;
        }
        if !self.expanded_entries.insert(entry_id) {
            self.expanded_entries.remove(&entry_id);
        }
        self.entries.clear();
        self.semantic_dirty = true;
        true
    }

    pub fn is_group_header(&self, entry_id: DshRenderEntryId) -> bool {
        self.projections
            .get(&entry_id)
            .is_some_and(|projection| projection.group_header)
    }

    pub fn is_empty(&self) -> bool {
        !self.has_visible_entries
    }

    pub fn is_animating(&self) -> bool {
        let now = now_epoch_ms();
        self.entries.values().any(|entry| {
            entry.lines.iter().any(|line| {
                line.accent.is_some_and(|accent| accent.animated)
                    || line.bullet.is_some_and(|bullet| bullet.animated)
                    || (line.accent_flash && finish_flash_active(entry.entry.finished_at_ms, now))
            })
        })
    }

    pub fn total_height(&mut self, scrollback: &mut Scrollback) -> usize {
        scrollback.total_height(self.width.max(1))
    }

    pub fn stats(&self) -> HostPaneStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = HostPaneStats::default();
    }

    #[cfg(test)]
    pub(crate) fn cached_entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn anchor_at(
        &mut self,
        scrollback: &mut Scrollback,
        scroll_top: usize,
    ) -> Option<ScrollAnchor> {
        let total_height = scrollback.total_height(self.width.max(1));
        let top = scroll_top.min(total_height.checked_sub(1)?);
        let window = scrollback.paint_window(self.width.max(1), top, 1, 0);
        let entry_idx = window.entries.start;
        let item = scrollback.entry_layout(self.width.max(1), entry_idx)?;
        if item.height == 0 {
            return None;
        }
        let entry = scrollback.render_entry_ref(entry_idx)?;
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
        let item = scrollback.entry_layout(self.width.max(1), entry_idx)?;
        Some(
            item.start_y
                .saturating_add(anchor.intra_row.min(item.height.saturating_sub(1))),
        )
    }

    /// Materialize only the viewport and one viewport of overscan on either
    /// side. Height corrections preserve a stable top entry, while follow
    /// mode re-pins to the final row after every correction.
    pub fn prepare_viewport(
        &mut self,
        scrollback: &mut Scrollback,
        scroll_top: usize,
        viewport_height: u16,
        follow: bool,
    ) -> usize {
        let viewport_height = viewport_height as usize;
        if viewport_height == 0 || self.is_empty() {
            self.entries.clear();
            return scroll_top;
        }
        let anchor = (!follow)
            .then(|| self.anchor_at(scrollback, scroll_top))
            .flatten();
        let mut top = if follow {
            self.total_height(scrollback)
                .saturating_sub(viewport_height)
        } else {
            scroll_top
        };
        let mut keep_ids = HashSet::new();

        // Every changing pass converts at least one estimate in the bounded
        // window to an exact rich-renderer height.
        loop {
            let window =
                scrollback.paint_window(self.width.max(1), top, viewport_height, viewport_height);
            if window.entries.is_empty() {
                break;
            }
            let (changed, ids) = self.materialize_range(scrollback, window.entries);
            keep_ids = ids;
            let next_top = if follow {
                self.total_height(scrollback)
                    .saturating_sub(viewport_height)
            } else {
                anchor
                    .and_then(|anchor| self.scroll_for_anchor(scrollback, anchor))
                    .unwrap_or(top)
            };
            if !changed && next_top == top {
                break;
            }
            top = next_top;
        }
        self.entries.retain(|id, _| keep_ids.contains(id));
        top
    }

    fn materialize_range(
        &mut self,
        scrollback: &mut Scrollback,
        range: std::ops::Range<usize>,
    ) -> (bool, HashSet<DshRenderEntryId>) {
        let mut changed = false;
        let mut keep_ids = HashSet::with_capacity(range.len());
        for entry_idx in range {
            let Some(entry_ref) = scrollback.render_entry_ref(entry_idx) else {
                continue;
            };
            let entry_id = entry_ref.id;
            keep_ids.insert(entry_id);
            let projection =
                self.projections.get(&entry_id).cloned().unwrap_or_else(|| {
                    ProjectionInfo::plain_ref(entry_ref, self.width, self.theme)
                });
            if entry_ref.visibility == DshRenderVisibility::Hidden || projection.group_hidden {
                changed |= scrollback.set_projected_height(self.width, entry_idx, 0);
                continue;
            }
            if !self.entries.contains_key(&entry_id) {
                let entry = entry_ref.to_owned();
                let lines = semantic_lines(
                    &entry,
                    self.width,
                    self.theme,
                    &projection,
                    &self.expanded_blocks,
                    self.show_timestamps,
                    &self.appearance,
                );
                self.entries
                    .insert(entry_id, CachedPaneEntry { entry, lines });
                self.stats.materialized_entries = self.stats.materialized_entries.saturating_add(1);
            }
            let cached = self
                .entries
                .get(&entry_id)
                .expect("materialized entry inserted");
            let height = cached
                .lines
                .len()
                .saturating_add(if projection.group_anchor.is_some() {
                    usize::from(projection.group_last_visible)
                } else {
                    1
                });
            changed |= scrollback.set_projected_height(self.width, entry_idx, height);
        }
        (changed, keep_ids)
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
        let top = self.prepare_viewport(scrollback, scroll_top, viewport_height, false);
        let host_window =
            scrollback.paint_window(self.width.max(1), top, viewport_height as usize, 0);
        let window = RenderWindow::new(
            host_window.entries,
            host_window.content_y0,
            host_window.total_height,
            top,
        );
        let mut painted = Vec::new();
        for entry_idx in window.entries {
            let Some(entry_id) = scrollback.render_entry_ref(entry_idx).map(|entry| entry.id)
            else {
                continue;
            };
            let Some(layout) = scrollback.entry_layout(self.width.max(1), entry_idx) else {
                continue;
            };
            if layout.height == 0 {
                continue;
            }
            let Some(cached) = self.entries.get(&entry_id) else {
                continue;
            };
            for line in &cached.lines {
                let virtual_y = layout.start_y.saturating_add(line.line_index);
                let slice_y = virtual_y.saturating_sub(window.content_y0);
                if slice_y < window.skip_rows {
                    continue;
                }
                let screen_y = slice_y.saturating_sub(window.skip_rows);
                if screen_y >= viewport_height as usize {
                    break;
                }
                let mut line = line.clone();
                line.screen_y = screen_y as u16;
                line.pending_user_input = self.pending_entry == Some(entry_id);
                let selected = self
                    .selected_target
                    .as_ref()
                    .is_some_and(|target| line_matches_target(&line, target));
                let flashing = line.accent_flash
                    && finish_flash_active(cached.entry.finished_at_ms, now_epoch_ms());
                EntryRenderer::paint_dynamic(
                    &mut line.line,
                    DynamicAccentSpec {
                        tick: self.wave_tick,
                        logical_row: line.accent_wave_row,
                        wave_rows: self.appearance.animation.wave_rows,
                        wave_speed: GROK_WAVE_SPEED,
                        background: line.background.unwrap_or(Theme::current().bg_base),
                        accent: line.accent,
                        flash_accent: line.flash_accent,
                        bullet: line.bullet,
                        bullet_span: line.bullet_span,
                        selected,
                        flash: flashing,
                        pending_user_input: line.pending_user_input,
                    },
                );
                painted.push(line);
            }
        }
        self.stats.painted_lines = self.stats.painted_lines.saturating_add(painted.len());
        painted
    }

    pub fn paint_buffer_line(
        &self,
        buf: &mut Buffer,
        area: Rect,
        paint: &RichPaintLine,
        mouse_pos: Option<(u16, u16)>,
    ) -> Option<TimestampPaint> {
        let rendered = RenderedEntryLine {
            line: paint.line.clone(),
            block_index: paint.block_index,
            line_index: paint.line_index,
            header: paint.header,
            group_header: paint.group_header,
            selectable: paint.selectable,
            accent: paint.accent,
            flash_accent: paint.flash_accent,
            bullet: paint.bullet,
            accent_flash: paint.accent_flash,
            background: paint.background,
            copy_text: paint.copy_text.clone(),
            content_offset: paint.content_offset,
            content_width: paint.content_width,
            timestamp: paint.timestamp.clone(),
            bullet_span: paint.bullet_span,
            joiner_to_previous: paint.joiner_to_previous.clone(),
        };
        let selected = self
            .selected_target
            .as_ref()
            .is_some_and(|target| line_matches_target(paint, target));
        let flashing = paint.accent_flash
            && self.entries.get(&paint.entry_id).is_some_and(|entry| {
                finish_flash_active(entry.entry.finished_at_ms, now_epoch_ms())
            });
        EntryRenderer::paint_buffer_line(
            buf,
            area,
            paint.screen_y,
            &rendered,
            DynamicAccentSpec {
                tick: self.wave_tick,
                logical_row: paint.accent_wave_row,
                wave_rows: self.appearance.animation.wave_rows,
                wave_speed: GROK_WAVE_SPEED,
                background: paint.background.unwrap_or(self.theme.bg_base),
                accent: paint.accent,
                flash_accent: paint.flash_accent,
                bullet: paint.bullet,
                bullet_span: paint.bullet_span,
                selected,
                flash: flashing,
                pending_user_input: paint.pending_user_input,
            },
            &self.appearance.scrollback.layout,
            mouse_pos,
        )
    }

    fn resolve_pending_entry(&self, entries: &[DshRenderEntryRef<'_>]) -> Option<DshRenderEntryId> {
        if !self.pending_user_input {
            return None;
        }
        if let Some(call_id) = self.pending_call_id.as_deref()
            && let Some(entry) = entries.iter().rev().find(|entry| {
                entry.content.blocks.iter().any(|block| {
                    matches!(
                        block,
                        DshRenderBlock::ToolCall {
                            call_id: Some(candidate),
                            ..
                        } if candidate == call_id
                    )
                })
            })
        {
            return Some(entry.id);
        }
        entries
            .iter()
            .rev()
            .find(|entry| {
                entry.finish == DshRenderFinish::Running
                    && matches!(
                        entry.kind,
                        DshRenderKind::ToolCall
                            | DshRenderKind::Thinking
                            | DshRenderKind::Assistant
                    )
            })
            .map(|entry| entry.id)
    }
}

fn is_foldable_ref(entry: DshRenderEntryRef<'_>) -> bool {
    matches!(
        entry.kind,
        DshRenderKind::User
            | DshRenderKind::Thinking
            | DshRenderKind::ToolCall
            | DshRenderKind::ToolResult
            | DshRenderKind::Error
    ) && !entry.text.trim().is_empty()
}

/// A `Break` entry cannot join or split Grok context/verb spans. Combined
/// with the previous projection having no group anchor, an in-place update to
/// this class is safe to reproject independently.
fn is_group_break_ref(entry: DshRenderEntryRef<'_>) -> bool {
    entry.visibility != DshRenderVisibility::Hidden
        && !matches!(
            entry.kind,
            DshRenderKind::AgentContext
                | DshRenderKind::Context
                | DshRenderKind::Compaction
                | DshRenderKind::Thinking
        )
        && !entry
            .content
            .blocks
            .iter()
            .any(|block| matches!(block, DshRenderBlock::ToolCall { .. }))
}

fn estimated_projected_height(
    entry: DshRenderEntryRef<'_>,
    projection: &ProjectionInfo,
    width: usize,
) -> usize {
    if entry.visibility == DshRenderVisibility::Hidden || projection.group_hidden {
        return 0;
    }
    let base = match projection.mode {
        DisplayMode::Collapsed => 2,
        DisplayMode::Truncated => entry.estimated_height(width).min(5),
        DisplayMode::Expanded => entry.estimated_height(width),
    };
    if projection.group_anchor.is_some() {
        base.saturating_sub(1)
            .saturating_add(usize::from(projection.group_last_visible))
            .max(1)
    } else {
        base.max(1)
    }
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

fn entry_projections(
    entries: &[DshRenderEntryRef<'_>],
    width: usize,
    theme: Theme,
    expanded_entries: &HashSet<DshRenderEntryId>,
    expanded_groups: &HashSet<DshRenderEntryId>,
    pending_entry: Option<DshRenderEntryId>,
) -> HashMap<DshRenderEntryId, ProjectionInfo> {
    let mut projections = entries
        .iter()
        .copied()
        .map(|entry| {
            let mut projection = ProjectionInfo::plain_ref(entry, width, theme);
            if expanded_entries.contains(&entry.id) && is_foldable_ref(entry) {
                projection.mode = DisplayMode::Expanded;
            }
            (entry.id, projection)
        })
        .collect::<HashMap<_, _>>();
    let display_modes = projections
        .iter()
        .map(|(id, projection)| (*id, projection.mode))
        .collect::<HashMap<_, _>>();
    for (id, group) in project_groups(
        entries,
        &display_modes,
        expanded_groups,
        pending_entry,
        theme,
    ) {
        let projection = projections
            .get_mut(&id)
            .expect("group projection references a canonical entry");
        projection.group_anchor = Some(group.anchor);
        projection.group_header = group.header;
        projection.group_hidden = group.hidden;
        projection.group_expanded = group.expanded;
        projection.group_last_visible = group.last_visible;
        projection.group_label = group.label;
        projection.group_running = group.running;
        projection.group_failed = group.failed;
        projection.rail = true;
    }
    projections
}
