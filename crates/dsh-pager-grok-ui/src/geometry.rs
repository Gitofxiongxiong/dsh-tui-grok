//! Render-time geometry shared by painting, pointer routing and selection.
//!
//! The map is deliberately data-only.  A frame builds it while it paints the
//! transcript and overlays; the next mouse event queries the same rectangles
//! instead of reimplementing a second set of row/column calculations.

use std::sync::Arc;

use dsh_grok_inline::LinkSpan;
use dsh_pager::DshRenderEntryId;
use ratatui::layout::{Position, Rect};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HitTarget {
    TranscriptEntry(DshRenderEntryId),
    TranscriptBlock {
        entry_id: DshRenderEntryId,
        block_index: usize,
    },
    Prompt,
    QueueItem(String),
    Overlay(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    pub url: Arc<str>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitRegion {
    pub target: HitTarget,
    pub rect: Rect,
    pub label: String,
    pub link: Option<LinkTarget>,
    pub priority: u8,
}

impl HitRegion {
    pub fn contains(&self, column: u16, row: u16) -> bool {
        self.rect.contains(Position::new(column, row))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HitMap {
    area: Rect,
    revision: u64,
    regions: Vec<HitRegion>,
}

impl HitMap {
    pub fn new(area: Rect) -> Self {
        Self {
            area,
            revision: 1,
            regions: Vec::new(),
        }
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Resize invalidates all old targets. Returning true lets a reducer drop
    /// a drag/hover target before it can accidentally hit the new layout.
    pub fn resize(&mut self, area: Rect) -> bool {
        if self.area == area {
            return false;
        }
        self.area = area;
        self.regions.clear();
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn clear(&mut self) {
        self.regions.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn insert(&mut self, mut region: HitRegion) {
        if region.rect.width == 0 || region.rect.height == 0 {
            return;
        }
        region.rect = region.rect.intersection(self.area);
        if region.rect.width == 0 || region.rect.height == 0 {
            return;
        }
        self.regions.push(region);
    }

    pub fn regions(&self) -> &[HitRegion] {
        &self.regions
    }

    /// Highest-priority, latest-painted region wins at an overlap. This gives
    /// overlays and modal chrome deterministic precedence over transcript rows.
    pub fn hit_test(&self, column: u16, row: u16) -> Option<&HitRegion> {
        self.regions
            .iter()
            .enumerate()
            .filter(|(_, region)| region.contains(column, row))
            .max_by_key(|(index, region)| (region.priority, *index))
            .map(|(_, region)| region)
    }

    pub fn link_at(&self, column: u16, row: u16) -> Option<&LinkTarget> {
        self.hit_test(column, row)?.link.as_ref()
    }

    pub fn link_spans(&self) -> Vec<LinkSpan> {
        self.regions
            .iter()
            .filter_map(|region| {
                let link = region.link.as_ref()?;
                Some(LinkSpan {
                    row: region.rect.y,
                    col_start: region.rect.x,
                    col_end: region.rect.right(),
                    url: Arc::clone(&link.url),
                    id: Some(stable_link_id(link.url.as_ref())),
                })
            })
            .collect()
    }
}

fn stable_link_id(url: &str) -> u32 {
    let mut hash = 2166136261u32;
    for byte in url.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16777619);
    }
    hash.max(1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryLine {
    pub target: HitTarget,
    pub line_index: usize,
    pub text: String,
    pub joiner_to_previous: Option<String>,
    pub rect: Rect,
}

/// Add one visible line to a map using grapheme/display columns. The function
/// never uses byte offsets as terminal columns, so CJK, emoji and combining
/// marks retain correct hit coordinates.
#[allow(clippy::too_many_arguments)]
pub fn insert_text_line(
    map: &mut HitMap,
    target: HitTarget,
    line_index: usize,
    x: u16,
    y: u16,
    width: u16,
    text: &str,
    joiner_to_previous: Option<String>,
    link: Option<LinkTarget>,
) -> GeometryLine {
    let rect = Rect::new(x, y, width, 1);
    map.insert(HitRegion {
        target: target.clone(),
        rect,
        label: text.to_string(),
        link: None,
        priority: 10,
    });
    if let Some(link) = link
        && let Some(byte_start) = text.find(link.label.as_str())
    {
        let start = x.saturating_add(UnicodeWidthStr::width(&text[..byte_start]) as u16);
        let width = UnicodeWidthStr::width(link.label.as_str()) as u16;
        map.insert(HitRegion {
            target: target.clone(),
            rect: Rect::new(start, y, width.max(1), 1),
            label: link.label.clone(),
            link: Some(link),
            priority: 11,
        });
    }
    GeometryLine {
        target,
        line_index,
        text: text.to_string(),
        joiner_to_previous,
        rect,
    }
}

/// Return the grapheme whose displayed columns contain `column`. Columns past
/// the final grapheme resolve to the logical end, which makes drag selection
/// at the right edge stable.
pub fn grapheme_at_column(text: &str, column: usize) -> usize {
    let mut current = 0usize;
    for (index, grapheme) in text.graphemes(true).enumerate() {
        let width = UnicodeWidthStr::width(grapheme).max(1);
        if column < current.saturating_add(width) {
            return index;
        }
        current = current.saturating_add(width);
    }
    text.graphemes(true).count()
}

pub fn column_for_grapheme(text: &str, grapheme_index: usize) -> usize {
    text.graphemes(true)
        .take(grapheme_index)
        .map(|grapheme| UnicodeWidthStr::width(grapheme).max(1))
        .sum()
}

/// Parse only terminal-safe web/file links. Host-side data remains untouched;
/// opening a target is a local UI action and never an RPC side effect.
pub fn link_target_at(text: &str, column: usize) -> Option<LinkTarget> {
    let mut offset = 0usize;
    for token in text.split_whitespace() {
        let token_start = text[offset..].find(token)? + offset;
        let token_end = token_start + token.len();
        offset = token_end;
        let trimmed = token.trim_matches(|character: char| {
            matches!(
                character,
                '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | '.' | ';'
            )
        });
        if !(trimmed.starts_with("https://")
            || trimmed.starts_with("http://")
            || trimmed.starts_with("file://"))
        {
            continue;
        }
        let start_column = UnicodeWidthStr::width(&text[..token_start]);
        let end_column = start_column + UnicodeWidthStr::width(trimmed);
        if (start_column..end_column).contains(&column) {
            return Some(LinkTarget {
                url: Arc::from(trimmed),
                label: trimmed.to_string(),
            });
        }
    }
    None
}

pub fn first_link_target(text: &str) -> Option<LinkTarget> {
    (0..UnicodeWidthStr::width(text)).find_map(|column| link_target_at(text, column))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> HitTarget {
        HitTarget::Prompt
    }

    #[test]
    fn hit_map_prefers_overlay_and_invalidates_on_resize() {
        let mut map = HitMap::new(Rect::new(0, 0, 20, 4));
        map.insert(HitRegion {
            target: target(),
            rect: Rect::new(0, 0, 10, 2),
            label: "body".into(),
            link: None,
            priority: 1,
        });
        map.insert(HitRegion {
            target: HitTarget::Overlay("modal".into()),
            rect: Rect::new(2, 1, 4, 1),
            label: "modal".into(),
            link: None,
            priority: 20,
        });
        assert!(matches!(
            map.hit_test(2, 1).unwrap().target,
            HitTarget::Overlay(_)
        ));
        let revision = map.revision();
        assert!(map.resize(Rect::new(0, 0, 40, 8)));
        assert!(map.revision() > revision);
        assert!(map.hit_test(2, 1).is_none());
    }

    #[test]
    fn unicode_columns_use_graphemes_not_bytes() {
        let text = "A界e\u{301}";
        assert_eq!(grapheme_at_column(text, 0), 0);
        assert_eq!(grapheme_at_column(text, 1), 1);
        assert_eq!(column_for_grapheme(text, 2), 3);
        assert_eq!(grapheme_at_column(text, 99), 3);
    }

    #[test]
    fn links_are_detected_without_passing_paths_to_host() {
        let link = link_target_at("see https://example.test/path, now", 7).unwrap();
        assert_eq!(link.url.as_ref(), "https://example.test/path");
        assert!(link_target_at("not a link", 4).is_none());
    }

    #[test]
    fn link_region_covers_only_the_url_columns() {
        let mut map = HitMap::new(Rect::new(0, 0, 80, 2));
        let link = first_link_target("see https://example.test/path").unwrap();
        insert_text_line(
            &mut map,
            HitTarget::Prompt,
            0,
            0,
            0,
            80,
            "see https://example.test/path",
            None,
            Some(link),
        );
        assert!(map.link_at(6, 0).is_some());
        assert!(map.link_at(1, 0).is_none());
    }
}
