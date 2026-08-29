//! DSH-neutral extraction of the fixed Grok `PromptWidget::draw` core.
//!
//! Text editing and viewport state remain in Grok's `TextArea`; this renderer
//! owns only prompt chrome, info labels, dimming, and cursor projection.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::StatefulWidgetRef;
use unicode_width::UnicodeWidthStr;
use xai_ratatui_textarea::{MouseAction, TextArea, TextAreaState};

use crate::render::line_utils::truncate_str;
use crate::theme::Theme;
use crate::views::prompt_contract::{
    PromptGeometry, PromptInfoContract, PromptStyleContract, PromptSurface, desired_prompt_height,
};

const PREFIX_WIDTH: u16 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PromptRenderResult {
    pub cursor_pos: Option<(u16, u16)>,
    pub textarea_area: Rect,
    /// Visible rectangles for `PromptInfoContract::flags`, in flag order.
    /// Returning draw-time geometry keeps mouse hit testing on the same
    /// truncation and right-alignment path as the Grok prompt painter.
    pub info_flag_areas: Vec<Rect>,
}

#[derive(Debug, Default)]
pub(crate) struct GrokPromptRenderer {
    textarea_state: TextAreaState,
    textarea_area: Rect,
}

impl GrokPromptRenderer {
    pub(crate) fn desired_height(
        textarea: &TextArea,
        outer_width: u16,
        style: &PromptStyleContract,
        info: Option<&PromptInfoContract>,
        max_height: u16,
    ) -> u16 {
        let geometry = PromptGeometry::compute(
            Rect::new(0, 0, outer_width, max_height.max(1)),
            style,
            info.is_some(),
            PREFIX_WIDTH,
        );
        let rows = textarea.desired_height(geometry.textarea.width.max(1));
        desired_prompt_height(rows, style, info.is_some(), false, max_height.max(1))
    }

    pub(crate) fn draw(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        textarea: &mut TextArea,
        style: &PromptStyleContract,
        info: Option<&PromptInfoContract>,
        theme: &Theme,
    ) -> PromptRenderResult {
        if area.height == 0 || area.width < 4 {
            return PromptRenderResult::default();
        }

        let bg = match style.surface {
            PromptSurface::Default => theme.bg_base,
            PromptSurface::Canvas(color) | PromptSurface::Panel(color) => color,
        };
        let border_color = style.border_color_override.unwrap_or(if style.focused {
            theme.prompt_border_active
        } else {
            theme.prompt_border
        });
        let accent_color = style.accent_color_override.unwrap_or(theme.accent_user);
        let geometry = PromptGeometry::compute(area, style, info.is_some(), PREFIX_WIDTH);
        self.textarea_area = geometry.textarea;
        let mut info_flag_areas = Vec::new();

        // Upstream establishes explicit colors before TextArea patches cells.
        buf.set_style(area, Style::default().fg(theme.text_primary).bg(bg));

        if style.chrome && style.show_accent_line {
            for y in area.top()..area.bottom() {
                if let Some(cell) = buf.cell_mut((area.x, y)) {
                    cell.set_char('┃');
                    cell.set_style(Style::default().fg(accent_color).bg(bg));
                }
            }
        }

        if style.vpad_top > 0 && style.chrome && style.show_borders && geometry.top.height > 0 {
            draw_divider(buf, area, geometry.top.y, '╭', '╮', border_color, bg);
            if let Some(title) = style
                .title
                .as_deref()
                .map(str::trim)
                .filter(|title| !title.is_empty())
            {
                let max_width = area.width.saturating_sub(6);
                if max_width >= 6 {
                    let label = truncate_str(&format!(" {title} "), max_width as usize);
                    let label_width = label.width() as u16;
                    let x = area.right().saturating_sub(3 + label_width);
                    buf.set_string(
                        x,
                        geometry.top.y,
                        label,
                        chrome_caption_style(bg, theme, style.focused),
                    );
                }
            }
        }

        if style.show_prefix && geometry.text.width > PREFIX_WIDTH {
            let prefix = style.prefix_override.as_deref().unwrap_or("❯ ");
            buf.set_stringn(
                geometry.text.x,
                geometry.text.y,
                prefix,
                PREFIX_WIDTH as usize,
                Style::default().fg(accent_color).bg(bg),
            );
        }

        textarea.scrollbar_track_style = Style::default().bg(theme.bg_base);
        textarea.scrollbar_thumb_style = Style::default()
            .fg(theme.selection_border)
            .bg(theme.bg_base);
        textarea.scrollbar_padding = 1;
        (&*textarea).render_ref(geometry.textarea, buf, &mut self.textarea_state);

        if textarea.is_empty()
            && geometry.textarea.width > 0
            && (!style.focused || style.placeholder_when_focused)
        {
            let placeholder = truncate_str(style.placeholder(), geometry.textarea.width as usize);
            buf.set_string(
                geometry.textarea.x,
                geometry.textarea.y,
                placeholder,
                Style::default().fg(theme.gray).bg(bg),
            );
        }

        if style.chrome && style.show_borders && area.width >= 2 {
            let divider_style = Style::default().fg(border_color).bg(bg);
            for y in geometry.text.top()..geometry.text.bottom() {
                if let Some(cell) = buf.cell_mut((area.x, y)) {
                    cell.set_char('│');
                    cell.set_style(divider_style);
                }
                if let Some(cell) = buf.cell_mut((area.right().saturating_sub(1), y)) {
                    cell.set_char('│');
                    cell.set_style(divider_style);
                }
            }
        }

        if info.is_some() && style.chrome && style.show_borders && geometry.info.height > 0 {
            draw_divider(buf, area, geometry.info.y, '╰', '╯', border_color, bg);
            if let Some(info) = info.filter(|info| !info.is_blank()) {
                info_flag_areas = render_info_line(
                    buf,
                    Rect::new(
                        geometry.content.x,
                        geometry.info.y,
                        geometry.content.width,
                        1,
                    ),
                    info,
                    bg,
                    theme,
                    style.focused,
                );
            }
        }

        if !style.focused && matches!(style.surface, PromptSurface::Default) {
            blend_area_foreground(buf, geometry.dim, bg, 0.66);
        }

        PromptRenderResult {
            cursor_pos: style
                .focused
                .then(|| textarea.cursor_pos_with_state(geometry.textarea, self.textarea_state))
                .flatten(),
            textarea_area: geometry.textarea,
            info_flag_areas,
        }
    }

    pub(crate) fn handle_mouse(
        &mut self,
        textarea: &mut TextArea,
        event: crossterm::event::MouseEvent,
    ) -> MouseAction {
        textarea.handle_mouse(event, self.textarea_area, self.textarea_state)
    }
}

fn render_info_line(
    buf: &mut Buffer,
    area: Rect,
    info: &PromptInfoContract,
    bg: Color,
    theme: &Theme,
    focused: bool,
) -> Vec<Rect> {
    if area.height == 0 || area.width == 0 {
        return Vec::new();
    }

    let separator_color = if focused {
        theme.gray_dim
    } else {
        blend_color(bg, theme.gray_dim, 0.6).unwrap_or(theme.gray_dim)
    };
    let separator_style = Style::default().fg(separator_color).bg(bg);
    let flag_opacity = if focused { 0.75 } else { 0.5 };
    let pad_style = Style::default().bg(bg);
    let mut left = vec![Span::styled(" ", pad_style)];
    let mut left_width = 1usize;
    if let Some(warning) = &info.usage_warning {
        let color = if info.usage_warning_critical {
            theme.warning
        } else {
            separator_color
        };
        left.push(Span::styled(
            warning.clone(),
            Style::default().fg(color).bg(bg),
        ));
        left.push(Span::styled(" · ", separator_style));
        left_width = left_width.saturating_add(warning.width()).saturating_add(3);
    }
    left.push(Span::styled(
        info.model_name.clone(),
        chrome_caption_style(bg, theme, focused),
    ));
    left_width = left_width.saturating_add(info.model_name.width());
    let mut flag_ranges = Vec::with_capacity(info.flags.len());
    for flag in &info.flags {
        left.push(Span::styled(" · ", separator_style));
        left_width = left_width.saturating_add(3);
        let flag_start = left_width;
        let flag_width = flag.text.width();
        let color = match (flag.color, flag.bold, focused) {
            (Some(color), true, _) => color,
            (Some(color), false, _) => blend_color(bg, color, flag_opacity).unwrap_or(theme.gray),
            (None, true, _) => theme.text_primary,
            (None, false, true) => theme.gray,
            (None, false, false) => blend_color(bg, theme.gray, flag_opacity).unwrap_or(theme.gray),
        };
        let mut flag_style = Style::default().fg(color).bg(bg);
        if flag.bold {
            flag_style = flag_style.add_modifier(Modifier::BOLD);
        }
        left.push(Span::styled(flag.text.clone(), flag_style));
        left_width = left_width.saturating_add(flag_width);
        flag_ranges.push((flag_start, flag_width));
    }
    left.push(Span::styled(" ", pad_style));

    let left_line = Line::from(left);
    let (left_x, painted_left_width) = if info.multiline {
        let right_line = Line::from(vec![
            Span::styled("multiline", Style::default().fg(theme.gray).bg(bg)),
            Span::styled(" ", pad_style),
        ]);
        let right_width = right_line.width() as u16;
        let left_width = (left_line.width() as u16)
            .min(area.width.saturating_sub(right_width.saturating_add(1)));
        let total_width = left_width.saturating_add(1).saturating_add(right_width);
        let left_x = area.right().saturating_sub(total_width);
        buf.set_line(left_x, area.y, &left_line, left_width);
        let right_x = area.right().saturating_sub(right_width);
        buf.set_line(right_x, area.y, &right_line, right_width.min(area.width));
        (left_x, left_width)
    } else {
        let width = (left_line.width() as u16).min(area.width);
        let x = area.right().saturating_sub(width);
        buf.set_line(x, area.y, &left_line, width);
        (x, width)
    };

    flag_ranges
        .into_iter()
        .map(|(start, width)| {
            let start = u16::try_from(start).unwrap_or(u16::MAX);
            let width = u16::try_from(width).unwrap_or(u16::MAX);
            if start >= painted_left_width || width == 0 {
                Rect::default()
            } else {
                Rect::new(
                    left_x.saturating_add(start),
                    area.y,
                    width.min(painted_left_width.saturating_sub(start)),
                    1,
                )
            }
        })
        .collect()
}

fn chrome_caption_style(bg: Color, theme: &Theme, focused: bool) -> Style {
    let opacity = if focused { 0.6 } else { 0.4 };
    let foreground = blend_color(bg, theme.text_secondary, opacity).unwrap_or(theme.gray);
    Style::default().fg(foreground).bg(bg)
}

fn draw_divider(
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    left: char,
    right: char,
    foreground: Color,
    background: Color,
) {
    for x in area.left()..area.right() {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char(if x == area.left() {
                left
            } else if x == area.right().saturating_sub(1) {
                right
            } else {
                '─'
            });
            cell.set_style(Style::default().fg(foreground).bg(background));
        }
    }
}

fn blend_area_foreground(buf: &mut Buffer, area: Rect, target: Color, opacity: f32) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y))
                && let Some(color) = blend_color(target, cell.fg, opacity)
            {
                cell.set_fg(color);
            }
        }
    }
}

fn blend_color(base: Color, original: Color, opacity: f32) -> Option<Color> {
    let (base_red, base_green, base_blue) = match base {
        Color::Rgb(red, green, blue) => (red, green, blue),
        _ => return None,
    };
    let (red, green, blue) = match original {
        Color::Rgb(red, green, blue) => (red, green, blue),
        _ => return None,
    };
    let blend = |base: u8, original: u8| {
        (base as f32 + (original as f32 - base as f32) * opacity.clamp(0.0, 1.0)).round() as u8
    };
    Some(Color::Rgb(
        blend(base_red, red),
        blend(base_green, green),
        blend(base_blue, blue),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn info(multiline: bool) -> PromptInfoContract {
        PromptInfoContract {
            model_name: "deepseek".into(),
            multiline,
            ..PromptInfoContract::default()
        }
    }

    #[test]
    fn draws_upstream_box_textarea_and_real_cursor() {
        let area = Rect::new(0, 0, 32, 3);
        let mut buffer = Buffer::empty(area);
        let mut textarea = TextArea::new();
        textarea.insert_str("hello");
        let mut renderer = GrokPromptRenderer::default();
        let result = renderer.draw(
            &mut buffer,
            area,
            &mut textarea,
            &PromptStyleContract::default(),
            Some(&info(false)),
            Theme::current(),
        );

        assert_eq!(buffer[(0, 0)].symbol(), "╭");
        assert_eq!(buffer[(31, 0)].symbol(), "╮");
        assert_eq!(buffer[(0, 1)].symbol(), "│");
        assert_eq!(buffer[(2, 1)].symbol(), "❯");
        assert_eq!(buffer[(4, 1)].symbol(), "h");
        assert_eq!(buffer[(0, 2)].symbol(), "╰");
        assert_eq!(buffer[(31, 2)].symbol(), "╯");
        assert_eq!(result.cursor_pos, Some((9, 1)));
        assert_eq!(result.textarea_area, Rect::new(4, 1, 27, 1));
    }

    #[test]
    fn textarea_wrap_drives_height_and_cursor_viewport() {
        let mut textarea = TextArea::new();
        textarea.insert_str(&"x".repeat(100));
        let style = PromptStyleContract::default();
        let info = info(true);
        assert_eq!(
            GrokPromptRenderer::desired_height(&textarea, 32, &style, Some(&info), 6),
            6
        );

        let area = Rect::new(0, 0, 32, 6);
        let mut buffer = Buffer::empty(area);
        let result = GrokPromptRenderer::default().draw(
            &mut buffer,
            area,
            &mut textarea,
            &style,
            Some(&info),
            Theme::current(),
        );
        let (_, cursor_y) = result.cursor_pos.expect("focused cursor");
        assert!(cursor_y >= result.textarea_area.y);
        assert!(cursor_y < result.textarea_area.bottom());
    }

    #[test]
    fn selection_style_survives_textarea_rendering() {
        let area = Rect::new(0, 0, 24, 3);
        let mut buffer = Buffer::empty(area);
        let mut textarea = TextArea::new();
        textarea.insert_str("select");
        textarea.set_selection(0, 3);
        GrokPromptRenderer::default().draw(
            &mut buffer,
            area,
            &mut textarea,
            &PromptStyleContract::default(),
            Some(&info(false)),
            Theme::current(),
        );
        assert_eq!(buffer[(4, 1)].bg, textarea.selection_style.bg.unwrap());
    }

    #[test]
    fn mouse_uses_the_rendered_textarea_geometry() {
        let area = Rect::new(0, 0, 24, 3);
        let mut buffer = Buffer::empty(area);
        let mut textarea = TextArea::new();
        textarea.insert_str("abcdef");
        let mut renderer = GrokPromptRenderer::default();
        renderer.draw(
            &mut buffer,
            area,
            &mut textarea,
            &PromptStyleContract::default(),
            Some(&info(false)),
            Theme::current(),
        );

        let action = renderer.handle_mouse(
            &mut textarea,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 6,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(action, MouseAction::CursorPlaced);
        assert_eq!(textarea.cursor(), 2);
    }

    #[test]
    fn info_flags_return_their_visible_draw_geometry() {
        let area = Rect::new(0, 0, 48, 3);
        let mut buffer = Buffer::empty(area);
        let mut textarea = TextArea::new();
        let mut renderer = GrokPromptRenderer::default();
        let info = PromptInfoContract {
            model_name: "deepseek-v4".into(),
            flags: vec![
                crate::views::prompt_contract::PromptFlagContract {
                    text: "standard ▾".into(),
                    color: None,
                    bold: true,
                },
                crate::views::prompt_contract::PromptFlagContract {
                    text: "YOLO".into(),
                    color: None,
                    bold: true,
                },
            ],
            ..PromptInfoContract::default()
        };
        let result = renderer.draw(
            &mut buffer,
            area,
            &mut textarea,
            &PromptStyleContract::default(),
            Some(&info),
            Theme::current(),
        );

        assert_eq!(result.info_flag_areas.len(), 2);
        assert!(result.info_flag_areas[0].width > 0);
        assert!(result.info_flag_areas[1].x > result.info_flag_areas[0].x);
        assert_eq!(result.info_flag_areas[0].y, 2);
    }
}
