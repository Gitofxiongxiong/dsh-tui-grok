//! Search tool block, retaining Grok's typed file/match output shape.
//!
//! B adaptation of `scrollback/blocks/tool/search.rs` at SOURCE_REV
//! `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`. Header, match summary,
//! metadata line, path colour, file-group panel and glob promotion follow
//! the upstream `output()` contract. `BlockContent`, process timing and
//! Grok grep `rawInput` parsing stay outside this file; DSH fills
//! [`SearchInputMeta`] in `project_tool`.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::{
    render::line_utils::truncate_line,
    render::tool_paths::shorten_path,
    scrollback::{
        tool::ToolBlockContext,
        types::{BlockLine, BlockOutput, DisplayMode},
    },
    theme::Theme,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchLineMatch {
    pub line_number: usize,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchFileMatch {
    pub path: String,
    pub matches: Vec<SearchLineMatch>,
}

/// Output mode mirroring Grok's search display, mapped from DSH grep vs glob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchOutputMode {
    #[default]
    Content,
    FilesWithMatches,
    Count,
}

/// Extra metadata from the search input — carried for display purposes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchInputMeta {
    pub path: Option<String>,
    pub glob: Option<String>,
    pub output_mode: SearchOutputMode,
    pub case_insensitive: bool,
    pub file_type: Option<String>,
    pub multiline: bool,
}

#[derive(Debug, Clone)]
pub struct SearchToolCallBlock {
    pub pattern: String,
    pub match_count: usize,
    pub file_matches: Vec<SearchFileMatch>,
    pub file_paths: Vec<String>,
    pub error: Option<String>,
    pub truncated: bool,
    pub meta: SearchInputMeta,
}

impl SearchToolCallBlock {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            match_count: 0,
            file_matches: Vec::new(),
            file_paths: Vec::new(),
            error: None,
            truncated: false,
            meta: SearchInputMeta::default(),
        }
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    fn match_summary(&self) -> String {
        if self.match_count == 0 {
            return match self.meta.output_mode {
                SearchOutputMode::FilesWithMatches => "(no files)".to_string(),
                _ => "(no matches)".to_string(),
            };
        }
        match self.meta.output_mode {
            SearchOutputMode::Content => {
                let file_count = self.file_matches.len();
                if file_count > 1 {
                    format!("({} matches in {} files)", self.match_count, file_count)
                } else if self.match_count == 1 {
                    "(1 match)".to_string()
                } else {
                    format!("({} matches)", self.match_count)
                }
            }
            SearchOutputMode::FilesWithMatches => {
                if self.match_count == 1 {
                    "(1 file)".to_string()
                } else {
                    format!("({} files)", self.match_count)
                }
            }
            SearchOutputMode::Count => {
                let file_count = self.file_paths.len().max(self.file_matches.len());
                if file_count > 1 {
                    format!("({} matches across {} files)", self.match_count, file_count)
                } else if self.match_count == 1 {
                    "(1 match)".to_string()
                } else {
                    format!("({} matches)", self.match_count)
                }
            }
        }
    }

    fn is_trivial_pattern(&self) -> bool {
        self.pattern.is_empty() || self.pattern == "."
    }

    fn header_line(
        &self,
        theme: &Theme,
        muted: bool,
        dim_details: bool,
        width: Option<usize>,
    ) -> Line<'static> {
        let text_style = if muted {
            theme.muted()
        } else {
            theme.primary()
        };
        let bold_style = text_style.add_modifier(Modifier::BOLD);
        let pattern_style = if muted {
            theme.muted()
        } else {
            theme.fg(theme.accent_success)
        };
        let detail_style = if dim_details {
            theme.dim()
        } else {
            theme.muted()
        };
        let path_style = if muted {
            theme.muted()
        } else {
            theme.fg(theme.path)
        };

        let mut spans = vec![Span::styled("Search ".to_string(), bold_style)];

        if self.is_trivial_pattern()
            && let Some(ref glob) = self.meta.glob
        {
            spans.push(Span::styled(glob.to_string(), pattern_style));
        } else {
            spans.push(Span::styled(format!("{:?}", self.pattern), pattern_style));
            if let Some(ref glob) = self.meta.glob {
                spans.push(Span::styled(" in ".to_string(), text_style));
                spans.push(Span::styled(glob.to_string(), pattern_style));
            }
        }

        if let Some(ref path) = self.meta.path {
            spans.push(Span::styled(" in ".to_string(), text_style));
            if let Some(w) = width {
                let used: usize = spans
                    .iter()
                    .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                    .sum();
                let summary = format!(" {}", self.match_summary());
                let path_budget = w.saturating_sub(used + summary.len());
                let shortened = shorten_path(path, path_budget);
                spans.push(Span::styled(shortened, path_style));
            } else {
                spans.push(Span::styled(path.to_string(), path_style));
            }
        }

        let summary = format!(" {}", self.match_summary());
        if let Some(w) = width {
            let used: usize = spans
                .iter()
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum();
            if used + summary.len() <= w {
                spans.push(Span::styled(summary, detail_style));
            }
        } else {
            spans.push(Span::styled(summary, detail_style));
        }

        let line = Line::from(spans);
        if let Some(w) = width {
            truncate_line(line, w)
        } else {
            line
        }
    }

    fn header_block_line(&self, line: Line<'static>) -> BlockLine {
        BlockLine::header(line)
    }

    fn metadata_line(&self, theme: &Theme) -> Line<'static> {
        let label_style = theme.muted();
        let value_style = theme.primary();

        let mut parts: Vec<Vec<Span<'static>>> = Vec::new();
        let mode_str = match self.meta.output_mode {
            SearchOutputMode::Content => "pattern",
            SearchOutputMode::FilesWithMatches => "files",
            SearchOutputMode::Count => "count",
        };
        parts.push(vec![
            Span::styled("mode: ", label_style),
            Span::styled(mode_str.to_string(), value_style),
        ]);
        if let Some(ref file_type) = self.meta.file_type {
            parts.push(vec![
                Span::styled("type: ", label_style),
                Span::styled(file_type.to_string(), value_style),
            ]);
        }
        if self.meta.case_insensitive {
            parts.push(vec![
                Span::styled("case-insensitive: ", label_style),
                Span::styled("true", value_style),
            ]);
        }
        if self.meta.multiline {
            parts.push(vec![
                Span::styled("multiline: ", label_style),
                Span::styled("true", value_style),
            ]);
        }

        let mut spans: Vec<Span<'static>> = vec![Span::styled("  ".to_string(), label_style)];
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(", ", label_style));
            }
            spans.extend(part);
        }
        Line::from(spans)
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let theme = ctx.theme;
        let tool_cfg = &ctx.appearance.scrollback.blocks.tool;
        let muted = ctx.mode == DisplayMode::Collapsed
            && tool_cfg.muted_collapsed
            && !ctx.is_running
            && !ctx.is_selected;
        let dim_details = tool_cfg.dim_details;

        match ctx.mode {
            DisplayMode::Collapsed => BlockOutput {
                lines: vec![self.header_block_line(self.header_line(
                    &theme,
                    muted,
                    dim_details,
                    Some(ctx.width),
                ))],
            },
            DisplayMode::Truncated | DisplayMode::Expanded => {
                let mut lines: Vec<BlockLine> = vec![self.header_block_line(self.header_line(
                    &theme,
                    false,
                    dim_details,
                    None,
                ))];
                lines.push(BlockLine::separator(Line::from("")));
                lines.push(BlockLine::separator(self.metadata_line(&theme)));

                let has_results = !self.file_matches.is_empty() || !self.file_paths.is_empty();
                if has_results {
                    lines.push(BlockLine::separator(Line::from("")));
                } else if self.match_count == 0 && self.error.is_none() {
                    lines.push(BlockLine::separator(Line::from("")));
                    lines.push(BlockLine::separator(Line::from(Span::styled(
                        "  (no results)".to_string(),
                        theme.muted(),
                    ))));
                }

                if let Some(error) = &self.error {
                    lines.push(BlockLine::content(Line::from(Span::styled(
                        error.clone(),
                        theme.fg(theme.accent_error),
                    ))));
                    return BlockOutput { lines };
                }

                if !self.file_matches.is_empty() {
                    let indent = "  ";
                    let match_indent = "    ";
                    for (i, file_match) in self.file_matches.iter().enumerate() {
                        if i > 0 {
                            lines.push(BlockLine::separator(Line::from("")));
                        }
                        lines.push(
                            BlockLine::content(Line::from(Span::styled(
                                format!("{}{}", indent, file_match.path),
                                theme.fg(theme.path),
                            )))
                            .with_panel_background(theme.bg_dark),
                        );
                        for matched in &file_match.matches {
                            let line_num_str = format!("{:>4}", matched.line_number);
                            let content_trimmed = matched.content.trim_end();
                            lines.push(
                                BlockLine::content(Line::from(vec![
                                    Span::styled(match_indent.to_string(), theme.primary()),
                                    Span::styled(line_num_str, theme.muted()),
                                    Span::styled("  ".to_string(), theme.primary()),
                                    Span::styled(content_trimmed.to_string(), theme.primary()),
                                ]))
                                .with_panel_background(theme.bg_dark),
                            );
                        }
                    }
                } else if !self.file_paths.is_empty() {
                    let indent = "  ";
                    let is_count = self.meta.output_mode == SearchOutputMode::Count;
                    for path in &self.file_paths {
                        let line = if is_count {
                            if let Some(colon_pos) = path.rfind(':') {
                                let file_part = &path[..colon_pos];
                                let count_part = &path[colon_pos..];
                                Line::from(vec![
                                    Span::styled(
                                        format!("{indent}{file_part}"),
                                        theme.fg(theme.path),
                                    ),
                                    Span::styled(count_part.to_string(), theme.primary()),
                                ])
                            } else {
                                Line::from(Span::styled(
                                    format!("{indent}{path}"),
                                    theme.fg(theme.path),
                                ))
                            }
                        } else {
                            Line::from(Span::styled(
                                format!("{indent}{path}"),
                                theme.fg(theme.path),
                            ))
                        };
                        lines.push(BlockLine::content(line).with_panel_background(theme.bg_dark));
                    }
                }

                if self.truncated {
                    lines.push(BlockLine::separator(Line::from(Span::styled(
                        format!("  … results truncated · {} total", self.match_count),
                        theme.muted(),
                    ))));
                }
                BlockOutput { lines }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::GrokAppearanceSnapshot;
    use crate::scrollback::tool::ToolBlockContext;

    fn context(mode: DisplayMode) -> ToolBlockContext<'static> {
        let theme = *Theme::current();
        let appearance = Box::leak(Box::new(
            GrokAppearanceSnapshot::default().scrollback(theme),
        ));
        ToolBlockContext {
            mode,
            is_running: false,
            is_selected: false,
            width: 80,
            appearance,
            theme,
        }
    }

    fn header_text(block: &SearchToolCallBlock, mode: DisplayMode) -> String {
        block.output(context(mode)).lines[0].content.to_string()
    }

    #[test]
    fn collapsed_header_quotes_pattern_and_summarizes_matches() {
        let mut block = SearchToolCallBlock::new("TODO");
        block.match_count = 3;
        block.file_matches.push(SearchFileMatch {
            path: "src/lib.rs".into(),
            matches: vec![SearchLineMatch {
                line_number: 9,
                content: "// TODO".into(),
            }],
        });
        let header = header_text(&block, DisplayMode::Collapsed);
        assert!(header.contains("Search "), "{header}");
        assert!(header.contains("\"TODO\""), "{header}");
        assert!(header.contains("(3 matches)"), "{header}");
    }

    #[test]
    fn glob_promotion_drops_quotes_and_uses_file_summary() {
        let mut block = SearchToolCallBlock::new(".");
        block.meta.glob = Some("*.rs".into());
        block.meta.output_mode = SearchOutputMode::FilesWithMatches;
        block.match_count = 2;
        block.file_paths = vec!["a.rs".into(), "b.rs".into()];
        let header = header_text(&block, DisplayMode::Collapsed);
        assert!(header.contains("*.rs"), "{header}");
        assert!(!header.contains("\"*.rs\""), "{header}");
        assert!(header.contains("(2 files)"), "{header}");
    }

    #[test]
    fn expanded_groups_use_path_color_and_panel_background() {
        let mut block = SearchToolCallBlock::new("TODO");
        block.match_count = 1;
        block.file_matches.push(SearchFileMatch {
            path: "src/lib.rs".into(),
            matches: vec![SearchLineMatch {
                line_number: 9,
                content: "// TODO".into(),
            }],
        });
        let theme = *Theme::current();
        let lines = block.output(context(DisplayMode::Expanded)).lines;
        let path_line = lines
            .iter()
            .find(|line| line.content.to_string().contains("src/lib.rs"))
            .expect("path group");
        assert_eq!(path_line.background, Some(theme.bg_dark));
        assert!(path_line.background_is_panel);
        assert_eq!(path_line.content.spans[0].style.fg, Some(theme.path));

        let match_line = lines
            .iter()
            .find(|line| line.content.to_string().contains("// TODO"))
            .expect("match");
        assert_eq!(match_line.background, Some(theme.bg_dark));
        assert!(
            match_line
                .content
                .spans
                .iter()
                .any(|span| span.content.contains("9") && span.style.fg == Some(theme.gray))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.content.to_string().contains("mode: pattern"))
        );
    }

    #[test]
    fn truncated_flag_keeps_dsh_total_notice() {
        let mut block = SearchToolCallBlock::new("TODO");
        block.match_count = 3;
        block.truncated = true;
        block.file_matches.push(SearchFileMatch {
            path: "src/lib.rs".into(),
            matches: vec![SearchLineMatch {
                line_number: 9,
                content: "// TODO".into(),
            }],
        });
        let text = block
            .output(context(DisplayMode::Expanded))
            .lines
            .iter()
            .map(|line| line.content.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("3 total"), "{text}");
    }
}
