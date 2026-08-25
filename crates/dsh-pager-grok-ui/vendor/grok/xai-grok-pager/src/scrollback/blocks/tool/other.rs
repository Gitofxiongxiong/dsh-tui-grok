//! Explicit fallback for tools without a richer neutral block.

use crate::scrollback::{
    tool::{ToolBlockContext, header_line, text_line},
    types::{BlockOutput, DisplayMode},
};

#[derive(Debug, Clone)]
pub struct OtherToolCallBlock {
    pub name: String,
    pub title: String,
    pub input: Option<String>,
    pub output_text: Option<String>,
    pub error: Option<String>,
}

impl OtherToolCallBlock {
    pub fn new(name: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: title.into(),
            input: None,
            output_text: None,
            error: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let mut lines = vec![header_line("", &self.title, None, ctx)];
        if ctx.mode == DisplayMode::Collapsed {
            return BlockOutput { lines };
        }
        if let Some(input) = &self.input {
            for line in input.lines() {
                lines.push(text_line(line.to_string(), ctx));
            }
        }
        if let Some(output) = &self.output_text {
            for line in output.lines() {
                lines.push(text_line(line.to_string(), ctx));
            }
        }
        if let Some(error) = &self.error {
            lines.push(text_line(error, ctx));
        }
        BlockOutput { lines }
    }
}
