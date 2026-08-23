//! Grok-derived transcript block projection.
//!
//! The host adapter keeps typed DSH blocks intact. This module owns the
//! user-visible role, indentation and copy projection so the runtime never
//! needs to inspect protocol JSON or flatten a tool result itself.

use dsh_pager::{DshRenderBlock, DshRenderContent, DshRenderKind};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{host_adapter::TranscriptRow, theme::Theme};

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
    } else if text.starts_with("diff ") || text.starts_with('+') {
        theme.text_primary
    } else if text.starts_with('-') {
        theme.accent_user
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
        DshRenderBlock::Markdown { text } | DshRenderBlock::Plain { text } => {
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
                Style::default().fg(theme.accent_user),
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
            Style::default().fg(theme.accent_user),
        )));
    }
    for line in new_text.lines() {
        lines.push(Line::from(Span::styled(
            format!("{prefix}+ {line}"),
            Style::default().fg(theme.text_primary),
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
        DshRenderKind::Assistant => theme.text_primary,
        DshRenderKind::Thinking => theme.fuzzy_accent,
        DshRenderKind::ToolCall | DshRenderKind::ToolResult => theme.gray_bright,
        DshRenderKind::Error => theme.accent_user,
        _ => theme.gray,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::{DshRenderBlock, DshRenderContent, DshRenderEntryId};

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
}
