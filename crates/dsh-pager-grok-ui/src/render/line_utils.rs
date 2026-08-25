use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

pub fn truncate_str(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let budget = max_width.saturating_sub(1);
    let mut used = 0;
    for ch in text.chars() {
        let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + width > budget {
            break;
        }
        out.push(ch);
        used += width;
    }
    out.push('…');
    out
}

/// Return the byte offset at which `max_width` display columns are reached.
/// The copied timeline uses this to split a hover preview without breaking a
/// UTF-8 character.
pub fn byte_offset_at_width(text: &str, max_width: usize) -> usize {
    if max_width == 0 {
        return 0;
    }
    let mut used = 0;
    for (offset, ch) in text.char_indices() {
        let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + width > max_width {
            return offset;
        }
        used += width;
    }
    text.len()
}

fn take_width(text: &str, max_width: usize) -> String {
    text[..byte_offset_at_width(text, max_width)].to_string()
}

/// Truncate a styled `Line` to `max_width` display columns, preserving span
/// styles and appending `…` on the last kept span. Copied from Grok
/// `xai-grok-pager-render/src/render/line_utils.rs`.
pub fn truncate_line(line: Line<'static>, max_width: usize) -> Line<'static> {
    if max_width == 0 {
        return Line::from(Vec::new());
    }

    let total: usize = line.spans.iter().map(|span| span.content.width()).sum();
    if total <= max_width {
        return line;
    }

    let budget = max_width.saturating_sub(1);
    let mut used = 0usize;
    let mut out = Vec::new();

    for span in line.spans {
        let span_width = span.content.width();
        if used + span_width <= budget {
            used += span_width;
            out.push(span);
            continue;
        }
        let remaining = budget.saturating_sub(used);
        if remaining > 0 {
            let truncated = take_width(&span.content, remaining);
            out.push(Span::styled(truncated, span.style));
        }
        let ellipsis_style = out.last().map(|span| span.style).unwrap_or_default();
        out.push(Span::styled("\u{2026}", ellipsis_style));
        return Line::from(out);
    }

    Line::from(out)
}

#[allow(dead_code)]
pub fn line_to_static(line: &Line<'_>) -> Line<'static> {
    Line {
        style: line.style,
        alignment: line.alignment,
        spans: line
            .spans
            .iter()
            .map(|span| Span::styled(span.content.to_string(), span.style))
            .collect(),
    }
}
