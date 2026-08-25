//! Width/vpad/background materialization extracted from Grok's BlockRenderer.

use ratatui::{buffer::Buffer, layout::Rect, style::Style, text::Span};

use crate::{
    render::wrapping::word_wrap_line_with_joiners,
    scrollback::types::{BlockLine, BlockOutput, RenderedBlock},
};

#[derive(Debug, Clone, Copy)]
pub struct BlockRenderSpec {
    pub width: usize,
    pub base_background: ratatui::style::Color,
}

pub struct BlockRenderer;

impl BlockRenderer {
    pub fn desired_height(block: &RenderedBlock, spec: BlockRenderSpec) -> usize {
        Self::render(block.clone(), spec).output.lines.len()
    }

    pub fn render(block: RenderedBlock, spec: BlockRenderSpec) -> RenderedBlock {
        let width = spec.width.max(1);
        let mut lines = Vec::new();
        for source in block.output.lines {
            let (wrapped, joiners) = word_wrap_line_with_joiners(&source.content, width);
            for (index, (content, wrap_joiner)) in wrapped.into_iter().zip(joiners).enumerate() {
                let mut line = BlockLine {
                    content,
                    background: source.background.or(block.background),
                    bg_start_col: source.bg_start_col,
                    background_is_panel: source.background_is_panel,
                    selectable: source.selectable,
                    header: source.header && index == 0,
                    joiner: if index == 0 {
                        source.joiner.clone()
                    } else {
                        wrap_joiner
                    },
                };
                Self::apply_background(&mut line, spec.base_background);
                lines.push(line);
            }
        }
        if block.vpad {
            let background = block.background;
            let mut blank = BlockLine::spacer();
            blank.background = background;
            blank.content.spans.push(Span::styled(
                " ".repeat(width),
                Style::default().bg(background.unwrap_or(spec.base_background)),
            ));
            lines.insert(0, blank.clone());
            lines.push(blank);
        }
        RenderedBlock {
            output: BlockOutput { lines },
            ..block
        }
    }

    fn apply_background(line: &mut BlockLine, base_background: ratatui::style::Color) {
        let Some(background) = line.background else {
            return;
        };
        if line.bg_start_col == 0 {
            for span in &mut line.content.spans {
                span.style = span.style.bg(background);
            }
        }
        if background == ratatui::style::Color::Reset {
            for span in &mut line.content.spans {
                span.style = span.style.bg(base_background);
            }
        }
    }

    /// Direct Buffer painter matching Grok's wrapper shape. `skip_rows`
    /// clips a partially visible block without allocating a scratch buffer.
    pub fn render_buffer(
        block: RenderedBlock,
        spec: BlockRenderSpec,
        area: Rect,
        buf: &mut Buffer,
        skip_rows: usize,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let rendered = Self::render(block, spec);
        for (screen_row, line) in rendered
            .output
            .lines
            .iter()
            .skip(skip_rows)
            .take(area.height as usize)
            .enumerate()
        {
            let y = area.y.saturating_add(screen_row as u16);
            let background = line.background.unwrap_or(spec.base_background);
            let owned = Rect::new(area.x, y, area.width, 1);
            buf.set_style(owned, Style::default().bg(background));
            for x in owned.x..owned.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ');
                }
            }
            if let Some(line_background) = line.background {
                let x = area.x.saturating_add(line.bg_start_col.min(area.width));
                let width = area.right().saturating_sub(x);
                if width > 0 {
                    buf.set_style(
                        Rect::new(x, y, width, 1),
                        Style::default().bg(line_background),
                    );
                }
            }
            buf.set_line(area.x, y, &line.content, area.width);
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{style::Color, text::Line};

    use super::*;
    use crate::scrollback::types::{BlockLine, BlockOutput, RenderedBlock};

    #[test]
    fn vpad_and_background_are_materialized_once() {
        let mut block = RenderedBlock::plain(BlockOutput {
            lines: vec![BlockLine::content(Line::from("body"))],
        });
        block.vpad = true;
        block.background = Some(Color::Blue);
        let rendered = BlockRenderer::render(
            block,
            BlockRenderSpec {
                width: 8,
                base_background: Color::Black,
            },
        );
        assert_eq!(rendered.output.lines.len(), 3);
        assert_eq!(rendered.output.lines[0].content.width(), 8);
        assert_eq!(rendered.output.lines[1].content.to_string(), "body");
        assert!(!rendered.output.lines[0].selectable);
    }

    #[test]
    fn direct_buffer_render_clips_without_a_scratch_buffer() {
        let block = RenderedBlock::plain(BlockOutput {
            lines: vec![
                BlockLine::content(Line::from("first")),
                BlockLine::content(Line::from("second")),
            ],
        });
        let spec = BlockRenderSpec {
            width: 12,
            base_background: Color::Black,
        };
        assert_eq!(BlockRenderer::desired_height(&block, spec), 2);
        let area = Rect::new(0, 0, 12, 1);
        let mut buffer = Buffer::empty(area);
        BlockRenderer::render_buffer(block, spec, area, &mut buffer, 1);
        assert_eq!(
            (0..6).map(|x| buffer[(x, 0)].symbol()).collect::<String>(),
            "second"
        );
    }
}
