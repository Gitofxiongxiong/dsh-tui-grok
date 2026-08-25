//! B adaptation of Grok Build `scrollback/blocks/subagent.rs`.
//!
//! Grok suppresses the spawn tool and paints a dedicated lifecycle row:
//! bold `Subagent` + muted `running:` / `started:` + quoted description.
//! DSH keeps the spawn as a tool call, so the same header contract lives in
//! the tool family and is projected from `task` / `subagent` / `spawn_subagent`.

use ratatui::{
    style::Modifier,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    render::color::blend_color,
    render::line_utils::truncate_str,
    scrollback::{
        tool::ToolBlockContext,
        types::{AccentStyle, BlockLine, BlockOutput},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentToolKind {
    Running,
    Started,
    Failed,
}

#[derive(Debug, Clone)]
pub struct SubagentToolCallBlock {
    pub description: String,
    pub kind: SubagentToolKind,
    pub error: Option<String>,
}

impl SubagentToolCallBlock {
    pub fn new(description: impl Into<String>, kind: SubagentToolKind) -> Self {
        Self {
            description: description.into(),
            kind,
            error: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.kind != SubagentToolKind::Failed
    }

    pub fn output(&self, ctx: ToolBlockContext<'_>) -> BlockOutput {
        let theme = ctx.theme;
        let bold = if ctx.is_selected || ctx.is_running {
            theme.primary().add_modifier(Modifier::BOLD)
        } else {
            theme.muted().add_modifier(Modifier::BOLD)
        };
        let muted = theme.muted();
        let verb = match self.kind {
            SubagentToolKind::Started => "started: ",
            SubagentToolKind::Failed => "failed: ",
            SubagentToolKind::Running => "running: ",
        };
        let detail = self
            .error
            .as_deref()
            .filter(|error| !error.is_empty())
            .map(|error| format!(" ({error})"))
            .unwrap_or_default();
        let overhead = 18 + verb.len().saturating_sub(9) + detail.width();
        let desc = quoted_desc(&self.description, (ctx.width).saturating_sub(overhead));
        let mut spans = vec![
            Span::styled("Subagent ", bold),
            Span::styled(verb, muted),
            Span::styled(desc, muted),
        ];
        if !detail.is_empty() {
            spans.push(Span::styled(detail, muted));
        }
        BlockOutput {
            lines: vec![BlockLine::header(Line::from(spans))],
        }
    }

    pub fn bullet_style(&self, ctx: ToolBlockContext<'_>) -> Option<AccentStyle> {
        let theme = ctx.theme;
        match self.kind {
            SubagentToolKind::Running | SubagentToolKind::Started if ctx.is_running => {
                let dimmed = blend_color(
                    theme.bg_base,
                    theme.accent_running,
                    ctx.appearance.scrollback.display.dim_accent,
                )
                .unwrap_or(theme.accent_running);
                Some(AccentStyle::animated(dimmed))
            }
            SubagentToolKind::Failed => Some(AccentStyle::static_color(theme.accent_error)),
            _ => None,
        }
    }
}

fn quoted_desc(desc: &str, max_width: usize) -> String {
    if max_width <= 2 {
        return "\u{201C}\u{2026}\u{201D}".to_string();
    }
    let inner = truncate_str(desc, max_width.saturating_sub(2));
    format!("\u{201C}{inner}\u{201D}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{appearance::GrokAppearanceSnapshot, scrollback::types::DisplayMode, theme::Theme};

    fn context(running: bool) -> ToolBlockContext<'static> {
        let theme = *Theme::current();
        let appearance = Box::leak(Box::new(
            GrokAppearanceSnapshot::default().scrollback(theme),
        ));
        ToolBlockContext {
            mode: DisplayMode::Collapsed,
            is_running: running,
            is_selected: false,
            width: 80,
            appearance,
            theme,
        }
    }

    #[test]
    fn collapsed_header_quotes_the_description() {
        let block = SubagentToolCallBlock::new("分析 Rust 工作区架构", SubagentToolKind::Running);
        let line = block.output(context(true)).lines[0]
            .content
            .spans
            .iter()
            .map(|span| span.content.clone())
            .collect::<String>();
        assert!(line.contains("Subagent"), "{line}");
        assert!(line.contains("running:"), "{line}");
        assert!(line.contains("分析 Rust 工作区架构"), "{line}");
    }
}
