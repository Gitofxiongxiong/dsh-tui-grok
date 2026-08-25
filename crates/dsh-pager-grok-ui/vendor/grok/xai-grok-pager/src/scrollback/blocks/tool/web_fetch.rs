//! Web-fetch tool block.

use crate::scrollback::{
    tool::{ToolBlockContext, header_line, text_line},
    types::{BlockOutput, DisplayMode},
};

#[derive(Debug, Clone)]
pub struct WebFetchToolCallBlock {
    pub url: String,
    pub status_code: Option<u64>,
    pub content: Option<String>,
    pub error: Option<String>,
    pub truncated: bool,
}

impl WebFetchToolCallBlock {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            status_code: None,
            content: None,
            error: None,
            truncated: false,
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.status_code.is_none_or(|status| status < 400)
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let detail = self.status_code.map(|status| format!("(HTTP {status})"));
        let mut lines = vec![header_line("Fetch", &self.url, detail.as_deref(), ctx)];
        if ctx.mode == DisplayMode::Collapsed {
            return BlockOutput { lines };
        }
        if let Some(error) = &self.error {
            lines.push(text_line(error, ctx));
        } else if let Some(content) = &self.content {
            let limit = if ctx.mode == DisplayMode::Truncated {
                6
            } else {
                usize::MAX
            };
            for line in content.lines().take(limit) {
                lines.push(text_line(line.to_string(), ctx));
            }
            if self.truncated || content.lines().count() > limit {
                lines.push(text_line("… content truncated", ctx));
            }
        }
        BlockOutput { lines }
    }
}
