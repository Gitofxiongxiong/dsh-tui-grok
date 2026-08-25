//! Edit tool block. Structured diff input stays neutral and replayable.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use similar::{ChangeTag, TextDiff};

use crate::scrollback::{
    tool::{ToolBlockContext, header_line, text_line},
    types::{BlockLine, BlockOutput, DisplayMode},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDiff {
    pub path: String,
    pub old_text: Option<String>,
    pub new_text: String,
}

#[derive(Debug, Clone)]
pub struct EditToolCallBlock {
    pub title: String,
    pub diffs: Vec<ToolDiff>,
    pub error: Option<String>,
}

impl EditToolCallBlock {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            diffs: Vec::new(),
            error: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let (added, removed) = self.diffs.iter().fold((0usize, 0usize), |counts, diff| {
            let summary = diff_summary(diff);
            (counts.0 + summary.0, counts.1 + summary.1)
        });
        let detail = (added > 0 || removed > 0).then(|| format!("+{added}/-{removed}"));
        let subject = self.title.strip_prefix("Edit ").unwrap_or(&self.title);
        let mut lines = vec![header_line("Edit", subject, detail.as_deref(), ctx)];
        if ctx.mode == DisplayMode::Collapsed {
            return BlockOutput { lines };
        }
        if let Some(error) = &self.error {
            lines.push(text_line(error, ctx));
        }
        let multiple = self.diffs.len() > 1;
        for (diff_index, diff) in self.diffs.iter().enumerate() {
            if multiple || diff.path != subject {
                lines.push(text_line(diff.path.clone(), ctx));
            }
            if diff_index > 0 {
                lines.push(BlockLine::spacer());
            }
            lines.extend(render_diff(diff, ctx));
        }
        BlockOutput { lines }
    }
}

fn diff_summary(diff: &ToolDiff) -> (usize, usize) {
    TextDiff::from_lines(diff.old_text.as_deref().unwrap_or(""), &diff.new_text)
        .iter_all_changes()
        .fold((0, 0), |(added, removed), change| match change.tag() {
            ChangeTag::Insert => (added + 1, removed),
            ChangeTag::Delete => (added, removed + 1),
            ChangeTag::Equal => (added, removed),
        })
}

fn render_diff(diff: &ToolDiff, ctx: ToolBlockContext<'_>) -> Vec<BlockLine> {
    let old = diff.old_text.as_deref().unwrap_or("");
    let text_diff = TextDiff::from_lines(old, &diff.new_text);
    let groups = text_diff.grouped_ops(3);
    if groups.is_empty() {
        return vec![text_line("(no changes)", ctx)];
    }
    let cfg = &ctx.appearance.scrollback.blocks.edit;
    let max_line = old
        .lines()
        .count()
        .max(diff.new_text.lines().count())
        .max(1);
    let number_width = max_line.to_string().len();
    let mut output = Vec::new();
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 && !cfg.hunk_separator.is_empty() {
            output.push(BlockLine {
                content: Line::from(Span::styled(
                    cfg.hunk_separator.clone(),
                    Style::default().fg(ctx.theme.gray_dim),
                )),
                background: None,
                bg_start_col: 0,
                background_is_panel: false,
                selectable: false,
                header: false,
                joiner: None,
            });
        }
        for op in group {
            for change in text_diff.iter_changes(op) {
                let (marker, foreground, background) = match change.tag() {
                    ChangeTag::Equal => (' ', ctx.theme.diff_equal_fg, None),
                    ChangeTag::Delete => (
                        '-',
                        ctx.theme.diff_delete_fg,
                        Some(ctx.theme.diff_delete_bg),
                    ),
                    ChangeTag::Insert => (
                        '+',
                        ctx.theme.diff_insert_fg,
                        Some(ctx.theme.diff_insert_bg),
                    ),
                };
                let old_number = change.old_index().map(|index| index + 1);
                let new_number = change.new_index().map(|index| index + 1);
                let gutter = if cfg.dual_line_numbers {
                    format!(
                        "{:>number_width$} {:>number_width$} {marker} ",
                        old_number.map_or_else(String::new, |n| n.to_string()),
                        new_number.map_or_else(String::new, |n| n.to_string()),
                    )
                } else {
                    let number = new_number.or(old_number);
                    format!(
                        "{:>number_width$} {marker} ",
                        number.map_or_else(String::new, |n| n.to_string())
                    )
                };
                let indent = if cfg.indent { "  " } else { "" };
                let text = change.value().trim_end_matches(['\r', '\n']);
                let content_start =
                    (indent.chars().count() + gutter.chars().count()).min(u16::MAX as usize) as u16;
                let mut line = BlockLine::content(Line::from(vec![
                    Span::styled(indent.to_string(), Style::default().fg(ctx.theme.gray_dim)),
                    Span::styled(gutter, Style::default().fg(ctx.theme.gray_dim)),
                    Span::styled(text.to_string(), Style::default().fg(foreground)),
                ]));
                line.background = background;
                line.bg_start_col = if cfg.gutter_bg { 0 } else { content_start };
                if let Some(background) = background {
                    for span in &mut line.content.spans {
                        span.style = span.style.bg(background);
                    }
                    if !cfg.gutter_bg {
                        for span in line.content.spans.iter_mut().take(2) {
                            span.style = span.style.bg(Color::Reset);
                        }
                    }
                }
                output.push(line);
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{appearance::GrokAppearanceSnapshot, theme::Theme};

    fn context(mode: DisplayMode) -> ToolBlockContext<'static> {
        let theme = *Theme::current();
        let appearance = Box::leak(Box::new(
            GrokAppearanceSnapshot::default().scrollback(theme),
        ));
        ToolBlockContext {
            mode,
            is_running: false,
            width: 80,
            appearance,
            theme,
        }
    }

    #[test]
    fn edit_summary_counts_changes_not_whole_file_lines() {
        let diff = ToolDiff {
            path: "src/lib.rs".into(),
            old_text: Some("keep\nold\ntail\n".into()),
            new_text: "keep\nnew\ntail\n".into(),
        };
        assert_eq!(diff_summary(&diff), (1, 1));
        let mut block = EditToolCallBlock::new("Edit src/lib.rs");
        block.diffs.push(diff);
        let lines = block.output(context(DisplayMode::Expanded)).lines;
        assert!(lines[0].content.to_string().contains("+1/-1"));
        assert!(lines.iter().any(|line| {
            line.content.to_string().contains("old")
                && line.background == Some(Theme::current().diff_delete_bg)
        }));
        assert!(lines.iter().any(|line| {
            line.content.to_string().contains("new")
                && line.background == Some(Theme::current().diff_insert_bg)
        }));
    }

    #[test]
    fn unchanged_context_is_bounded_into_hunks() {
        let old = (0..20).map(|n| format!("line {n}\n")).collect::<String>();
        let new = old.replace("line 10", "changed 10");
        let rows = render_diff(
            &ToolDiff {
                path: "a.txt".into(),
                old_text: Some(old),
                new_text: new,
            },
            context(DisplayMode::Expanded),
        );
        assert!(rows.len() < 20);
        assert!(
            rows.iter()
                .any(|line| line.content.to_string().contains("changed 10"))
        );
    }
}
