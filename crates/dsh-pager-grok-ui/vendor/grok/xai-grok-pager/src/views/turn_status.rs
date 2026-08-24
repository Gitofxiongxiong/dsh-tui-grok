//! Turn status line — single-row widget showing current turn activity.
//!
//! Layout: `⠧ Run command 0.2s              1m20s ⇣12k [stop]`
//!
//! Adapted from Grok Build at mirror commit
//! `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`. This B adaptation preserves
//! Grok's row geometry, animation cadence, timers, token count and cancel hit
//! area while replacing Grok agent/MCP/watcher state with DSH-neutral DTOs.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::host_adapter::TurnActivitySnapshot;
use crate::render::line_utils::truncate_str;
use crate::theme::Theme;

/// Show each spinner frame for this many animation ticks.
/// At ~30fps, 4 ticks = ~133ms per frame = ~7.5 spinner fps.
pub const SPINNER_DIVISOR: u64 = 4;

/// Pulse cadence copied from Grok's shared "waiting on you" diamond.
const USER_WAITING_PULSE_SPEED: f32 = 0.08;

#[derive(Debug, Clone, Copy, Default)]
pub struct MouseButtons {
    pub cancel_hovered: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TurnStatusOutput {
    pub cancel_button: Option<Rect>,
}

#[derive(Debug)]
pub struct TurnStatusArgs<'a> {
    pub activity: &'a TurnActivitySnapshot,
    pub turn_elapsed: Option<Duration>,
    pub activity_elapsed: Option<Duration>,
    pub tick: u64,
    pub pending_user_input: bool,
    /// Mouse affordance and hover state. `None` is a keyboard-only host.
    pub buttons: Option<MouseButtons>,
    pub total_tokens: Option<u64>,
    pub cancelling: bool,
}

pub fn render_turn_status(
    buf: &mut Buffer,
    area: Rect,
    args: TurnStatusArgs<'_>,
    theme: &Theme,
) -> TurnStatusOutput {
    if area.height == 0 || area.width < 10 {
        return TurnStatusOutput::default();
    }

    let show_cancel = args.buttons.is_some();
    let cancel_hovered = args.buttons.is_some_and(|buttons| buttons.cancel_hovered);
    let (activity_style, label, tool) =
        activity_presentation(args.activity, args.cancelling, theme);

    let turn_timer = match (args.turn_elapsed, args.total_tokens) {
        (Some(elapsed), Some(tokens)) if tokens > 0 => format!(
            "{} {}{}",
            format_turn_timer(elapsed),
            crate::glyphs::token_arrow(),
            format_tokens_short(tokens)
        ),
        (Some(elapsed), _) => format_turn_timer(elapsed),
        _ => String::new(),
    };
    let turn_timer_width = turn_timer.width();
    let cancel_text = if show_cancel { " [stop]" } else { "" };
    let cancel_width = cancel_text.width();
    let right_width = turn_timer_width + cancel_width;

    let spinner = if args.pending_user_input {
        format!("{} ", crate::glyphs::diamond_filled())
    } else {
        let frames = crate::glyphs::braille_spinner_frames();
        let index = (args.tick / SPINNER_DIVISOR) as usize % frames.len();
        format!("{} ", frames[index])
    };
    let spinner_width = spinner.width();

    // Asking the user a question should not render a pressure-inducing phase
    // timer. Tool permission waits retain the tool timer, matching Grok.
    let activity_timer = if matches!(args.activity, TurnActivitySnapshot::WaitingForInput) {
        String::new()
    } else {
        args.activity_elapsed
            .map(|elapsed| format!(" {}", format_turn_timer(elapsed)))
            .unwrap_or_default()
    };
    let activity_timer_width = activity_timer.width();
    let available_for_label = (area.width as usize)
        .saturating_sub(spinner_width)
        .saturating_sub(activity_timer_width)
        .saturating_sub(1)
        .saturating_sub(right_width);

    let spinner_style = if args.pending_user_input {
        Style::default().fg(pending_diamond_color(theme, theme.accent_user, args.tick))
    } else {
        activity_style
    };
    let mut left = vec![Span::styled(spinner, spinner_style)];
    if let Some((prefix, detail, detail_color)) = tool {
        let prefix_width = prefix.width();
        let detail_width = available_for_label.saturating_sub(prefix_width).max(5);
        left.push(Span::styled(prefix, Style::default().fg(theme.gray)));
        left.push(Span::styled(
            truncate_str(detail, detail_width),
            Style::default().fg(detail_color),
        ));
    } else {
        left.push(Span::styled(
            truncate_str(&label, available_for_label),
            activity_style,
        ));
    }

    let timer_style = Style::default()
        .fg(theme.gray)
        .bg(theme.bg_base)
        .remove_modifier(Modifier::all());
    if !activity_timer.is_empty() {
        left.push(Span::styled(activity_timer, timer_style));
    }
    buf.set_line(area.x, area.y, &Line::from(left), area.width);

    let right_x = area.x + area.width.saturating_sub(right_width as u16);
    let mut x = right_x;
    if !turn_timer.is_empty() {
        buf.set_span(
            x,
            area.y,
            &Span::styled(turn_timer, timer_style),
            turn_timer_width as u16,
        );
        x = x.saturating_add(turn_timer_width as u16);
    }

    let cancel_button = if show_cancel {
        let style = Style::default()
            .fg(if cancel_hovered {
                theme.accent_error
            } else {
                theme.gray
            })
            .bg(theme.bg_base)
            .remove_modifier(Modifier::all());
        buf.set_span(
            x,
            area.y,
            &Span::styled(cancel_text, style),
            cancel_width as u16,
        );
        Some(Rect::new(x, area.y, cancel_width as u16, 1))
    } else {
        None
    };

    TurnStatusOutput { cancel_button }
}

/// Return the base label/style and, for tool rows, split prefix/detail data.
fn activity_presentation<'a>(
    activity: &'a TurnActivitySnapshot,
    cancelling: bool,
    theme: &Theme,
) -> (Style, String, Option<(&'static str, &'a str, Color)>) {
    if cancelling {
        return (
            Style::default().fg(theme.accent_error),
            "Cancelling…".into(),
            None,
        );
    }
    match activity {
        TurnActivitySnapshot::Thinking => (
            Style::default().fg(theme.text_secondary),
            "Thinking…".into(),
            None,
        ),
        TurnActivitySnapshot::Responding => (
            Style::default().fg(theme.text_secondary),
            "Responding…".into(),
            None,
        ),
        TurnActivitySnapshot::ToolRunning { title, description } => {
            if let Some(description) = description
                .as_deref()
                .map(str::trim)
                .filter(|description| !description.is_empty())
            {
                return (
                    Style::default().fg(theme.text_secondary),
                    waiting_subject(description),
                    None,
                );
            }
            if let Some(query) = title.strip_prefix("Web search: ") {
                return (
                    Style::default().fg(theme.accent_success),
                    String::new(),
                    Some(("Search ", query.trim_matches('"'), theme.command)),
                );
            }
            if let Some(url) = title.strip_prefix("Fetch: ") {
                return (
                    Style::default().fg(theme.accent_success),
                    String::new(),
                    Some(("Fetch ", url, theme.command)),
                );
            }
            (
                Style::default().fg(theme.accent_success),
                String::new(),
                Some(("Run ", title.lines().next().unwrap_or(title), theme.command)),
            )
        }
        TurnActivitySnapshot::Compacting => (
            Style::default().fg(theme.text_secondary),
            "Compacting…".into(),
            None,
        ),
        TurnActivitySnapshot::Retrying { attempt } => (
            Style::default().fg(theme.warning),
            format!("Retrying (attempt {attempt})…"),
            None,
        ),
        TurnActivitySnapshot::WritingToolCall => (
            Style::default().fg(theme.text_secondary),
            "Preparing tool call…".into(),
            None,
        ),
        TurnActivitySnapshot::Waiting => (
            Style::default().fg(theme.text_secondary),
            "Waiting for response…".into(),
            None,
        ),
        TurnActivitySnapshot::WaitingForInput => (
            Style::default().fg(theme.text_secondary),
            "Waiting on your answer…".into(),
            None,
        ),
    }
}

fn waiting_subject(subject: &str) -> String {
    let subject = subject.trim_end_matches(['.', '…']);
    format!("{subject}…")
}

pub fn format_turn_timer(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 10 {
        return format!("{:.1}s", duration.as_secs_f64());
    }
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    if minutes < 60 {
        return format!("{minutes}m{remaining_seconds}s");
    }
    format!("{}h{}m", minutes / 60, minutes % 60)
}

fn format_tokens_short(tokens: u64) -> String {
    if tokens < 1_000 {
        format!("{tokens}")
    } else if tokens < 100_000 {
        let thousands = tokens as f64 / 1_000.0;
        if tokens < 10_000 {
            format!("{thousands:.2}k")
        } else {
            format!("{thousands:.1}k")
        }
    } else if tokens < 1_000_000 {
        format!("{}k", tokens / 1_000)
    } else {
        let millions = tokens as f64 / 1_000_000.0;
        if tokens < 10_000_000 {
            format!("{millions:.2}m")
        } else {
            format!("{millions:.1}m")
        }
    }
}

fn pending_diamond_color(theme: &Theme, accent: Color, tick: u64) -> Color {
    let wave = ((tick as f32) * USER_WAITING_PULSE_SPEED).sin().powi(2);
    blend_color(theme.bg_base, accent, 0.3 + wave * 0.7).unwrap_or(accent)
}

fn blend_color(base: Color, foreground: Color, opacity: f32) -> Option<Color> {
    let (Color::Rgb(br, bg, bb), Color::Rgb(fr, fg, fb)) = (base, foreground) else {
        return None;
    };
    let channel = |base: u8, foreground: u8| {
        (base as f32 * (1.0 - opacity) + foreground as f32 * opacity).round() as u8
    };
    Some(Color::Rgb(
        channel(br, fr),
        channel(bg, fg),
        channel(bb, fb),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(buffer: &Buffer, width: u16) -> String {
        (0..width)
            .map(|x| buffer[(x, 0)].symbol())
            .collect::<String>()
    }

    fn args(activity: &TurnActivitySnapshot, tick: u64) -> TurnStatusArgs<'_> {
        TurnStatusArgs {
            activity,
            turn_elapsed: Some(Duration::from_secs(80)),
            activity_elapsed: Some(Duration::from_millis(200)),
            tick,
            pending_user_input: false,
            buttons: Some(MouseButtons::default()),
            total_tokens: Some(12_000),
            cancelling: false,
        }
    }

    #[test]
    fn spinner_advances_on_grok_divisor() {
        let theme = Theme::current();
        let activity = TurnActivitySnapshot::Thinking;
        let area = Rect::new(0, 0, 60, 1);
        let mut first = Buffer::empty(area);
        let mut second = Buffer::empty(area);
        render_turn_status(&mut first, area, args(&activity, 0), theme);
        render_turn_status(&mut second, area, args(&activity, 4), theme);
        assert_ne!(first[(0, 0)].symbol(), second[(0, 0)].symbol());
        assert!(row(&first, 60).contains("Thinking… 0.2s"));
    }

    #[test]
    fn right_side_is_aligned_and_returns_stop_hit_area() {
        let theme = Theme::current();
        let activity = TurnActivitySnapshot::ToolRunning {
            title: "cargo test --workspace".into(),
            description: None,
        };
        let area = Rect::new(0, 0, 64, 1);
        let mut buffer = Buffer::empty(area);
        let output = render_turn_status(&mut buffer, area, args(&activity, 0), theme);
        let text = row(&buffer, 64);
        assert!(text.contains("Run cargo test --workspace 0.2s"));
        assert!(text.ends_with("1m20s ⇣12.0k [stop]"));
        let stop = output.cancel_button.expect("stop hit area");
        assert_eq!(stop.right(), 64);
        assert_eq!(stop.height, 1);
    }

    #[test]
    fn pending_input_uses_a_pulsing_diamond() {
        let theme = Theme::current();
        let activity = TurnActivitySnapshot::WaitingForInput;
        let mut first_args = args(&activity, 0);
        first_args.pending_user_input = true;
        let mut second_args = args(&activity, 12);
        second_args.pending_user_input = true;
        let area = Rect::new(0, 0, 48, 1);
        let mut first = Buffer::empty(area);
        let mut second = Buffer::empty(area);
        render_turn_status(&mut first, area, first_args, theme);
        render_turn_status(&mut second, area, second_args, theme);
        assert_eq!(first[(0, 0)].symbol(), "◆");
        assert_ne!(first[(0, 0)].fg, second[(0, 0)].fg);
        assert!(row(&first, 48).contains("Waiting on your answer…"));
        assert!(!row(&first, 48).contains("0.2s"));
    }

    #[test]
    fn narrow_rows_truncate_only_the_activity_label() {
        let theme = Theme::current();
        let activity = TurnActivitySnapshot::ToolRunning {
            title: "a very long command that must be truncated".into(),
            description: None,
        };
        let area = Rect::new(0, 0, 32, 1);
        let mut buffer = Buffer::empty(area);
        render_turn_status(&mut buffer, area, args(&activity, 0), theme);
        let text = row(&buffer, 32);
        assert!(text.contains('…'));
        assert!(text.ends_with("1m20s ⇣12.0k [stop]"));
    }

    #[test]
    fn duration_format_matches_grok_buckets() {
        assert_eq!(format_turn_timer(Duration::from_millis(500)), "0.5s");
        assert_eq!(format_turn_timer(Duration::from_secs(32)), "32s");
        assert_eq!(format_turn_timer(Duration::from_secs(125)), "2m5s");
        assert_eq!(format_turn_timer(Duration::from_secs(3725)), "1h2m");
    }
}
