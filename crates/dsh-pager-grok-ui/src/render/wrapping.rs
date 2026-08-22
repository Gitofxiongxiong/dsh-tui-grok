//! Small, style-preserving wrapping seam used by the copied Grok picker.
//!
//! The production Grok module also carries joiners and indentation metadata.
//! This spike keeps the same public entry point while preserving the span
//! styles that drive picker highlights (including underlined links).

use std::borrow::Cow;

use ratatui::text::{Line, Span};
use textwrap::Options;

pub fn word_wrap_line<'a, O>(line: &'a Line<'a>, width_or_options: O) -> Vec<Line<'static>>
where
    O: Into<Options<'a>>,
{
    let options = width_or_options.into();
    let text = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();

    if options.width == 0 || text.is_empty() {
        return vec![owned_line(line)];
    }

    let wrapped = textwrap::wrap(&text, options);
    if wrapped.is_empty() {
        return vec![owned_line(line)];
    }

    let mut search_from = 0usize;
    wrapped
        .into_iter()
        .map(|part| {
            let part = part.as_ref();
            // `textwrap` normally returns borrowed slices. Searching from the
            // previous end also handles an owned Cow without unsafe pointer
            // arithmetic and keeps repeated words deterministic.
            let start = text[search_from..]
                .find(part)
                .map_or(search_from, |offset| search_from + offset);
            let end = start + part.len();
            let result = slice_line(line, start..end);
            search_from = end;
            result
        })
        .collect()
}

fn owned_line(line: &Line<'_>) -> Line<'static> {
    Line {
        style: line.style,
        alignment: line.alignment,
        spans: line
            .spans
            .iter()
            .map(|span| Span {
                style: span.style,
                content: Cow::Owned(span.content.to_string()),
            })
            .collect(),
    }
}

fn slice_line(line: &Line<'_>, range: std::ops::Range<usize>) -> Line<'static> {
    let mut offset = 0usize;
    let mut spans = Vec::new();
    for span in &line.spans {
        let text = span.content.as_ref();
        let span_start = offset;
        let span_end = offset + text.len();
        let start = range.start.max(span_start);
        let end = range.end.min(span_end);
        if start < end {
            let local_start = start - span_start;
            let local_end = end - span_start;
            spans.push(Span {
                style: span.style,
                content: Cow::Owned(text[local_start..local_end].to_owned()),
            });
        }
        offset = span_end;
        if offset >= range.end {
            break;
        }
    }
    Line {
        style: line.style,
        alignment: line.alignment,
        spans,
    }
}
