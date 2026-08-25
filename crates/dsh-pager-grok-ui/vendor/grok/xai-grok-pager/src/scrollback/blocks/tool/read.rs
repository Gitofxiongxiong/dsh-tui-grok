//! Read tool block, adapted from Grok Build's renderer branches.

use crate::scrollback::{
    tool::{ToolBlockContext, header_line, text_line},
    types::{BlockLine, BlockOutput, DisplayMode},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

impl LineRange {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLine {
    pub number: u64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ReadToolCallBlock {
    pub path: String,
    pub line_range: Option<LineRange>,
    pub error: Option<String>,
    pub lines: Vec<ReadLine>,
    pub total_lines: Option<usize>,
    pub language: Option<String>,
}

impl ReadToolCallBlock {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            line_range: None,
            error: None,
            lines: Vec::new(),
            total_lines: None,
            language: None,
        }
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    pub fn is_skill_read(&self) -> bool {
        self.path.ends_with("/SKILL.md") || self.path == "SKILL.md"
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let detail = self.line_range.map(|range| match self.total_lines {
            Some(total) if total > range.end.saturating_sub(range.start) + 1 => {
                format!("({}-{} of {total})", range.start, range.end)
            }
            _ => format!("({}-{})", range.start, range.end),
        });
        let (verb, subject) = if self.is_skill_read() {
            (
                "Skill",
                self.path.rsplit('/').nth(1).unwrap_or(self.path.as_str()),
            )
        } else {
            ("Read", self.path.as_str())
        };
        let mut lines = vec![header_line(verb, subject, detail.as_deref(), ctx)];
        if ctx.mode == DisplayMode::Collapsed {
            return BlockOutput { lines };
        }
        if let Some(error) = &self.error {
            lines.push(text_line(error, ctx));
            return BlockOutput { lines };
        }
        let visible = if ctx.mode == DisplayMode::Truncated && self.lines.len() > 8 {
            5
        } else {
            self.lines.len()
        };
        for line in self.lines.iter().take(visible) {
            lines.push(text_line(format!("{:>4}  {}", line.number, line.text), ctx));
        }
        if visible < self.lines.len() {
            lines.push(BlockLine::content(ratatui::text::Line::from(format!(
                "… {} lines hidden",
                self.lines.len() - visible
            ))));
            for line in self.lines.iter().rev().take(3).rev() {
                lines.push(text_line(format!("{:>4}  {}", line.number, line.text), ctx));
            }
        }
        BlockOutput { lines }
    }
}
