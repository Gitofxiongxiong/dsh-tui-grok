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
