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
    /// Exact paint-time rectangle of the visible model label.
    pub model_area: Rect,
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
        let mut model_area = Rect::default();
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
                let rendered = render_info_line(
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
                model_area = rendered.model_area;
                info_flag_areas = rendered.flag_areas;
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
            model_area,
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

#[derive(Debug)]
struct FittedInfoLine {
    line: Line<'static>,
    model_range: Option<(usize, usize)>,
    flag_ranges: Vec<Option<(usize, usize)>>,
}

#[derive(Debug, Default)]
struct RenderedInfoLine {
    model_area: Rect,
    flag_areas: Vec<Rect>,
}

/// Fit the prompt caption without ever splitting a semantic flag.
///
/// DSH adds preset/plan/YOLO flags to Grok's model caption. A plain
/// `Buffer::set_line(..., min(line.width(), area.width))` clips the last flag
/// character-by-character (`YOLO` -> `YOL`). Keep right-most safety/mode flags
/// atomically, then use whatever remains for a middle-elided model name.
fn fit_info_line(
    info: &PromptInfoContract,
    max_width: usize,
    bg: Color,
    theme: &Theme,
    focused: bool,
) -> FittedInfoLine {
    let mut flag_ranges = vec![None; info.flags.len()];
    if max_width < 2 {
        return FittedInfoLine {
            line: Line::default(),
            model_range: None,
            flag_ranges,
        };
    }

    let separator_color = if focused {
        theme.gray_dim
    } else {
        blend_color(bg, theme.gray_dim, 0.6).unwrap_or(theme.gray_dim)
    };
    let separator_style = Style::default().fg(separator_color).bg(bg);
    let flag_opacity = if focused { 0.75 } else { 0.5 };
    let pad_style = Style::default().bg(bg);
    let inner_budget = max_width.saturating_sub(2);

    // Select whole flags from right to left. The right-most items carry the
    // most immediate state (YOLO after plan after preset) and therefore win
    // when an extremely narrow terminal cannot show every flag.
    let mut selected_flags = vec![false; info.flags.len()];
    let mut used = 0usize;
    let mut item_count = 0usize;
    for (index, flag) in info.flags.iter().enumerate().rev() {
        let width = flag.text.width();
        let separator = usize::from(item_count > 0) * 3;
        if width > 0 && used.saturating_add(separator).saturating_add(width) <= inner_budget {
            selected_flags[index] = true;
            used = used.saturating_add(separator).saturating_add(width);
            item_count = item_count.saturating_add(1);
        }
    }

    // Usage warnings are also atomic. They follow the flags in fit priority,
    // while retaining their original left-most paint order when selected.
    let selected_warning = info.usage_warning.as_ref().is_some_and(|warning| {
        let width = warning.width();
        let separator = usize::from(item_count > 0) * 3;
        if width > 0 && used.saturating_add(separator).saturating_add(width) <= inner_budget {
            used = used.saturating_add(separator).saturating_add(width);
            item_count = item_count.saturating_add(1);
            true
        } else {
            false
        }
    });

    // Model is the elastic item: middle-elide it into the remaining columns,
    // or omit it entirely when the atomic labels consume the whole budget.
    let model_separator = usize::from(item_count > 0) * 3;
    let model_budget = inner_budget.saturating_sub(used.saturating_add(model_separator));
    let fitted_model = (!info.model_name.is_empty() && model_budget > 0)
        .then(|| elide_middle_to_width(&info.model_name, model_budget));

    let mut spans = vec![Span::styled(" ", pad_style)];
    let mut cursor = 1usize;
    let mut has_item = false;
    let mut model_range = None;
    let push_separator = |spans: &mut Vec<Span<'static>>, cursor: &mut usize| {
        spans.push(Span::styled(" · ", separator_style));
        *cursor = cursor.saturating_add(3);
    };

    if selected_warning && let Some(warning) = &info.usage_warning {
        let color = if info.usage_warning_critical {
            theme.warning
        } else {
            separator_color
        };
        spans.push(Span::styled(
            warning.clone(),
            Style::default().fg(color).bg(bg),
        ));
        cursor = cursor.saturating_add(warning.width());
        has_item = true;
    }

    if let Some(model) = fitted_model.filter(|model| !model.is_empty()) {
        if has_item {
            push_separator(&mut spans, &mut cursor);
        }
        let start = cursor;
        let width = model.width();
        spans.push(Span::styled(
            model,
            chrome_caption_style(bg, theme, focused),
        ));
        cursor = cursor.saturating_add(width);
        model_range = Some((start, width));
        has_item = true;
    }

    for (index, flag) in info.flags.iter().enumerate() {
        if !selected_flags[index] {
            continue;
        }
        if has_item {
            push_separator(&mut spans, &mut cursor);
        }
        let start = cursor;
        let width = flag.text.width();
        let color = match (flag.color, flag.bold, focused) {
            (Some(color), true, _) => color,
            (Some(color), false, _) => blend_color(bg, color, flag_opacity).unwrap_or(theme.gray),
            (None, true, _) => theme.text_primary,
            (None, false, true) => theme.gray,
            (None, false, false) => blend_color(bg, theme.gray, flag_opacity).unwrap_or(theme.gray),
        };
        let mut style = Style::default().fg(color).bg(bg);
        if flag.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(flag.text.clone(), style));
        cursor = cursor.saturating_add(width);
        flag_ranges[index] = Some((start, width));
        has_item = true;
    }
    spans.push(Span::styled(" ", pad_style));

    FittedInfoLine {
        line: Line::from(spans),
        model_range,
        flag_ranges,
    }
}

fn elide_middle_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let remaining = max_width - 1;
    let left_budget = remaining.div_ceil(2);
    let right_budget = remaining - left_budget;
    let mut left = String::new();
    let mut left_width = 0usize;
    for character in text.chars() {
        let width = character.to_string().width();
        if left_width.saturating_add(width) > left_budget {
            break;
        }
        left.push(character);
        left_width = left_width.saturating_add(width);
    }
    let mut right = String::new();
    let mut right_width = 0usize;
    for character in text.chars().rev() {
        let width = character.to_string().width();
        if right_width.saturating_add(width) > right_budget {
            break;
        }
        right.insert(0, character);
        right_width = right_width.saturating_add(width);
    }
    format!("{left}…{right}")
}

fn render_info_line(
    buf: &mut Buffer,
    area: Rect,
    info: &PromptInfoContract,
    bg: Color,
    theme: &Theme,
    focused: bool,
) -> RenderedInfoLine {
    if area.height == 0 || area.width == 0 {
        return RenderedInfoLine::default();
    }

    let pad_style = Style::default().bg(bg);
    let (right_line, right_width) = if info.multiline {
        let right_line = Line::from(vec![
            Span::styled("multiline", Style::default().fg(theme.gray).bg(bg)),
            Span::styled(" ", pad_style),
        ]);
        let width = right_line.width() as u16;
        (Some(right_line), width)
    } else {
        (None, 0)
    };
    let reserved_gap = u16::from(right_line.is_some());
    let left_budget = area
        .width
        .saturating_sub(right_width.saturating_add(reserved_gap));
    let fitted = fit_info_line(info, left_budget as usize, bg, theme, focused);
    let left_width = fitted.line.width() as u16;
    let gap = u16::from(left_width > 0 && right_line.is_some());
    let total_width = left_width.saturating_add(gap).saturating_add(right_width);
    let left_x = area.right().saturating_sub(total_width);
    if left_width > 0 {
        buf.set_line(left_x, area.y, &fitted.line, left_width);
    }
    if let Some(right_line) = right_line {
        let right_x = area.right().saturating_sub(right_width);
        buf.set_line(right_x, area.y, &right_line, right_width.min(area.width));
    }

    let model_area = range_to_rect(fitted.model_range, left_x, area.y);
    let flag_areas = fitted
        .flag_ranges
        .into_iter()
        .map(|range| range_to_rect(range, left_x, area.y))
        .collect();
    RenderedInfoLine {
        model_area,
        flag_areas,
    }
}

fn range_to_rect(range: Option<(usize, usize)>, left_x: u16, y: u16) -> Rect {
    let Some((start, width)) = range else {
        return Rect::default();
    };
    let start = u16::try_from(start).unwrap_or(u16::MAX);
    let width = u16::try_from(width).unwrap_or(u16::MAX);
    if width == 0 {
        Rect::default()
    } else {
        Rect::new(left_x.saturating_add(start), y, width, 1)
    }
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

    fn rendered_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
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
        let area = Rect::new(0, 0, 51, 3);
        let mut buffer = Buffer::empty(area);
        let mut textarea = TextArea::new();
        let mut renderer = GrokPromptRenderer::default();
        let info = PromptInfoContract {
            model_name: "DeepSeek-V4-Flash-Vision-Exp".into(),
            flags: vec![
                crate::views::prompt_contract::PromptFlagContract {
                    text: "标准模式 ▾".into(),
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
        assert_eq!(result.model_area.width, 26);
        assert_eq!(result.info_flag_areas[0].width, 10);
        assert_eq!(result.info_flag_areas[1].width, 4);
        assert!(result.info_flag_areas[1].x > result.info_flag_areas[0].x);
        assert_eq!(result.info_flag_areas[0].y, 2);
    }

    #[test]
    fn info_fitter_keeps_complete_mode_labels_at_the_regression_width() {
        let info = PromptInfoContract {
            model_name: "DeepSeek-V4-Flash-Vision-Exp".into(),
            flags: vec![
                crate::views::prompt_contract::PromptFlagContract {
                    text: "标准模式 ▾".into(),
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
        let theme = Theme::current();
        let fitted = fit_info_line(&info, 48, theme.bg_base, theme, true);
        let text = rendered_text(&fitted.line);

        assert_eq!(fitted.line.width(), 48);
        assert!(text.contains("标准模式 ▾"));
        assert!(text.contains("YOLO"));
        assert!(text.contains('…'));
        assert!(!text.contains("YOL "));
        assert_eq!(fitted.model_range.map(|(_, width)| width), Some(26));
        assert_eq!(fitted.flag_ranges[0].map(|(_, width)| width), Some(10));
        assert_eq!(fitted.flag_ranges[1].map(|(_, width)| width), Some(4));
    }

    #[test]
    fn info_fitter_omits_whole_lower_priority_items_when_extremely_narrow() {
        let info = PromptInfoContract {
            model_name: "dsv4 flash".into(),
            flags: vec![
                crate::views::prompt_contract::PromptFlagContract {
                    text: "标准模式 ▾".into(),
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
        let theme = Theme::current();

        let both = fit_info_line(&info, 19, theme.bg_base, theme, true);
        assert!(rendered_text(&both.line).contains("标准模式 ▾ · YOLO"));
        assert_eq!(both.model_range, None);
        assert_eq!(both.flag_ranges[0].map(|(_, width)| width), Some(10));
        assert_eq!(both.flag_ranges[1].map(|(_, width)| width), Some(4));

        let yolo_only = fit_info_line(&info, 18, theme.bg_base, theme, true);
        let text = rendered_text(&yolo_only.line);
        assert!(!text.contains("标准模式"));
        assert!(text.contains("YOLO"));
        assert_eq!(yolo_only.flag_ranges[0], None);
        assert_eq!(yolo_only.flag_ranges[1].map(|(_, width)| width), Some(4));
    }

    #[test]
    fn prompt_returns_the_exact_visible_alias_geometry() {
        let area = Rect::new(0, 0, 48, 3);
        let mut buffer = Buffer::empty(area);
        let mut textarea = TextArea::new();
        let result = GrokPromptRenderer::default().draw(
            &mut buffer,
            area,
            &mut textarea,
            &PromptStyleContract::default(),
            Some(&PromptInfoContract {
                model_name: "dsv4 flash-v".into(),
                ..PromptInfoContract::default()
            }),
            Theme::current(),
        );

        assert_eq!(result.model_area.width, 12);
        assert_eq!(result.model_area.y, 2);
        assert!(result.model_area.right() < area.right());
    }
}
