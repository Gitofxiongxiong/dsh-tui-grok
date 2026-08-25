//! Directory-list tool block.

use crate::scrollback::{
    tool::{ToolBlockContext, header_line, text_line},
    types::{BlockOutput, DisplayMode},
};

#[derive(Debug, Clone)]
pub struct ListDirToolCallBlock {
    pub path: String,
    pub entries: Vec<String>,
    pub error: Option<String>,
}

impl ListDirToolCallBlock {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            entries: Vec::new(),
            error: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let detail = (!self.entries.is_empty()).then(|| {
            format!(
                "({} {})",
                self.entries.len(),
                if self.entries.len() == 1 {
                    "entry"
                } else {
                    "entries"
                }
            )
        });
        let mut lines = vec![header_line("List", &self.path, detail.as_deref(), ctx)];
        if ctx.mode == DisplayMode::Collapsed {
            return BlockOutput { lines };
        }
        if let Some(error) = &self.error {
            lines.push(text_line(error, ctx));
        } else {
            for entry in &self.entries {
                lines.push(text_line(entry.clone(), ctx));
            }
        }
        BlockOutput { lines }
    }
}
