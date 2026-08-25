//! Renderer-neutral entry wrapper adapted from Grok's `EntryRenderer`.
//!
//! The DSH adapter supplies value-only block lines and stable identity lives
//! outside this module.  Entry-level visual policy stays here: horizontal
//! chrome, final wrapping, group headers, timestamp reservation/hover,
//! clipped rows, background ownership, and animated accent/bullet painting.

use chrono::{Local, TimeZone};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
};

use crate::{
    appearance::LayoutConfig,
    render::color::blend_color,
    scrollback::{
        layout::HorizontalLayout,
        types::{AccentStyle, BlockLine},
    },
    theme::wave_brightness,
};

#[derive(Debug, Clone, Copy)]
pub struct EntryRenderSpec {
    pub width: usize,
    pub layout: LayoutConfig,
    pub accent: Option<AccentStyle>,
    pub collapsed_accent: bool,
    pub background: Option<Color>,
    pub accent_background: bool,
    pub base_background: Color,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampLabel {
    pub short: String,
    pub hover: String,
}

/// Grok reserves ten columns for the short message timestamp.  The expanded
/// hover label is allowed to grow leftward beyond this reservation.
pub const TIMESTAMP_RESERVED_WIDTH: usize = 10;

pub fn timestamp_label(created_at_ms: Option<u64>) -> Option<TimestampLabel> {
    let millis = i64::try_from(created_at_ms?).ok()?;
    let local = Local.timestamp_millis_opt(millis).single()?;
    Some(TimestampLabel {
        short: local.format("%-I:%M %p").to_string(),
        hover: local.format("%H:%M:%S | %b %d").to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupHeaderSpec {
    pub label: String,
    pub expanded: bool,
    pub running: bool,
    pub failed: bool,
    pub tool_accent: Color,
    pub error_accent: Color,
    pub muted: Color,
    pub text: Color,
}

#[derive(Debug, Clone)]
pub struct EntrySourceLine {
    pub content: Line<'static>,
    pub block_index: Option<usize>,
    pub rail: bool,
    pub header: bool,
    pub selectable: bool,
    pub accent: Option<AccentStyle>,
    pub bullet: Option<AccentStyle>,
    pub background: Option<Color>,
    pub accent_background: bool,
    pub joiner: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EntryLayoutSpec {
    pub width: usize,
    pub layout: LayoutConfig,
    pub fallback_accent: AccentStyle,
    pub collapsed_accent: bool,
    pub fallback_background: Option<Color>,
    pub base_background: Color,
    pub flash_accent: Option<Color>,
    pub accent_flash: bool,
    pub timestamp: Option<TimestampLabel>,
    pub group_header: Option<GroupHeaderSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedEntryLine {
    pub line: Line<'static>,
    pub block_index: Option<usize>,
    pub line_index: usize,
    pub header: bool,
    pub group_header: bool,
    pub selectable: bool,
    pub accent: Option<AccentStyle>,
    pub flash_accent: Option<Color>,
    pub bullet: Option<AccentStyle>,
    pub accent_flash: bool,
    pub background: Option<Color>,
    pub copy_text: String,
    pub content_offset: u16,
    pub content_width: u16,
    pub timestamp: Option<TimestampLabel>,
    pub bullet_span: Option<usize>,
    pub joiner_to_previous: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampPaint {
    pub rect: Rect,
    pub label: String,
}

#[derive(Debug, Clone, Copy)]
pub struct DynamicAccentSpec {
    pub tick: u64,
    pub logical_row: u16,
    pub wave_rows: u16,
    pub wave_speed: f32,
    pub background: Color,
    pub accent: Option<AccentStyle>,
    pub flash_accent: Option<Color>,
    pub bullet: Option<AccentStyle>,
    pub bullet_span: Option<usize>,
    pub selected: bool,
    pub flash: bool,
    pub pending_user_input: bool,
}

pub struct EntryRenderer;

impl EntryRenderer {
    pub const ACCENT_WIDTH: usize = HorizontalLayout::ACCENT as usize;
    /// Legacy default estimate. Production geometry uses `chrome_width`.
    pub const CHROME_WIDTH: usize = 5;
    /// Legacy default estimate. Production geometry uses `content_offset`.
    pub const CONTENT_OFFSET: usize = 3;

    pub fn chrome_width(layout: &LayoutConfig) -> usize {
        usize::from(HorizontalLayout::chrome_width(layout))
    }

    pub fn content_offset(layout: &LayoutConfig) -> u16 {
        HorizontalLayout::ACCENT + layout.block_pad_left
    }

    /// Materialize one complete entry from block-owned semantic lines.
    ///
    /// This follows the upstream wrapper boundary: blocks own their semantic
    /// output while the entry wrapper owns group replacement, timestamp width,
    /// final terminal wrapping and chrome. `skip_rows` is deliberately applied
    /// after layout so logical line indices and animation phases remain stable.
    pub fn render_entry(
        mut source: Vec<EntrySourceLine>,
        spec: EntryLayoutSpec,
        skip_rows: usize,
        max_rows: Option<usize>,
    ) -> Vec<RenderedEntryLine> {
        if let Some(group) = spec.group_header.as_ref() {
            let header = Self::group_header_line(group);
            if group.expanded {
                source.insert(0, header);
            } else {
                source = vec![header];
            }
        }

        let reserve_timestamp = spec.timestamp.is_some();
        let wrap_width = spec
            .width
            .saturating_sub(Self::chrome_width(&spec.layout))
            .saturating_sub(usize::from(reserve_timestamp) * TIMESTAMP_RESERVED_WIDTH)
            .max(1);
        let mut timestamp_pending = spec.timestamp;
        let mut rendered = Vec::new();
        for source_line in source {
            let accent = source_line
                .accent
                .or_else(|| source_line.rail.then_some(spec.fallback_accent));
            let bullet = source_line.bullet.or_else(|| {
                (source_line.rail && source_line.header).then_some(spec.fallback_accent)
            });
            let background = source_line.background.or(spec.fallback_background);
            let attach_timestamp = timestamp_pending.is_some() && source_line.selectable;
            let (wrapped, joiners) = crate::render::wrapping::word_wrap_line_with_joiners(
                &source_line.content,
                wrap_width,
            );
            for (wrap_index, (wrapped, wrap_joiner)) in wrapped.into_iter().zip(joiners).enumerate()
            {
                let line_index = rendered.len();
                let copy_text = wrapped
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>();
                let mut line = Self::render_line(
                    BlockLine {
                        content: wrapped,
                        background,
                        bg_start_col: 0,
                        background_is_panel: false,
                        selectable: source_line.selectable,
                        header: source_line.header && wrap_index == 0,
                        joiner: None,
                    },
                    EntryRenderSpec {
                        width: spec.width,
                        layout: spec.layout,
                        accent: accent.or(spec.flash_accent.map(AccentStyle::static_color)),
                        collapsed_accent: spec.collapsed_accent,
                        background,
                        accent_background: source_line.accent_background,
                        base_background: spec.base_background,
                    },
                );
                line.block_index = source_line.block_index;
                line.line_index = line_index;
                line.header = source_line.header && wrap_index == 0;
                line.group_header = spec.group_header.is_some() && line_index == 0;
                line.selectable = source_line.selectable;
                line.accent = accent;
                line.flash_accent = spec.flash_accent;
                line.bullet = bullet;
                line.accent_flash = spec.accent_flash;
                line.background = background;
                line.copy_text = copy_text;
                line.content_offset = if source_line.selectable {
                    Self::content_offset(&spec.layout)
                } else {
                    0
                };
                line.content_width = if source_line.selectable {
                    wrap_width.min(u16::MAX as usize) as u16
                } else {
                    0
                };
                line.timestamp = if attach_timestamp {
                    timestamp_pending.take()
                } else {
                    None
                };
                line.joiner_to_previous = if wrap_index == 0 {
                    source_line.joiner.clone()
                } else {
                    wrap_joiner
                };
                rendered.push(line);
            }
        }

        rendered
            .into_iter()
            .skip(skip_rows)
            .take(max_rows.unwrap_or(usize::MAX))
            .collect()
    }

    fn group_header_line(spec: &GroupHeaderSpec) -> EntrySourceLine {
        let accent = if spec.failed {
            AccentStyle::static_color(spec.error_accent)
        } else if spec.running {
            AccentStyle::animated(spec.tool_accent)
        } else {
            AccentStyle::static_color(spec.muted)
        };
        let glyph_color = if spec.failed {
            spec.error_accent
        } else if spec.running {
            spec.tool_accent
        } else {
            spec.muted
        };
        EntrySourceLine {
            content: Line::from(vec![
                Span::styled(
                    format!("{} ", crate::glyphs::diamond_dotted()),
                    Style::default().fg(glyph_color),
                ),
                Span::styled(
                    spec.label.clone(),
                    Style::default()
                        .fg(if spec.failed {
                            spec.error_accent
                        } else {
                            spec.text
                        })
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
            ]),
            block_index: None,
            rail: true,
            header: true,
            selectable: true,
            accent: Some(accent),
            bullet: None,
            background: None,
            accent_background: false,
            joiner: None,
        }
    }

    pub fn render_line(mut content: BlockLine, spec: EntryRenderSpec) -> RenderedEntryLine {
        let bullet_span = content
            .content
            .spans
            .iter()
            .position(|span| is_bullet_span(span.content.as_ref()))
            .map(|index| index + 1);
        let accent_background = if spec.accent_background {
            spec.background.unwrap_or(spec.base_background)
        } else {
            spec.base_background
        };
        let (glyph, color) = match spec.accent {
            Some(accent) if spec.collapsed_accent => {
                (crate::glyphs::collapsed_accent(), accent.color)
            }
            Some(accent) => (crate::glyphs::accent_bar(), accent.color),
            None => (" ", spec.base_background),
        };
        let prefix = format!("{glyph}{}", " ".repeat(spec.layout.block_pad_left.into()));
        content.content.spans.insert(
            0,
            Span::styled(prefix, Style::default().fg(color).bg(accent_background)),
        );
        if let Some(background) = spec.background {
            for span in content.content.spans.iter_mut().skip(1) {
                span.style = span.style.bg(background);
            }
            let used = content.content.width();
            if used < spec.width {
                content.content.spans.push(Span::styled(
                    " ".repeat(spec.width - used),
                    Style::default().bg(background),
                ));
            }
        }
        RenderedEntryLine {
            line: content.content,
            block_index: None,
            line_index: 0,
            header: content.header,
            group_header: false,
            selectable: content.selectable,
            accent: spec.accent,
            flash_accent: None,
            bullet: None,
            accent_flash: false,
            background: content.background,
            copy_text: String::new(),
            content_offset: if content.selectable {
                Self::content_offset(&spec.layout)
            } else {
                0
            },
            content_width: 0,
            timestamp: None,
            bullet_span,
            joiner_to_previous: content.joiner,
        }
    }

    /// Paint one already-laid-out entry row directly into the frame buffer.
    /// The method owns every cell in the row, including the timestamp gutter,
    /// which prevents stale wide-glyph remnants between frames.
    pub fn paint_buffer_line(
        buf: &mut Buffer,
        area: Rect,
        screen_row: u16,
        rendered: &RenderedEntryLine,
        dynamic: DynamicAccentSpec,
        layout: &LayoutConfig,
        mouse_pos: Option<(u16, u16)>,
    ) -> Option<TimestampPaint> {
        if area.width == 0 || screen_row >= area.height {
            return None;
        }
        let y = area.y.saturating_add(screen_row);
        let background = rendered.background.unwrap_or(dynamic.background);
        let row = Rect::new(area.x, y, area.width, 1);
        buf.set_style(row, Style::default().bg(background));
        for x in row.x..row.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(' ');
            }
        }

        let mut line = rendered.line.clone();
        Self::paint_dynamic(&mut line, dynamic);
        buf.set_line(area.x, y, &line, area.width);

        let timestamp = rendered.timestamp.as_ref()?;
        let content_right = area
            .right()
            .saturating_sub(layout.block_pad_right.min(area.width));
        let hover_start = content_right.saturating_sub(TIMESTAMP_RESERVED_WIDTH as u16);
        let hovered =
            mouse_pos.is_some_and(|(mx, my)| my == y && mx >= hover_start && mx < content_right);
        let text = if hovered {
            format!("  {}", timestamp.hover)
        } else {
            format!("  {}", timestamp.short)
        };
        let text_width = text.chars().count().min(u16::MAX as usize) as u16;
        let content_width = area.width.saturating_sub(Self::chrome_width(layout) as u16);
        if content_width <= text_width.saturating_add(1) {
            return None;
        }
        let x = content_right.saturating_sub(text_width);
        buf.set_string(x, y, &text, Style::default().fg(Color::Gray).bg(background));
        Some(TimestampPaint {
            rect: Rect::new(hover_start, y, content_right.saturating_sub(hover_start), 1),
            label: timestamp.hover.clone(),
        })
    }

    pub fn paint_dynamic(line: &mut Line<'static>, spec: DynamicAccentSpec) {
        if let Some(prefix) = line.spans.first_mut() {
            let accent = if spec.flash {
                spec.flash_accent
                    .map(AccentStyle::static_color)
                    .or(spec.accent)
            } else {
                spec.accent
            };
            let Some(accent) = accent else {
                prefix.style = prefix.style.fg(spec.background);
                return;
            };
            let color = if spec.flash
                || spec.selected
                || spec.pending_user_input
                || !accent.animated
            {
                accent.color
            } else {
                blend_color(
                    spec.background,
                    accent.color,
                    wave_brightness(spec.tick, spec.logical_row, spec.wave_rows, spec.wave_speed),
                )
                .unwrap_or(accent.color)
            };
            prefix.style = prefix.style.fg(color);
        }
        if let (Some(bullet), Some(index)) = (spec.bullet, spec.bullet_span)
            && let Some(span) = line.spans.get_mut(index)
        {
            let color =
                if spec.flash || spec.selected || spec.pending_user_input || !bullet.animated {
                    bullet.color
                } else {
                    blend_color(
                        spec.background,
                        bullet.color,
                        wave_brightness(spec.tick, 0, spec.wave_rows, spec.wave_speed),
                    )
                    .unwrap_or(bullet.color)
                };
            span.style = span.style.fg(color);
        }
    }
}

fn is_bullet_span(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with('◆') || trimmed.starts_with('♦')
}

#[cfg(test)]
mod tests {
    use ratatui::{buffer::Buffer, layout::Rect, style::Color, text::Span};

    use super::*;

    #[test]
    fn entry_renderer_owns_accent_padding_and_bullet_phase() {
        let content = BlockLine::content(Line::from(vec![Span::raw("◆ "), Span::raw("Thinking…")]));
        let accent = AccentStyle::animated(Color::Magenta);
        let mut rendered = EntryRenderer::render_line(
            content,
            EntryRenderSpec {
                width: 30,
                layout: LayoutConfig::default(),
                accent: Some(accent),
                collapsed_accent: false,
                background: None,
                accent_background: false,
                base_background: Color::Black,
            },
        );
        assert_eq!(rendered.line.spans[0].content, "┃  ");
        assert_eq!(rendered.bullet_span, Some(1));
        EntryRenderer::paint_dynamic(
            &mut rendered.line,
            DynamicAccentSpec {
                tick: 4,
                logical_row: 2,
                wave_rows: 32,
                wave_speed: 0.15,
                background: Color::Black,
                accent: Some(accent),
                flash_accent: None,
                bullet: Some(accent),
                bullet_span: rendered.bullet_span,
                selected: false,
                flash: false,
                pending_user_input: false,
            },
        );
        assert_ne!(rendered.line.spans[0].style.fg, Some(Color::Black));
        assert_ne!(rendered.line.spans[1].style.fg, Some(Color::Black));
    }

    #[test]
    fn pending_user_input_freezes_animated_rail_and_bullet() {
        let accent = AccentStyle::animated(Color::Magenta);
        let mut rendered = EntryRenderer::render_line(
            BlockLine::content(Line::from(vec![Span::raw("◆ "), Span::raw("Run")])),
            EntryRenderSpec {
                width: 20,
                layout: LayoutConfig::default(),
                accent: Some(accent),
                collapsed_accent: false,
                background: None,
                accent_background: false,
                base_background: Color::Black,
            },
        );
        EntryRenderer::paint_dynamic(
            &mut rendered.line,
            DynamicAccentSpec {
                tick: 17,
                logical_row: 9,
                wave_rows: 32,
                wave_speed: 0.15,
                background: Color::Black,
                accent: Some(accent),
                flash_accent: None,
                bullet: Some(accent),
                bullet_span: rendered.bullet_span,
                selected: false,
                flash: false,
                pending_user_input: true,
            },
        );
        assert_eq!(rendered.line.spans[0].style.fg, Some(Color::Magenta));
        assert_eq!(rendered.line.spans[1].style.fg, Some(Color::Magenta));
    }

    #[test]
    fn chrome_width_uses_both_upstream_padding_columns() {
        let layout = LayoutConfig {
            block_pad_left: 3,
            block_pad_right: 4,
            ..LayoutConfig::default()
        };
        assert_eq!(EntryRenderer::chrome_width(&layout), 8);
        assert_eq!(EntryRenderer::content_offset(&layout), 4);
    }

    fn source(text: &str) -> EntrySourceLine {
        EntrySourceLine {
            content: Line::from(text.to_string()),
            block_index: Some(7),
            rail: true,
            header: true,
            selectable: true,
            accent: None,
            bullet: None,
            background: None,
            accent_background: false,
            joiner: None,
        }
    }

    fn layout_spec(width: usize) -> EntryLayoutSpec {
        EntryLayoutSpec {
            width,
            layout: LayoutConfig::default(),
            fallback_accent: AccentStyle::static_color(Color::Blue),
            collapsed_accent: false,
            fallback_background: None,
            base_background: Color::Black,
            flash_accent: None,
            accent_flash: false,
            timestamp: None,
            group_header: None,
        }
    }

    #[test]
    fn entry_layout_uses_dynamic_padding_and_timestamp_reservation() {
        let mut spec = layout_spec(50);
        spec.layout.block_pad_left = 3;
        spec.layout.block_pad_right = 4;
        spec.timestamp = Some(TimestampLabel {
            short: "1:23 PM".into(),
            hover: "13:23:00 | Aug 25".into(),
        });
        let lines = EntryRenderer::render_entry(vec![source("hello")], spec, 0, None);
        assert_eq!(lines[0].content_offset, 4);
        assert_eq!(lines[0].content_width, 32); // 50 - (1 + 3 + 4) - 10
        assert!(lines[0].timestamp.is_some());
    }

    #[test]
    fn group_header_replaces_collapsed_content_and_prepends_expanded_content() {
        let group = GroupHeaderSpec {
            label: "Reading 2 files".into(),
            expanded: false,
            running: true,
            failed: false,
            tool_accent: Color::Cyan,
            error_accent: Color::Red,
            muted: Color::DarkGray,
            text: Color::Gray,
        };
        let mut spec = layout_spec(50);
        spec.group_header = Some(group.clone());
        let collapsed = EntryRenderer::render_entry(vec![source("hidden")], spec, 0, None);
        assert_eq!(collapsed.len(), 1);
        assert!(collapsed[0].copy_text.starts_with('◈'));
        assert!(collapsed[0].copy_text.contains("Reading 2 files"));
        assert!(collapsed[0].accent.is_some_and(|accent| accent.animated));

        let mut spec = layout_spec(50);
        spec.group_header = Some(GroupHeaderSpec {
            expanded: true,
            ..group
        });
        let expanded = EntryRenderer::render_entry(vec![source("visible")], spec, 0, None);
        assert_eq!(expanded.len(), 2);
        assert!(expanded[0].group_header);
        assert_eq!(expanded[1].copy_text, "visible");
    }

    #[test]
    fn clipped_render_keeps_original_logical_row_phase() {
        let lines = EntryRenderer::render_entry(
            vec![source("one two three four five six")],
            layout_spec(14),
            1,
            Some(1),
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line_index, 1);
        assert!(lines[0].joiner_to_previous.is_some());
    }

    #[test]
    fn narrow_timestamp_is_suppressed() {
        let mut spec = layout_spec(12);
        spec.timestamp = Some(TimestampLabel {
            short: "1:23 PM".into(),
            hover: "13:23:00 | Aug 25".into(),
        });
        let rendered = EntryRenderer::render_entry(vec![source("hi")], spec, 0, None);
        let area = Rect::new(0, 0, 12, 1);
        let mut buf = Buffer::empty(area);
        let paint = EntryRenderer::paint_buffer_line(
            &mut buf,
            area,
            0,
            &rendered[0],
            DynamicAccentSpec {
                tick: 0,
                logical_row: 0,
                wave_rows: 32,
                wave_speed: 0.15,
                background: Color::Black,
                accent: rendered[0].accent,
                flash_accent: None,
                bullet: None,
                bullet_span: None,
                selected: false,
                flash: false,
                pending_user_input: false,
            },
            &LayoutConfig::default(),
            None,
        );
        assert!(paint.is_none());
        assert!(
            !(0..12)
                .map(|x| buf[(x, 0)].symbol())
                .collect::<String>()
                .contains("PM")
        );
    }

    #[test]
    fn timestamp_expands_on_hover_and_row_paint_clears_gutter() {
        let mut spec = layout_spec(80);
        spec.timestamp = Some(TimestampLabel {
            short: "1:23 PM".into(),
            hover: "13:23:00 | Aug 25".into(),
        });
        let rendered = EntryRenderer::render_entry(vec![source("hello")], spec, 0, None);
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        buf.set_string(72, 0, "GHOST", Style::default());
        let paint = EntryRenderer::paint_buffer_line(
            &mut buf,
            area,
            0,
            &rendered[0],
            DynamicAccentSpec {
                tick: 0,
                logical_row: 0,
                wave_rows: 32,
                wave_speed: 0.15,
                background: Color::Black,
                accent: rendered[0].accent,
                flash_accent: None,
                bullet: None,
                bullet_span: None,
                selected: false,
                flash: false,
                pending_user_input: false,
            },
            &LayoutConfig::default(),
            Some((74, 0)),
        );
        assert_eq!(paint.expect("timestamp hit").label, "13:23:00 | Aug 25");
        let row = (0..80).map(|x| buf[(x, 0)].symbol()).collect::<String>();
        assert!(row.contains("13:23:00 | Aug 25"));
        assert!(!row.contains("GHOST"));
    }
}
