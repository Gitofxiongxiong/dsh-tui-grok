//! Web-search tool block with citation-aware aggregation.

use crate::scrollback::{
    tool::{ToolBlockContext, header_line, text_line},
    types::{BlockOutput, DisplayMode},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSearchSource {
    pub url: String,
    pub title: Option<String>,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WebSearchToolCallBlock {
    pub query: String,
    pub sources: Vec<WebSearchSource>,
    pub citations: Vec<String>,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub truncated: bool,
}

impl WebSearchToolCallBlock {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            sources: Vec::new(),
            citations: Vec::new(),
            answer: None,
            error: None,
            truncated: false,
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let detail = (!self.sources.is_empty()).then(|| {
            format!(
                "({} {})",
                self.sources.len(),
                if self.sources.len() == 1 {
                    "source"
                } else {
                    "sources"
                }
            )
        });
        let mut lines = vec![header_line(
            "Web Search",
            &self.query,
            detail.as_deref(),
            ctx,
        )];
        if ctx.mode == DisplayMode::Collapsed {
            return BlockOutput { lines };
        }
        if let Some(error) = &self.error {
            lines.push(text_line(error, ctx));
            return BlockOutput { lines };
        }
        if let Some(answer) = &self.answer {
            for line in answer.lines() {
                lines.push(text_line(line.to_string(), ctx));
            }
        }
        let limit = if ctx.mode == DisplayMode::Truncated {
            3
        } else {
            usize::MAX
        };
        for source in self.sources.iter().take(limit) {
            lines.push(text_line(
                source.title.as_deref().unwrap_or(&source.url).to_string(),
                ctx,
            ));
            if source.title.is_some() {
                lines.push(text_line(source.url.clone(), ctx));
            }
            if let Some(snippet) = &source.snippet {
                lines.push(text_line(format!("  {snippet}"), ctx));
            }
        }
        if self.truncated || self.sources.len() > limit {
            lines.push(text_line("… sources truncated", ctx));
        }
        BlockOutput { lines }
    }
}
