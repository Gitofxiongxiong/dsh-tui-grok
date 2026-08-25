use std::collections::HashMap;
use std::ops::Range;

use dsh_pager_protocol::HistoryEntry;
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

use crate::presentation::{
    DshPresentationAdapter, DshRenderContent, DshRenderEntry, DshRenderEntryId, DshRenderKind,
    DshRenderUpdate,
};
use crate::presentation::{DshRenderFinish, DshRenderRole, DshRenderVisibility};

/// Compatibility names retained at the scrollback API boundary. Presentation
/// owns the actual identity and semantic kind definitions.
pub type EntryId = DshRenderEntryId;
pub type EntryKind = DshRenderKind;

#[derive(Debug)]
struct LineCache {
    width: usize,
    lines: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct MeasuredHeight {
    width: usize,
    height: usize,
}

#[derive(Debug)]
pub struct ScrollbackEntry {
    pub id: EntryId,
    pub source_seq: i64,
    pub created_at_ms: Option<u64>,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub kind: EntryKind,
    pub text: String,
    pub partial: bool,
    pub visibility: DshRenderVisibility,
    pub finish: DshRenderFinish,
    pub group_key: Option<String>,
    pub selectable: bool,
    pub lineage: Vec<i64>,
    pub content: DshRenderContent,
    cache: Option<LineCache>,
    measured: Option<MeasuredHeight>,
}

impl ScrollbackEntry {
    fn new(entry: DshRenderEntry) -> Self {
        Self {
            id: entry.id,
            source_seq: entry.source_seq,
            created_at_ms: entry.created_at_ms,
            started_at_ms: entry.started_at_ms,
            finished_at_ms: entry.finished_at_ms,
            kind: entry.kind,
            text: entry.text,
            partial: entry.partial,
            visibility: entry.visibility,
            finish: entry.finish,
            group_key: entry.group_key,
            selectable: entry.selectable,
            lineage: entry.lineage,
            content: entry.content,
            cache: None,
            measured: None,
        }
    }

    fn set(&mut self, entry: ScrollbackEntry) -> bool {
        if self.source_seq == entry.source_seq
            && self.created_at_ms == entry.created_at_ms
            && self.started_at_ms == entry.started_at_ms
            && self.finished_at_ms == entry.finished_at_ms
            && self.kind == entry.kind
            && self.text == entry.text
            && self.partial == entry.partial
            && self.visibility == entry.visibility
            && self.finish == entry.finish
            && self.group_key == entry.group_key
            && self.selectable == entry.selectable
            && self.lineage == entry.lineage
            && self.content == entry.content
        {
            return false;
        }
        self.source_seq = entry.source_seq;
        self.created_at_ms = entry.created_at_ms;
        self.started_at_ms = entry.started_at_ms;
        self.finished_at_ms = entry.finished_at_ms;
        self.kind = entry.kind;
        self.text = entry.text;
        self.partial = entry.partial;
        self.visibility = entry.visibility;
        self.finish = entry.finish;
        self.group_key = entry.group_key;
        self.selectable = entry.selectable;
        self.lineage = entry.lineage;
        self.content = entry.content;
        self.cache = None;
        self.measured = None;
        true
    }

    fn rendered_lines(&mut self, width: usize) -> &[String] {
        let width = width.max(1);
        if self.visibility == DshRenderVisibility::Hidden {
            self.cache = Some(LineCache {
                width,
                lines: Vec::new(),
            });
            self.measured = Some(MeasuredHeight { width, height: 0 });
            return &self.cache.as_ref().expect("cache initialized").lines;
        }
        if self.cache.as_ref().is_none_or(|cache| cache.width != width) {
            let mut lines = vec![self.kind.label().to_string()];
            let body_width = width.saturating_sub(2).max(1);
            let collapsed = self.visibility == DshRenderVisibility::Collapsed;
            let display_text = if collapsed {
                self.text
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or(self.kind.label())
                    .trim()
                    .to_string()
            } else if self.content.blocks.is_empty() {
                self.text.clone()
            } else {
                self.content.display_text()
            };
            for logical_line in display_text.split('\n') {
                if logical_line.is_empty() {
                    lines.push(String::new());
                    continue;
                }
                for wrapped in dsh_pager_primitives::wrapping::word_wrap_line(
                    &Line::from(logical_line),
                    body_width,
                ) {
                    let text = wrapped
                        .spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect::<String>();
                    lines.push(format!("  {text}"));
                }
            }
            self.measured = Some(MeasuredHeight {
                width,
                height: lines.len().saturating_add(1),
            });
            self.cache = Some(LineCache { width, lines });
        }
        &self.cache.as_ref().expect("cache initialized").lines
    }

    fn measured_height(&self, width: usize) -> Option<usize> {
        self.measured
            .as_ref()
            .filter(|measured| measured.width == width.max(1))
            .map(|measured| measured.height)
    }

    /// Cheap approximate layout height used before an entry enters the
    /// viewport. It scans text widths but allocates no wrapped strings.
    fn estimated_height(&self, width: usize) -> usize {
        if self.visibility == DshRenderVisibility::Hidden {
            return 0;
        }
        if self.visibility == DshRenderVisibility::Collapsed {
            return 3;
        }
        let body_width = width.max(1).saturating_sub(2).max(1);
        let body_rows = self
            .text
            .split('\n')
            .map(|line| {
                if line.is_empty() {
                    1
                } else {
                    UnicodeWidthStr::width(line)
                        .max(1)
                        .saturating_add(body_width.saturating_sub(1))
                        / body_width
                }
            })
            .sum::<usize>();
        // One header row plus one spacer row between entries.
        body_rows.saturating_add(2)
    }

    fn semantic_eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.source_seq == other.source_seq
            && self.created_at_ms == other.created_at_ms
            && self.started_at_ms == other.started_at_ms
            && self.finished_at_ms == other.finished_at_ms
            && self.kind == other.kind
            && self.text == other.text
            && self.partial == other.partial
            && self.visibility == other.visibility
            && self.finish == other.finish
            && self.group_key == other.group_key
            && self.selectable == other.selectable
            && self.lineage == other.lineage
            && self.content == other.content
    }

    fn as_render_ref(&self) -> DshRenderEntryRef<'_> {
        DshRenderEntryRef {
            id: self.id,
            source_seq: self.source_seq,
            created_at_ms: self.created_at_ms,
            started_at_ms: self.started_at_ms,
            finished_at_ms: self.finished_at_ms,
            kind: self.kind,
            text: &self.text,
            partial: self.partial,
            visibility: self.visibility,
            finish: self.finish,
            group_key: self.group_key.as_deref(),
            selectable: self.selectable,
            lineage: &self.lineage,
            content: &self.content,
        }
    }
}

impl From<DshRenderEntry> for ScrollbackEntry {
    fn from(entry: DshRenderEntry) -> Self {
        Self::new(entry)
    }
}

/// Borrowed presentation view over one host-owned scrollback entry.
///
/// The view deliberately contains no owned text or block payload. Viewers may
/// scan identity and semantic metadata for a revision, then clone only the
/// entries that intersect their paint window through [`Self::to_owned`].
#[derive(Debug, Clone, Copy)]
pub struct DshRenderEntryRef<'a> {
    pub id: DshRenderEntryId,
    pub source_seq: i64,
    pub created_at_ms: Option<u64>,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub kind: DshRenderKind,
    pub text: &'a str,
    pub partial: bool,
    pub visibility: DshRenderVisibility,
    pub finish: DshRenderFinish,
    pub group_key: Option<&'a str>,
    pub selectable: bool,
    pub lineage: &'a [i64],
    pub content: &'a DshRenderContent,
}

impl DshRenderEntryRef<'_> {
    pub fn to_owned(self) -> DshRenderEntry {
        DshRenderEntry {
            id: self.id,
            source_seq: self.source_seq,
            created_at_ms: self.created_at_ms,
            started_at_ms: self.started_at_ms,
            finished_at_ms: self.finished_at_ms,
            kind: self.kind,
            text: self.text.to_owned(),
            partial: self.partial,
            visibility: self.visibility,
            finish: self.finish,
            group_key: self.group_key.map(str::to_owned),
            selectable: self.selectable,
            lineage: self.lineage.to_vec(),
            content: self.content.clone(),
        }
    }

    /// Allocation-free height estimate used until a richer viewer measures
    /// this entry. It intentionally matches the host's initial Fenwick value.
    pub fn estimated_height(self, width: usize) -> usize {
        if self.visibility == DshRenderVisibility::Hidden {
            return 0;
        }
        if self.visibility == DshRenderVisibility::Collapsed {
            return 3;
        }
        let body_width = width.max(1).saturating_sub(2).max(1);
        let body_rows = self
            .text
            .split('\n')
            .map(|line| {
                if line.is_empty() {
                    1
                } else {
                    UnicodeWidthStr::width(line)
                        .max(1)
                        .saturating_add(body_width.saturating_sub(1))
                        / body_width
                }
            })
            .sum::<usize>();
        body_rows.saturating_add(2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryLayout {
    pub entry_idx: usize,
    pub start_y: usize,
    pub height: usize,
}

impl EntryLayout {
    fn end_y(self) -> usize {
        self.start_y.saturating_add(self.height)
    }
}

/// Prefix-sum index for estimated and measured entry heights. A visible entry
/// can replace its estimate in O(log n), and viewport lookup remains O(log n)
/// without rewriting every later `start_y`.
#[derive(Debug, Default)]
struct HeightIndex {
    values: Vec<usize>,
    tree: Vec<usize>,
}

impl HeightIndex {
    fn rebuild(&mut self, values: Vec<usize>) {
        self.tree = vec![0; values.len().saturating_add(1)];
        self.values = values;
        for index in 0..self.values.len() {
            let value = self.values[index];
            self.add(index, value, true);
        }
    }

    fn set(&mut self, index: usize, value: usize) -> bool {
        let Some(previous) = self.values.get_mut(index) else {
            return false;
        };
        if *previous == value {
            return false;
        }
        let old = *previous;
        *previous = value;
        if value >= old {
            self.add(index, value - old, true);
        } else {
            self.add(index, old - value, false);
        }
        true
    }

    fn push(&mut self, value: usize) {
        let old_len = self.values.len();
        let tree_index = old_len.saturating_add(1);
        let range_start = tree_index.saturating_sub(tree_index.isolate_lowest_one());
        let prior_sum = self
            .prefix_sum(old_len)
            .saturating_sub(self.prefix_sum(range_start));
        self.values.push(value);
        self.tree.push(prior_sum.saturating_add(value));
    }

    fn add(&mut self, index: usize, delta: usize, increase: bool) {
        let mut tree_index = index.saturating_add(1);
        while tree_index < self.tree.len() {
            if increase {
                self.tree[tree_index] = self.tree[tree_index].saturating_add(delta);
            } else {
                self.tree[tree_index] = self.tree[tree_index].saturating_sub(delta);
            }
            tree_index = tree_index.saturating_add(tree_index.isolate_lowest_one());
        }
    }

    fn prefix_sum(&self, end: usize) -> usize {
        let mut index = end.min(self.values.len());
        let mut sum = 0usize;
        while index > 0 {
            sum = sum.saturating_add(self.tree[index]);
            index &= index - 1;
        }
        sum
    }

    fn total(&self) -> usize {
        self.prefix_sum(self.values.len())
    }

    fn start_y(&self, index: usize) -> usize {
        self.prefix_sum(index)
    }

    fn height(&self, index: usize) -> Option<usize> {
        self.values.get(index).copied()
    }

    /// Find the entry containing `virtual_y`, or `len` when it is below the
    /// indexed transcript. This is the Fenwick lower-bound operation.
    fn entry_at(&self, virtual_y: usize) -> usize {
        if self.values.is_empty() || virtual_y >= self.total() {
            return self.values.len();
        }
        let mut index = 0usize;
        let mut sum = 0usize;
        let mut bit = 1usize;
        while bit.saturating_mul(2) <= self.values.len() {
            bit = bit.saturating_mul(2);
        }
        while bit > 0 {
            let next = index.saturating_add(bit);
            if next <= self.values.len() && sum.saturating_add(self.tree[next]) <= virtual_y {
                index = next;
                sum = sum.saturating_add(self.tree[next]);
            }
            bit /= 2;
        }
        index.min(self.values.len())
    }

    fn window(&self, scroll_top: usize, viewport_height: usize) -> Range<usize> {
        if self.values.is_empty() || viewport_height == 0 || scroll_top >= self.total() {
            return 0..0;
        }
        let bottom = scroll_top.saturating_add(viewport_height).min(self.total());
        let first = self.entry_at(scroll_top);
        let last = self.entry_at(bottom.saturating_sub(1)).saturating_add(1);
        first.min(self.values.len())..last.min(self.values.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollAnchor {
    pub entry_id: EntryId,
    pub intra_row: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintLine {
    pub screen_y: u16,
    pub entry_id: EntryId,
    /// Stable line index within the entry's width-specific rendered output.
    /// This is deliberately separate from `screen_y`, which changes whenever
    /// the viewport scrolls.
    pub line_index: usize,
    pub kind: EntryKind,
    pub header: bool,
    pub text: String,
    pub viewer_role: Option<DshRenderRole>,
}

/// Read-only layout boundary for viewers and virtualization consumers.
/// `Scrollback` remains the owner of mutable line caches; consumers receive
/// stable geometry without depending on the projection internals.
#[derive(Debug, Clone, Copy)]
pub struct ScrollbackLayout<'a> {
    pub width: usize,
    pub total_height: usize,
    pub entries: &'a [EntryLayout],
}

/// Owned description of a viewport-relative entry slice. The range indexes
/// the canonical entry array; `content_y0` is the virtual row of its first
/// entry and lets a renderer skip clipped rows without copying all layouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintWindow {
    pub entries: Range<usize>,
    pub content_y0: usize,
    pub total_height: usize,
}

/// Entry-based scrollback projection with width-sensitive line/layout caches.
#[derive(Debug, Default)]
pub struct Scrollback {
    entries: Vec<ScrollbackEntry>,
    positions: HashMap<EntryId, usize>,
    adapter: DshPresentationAdapter,
    layout_snapshot: Vec<EntryLayout>,
    layout_snapshot_dirty: bool,
    heights: HeightIndex,
    layout_width: usize,
    dirty_from: Option<usize>,
    content_revision: u64,
    layout_revision: u64,
}

impl Scrollback {
    pub fn entries(&self) -> &[ScrollbackEntry] {
        &self.entries
    }

    /// Monotonic revision of canonical entry content and topology.
    pub fn revision(&self) -> u64 {
        self.content_revision
    }

    /// Monotonic revision of width-specific height geometry. Rich viewers may
    /// update this revision without invalidating their semantic entry cache.
    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }

    pub fn render_entry_refs(
        &self,
    ) -> impl DoubleEndedIterator<Item = DshRenderEntryRef<'_>> + ExactSizeIterator {
        self.entries.iter().map(ScrollbackEntry::as_render_ref)
    }

    pub fn render_entry_ref(&self, entry_idx: usize) -> Option<DshRenderEntryRef<'_>> {
        self.entries
            .get(entry_idx)
            .map(ScrollbackEntry::as_render_ref)
    }

    pub fn render_entry_range(&self, range: Range<usize>) -> &[ScrollbackEntry] {
        let start = range.start.min(self.entries.len());
        let end = range.end.min(self.entries.len()).max(start);
        &self.entries[start..end]
    }

    pub fn rebuild(&mut self, history: &[HistoryEntry]) {
        let revision = self.content_revision;
        let previous = std::mem::take(&mut self.entries);
        self.positions.clear();
        let updates = self.adapter.adapt_history(history);
        self.layout_snapshot.clear();
        self.layout_snapshot_dirty = true;
        self.heights.rebuild(Vec::new());
        self.dirty_from = Some(0);
        for update in updates {
            self.apply_update(update);
        }
        let changed = previous.len() != self.entries.len()
            || previous
                .iter()
                .zip(&self.entries)
                .any(|(left, right)| !left.semantic_eq(right));
        self.content_revision = revision.saturating_add(u64::from(changed));
    }

    pub fn apply_event(&mut self, entry: &HistoryEntry) {
        let before = self.content_revision;
        for update in self.adapter.adapt_event(entry) {
            self.apply_update(update);
        }
        if self.content_revision != before {
            // `apply_update` already advanced the revision; coalesce multiple
            // adapter updates produced by one authoritative event.
            self.content_revision = before.saturating_add(1);
        }
    }

    /// Finalize any running streaming surfaces without fabricating a history
    /// event. The canonical surface remains in the scrollback with its stable
    /// `Partial { turn, step }` identity and a non-running finish state.
    pub fn finalize_stream(
        &mut self,
        seq: i64,
        finish: DshRenderFinish,
        reason: Option<&str>,
    ) -> bool {
        let updates = self.adapter.finalize_all(seq, finish, reason);
        let changed = !updates.is_empty();
        let before = self.content_revision;
        for update in updates {
            self.apply_update(update);
        }
        if self.content_revision != before {
            self.content_revision = before.saturating_add(1);
        }
        changed
    }

    fn apply_update(&mut self, update: DshRenderUpdate) {
        match update {
            DshRenderUpdate::Upsert(entry) => self.upsert(ScrollbackEntry::from(entry)),
            DshRenderUpdate::Remove(id) => self.remove(id),
            DshRenderUpdate::RemoveSourceRange { start, end } => self.remove_seq_range(start, end),
        }
    }

    /// Snapshot render data for a block viewer without exposing mutable layout
    /// caches or protocol event payloads.
    pub fn render_entries(&self) -> Vec<DshRenderEntry> {
        self.render_entry_refs()
            .map(DshRenderEntryRef::to_owned)
            .collect()
    }

    pub fn total_height(&mut self, width: usize) -> usize {
        self.ensure_layout(width);
        self.heights.total()
    }

    /// Ensure the width-specific cache and expose its immutable geometry.
    pub fn layout(&mut self, width: usize) -> ScrollbackLayout<'_> {
        self.ensure_layout(width);
        if self.layout_snapshot_dirty {
            self.layout_snapshot.clear();
            self.layout_snapshot.reserve(
                self.heights
                    .values
                    .len()
                    .saturating_sub(self.layout_snapshot.len()),
            );
            let mut start_y = 0usize;
            for (entry_idx, height) in self.heights.values.iter().copied().enumerate() {
                self.layout_snapshot.push(EntryLayout {
                    entry_idx,
                    start_y,
                    height,
                });
                start_y = start_y.saturating_add(height);
            }
            self.layout_snapshot_dirty = false;
        }
        ScrollbackLayout {
            width: width.max(1),
            total_height: self.heights.total(),
            entries: &self.layout_snapshot,
        }
    }

    /// Resolve the entry range intersecting a viewport plus symmetric
    /// overscan directly from the Fenwick index. This does not build or copy
    /// the full `EntryLayout` snapshot.
    pub fn paint_window(
        &mut self,
        width: usize,
        scroll_top: usize,
        viewport_height: usize,
        overscan_rows: usize,
    ) -> PaintWindow {
        self.ensure_layout(width);
        let total_height = self.heights.total();
        let start = scroll_top.saturating_sub(overscan_rows);
        let height = viewport_height.saturating_add(overscan_rows.saturating_mul(2));
        let entries = self.heights.window(start, height);
        let content_y0 = self.heights.start_y(entries.start);
        PaintWindow {
            entries,
            content_y0,
            total_height,
        }
    }

    pub fn entry_layout(&mut self, width: usize, entry_idx: usize) -> Option<EntryLayout> {
        self.ensure_layout(width);
        Some(EntryLayout {
            entry_idx,
            start_y: self.heights.start_y(entry_idx),
            height: self.heights.height(entry_idx)?,
        })
    }

    /// Return the cached rendered lines for one projected entry. The returned
    /// slice is invalidated only by changing that entry or requesting another
    /// width, which gives block viewers a small, explicit cache contract.
    pub fn entry_lines(&mut self, width: usize, entry_idx: usize) -> Option<&[String]> {
        self.ensure_layout(width);
        self.measure_entry(width.max(1), entry_idx)?;
        self.entries
            .get_mut(entry_idx)
            .map(|entry| entry.rendered_lines(width))
    }

    /// Owned rendered lines for a stable entry id. The copy path uses this
    /// after a drag may have scrolled the entry out of the visible window.
    pub fn entry_lines_for_id(&mut self, width: usize, id: EntryId) -> Option<Vec<String>> {
        let index = *self.positions.get(&id)?;
        Some(self.entry_lines(width, index)?.to_vec())
    }

    /// Supply the measured height from a renderer that owns richer semantic
    /// block layout than the compatibility plain-text cache.  The entry and
    /// its stable identity remain owned by `Scrollback`; this boundary only
    /// replaces the width-specific height used by its Fenwick index.
    pub fn set_rendered_height(&mut self, width: usize, entry_idx: usize, height: usize) -> bool {
        self.set_height(width, entry_idx, height, false)
    }

    /// Supply a height from a view-time projection. Unlike the regular rich
    /// renderer boundary, a projection may intentionally assign zero height
    /// to a canonical entry hidden behind a synthetic group header.
    pub fn set_projected_height(&mut self, width: usize, entry_idx: usize, height: usize) -> bool {
        self.set_height(width, entry_idx, height, true)
    }

    fn set_height(
        &mut self,
        width: usize,
        entry_idx: usize,
        height: usize,
        allow_zero: bool,
    ) -> bool {
        let width = width.max(1);
        self.ensure_layout(width);
        let Some(entry) = self.entries.get_mut(entry_idx) else {
            return false;
        };
        let height = if entry.visibility == DshRenderVisibility::Hidden {
            0
        } else if allow_zero {
            height
        } else {
            height.max(1)
        };
        entry.measured = Some(MeasuredHeight { width, height });
        let changed = self.heights.set(entry_idx, height);
        if changed {
            self.layout_snapshot_dirty = true;
            self.bump_layout_revision();
        }
        changed
    }

    pub fn entry_index(&self, id: EntryId) -> Option<usize> {
        self.positions.get(&id).copied()
    }

    pub fn anchor_at(&mut self, width: usize, scroll_top: usize) -> Option<ScrollAnchor> {
        self.ensure_layout(width);
        let top = scroll_top.min(self.heights.total().checked_sub(1)?);
        let index = self.heights.entry_at(top);
        let entry_id = self.entries.get(index)?.id;
        let intra_row = top.saturating_sub(self.heights.start_y(index));
        self.measure_entry(width.max(1), index)?;
        Some(ScrollAnchor {
            entry_id,
            intra_row: intra_row.min(self.heights.height(index)?.saturating_sub(1)),
        })
    }

    pub fn scroll_for_anchor(&mut self, width: usize, anchor: ScrollAnchor) -> Option<usize> {
        self.ensure_layout(width);
        let index = *self.positions.get(&anchor.entry_id)?;
        self.measure_entry(width.max(1), index)?;
        let start_y = self.heights.start_y(index);
        let height = self.heights.height(index)?;
        Some(start_y.saturating_add(anchor.intra_row.min(height.saturating_sub(1))))
    }

    pub fn visible_lines(
        &mut self,
        width: usize,
        scroll_top: usize,
        viewport_height: u16,
    ) -> Vec<PaintLine> {
        let scroll_top = self.materialize_viewport(width, scroll_top, viewport_height as usize);
        let range = self.heights.window(scroll_top, viewport_height as usize);
        let mut painted = Vec::new();
        for entry_idx in range {
            let start_y = self.heights.start_y(entry_idx);
            let lines = self.entries[entry_idx].rendered_lines(width).to_vec();
            for (line_idx, line) in lines.into_iter().enumerate() {
                let virtual_y = start_y.saturating_add(line_idx);
                if virtual_y < scroll_top {
                    continue;
                }
                let screen_y = virtual_y - scroll_top;
                if screen_y >= viewport_height as usize {
                    break;
                }
                painted.push(PaintLine {
                    screen_y: screen_y as u16,
                    entry_id: self.entries[entry_idx].id,
                    line_index: line_idx,
                    kind: self.entries[entry_idx].kind,
                    header: line_idx == 0,
                    text: line,
                    viewer_role: None,
                });
            }
        }
        painted
    }

    pub fn plain_text(&self) -> String {
        self.entries
            .iter()
            .map(|entry| format!("{}:\n{}", entry.kind.label(), entry.text))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Number of entries whose width-specific wrapped output is currently
    /// materialized. This is intentionally public for virtualization tests and
    /// diagnostics; callers should not depend on the cache representation.
    pub fn materialized_entry_count(&self, width: usize) -> usize {
        self.entries
            .iter()
            .filter(|entry| {
                entry
                    .cache
                    .as_ref()
                    .is_some_and(|cache| cache.width == width.max(1))
            })
            .count()
    }

    /// Materialize the viewport and its overscan while preserving the stable
    /// entry and intra-entry row that were at the top before estimates were
    /// replaced by measured heights. The returned top must be written back by
    /// viewport owners before painting.
    pub fn materialize_viewport(
        &mut self,
        width: usize,
        scroll_top: usize,
        viewport_height: usize,
    ) -> usize {
        self.ensure_layout(width);
        if viewport_height == 0 || self.entries.is_empty() || self.heights.total() == 0 {
            return scroll_top.min(self.heights.total());
        }

        let width = width.max(1);
        let top = scroll_top.min(self.heights.total().saturating_sub(1));
        let anchor_index = self.heights.entry_at(top);
        let anchor = ScrollAnchor {
            entry_id: self.entries[anchor_index].id,
            intra_row: top.saturating_sub(self.heights.start_y(anchor_index)),
        };
        let mut anchored_top = top;
        let mut keep = 0..0;

        // Each changing pass permanently replaces at least one estimate with a
        // measured height. The overscan is viewport-bounded, so convergence is
        // independent of the total history length.
        loop {
            let range = self.heights.window(
                anchored_top.saturating_sub(viewport_height),
                viewport_height.saturating_mul(3),
            );
            if range.is_empty() {
                break;
            }
            let mut changed = false;
            for entry_idx in range.clone() {
                let entry = &mut self.entries[entry_idx];
                let actual = if entry.visibility == DshRenderVisibility::Hidden {
                    entry.rendered_lines(width);
                    0
                } else {
                    entry.rendered_lines(width).len().saturating_add(1)
                };
                if self.heights.set(entry_idx, actual) {
                    self.layout_snapshot_dirty = true;
                    changed = true;
                }
            }
            let next_top = self
                .positions
                .get(&anchor.entry_id)
                .and_then(|index| {
                    self.heights.height(*index).map(|height| {
                        self.heights
                            .start_y(*index)
                            .saturating_add(anchor.intra_row.min(height.saturating_sub(1)))
                    })
                })
                .unwrap_or(anchored_top);
            keep = range;
            if !changed && next_top == anchored_top {
                break;
            }
            anchored_top = next_top;
        }

        self.evict_unneeded_caches(&keep);
        anchored_top
    }

    /// Materialize a viewport pinned to the transcript tail. Unlike a stable
    /// top anchor, follow mode must recompute its top after every height
    /// correction so the final entry remains visible.
    pub fn materialize_following_viewport(
        &mut self,
        width: usize,
        viewport_height: usize,
    ) -> usize {
        self.ensure_layout(width);
        if viewport_height == 0 || self.entries.is_empty() {
            return self.heights.total().saturating_sub(viewport_height);
        }

        let width = width.max(1);
        let mut keep = 0..0;
        loop {
            let top = self.heights.total().saturating_sub(viewport_height);
            let range = self.heights.window(
                top.saturating_sub(viewport_height),
                viewport_height.saturating_mul(3),
            );
            if range.is_empty() {
                break;
            }
            let mut changed = false;
            for entry_idx in range.clone() {
                let entry = &mut self.entries[entry_idx];
                let actual = if entry.visibility == DshRenderVisibility::Hidden {
                    entry.rendered_lines(width);
                    0
                } else {
                    entry.rendered_lines(width).len().saturating_add(1)
                };
                if self.heights.set(entry_idx, actual) {
                    self.layout_snapshot_dirty = true;
                    changed = true;
                }
            }
            keep = range;
            if !changed {
                break;
            }
        }

        self.evict_unneeded_caches(&keep);
        self.heights.total().saturating_sub(viewport_height)
    }

    fn ensure_layout(&mut self, width: usize) {
        let width = width.max(1);
        if self.layout_width != width {
            self.layout_width = width;
            self.dirty_from = Some(0);
            for entry in &mut self.entries {
                entry.cache = None;
                entry.measured = None;
            }
        }
        let Some(_dirty_from) = self.dirty_from.take() else {
            return;
        };
        let heights = self
            .entries
            .iter()
            .map(|entry| {
                entry
                    .measured_height(width)
                    .unwrap_or_else(|| entry.estimated_height(width))
            })
            .collect();
        self.heights.rebuild(heights);
        self.layout_snapshot_dirty = true;
        self.bump_layout_revision();
    }

    fn evict_unneeded_caches(&mut self, keep: &Range<usize>) {
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if !keep.contains(&index) {
                entry.cache = None;
            }
        }
    }

    fn measure_entry(&mut self, width: usize, index: usize) -> Option<usize> {
        if let Some(measured) = self
            .entries
            .get(index)
            .and_then(|entry| entry.measured_height(width))
        {
            return Some(measured);
        }
        let entry = self.entries.get_mut(index)?;
        let actual = if entry.visibility == DshRenderVisibility::Hidden {
            entry.rendered_lines(width);
            0
        } else {
            entry.rendered_lines(width).len().saturating_add(1)
        };
        if self.heights.height(index) != Some(actual) {
            self.heights.set(index, actual);
            self.layout_snapshot_dirty = true;
        }
        Some(actual)
    }

    fn upsert(&mut self, entry: ScrollbackEntry) {
        if let Some(&index) = self.positions.get(&entry.id) {
            if !self.entries[index].set(entry) {
                return;
            }
            self.bump_content_revision();
            if self.layout_width > 0
                && self.dirty_from.is_none()
                && self.heights.values.len() == self.entries.len()
            {
                let height = self.entries[index].estimated_height(self.layout_width);
                self.heights.set(index, height);
                self.layout_snapshot_dirty = true;
            } else {
                self.mark_dirty(index);
            }
            return;
        }
        let index = self.entries.len();
        self.positions.insert(entry.id, index);
        self.entries.push(entry);
        self.bump_content_revision();
        if self.layout_width > 0 && self.dirty_from.is_none() && self.heights.values.len() == index
        {
            let height = self.entries[index].estimated_height(self.layout_width);
            self.heights.push(height);
            self.layout_snapshot_dirty = true;
        } else {
            self.mark_dirty(index);
        }
    }

    fn remove(&mut self, id: EntryId) {
        let Some(index) = self.positions.remove(&id) else {
            return;
        };
        self.entries.remove(index);
        self.bump_content_revision();
        self.reindex_from(index);
        self.mark_dirty(index);
    }

    fn remove_seq_range(&mut self, start: i64, end: i64) {
        let before = self.entries.len();
        self.entries
            .retain(|entry| entry.source_seq < start || entry.source_seq > end);
        if self.entries.len() == before {
            return;
        }
        self.bump_content_revision();
        self.positions.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            self.positions.insert(entry.id, index);
        }
        self.mark_dirty(0);
    }

    fn reindex_from(&mut self, start: usize) {
        for (index, entry) in self.entries.iter().enumerate().skip(start) {
            self.positions.insert(entry.id, index);
        }
    }

    fn mark_dirty(&mut self, index: usize) {
        self.dirty_from = Some(self.dirty_from.map_or(index, |dirty| dirty.min(index)));
        self.layout_snapshot_dirty = true;
    }

    fn bump_content_revision(&mut self) {
        self.content_revision = self.content_revision.saturating_add(1);
    }

    fn bump_layout_revision(&mut self) {
        self.layout_revision = self.layout_revision.saturating_add(1);
    }
}

/// Find the entry range intersecting a virtual-y viewport in O(log n).
pub fn compute_paint_window(
    layouts: &[EntryLayout],
    scroll_top: usize,
    viewport_height: usize,
) -> Range<usize> {
    if layouts.is_empty() || viewport_height == 0 {
        return 0..0;
    }
    let bottom = scroll_top.saturating_add(viewport_height);
    let first = layouts.partition_point(|layout| layout.end_y() <= scroll_top);
    let last = layouts.partition_point(|layout| layout.start_y < bottom);
    first.min(layouts.len())..last.min(layouts.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::SessionEvent;
    use serde_json::{json, Value};

    fn history(seq: i64, event_type: &str, data: Value) -> HistoryEntry {
        HistoryEntry {
            event: SessionEvent {
                event_type: event_type.into(),
                seq,
                time: 1.0,
                data,
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        }
    }

    #[test]
    fn paint_window_binary_searches_intersections() {
        let layouts = vec![
            EntryLayout {
                entry_idx: 0,
                start_y: 0,
                height: 4,
            },
            EntryLayout {
                entry_idx: 1,
                start_y: 4,
                height: 3,
            },
            EntryLayout {
                entry_idx: 2,
                start_y: 7,
                height: 5,
            },
        ];
        assert_eq!(compute_paint_window(&layouts, 3, 5), 0..3);
        assert_eq!(compute_paint_window(&layouts, 4, 3), 1..2);
        assert_eq!(compute_paint_window(&layouts, 12, 4), 3..3);
        assert_eq!(compute_paint_window(&layouts, 0, 0), 0..0);
    }

    #[test]
    fn height_index_prefix_and_entry_boundaries_are_exact() {
        let mut index = HeightIndex::default();
        index.rebuild(vec![3, 1, 4]);

        assert_eq!(index.prefix_sum(0), 0);
        assert_eq!(index.prefix_sum(1), 3);
        assert_eq!(index.prefix_sum(2), 4);
        assert_eq!(index.prefix_sum(99), 8);
        assert_eq!(index.total(), 8);
        assert_eq!(index.entry_at(0), 0);
        assert_eq!(index.entry_at(2), 0);
        assert_eq!(index.entry_at(3), 1);
        assert_eq!(index.entry_at(4), 2);
        assert_eq!(index.entry_at(7), 2);
        assert_eq!(index.entry_at(8), 3);
        assert_eq!(index.window(0, 3), 0..1);
        assert_eq!(index.window(3, 2), 1..3);
        assert_eq!(index.window(8, 2), 0..0);
    }

    #[test]
    fn height_index_point_updates_keep_prefix_lookup_consistent() {
        let mut index = HeightIndex::default();
        index.rebuild(vec![2, 2, 2]);
        assert!(index.set(1, 5));
        assert_eq!(index.total(), 9);
        assert_eq!(index.start_y(2), 7);
        assert_eq!(index.entry_at(6), 1);
        assert_eq!(index.entry_at(7), 2);
        assert!(index.set(1, 1));
        assert_eq!(index.total(), 5);
        assert_eq!(index.start_y(2), 3);
        assert_eq!(index.entry_at(3), 2);
        assert!(!index.set(8, 1));
    }

    #[test]
    fn content_revision_is_monotonic_and_reads_do_not_advance_it() {
        let mut scrollback = Scrollback::default();
        assert_eq!(scrollback.revision(), 0);
        scrollback.apply_event(&history(
            1,
            "user/message",
            json!({
                "source": { "kind": "user" },
                "content": [{ "type": "text", "text": "hello" }]
            }),
        ));
        let revision = scrollback.revision();
        assert!(revision > 0);

        let _ = scrollback.render_entry_refs().collect::<Vec<_>>();
        let _ = scrollback.paint_window(80, 0, 10, 10);
        let _ = scrollback.entry_layout(80, 0);
        assert_eq!(scrollback.revision(), revision);

        let layout_revision = scrollback.layout_revision();
        assert!(scrollback.set_projected_height(80, 0, 9));
        assert_eq!(scrollback.revision(), revision);
        assert!(scrollback.layout_revision() > layout_revision);
    }

    #[test]
    fn borrowed_window_clones_only_on_explicit_entry_conversion() {
        let mut scrollback = Scrollback::default();
        for seq in 0..100 {
            scrollback.apply_event(&history(
                seq,
                "user/message",
                json!({
                    "source": { "kind": "user" },
                    "content": [{ "type": "text", "text": format!("entry {seq}") }]
                }),
            ));
        }
        let window = scrollback.paint_window(40, 30, 8, 8);
        assert!(window.entries.len() < 20);
        assert_eq!(scrollback.materialized_entry_count(40), 0);
        let borrowed = scrollback
            .render_entry_ref(window.entries.start)
            .expect("borrowed window entry");
        assert!(borrowed.text.starts_with("entry "));
        let owned = borrowed.to_owned();
        assert_eq!(owned.id, borrowed.id);
        assert_eq!(scrollback.materialized_entry_count(40), 0);
    }

    #[test]
    fn final_message_replaces_streaming_partial() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&history(
            0,
            "assistant/chunk",
            json!({
                "turn": 1, "step": 0,
                "chunk": { "type": "text-delta", "index": 0, "text": "hel" }
            }),
        ));
        scrollback.apply_event(&history(
            1,
            "assistant/chunk",
            json!({
                "turn": 1, "step": 0,
                "chunk": { "type": "text-delta", "index": 0, "text": "lo" }
            }),
        ));
        assert_eq!(scrollback.entries.len(), 1);
        assert_eq!(scrollback.entries[0].text, "hello");

        scrollback.apply_event(&history(
            2,
            "assistant/message",
            json!({
                "turn": 1, "step": 0,
                "message": {
                    "content": [{ "type": "text", "text": "hello!" }]
                }
            }),
        ));
        assert_eq!(scrollback.entries.len(), 1);
        assert_eq!(
            scrollback.entries[0].id,
            EntryId::Partial {
                turn: 1,
                step: 0,
                surface: 0,
            }
        );
        assert_eq!(scrollback.entries[0].text, "hello!");
        assert!(!scrollback.entries[0].partial);
    }

    #[test]
    fn hidden_context_remains_canonical_but_has_zero_layout_height() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&history(
            0,
            "user/message",
            json!({
                "source": { "kind": "system" },
                "content": [{ "type": "text", "text": "secret" }]
            }),
        ));
        scrollback.apply_event(&history(
            1,
            "user/message",
            json!({
                "source": { "kind": "user" },
                "content": [{ "type": "text", "text": "visible" }]
            }),
        ));
        assert_eq!(scrollback.entries.len(), 2);
        let layout = scrollback.layout(80);
        assert_eq!(layout.entries[0].height, 0);
        assert!(layout.entries[1].height > 0);
        assert!(!scrollback.plain_text().is_empty());
    }

    #[test]
    fn stable_anchor_survives_appended_entries() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&history(
            2,
            "user/message",
            json!({
                "source": { "kind": "user" },
                "content": [{ "type": "text", "text": "anchor body" }]
            }),
        ));
        let anchor = scrollback.anchor_at(20, 1).expect("anchor");
        scrollback.apply_event(&history(
            3,
            "assistant/message",
            json!({
                "turn": 0, "step": 0,
                "message": { "content": [{ "type": "text", "text": "tail" }] }
            }),
        ));
        assert_eq!(scrollback.scroll_for_anchor(20, anchor), Some(1));
    }

    #[test]
    fn rebuild_prepend_keeps_the_same_entry_and_row_anchored() {
        let mut scrollback = Scrollback::default();
        let current = vec![
            history(
                2,
                "user/message",
                json!({
                    "source": { "kind": "user" },
                    "content": [{ "type": "text", "text": "current anchor" }]
                }),
            ),
            history(
                3,
                "assistant/message",
                json!({
                    "turn": 1,
                    "step": 0,
                    "message": { "content": [{ "type": "text", "text": "tail" }] }
                }),
            ),
        ];
        scrollback.rebuild(&current);
        let anchor = scrollback.anchor_at(24, 2).expect("current anchor");

        let mut prepended = vec![
            history(
                0,
                "user/message",
                json!({
                    "source": { "kind": "user" },
                    "content": [{ "type": "text", "text": "older line one that wraps" }]
                }),
            ),
            history(
                1,
                "assistant/message",
                json!({
                    "turn": 0,
                    "step": 0,
                    "message": { "content": [{ "type": "text", "text": "older line two" }] }
                }),
            ),
        ];
        prepended.extend(current);
        scrollback.rebuild(&prepended);
        let restored = scrollback
            .scroll_for_anchor(24, anchor)
            .expect("restored anchor");
        assert_eq!(
            scrollback
                .anchor_at(24, restored)
                .map(|value| value.entry_id),
            Some(anchor.entry_id)
        );
    }

    #[test]
    fn resize_invalidates_width_specific_materialized_rows() {
        let mut scrollback = Scrollback::default();
        for seq in 0..8 {
            scrollback.apply_event(&history(
                seq,
                "user/message",
                json!({
                    "source": { "kind": "user" },
                    "content": [{ "type": "text", "text": format!("entry {seq} with a long body") }]
                }),
            ));
        }

        scrollback.visible_lines(24, 0, 5);
        assert!(scrollback.materialized_entry_count(24) > 0);
        scrollback.visible_lines(10, 0, 5);
        assert_eq!(scrollback.materialized_entry_count(24), 0);
        assert!(scrollback.materialized_entry_count(10) > 0);
    }

    #[test]
    fn materializing_a_measured_window_returns_a_stable_top_anchor() {
        let mut scrollback = Scrollback::default();
        for seq in 0..100 {
            scrollback.apply_event(&history(
                seq,
                "user/message",
                json!({
                    "source": { "kind": "user" },
                    "content": [{ "type": "text", "text": format!("entry {seq} with words that wrap differently") }]
                }),
            ));
        }

        let before = scrollback.anchor_at(18, 120).expect("estimated anchor");
        let adjusted = scrollback.materialize_viewport(18, 120, 8);
        let after = scrollback.anchor_at(18, adjusted).expect("measured anchor");
        assert_eq!(after.entry_id, before.entry_id);
        assert_eq!(after.intra_row, before.intra_row);
    }

    #[test]
    fn layout_boundary_exposes_width_and_cached_entry_lines() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&history(
            0,
            "user/message",
            json!({
                "source": { "kind": "user" },
                "content": [{ "type": "text", "text": "a long line" }]
            }),
        ));
        let (layout_width, layout_len, total_height) = {
            let layout = scrollback.layout(6);
            (layout.width, layout.entries.len(), layout.total_height)
        };
        assert_eq!(layout_width, 6);
        assert_eq!(layout_len, 1);
        assert!(total_height >= 3);
        let lines = scrollback.entry_lines(6, 0).expect("entry cache");
        assert!(lines.iter().any(|line| line.contains("long")));
    }

    #[test]
    fn layout_estimates_history_without_materializing_every_entry() {
        let mut scrollback = Scrollback::default();
        for seq in 0..1_000 {
            scrollback.apply_event(&history(
                seq,
                "user/message",
                json!({
                    "source": { "kind": "user" },
                    "content": [{ "type": "text", "text": format!("entry {seq} with a little body") }]
                }),
            ));
        }

        let total = scrollback.total_height(32);
        assert!(total > 1_000);
        assert_eq!(scrollback.materialized_entry_count(32), 0);

        let painted = scrollback.visible_lines(32, 0, 8);
        assert!(!painted.is_empty());
        assert!(scrollback.materialized_entry_count(32) < 32);
        assert!(scrollback.entries[500].cache.is_none());

        let bottom = scrollback.total_height(32).saturating_sub(8);
        scrollback.visible_lines(32, bottom, 8);
        assert!(scrollback.entries[0].cache.is_none());
        assert!(scrollback.entries[500].cache.is_none());
        assert!(scrollback.materialized_entry_count(32) < 64);
    }

    #[test]
    fn renderer_height_boundary_drives_shared_anchor_geometry() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&history(
            0,
            "assistant/message",
            json!({
                "turn": 0,
                "step": 0,
                "message": { "content": [{ "type": "text", "text": "short" }] }
            }),
        ));
        assert!(scrollback.set_rendered_height(80, 0, 9));
        assert_eq!(scrollback.total_height(80), 9);
        let anchor = scrollback.anchor_at(80, 7).expect("shared anchor");
        assert_eq!(anchor.entry_id, EntryId::Event { seq: 0 });
        assert_eq!(anchor.intra_row, 7);
        assert_eq!(scrollback.scroll_for_anchor(80, anchor), Some(7));
    }

    #[test]
    fn projected_height_can_hide_canonical_group_member_without_deleting_it() {
        let mut scrollback = Scrollback::default();
        for seq in 0..2 {
            scrollback.apply_event(&history(
                seq,
                "tool/call",
                json!({ "name": format!("tool-{seq}"), "arguments": "{}" }),
            ));
        }
        assert!(scrollback.set_projected_height(80, 0, 1));
        assert!(scrollback.set_projected_height(80, 1, 0));
        assert_eq!(scrollback.entries().len(), 2);
        assert_eq!(scrollback.layout(80).entries[1].height, 0);
        assert_eq!(scrollback.total_height(80), 1);
        assert!(scrollback.set_rendered_height(80, 1, 2));
        assert_eq!(scrollback.layout(80).entries[1].height, 2);
    }
}
