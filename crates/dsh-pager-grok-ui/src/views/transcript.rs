//! Grok-derived transcript block projection.
//!
//! The host adapter keeps typed DSH blocks intact. This module owns the
//! user-visible role, indentation and copy projection so the runtime never
//! needs to inspect protocol JSON or flatten a tool result itself.

use std::collections::HashMap;

use dsh_pager::scrollback::{Scrollback, compute_paint_window};
use dsh_pager::{
    DshRenderBlock, DshRenderContent, DshRenderEntry, DshRenderEntryId, DshRenderKind, ScrollAnchor,
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{host_adapter::TranscriptRow, render::wrapping::word_wrap_line, theme::Theme};

/// A rich line after semantic block rendering and terminal-width wrapping.
/// `line_index` is stable within an entry at a given width and is shared with
/// hit testing and selection; `screen_y` is filled only by the viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichPaintLine {
    pub entry_id: DshRenderEntryId,
    pub line_index: usize,
    pub header: bool,
    pub screen_y: u16,
    pub line: Line<'static>,
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
    lines: Vec<RichPaintLine>,
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
    entries: HashMap<DshRenderEntryId, CachedPaneEntry>,
}

impl ScrollbackPane {
    pub fn clear(&mut self) {
        self.width = 0;
        self.entries.clear();
    }

    pub fn sync(&mut self, scrollback: &mut Scrollback, width: usize, theme: Theme) {
        let width = width.max(1);
        if self.width != width {
            self.entries.clear();
        }
        self.width = width;
        let entries = scrollback.render_entries();
        let mut live = HashMap::with_capacity(entries.len());
        for (entry_idx, entry) in entries.into_iter().enumerate() {
            let cached = self.entries.remove(&entry.id);
            let cached = match cached {
                Some(cached) if cached.entry == entry => cached,
                _ => CachedPaneEntry {
                    lines: semantic_lines(&entry, width, theme),
                    entry: entry.clone(),
                },
            };
            scrollback.set_rendered_height(width, entry_idx, cached.lines.len().saturating_add(1));
            live.insert(entry.id, cached);
        }
        self.entries = live;
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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
        let item = entries.iter().rev().find(|item| item.start_y <= top)?;
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
                painted.push(line);
            }
        }
        painted
    }
}

fn semantic_lines(entry: &DshRenderEntry, width: usize, theme: Theme) -> Vec<RichPaintLine> {
    let row = TranscriptRow::from(entry.clone());
    let semantic = render_row(&row, theme);
    let mut lines = Vec::new();
    for (source_index, line) in semantic.iter().enumerate() {
        for wrapped_line in word_wrap_line(line, width) {
            lines.push(RichPaintLine {
                entry_id: entry.id,
                line_index: lines.len(),
                header: source_index == 0,
                screen_y: 0,
                line: wrapped_line,
            });
        }
    }
    lines
}

impl RichTranscript {
    pub fn new(entries: &[DshRenderEntry], width: usize, theme: Theme) -> Self {
        let width = width.max(1);
        let mut projected = Vec::with_capacity(entries.len());
        let mut start_y = 0usize;
        for entry in entries {
            let row = TranscriptRow::from(entry.clone());
            let semantic_lines = render_row(&row, theme);
            let mut lines = Vec::new();
            for (source_index, line) in semantic_lines.iter().enumerate() {
                let wrapped = word_wrap_line(line, width);
                for wrapped_line in wrapped {
                    lines.push(RichPaintLine {
                        entry_id: entry.id,
                        line_index: lines.len(),
                        header: source_index == 0,
                        screen_y: 0,
                        line: wrapped_line,
                    });
                }
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
    let color = if text.starts_with("▸ ") || text.starts_with('✓') {
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
/// Grok block widgets. A row always starts with a stable kind/sequence header;
/// every typed block then contributes its own spacing and semantic color.
pub fn render_row(row: &TranscriptRow, theme: Theme) -> Vec<Line<'static>> {
    let color = color_for_kind(row.kind, theme);
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} ", row.label),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("#{}", row.source_seq),
            Style::default().fg(theme.gray_dim),
        ),
    ])];

    if row.content.blocks.is_empty() {
        push_plain_lines(&mut lines, &row.text, Style::default().fg(color), "");
    } else {
        for block in &row.content.blocks {
            render_block(&mut lines, block, theme, 0);
        }
    }
    lines
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

fn render_block(
    lines: &mut Vec<Line<'static>>,
    block: &DshRenderBlock,
    theme: Theme,
    indent: usize,
) {
    let prefix = " ".repeat(indent.saturating_mul(2));
    match block {
        DshRenderBlock::Markdown { text } => {
            render_markdown(lines, text, theme, &prefix);
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
            push_plain_lines(
                lines,
                text,
                Style::default().fg(theme.fuzzy_accent),
                &prefix,
            );
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
        DshRenderBlock::ToolCall {
            name,
            arguments,
            edit,
            ..
        } => {
            lines.push(Line::from(Span::styled(
                format!("{prefix}▸ {name}"),
                Style::default()
                    .fg(theme.gray_bright)
                    .add_modifier(Modifier::BOLD),
            )));
            if !arguments.is_empty() {
                push_plain_lines(
                    lines,
                    arguments,
                    Style::default().fg(theme.gray),
                    &format!("{prefix}  "),
                );
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
                    theme.accent_user
                } else {
                    theme.gray_bright
                }),
            )));
            for child in blocks {
                render_block(lines, child, theme, indent + 1);
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

/// Small, deterministic Markdown projection used until the full Grok AST
/// renderer is vendored. It preserves the source text while applying the same
/// semantic roles for headings, fenced code, task markers, and links.
fn render_markdown(lines: &mut Vec<Line<'static>>, text: &str, theme: Theme, prefix: &str) {
    let mut in_code = false;
    for source in text.split('\n') {
        let trimmed = source.trim_start();
        let (style, content) = if trimmed.starts_with("```") {
            in_code = !in_code;
            (
                Style::default().fg(theme.md_code).bg(theme.md_code_bg),
                source,
            )
        } else if in_code {
            (
                Style::default().fg(theme.md_text).bg(theme.md_code_bg),
                source,
            )
        } else if trimmed.starts_with("# ") {
            (
                Style::default()
                    .fg(theme.md_heading_h1)
                    .add_modifier(theme.md_heading_h1_mod),
                source,
            )
        } else if trimmed.starts_with("## ") {
            (
                Style::default()
                    .fg(theme.md_heading_h2)
                    .add_modifier(theme.md_heading_h2_mod),
                source,
            )
        } else if trimmed.starts_with("### ") {
            (
                Style::default()
                    .fg(theme.md_heading_h3)
                    .add_modifier(theme.md_heading_h3_mod),
                source,
            )
        } else if trimmed.starts_with("- [x]") || trimmed.starts_with("* [x]") {
            (Style::default().fg(theme.md_task_checked), source)
        } else if trimmed.starts_with("- [ ]") || trimmed.starts_with("* [ ]") {
            (Style::default().fg(theme.md_task_unchecked), source)
        } else if source.contains('`') {
            (Style::default().fg(theme.md_code), source)
        } else if source.contains("http://") || source.contains("https://") {
            (Style::default().fg(theme.link_fg), source)
        } else if source.trim().is_empty() {
            (Style::default().fg(theme.md_muted), source)
        } else {
            (Style::default().fg(theme.md_text), source)
        };
        lines.push(Line::from(Span::styled(
            format!("{prefix}{content}"),
            style,
        )));
    }
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
        DshRenderKind::Thinking => theme.accent_thinking,
        DshRenderKind::ToolCall | DshRenderKind::ToolResult => theme.accent_tool,
        DshRenderKind::Error => theme.accent_error,
        _ => theme.gray,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::{DshRenderBlock, DshRenderContent, DshRenderEntryId};
    use dsh_pager_protocol::{HistoryEntry, SessionEvent};
    use serde_json::json;

    fn row() -> TranscriptRow {
        TranscriptRow {
            id: DshRenderEntryId::Event { seq: 7 },
            label: "Assistant".into(),
            text: "fallback".into(),
            kind: DshRenderKind::Assistant,
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
    fn rich_transcript_uses_structured_blocks_and_stable_identity() {
        let row = row();
        let entry = DshRenderEntry {
            id: row.id,
            source_seq: row.source_seq,
            kind: row.kind,
            text: row.text,
            partial: false,
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
                kind: row.kind,
                text: row.text,
                partial: false,
                lineage: Vec::new(),
                content: row.content,
            }],
            4,
            *Theme::current(),
        );
        assert!(rich.total_height() > 4);
        assert!(
            rich.visible_lines(0, 20)
                .iter()
                .any(|line| line.line.to_string().contains("中"))
        );
    }

    #[test]
    fn rich_renderer_covers_all_block_fallbacks() {
        let row = TranscriptRow {
            id: DshRenderEntryId::Event { seq: 9 },
            label: "Assistant".into(),
            text: "fallback".into(),
            kind: DshRenderKind::Assistant,
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
            "▸ shell",
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
    fn markdown_and_diff_use_dedicated_theme_roles() {
        let theme = *Theme::current();
        let mut lines = Vec::new();
        render_markdown(
            &mut lines,
            "# heading\n```rust\nlet x = 1;\n```\nhttps://example.com",
            theme,
            "",
        );
        assert_eq!(lines[0].spans[0].style.fg, Some(theme.md_heading_h1));
        assert_eq!(lines[1].spans[0].style.fg, Some(theme.md_code));
        assert_eq!(lines[2].spans[0].style.bg, Some(theme.md_code_bg));
        assert_eq!(lines[4].spans[0].style.fg, Some(theme.link_fg));

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
}
