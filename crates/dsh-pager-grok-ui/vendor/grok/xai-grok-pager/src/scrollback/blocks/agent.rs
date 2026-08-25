//! Grok agent message block over the fixed markdown renderer's styled lines.

use ratatui::text::Line;

use crate::scrollback::types::{BlockLine, BlockOutput, RenderedBlock};

#[derive(Debug, Clone)]
pub struct AgentMessageBlock {
    body: Vec<Line<'static>>,
}

impl AgentMessageBlock {
    pub fn new(body: Vec<Line<'static>>) -> Self {
        Self { body }
    }

    pub fn render(self) -> RenderedBlock {
        RenderedBlock::plain(BlockOutput {
            lines: self.body.into_iter().map(BlockLine::content).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};

    #[test]
    fn agent_message_has_no_local_chrome_or_vpad() {
        let rendered = AgentMessageBlock::new(vec![Line::from("answer")]).render();
        assert_eq!(rendered.output.lines.len(), 1);
        assert_eq!(rendered.accent, None);
        assert!(!rendered.vpad);
    }

    #[test]
    fn agent_message_preserves_markdown_rows_and_styles_without_rewrapping() {
        let rendered = AgentMessageBlock::new(vec![
            Line::from(ratatui::text::Span::styled(
                "heading",
                Style::default().fg(Color::Cyan),
            )),
            Line::from("body"),
        ])
        .render();
        assert_eq!(rendered.output.lines.len(), 2);
        assert_eq!(
            rendered.output.lines[0].content.spans[0].style.fg,
            Some(Color::Cyan)
        );
        assert_eq!(rendered.output.lines[1].content.to_string(), "body");
        assert!(rendered.output.lines.iter().all(|line| line.selectable));
    }
}
