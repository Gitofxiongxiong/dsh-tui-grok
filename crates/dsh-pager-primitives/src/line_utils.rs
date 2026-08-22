use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn line_to_static(line: &Line<'_>) -> Line<'static> {
    Line {
        style: line.style,
        alignment: line.alignment,
        spans: line
            .spans
            .iter()
            .map(|span| Span {
                style: span.style,
                content: std::borrow::Cow::Owned(span.content.to_string()),
            })
            .collect(),
    }
}

pub fn push_owned_lines(source: &[Line<'_>], output: &mut Vec<Line<'static>>) {
    output.extend(source.iter().map(line_to_static));
}

pub fn fit_line_to_width<'a>(line: Line<'a>, width: usize) -> Line<'a> {
    let total: usize = line.spans.iter().map(|span| span.content.width()).sum();
    if total == width {
        return line;
    }
    let Line {
        style,
        alignment,
        spans,
    } = line;
    if total < width {
        let mut padded = spans;
        padded.push(Span::raw(" ".repeat(width - total)));
        return Line {
            style,
            alignment,
            spans: padded,
        };
    }
    let mut output = Vec::new();
    let mut used = 0;
    for span in spans {
        let span_width = span.content.width();
        if used + span_width <= width {
            used += span_width;
            output.push(span);
            if used == width {
                break;
            }
            continue;
        }
        let remaining = width.saturating_sub(used);
        if remaining > 0 {
            let mut text = String::new();
            let mut text_width = 0;
            for grapheme in span.content.graphemes(true) {
                let grapheme_width = grapheme.width();
                if text_width + grapheme_width > remaining {
                    break;
                }
                text_width += grapheme_width;
                text.push_str(grapheme);
            }
            used += text_width;
            if !text.is_empty() {
                output.push(Span::styled(text, span.style));
            }
        }
        if used < width {
            output.push(Span::raw(" ".repeat(width - used)));
        }
        break;
    }
    Line {
        style,
        alignment,
        spans: output,
    }
}
