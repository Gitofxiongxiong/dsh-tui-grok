//! Search tool block, retaining Grok's typed file/match output shape.

use crate::scrollback::{
    tool::{ToolBlockContext, header_line, text_line},
    types::{BlockOutput, DisplayMode},
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

#[derive(Debug, Clone)]
pub struct SearchToolCallBlock {
    pub pattern: String,
    pub match_count: usize,
    pub file_matches: Vec<SearchFileMatch>,
    pub file_paths: Vec<String>,
    pub error: Option<String>,
    pub truncated: bool,
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
        }
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    fn summary(&self) -> String {
        match self.match_count {
            0 => "(no matches)".into(),
            1 => "(1 match)".into(),
            count => format!("({count} matches)"),
        }
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let mut lines = vec![header_line(
            "Search",
            &self.pattern,
            Some(&self.summary()),
            ctx,
        )];
        if ctx.mode == DisplayMode::Collapsed {
            return BlockOutput { lines };
        }
        if let Some(error) = &self.error {
            lines.push(text_line(error, ctx));
            return BlockOutput { lines };
        }
        let limit = if ctx.mode == DisplayMode::Truncated {
            6
        } else {
            usize::MAX
        };
        let mut painted = 0usize;
        for file in &self.file_matches {
            if painted >= limit {
                break;
            }
            lines.push(text_line(format!("  {}", file.path), ctx));
            painted += 1;
            for matched in &file.matches {
                if painted >= limit {
                    break;
                }
                lines.push(text_line(
                    format!(
                        "    {:>4}  {}",
                        matched.line_number,
                        matched.content.trim_end()
                    ),
                    ctx,
                ));
                painted += 1;
            }
        }
        for path in self.file_paths.iter().take(limit.saturating_sub(painted)) {
            lines.push(text_line(path.clone(), ctx));
            painted += 1;
        }
        if self.truncated || painted < self.match_count {
            lines.push(text_line(
                format!("… results truncated · {} total", self.match_count),
                ctx,
            ));
        }
        BlockOutput { lines }
    }
}
