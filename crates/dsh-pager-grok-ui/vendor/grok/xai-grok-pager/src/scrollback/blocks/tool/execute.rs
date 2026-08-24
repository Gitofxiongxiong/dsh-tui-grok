//! ExecuteToolCallBlock - runs shell commands with streaming output.
//!
//! Derived from Grok Build at SOURCE_REV
//! `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`.
//!
//! Local structural adaptation: the original `BlockContent`, appearance store,
//! permission-overlay highlighter and selection-range types are projected into
//! the renderer-only context below.  The component still owns Grok's display
//! modes, description-first headers, output truncation, panel metadata, accent,
//! foldability and finish-mode rules.  Host/Harness DTO mapping stays outside
//! this vendored file.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::render::line_utils::truncate_str;
use crate::render::wrapping::word_wrap_line;
use crate::theme::Theme;

/// How the Execute component is currently displayed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum DisplayMode {
    Collapsed,
    Truncated,
    #[default]
    Expanded,
}

/// Renderer-only projection of Grok's Execute block context.
#[derive(Debug, Clone, Copy)]
pub struct ExecuteBlockContext<'a> {
    pub mode: DisplayMode,
    pub is_running: bool,
    pub width: usize,
    pub theme: &'a Theme,
    pub muted_command_collapsed: bool,
    pub first_lines: usize,
    pub last_lines: usize,
}

impl<'a> ExecuteBlockContext<'a> {
    pub fn new(mode: DisplayMode, is_running: bool, width: usize, theme: &'a Theme) -> Self {
        Self {
            mode,
            is_running,
            width: width.max(1),
            theme,
            muted_command_collapsed: false,
            first_lines: 3,
            last_lines: 3,
        }
    }
}

/// One component output row. Panel metadata is consumed by the scrollback
/// wrapper, mirroring Grok's `BlockLine::panel_background` boundary.
#[derive(Debug, Clone)]
pub struct ExecuteBlockLine {
    pub content: Line<'static>,
    pub panel_background: Option<Color>,
    pub selectable_span_start: usize,
    pub joiner: Option<String>,
}

impl ExecuteBlockLine {
    fn header(content: Line<'static>, selectable_span_start: usize) -> Self {
        Self {
            content,
            panel_background: None,
            selectable_span_start,
            joiner: None,
        }
    }

    fn separator() -> Self {
        Self::header(Line::default(), 0)
    }

    fn panel(content: Line<'static>, background: Color, joiner: Option<String>) -> Self {
        Self {
            content,
            panel_background: Some(background),
            selectable_span_start: 0,
            joiner,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExecuteBlockOutput {
    pub lines: Vec<ExecuteBlockLine>,
}

/// Execute tool call - runs a shell command.
#[derive(Debug, Clone)]
pub struct ExecuteToolCallBlock {
    /// Full command that was run (copy/export source of truth).
    pub command: String,
    /// Error message if the command failed (None = success).
    pub error: Option<String>,
    /// Optional model-supplied description of what the command does.
    pub description: Option<String>,
    /// Terminal output, streamed incrementally by the host projection.
    pub output: Option<String>,
    /// Whether this is a user-initiated bash-mode (`!`) command.
    pub bash_mode: bool,
    /// Peeled display form for the header; `command` remains the full source.
    pub header_display: Option<String>,
}

impl ExecuteToolCallBlock {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            error: None,
            description: None,
            output: None,
            bash_mode: false,
            header_display: None,
        }
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    pub fn push_output(&mut self, chunk: &str) {
        match &mut self.output {
            Some(output) => output.push_str(chunk),
            None => self.output = Some(chunk.to_string()),
        }
    }

    pub fn copy_text(&self) -> String {
        self.output.clone().unwrap_or_default()
    }

    fn command_display(&self) -> &str {
        self.header_display.as_deref().unwrap_or(&self.command)
    }

    /// Non-empty, single-logical-line description for the header.
    fn description_display(&self, strip_run_prefix: bool) -> Option<String> {
        self.description.as_ref().and_then(|description| {
            let trimmed = description.trim();
            if trimmed.is_empty() {
                return None;
            }
            let mut text = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
            if strip_run_prefix {
                text = strip_leading_run_word(&text);
                if text.is_empty() {
                    return None;
                }
            }
            Some(text)
        })
    }

    fn command_body(&self) -> String {
        let command = self.command_display();
        if command.trim().is_empty() {
            "…".to_string()
        } else {
            command.replace('\n', " ")
        }
    }

    fn label_title_line(&self, theme: &Theme, muted: bool, title: &str) -> Line<'static> {
        let label_style = Style::default()
            .fg(if muted {
                theme.gray
            } else {
                theme.text_primary
            })
            .add_modifier(Modifier::BOLD);
        let mut spans = vec![Span::styled("Run ", label_style)];
        if self.bash_mode {
            spans.push(Span::styled("(user) ", Style::default().fg(theme.gray)));
        }
        spans.push(Span::styled(
            title.to_string(),
            Style::default().fg(if muted {
                theme.gray
            } else {
                theme.text_primary
            }),
        ));
        Line::from(spans)
    }

    fn shell_command_line(&self, theme: &Theme, muted: bool) -> Line<'static> {
        Line::from(vec![
            Span::styled("$ ", Style::default().fg(theme.gray_dim)),
            Span::styled(
                self.command_body(),
                Style::default().fg(if muted { theme.gray } else { theme.command }),
            ),
        ])
    }

    fn collapsed_header(&self, ctx: &ExecuteBlockContext<'_>) -> ExecuteBlockLine {
        let theme = ctx.theme;
        let muted = ctx.muted_command_collapsed;
        let prefix_width = 4 + usize::from(self.bash_mode) * 7;
        if let Some(description) = self.description_display(true) {
            let description = truncate_str(&description, ctx.width.saturating_sub(prefix_width));
            return ExecuteBlockLine::header(
                self.label_title_line(theme, muted, &description),
                1 + usize::from(self.bash_mode),
            );
        }

        let command = truncate_str(&self.command_body(), ctx.width.saturating_sub(prefix_width));
        ExecuteBlockLine::header(
            self.label_title_line(theme, muted, &command),
            1 + usize::from(self.bash_mode),
        )
    }

    fn push_wrapped_header(
        &self,
        lines: &mut Vec<ExecuteBlockLine>,
        ctx: &ExecuteBlockContext<'_>,
    ) {
        let theme = ctx.theme;
        if let Some(description) = self.description_display(true) {
            let title = self.label_title_line(theme, false, &description);
            push_hanging_line(
                lines,
                title,
                ctx.width,
                4 + usize::from(self.bash_mode) * 7,
                1 + usize::from(self.bash_mode),
            );
            push_hanging_line(
                lines,
                self.shell_command_line(theme, false),
                ctx.width,
                2,
                1,
            );
        } else {
            let title = self.label_title_line(theme, false, &self.command_body());
            push_hanging_line(
                lines,
                title,
                ctx.width,
                4 + usize::from(self.bash_mode) * 7,
                1 + usize::from(self.bash_mode),
            );
        }
    }

    fn render_with_truncation(
        &self,
        ctx: &ExecuteBlockContext<'_>,
        truncate: Option<(usize, usize)>,
    ) -> ExecuteBlockOutput {
        let mut lines = Vec::new();
        self.push_wrapped_header(&mut lines, ctx);

        if self.output.is_none()
            && let Some(error) = self.error.as_deref()
            && !error.is_empty()
        {
            lines.push(ExecuteBlockLine::separator());
            for line in error.lines() {
                lines.push(ExecuteBlockLine::header(
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(ctx.theme.accent_error),
                    )),
                    0,
                ));
            }
        }

        if let Some(output) = self.output.as_deref()
            && !output.is_empty()
        {
            lines.push(ExecuteBlockLine::separator());
            let mut wrapped = Vec::new();
            let wrap_width = ctx.width.saturating_sub(2).max(1);
            for source_line in output.lines() {
                let line = Line::from(Span::styled(
                    source_line.to_string(),
                    Style::default().fg(ctx.theme.text_primary),
                ));
                let rows = word_wrap_line(&line, wrap_width);
                for (index, row) in rows.into_iter().enumerate() {
                    wrapped.push((row, (index > 0).then_some(String::new())));
                }
            }

            let total = wrapped.len();
            let (first, last) = truncate.unwrap_or((total, 0));
            if truncate.is_some() && total > first.saturating_add(last) {
                for (line, joiner) in wrapped.iter().take(first) {
                    lines.push(ExecuteBlockLine::panel(
                        line.clone(),
                        ctx.theme.bg_terminal,
                        joiner.clone(),
                    ));
                }
                lines.push(ExecuteBlockLine::panel(
                    Line::from(Span::styled(
                        format!("… +{} lines", total - first - last),
                        Style::default().fg(ctx.theme.gray),
                    )),
                    ctx.theme.bg_terminal,
                    None,
                ));
                for (line, joiner) in wrapped.iter().skip(total - last) {
                    lines.push(ExecuteBlockLine::panel(
                        line.clone(),
                        ctx.theme.bg_terminal,
                        joiner.clone(),
                    ));
                }
            } else {
                for (line, joiner) in wrapped {
                    lines.push(ExecuteBlockLine::panel(line, ctx.theme.bg_terminal, joiner));
                }
            }
        }

        ExecuteBlockOutput { lines }
    }

    pub fn output(&self, ctx: &ExecuteBlockContext<'_>) -> ExecuteBlockOutput {
        match ctx.mode {
            DisplayMode::Collapsed => ExecuteBlockOutput {
                lines: vec![self.collapsed_header(ctx)],
            },
            DisplayMode::Truncated => self
                .render_with_truncation(ctx, Some((ctx.first_lines.max(1), ctx.last_lines.max(1)))),
            DisplayMode::Expanded => self.render_with_truncation(ctx, None),
        }
    }

    pub fn accent(&self, ctx: &ExecuteBlockContext<'_>) -> Color {
        if self.error.is_some() {
            ctx.theme.accent_error
        } else if ctx.is_running {
            ctx.theme.accent_running
        } else {
            ctx.theme.accent_success
        }
    }

    pub fn is_foldable(&self) -> bool {
        self.description_display(true).is_some() || self.output.is_some() || self.error.is_some()
    }

    pub fn collapse_mode(&self, is_running: bool) -> DisplayMode {
        if self.bash_mode && is_running {
            DisplayMode::Truncated
        } else {
            DisplayMode::Collapsed
        }
    }

    pub fn default_display_mode(&self) -> DisplayMode {
        if self.bash_mode {
            DisplayMode::Truncated
        } else {
            DisplayMode::Collapsed
        }
    }

    pub fn finished_display_mode(&self) -> Option<DisplayMode> {
        self.bash_mode.then_some(DisplayMode::Expanded)
    }
}

fn push_hanging_line(
    lines: &mut Vec<ExecuteBlockLine>,
    line: Line<'static>,
    width: usize,
    hanging_indent: usize,
    selectable_span_start: usize,
) {
    for (index, mut wrapped) in word_wrap_line(&line, width.max(1)).into_iter().enumerate() {
        if index > 0 {
            wrapped
                .spans
                .insert(0, Span::raw(" ".repeat(hanging_indent)));
        }
        let mut output =
            ExecuteBlockLine::header(wrapped, if index == 0 { selectable_span_start } else { 0 });
        output.joiner = (index > 0).then_some(" ".to_string());
        lines.push(output);
    }
}

/// Drop a leading Run/Running word so Label headers never read `Run Run …`.
fn strip_leading_run_word(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("running") {
        rest
    } else if let Some(rest) = lower.strip_prefix("run") {
        rest
    } else {
        return input.to_string();
    };
    if rest.is_empty() {
        return String::new();
    }
    if !rest.starts_with(char::is_whitespace) {
        return input.to_string();
    }
    let prefix_len = input.len() - rest.len();
    input[prefix_len..].trim_start().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(output: ExecuteBlockOutput) -> Vec<String> {
        output
            .lines
            .into_iter()
            .map(|line| line.content.to_string())
            .collect()
    }

    #[test]
    fn collapsed_header_with_description_hides_command() {
        let block = ExecuteToolCallBlock::new("cargo test --lib")
            .with_description("Run the unit test suite");
        let theme = Theme::current();
        let output = block.output(&ExecuteBlockContext::new(
            DisplayMode::Collapsed,
            false,
            80,
            theme,
        ));
        assert_eq!(text(output), vec!["Run the unit test suite"]);
    }

    #[test]
    fn expanded_description_shows_title_then_command_and_panel() {
        let block = ExecuteToolCallBlock::new("cargo test --lib")
            .with_description("Run the unit test suite")
            .with_output("one\ntwo\n");
        let theme = Theme::current();
        let output = block.output(&ExecuteBlockContext::new(
            DisplayMode::Expanded,
            false,
            80,
            theme,
        ));
        let rendered = text(output.clone());
        assert_eq!(rendered[0], "Run the unit test suite");
        assert_eq!(rendered[1], "$ cargo test --lib");
        assert_eq!(rendered[2], "");
        assert_eq!(rendered[3], "one");
        assert_eq!(rendered[4], "two");
        assert_eq!(output.lines[3].panel_background, Some(theme.bg_terminal));
        assert_eq!(output.lines[4].panel_background, Some(theme.bg_terminal));
    }

    #[test]
    fn truncated_output_keeps_head_tail_and_hidden_count() {
        let block = ExecuteToolCallBlock::new("seq 8")
            .with_description("List numbers")
            .with_output("1\n2\n3\n4\n5\n6\n7\n8");
        let theme = Theme::current();
        let mut ctx = ExecuteBlockContext::new(DisplayMode::Truncated, true, 80, theme);
        ctx.first_lines = 2;
        ctx.last_lines = 2;
        let rendered = text(block.output(&ctx)).join("\n");
        assert!(rendered.contains("1\n2\n… +4 lines\n7\n8"));
    }

    #[test]
    fn strip_leading_run_word_handles_run_and_running() {
        assert_eq!(strip_leading_run_word("Run the tests"), "the tests");
        assert_eq!(strip_leading_run_word("running checks"), "checks");
        assert_eq!(strip_leading_run_word("runtime config"), "runtime config");
        assert_eq!(strip_leading_run_word("Check status"), "Check status");
    }

    #[test]
    fn description_and_output_make_agent_execute_foldable() {
        assert!(
            ExecuteToolCallBlock::new("true")
                .with_description("no-op")
                .is_foldable()
        );
        assert!(
            ExecuteToolCallBlock::new("true")
                .with_output("ok\n")
                .is_foldable()
        );
        assert!(!ExecuteToolCallBlock::new("true").is_foldable());
    }

    #[test]
    fn agent_execute_starts_collapsed_and_preserves_fold_on_finish() {
        let agent = ExecuteToolCallBlock::new("echo hi").with_description("say hi");
        assert_eq!(agent.default_display_mode(), DisplayMode::Collapsed);
        assert_eq!(agent.collapse_mode(true), DisplayMode::Collapsed);
        assert_eq!(agent.finished_display_mode(), None);
    }

    #[test]
    fn completed_and_failed_accents_match_execute_component() {
        let theme = Theme::current();
        let success = ExecuteToolCallBlock::new("true");
        let success_ctx = ExecuteBlockContext::new(DisplayMode::Collapsed, false, 80, theme);
        assert_eq!(success.accent(&success_ctx), theme.accent_success);
        let failure = ExecuteToolCallBlock::new("false").with_error("exit 1");
        assert_eq!(failure.accent(&success_ctx), theme.accent_error);
    }
}
