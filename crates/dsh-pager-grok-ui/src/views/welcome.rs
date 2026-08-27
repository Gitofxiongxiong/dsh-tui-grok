//! DSH empty-session welcome surface.
//!
//! The whale sprite and opening sequence are ported from dsh-TUI:
//! src/components/whaleFrames.ts and src/components/Whale.tsx at
//! c0f4f87bd31a3fb0daf420504f8a7fc22b35fd0e.
//! Source file SHA-256:
//! 6db1ac3cc671476614fd55ac37a99293c27942cf25ca45d369faf4983f28dfb4.
//!
//! Copyright (c) 2026, chimney (ccch1mneyyy)
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in
//! all copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

pub const WIDE_MIN_WIDTH: u16 = 64;
pub const WIDE_MIN_HEIGHT: u16 = 13;
const WHALE_WIDTH: usize = 40;
const SPRITE_HEIGHT: usize = 25;
const WHALE_HEIGHT: u16 = 13;
const FRAME_COUNT: usize = 13;
pub const OPENING_DURATION: Duration = Duration::from_millis(3_340);

const OUTLINE: Color = Color::Rgb(20, 38, 96);
const BODY: Color = Color::Rgb(78, 111, 255);
const BELLY: Color = Color::Rgb(190, 225, 255);
const MOUTH: Color = Color::Rgb(255, 255, 255);

const OPENING_SEQUENCE: [(usize, u64); 16] = [
    (0, 400),
    (1, 250),
    (0, 300),
    (4, 150),
    (5, 150),
    (6, 150),
    (7, 150),
    (8, 150),
    (9, 150),
    (0, 250),
    (10, 170),
    (11, 170),
    (12, 260),
    (11, 170),
    (10, 170),
    (0, 300),
];

const WHALE_FRAMES: [[[u8; WHALE_WIDTH]; SPRITE_HEIGHT]; FRAME_COUNT] = [
    // standard
    [
        *b"........................................",
        *b"........................................",
        *b"........................D...............",
        *b".......................DBD.......D......",
        *b".......................DBBD.....DBD.....",
        *b".......................DBBBD..DDBBD.....",
        *b".......................DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // blink
    [
        *b"........................................",
        *b"........................................",
        *b"........................D...............",
        *b".......................DBD.......D......",
        *b".......................DBBD.....DBD.....",
        *b".......................DBBBD..DDBBD.....",
        *b".......................DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // fin1
    [
        *b"........................................",
        *b"........................................",
        *b"........................D...............",
        *b".......................DBD.......D......",
        *b".......................DBBD.....DBD.....",
        *b".......................DBBBD..DDBBD.....",
        *b".......................DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBBDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBBD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBBDD...........",
        *b"........DLLLLLLLLLLLLDDBBBBBBD..........",
        *b".........DDDDDDDDDDDD..DDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // fin2
    [
        *b"........................................",
        *b"........................................",
        *b"........................D...............",
        *b".......................DBD.......D......",
        *b".......................DBBD.....DBD.....",
        *b".......................DBBBD..DDBBD.....",
        *b".......................DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDDBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBBBDD............",
        *b".....DLLLWWWWWWWWWWDBBBBBBBBDD..........",
        *b"......DDLLLWWWWWWLLLDBBBBBBBBBD.........",
        *b"........DLLLLLLLLLLLLDDDBBBBBD..........",
        *b".........DDDDDDDDDDDD...DDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // spout1
    [
        *b"........................................",
        *b"........................................",
        *b"........................D...............",
        *b".......................DBD.......D......",
        *b".......................DBBD.....DBD.....",
        *b"..........LLL..........DBBBD..DDBBD.....",
        *b"...........L...........DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // spout2
    [
        *b"........................................",
        *b"........................................",
        *b"........................D...............",
        *b"...........L...........DBD.......D......",
        *b".........LLLLL.........DBBD.....DBD.....",
        *b"..........LLL..........DBBBD..DDBBD.....",
        *b"...........L...........DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // spout3
    [
        *b"........................................",
        *b"........................................",
        *b"...........L............D...............",
        *b".........LLLLL.........DBD.......D......",
        *b".......LLLLLLLLL.......DBBD.....DBD.....",
        *b"..........LLL..........DBBBD..DDBBD.....",
        *b"...........L...........DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // spout4
    [
        *b"........................................",
        *b"...........L............................",
        *b"........LLLLLLL.........D...............",
        *b"......LLLLLLLLLLL......DBD.......D......",
        *b"......L...LLL...L......DBBD.....DBD.....",
        *b"...........L...........DBBBD..DDBBD.....",
        *b".......................DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // spout5
    [
        *b"........................................",
        *b".......LL..L..LL........................",
        *b"......LLLL.L.LLLL.......D...............",
        *b".....L....LLL....L.....DBD.......D......",
        *b"....L......L......L....DBBD.....DBD.....",
        *b".......................DBBBD..DDBBD.....",
        *b".......................DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // spout6
    [
        *b"........................................",
        *b"......LL...L...LL.......................",
        *b".....L..LL.L.LL..L......D...............",
        *b"....L.....LLL.....L....DBD.......D......",
        *b"...L.......L.......L...DBBD.....DBD.....",
        *b".......................DBBBD..DDBBD.....",
        *b".......................DBBBBDDBBBBD.....",
        *b".......DDDDDDDDD........DBBBBBBBBD......",
        *b"......DBBBBBBBBBDD.......DBBBBBBBD......",
        *b".....DBBBBBBBBBBBBDD.....DBBBBBDD.......",
        *b"....DBBBBBBBBBBBBBBBDD....DBBBD.........",
        *b"...DDBBBBBBBBBBBBBBBBBD..DBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBD..........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBD...........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // tail1
    [
        *b"........................................",
        *b"........................................",
        *b".........................D..............",
        *b"........................DBD.......D.....",
        *b"........................DBBD.....DBD....",
        *b"........................DBBBD..DDBBD....",
        *b".........................DBBBDDBBBBD....",
        *b".......DDDDDDDDD.........DBBBBBBBBD.....",
        *b"......DBBBBBBBBBDD........DBBBBBBBD.....",
        *b".....DBBBBBBBBBBBBDD......DBBBBBDD......",
        *b"....DBBBBBBBBBBBBBBBDD.....DBBBD........",
        *b"...DDBBBBBBBBBBBBBBBBBD...DBBBBD........",
        *b"...DBBBBBBBBBBBBBBBBBBBDDDBBBBBD........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBD..........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBBD..........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // tail2
    [
        *b"........................................",
        *b"........................................",
        *b"..........................D.............",
        *b".........................DBD........DD..",
        *b".........................DBBD......DBD..",
        *b".........................DBBBD....DBBD..",
        *b"..........................DBBBD.DDBBD...",
        *b".......DDDDDDDDD...........DBBBDBBBBD...",
        *b"......DBBBBBBBBBDD.........DBBBBBBBD....",
        *b".....DBBBBBBBBBBBBDD........DBBBBBD.....",
        *b"....DBBBBBBBBBBBBBBBDD......DBBBBBD.....",
        *b"...DDBBBBBBBBBBBBBBBBBD..DDDBBBBDD......",
        *b"...DBBBBBBBBBBBBBBBBBBBDDBBBBBBBD.......",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBBD........",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBD.........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBBD.........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBBD..........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBD...........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBD............",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
    // tail3
    [
        *b"........................................",
        *b"........................................",
        *b"........................................",
        *b".............................D..........",
        *b"............................DBD.........",
        *b"............................DBD.........",
        *b"...........................DBBD.....DD..",
        *b".......DDDDDDDDD...........DBBBD...DBBD.",
        *b"......DBBBBBBBBBDD.........DBBBD.DDBBD..",
        *b".....DBBBBBBBBBBBBDD.......DBBBBDBBBBD..",
        *b"....DBBBBBBBBBBBBBBBDD......DBBBBBBBD...",
        *b"...DDBBBBBBBBBBBBBBBBBD...DDBBBBBBBD....",
        *b"...DBBBBBBBBBBBBBBBBBBBDDDBBBBBBBBD.....",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBBBDD......",
        *b"...DBBBDBBBBBBDBBBBBBBBBBBBBBBBD........",
        *b"...DBBBBBBBBBBBBBBBBBBBBBBBBBBBD........",
        *b"...DBBBBWWWWWWWBBBBBBBBDBBBBBBD.........",
        *b"...DDBWWWWWWWWWWWWBBBBBBDBBBDD..........",
        *b"....DLLWWWWWWWWWWWWDBBBBDDBDD...........",
        *b".....DLLLWWWWWWWWWWDBBBBBDD.............",
        *b"......DDLLLWWWWWWLLLDBBBBBDD............",
        *b"........DLLLLLLLLLLLDDBBBBBBD...........",
        *b".........DDDDDDDDDDD..DDDDDDD...........",
        *b"........................................",
        *b"........................................",
    ],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WelcomeLayout {
    Wide,
    Compact,
}

#[derive(Debug, Default)]
pub struct WelcomeAnimation {
    session_id: Option<String>,
    started_at: Option<Instant>,
}

impl WelcomeAnimation {
    pub fn observe_session(&mut self, session_id: &str) {
        if self.session_id.as_deref() != Some(session_id) {
            self.session_id = Some(session_id.to_string());
            self.started_at = None;
        }
    }

    pub fn elapsed(&mut self, now: Instant) -> Duration {
        let started_at = *self.started_at.get_or_insert(now);
        now.saturating_duration_since(started_at)
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.started_at
            .is_some_and(|started_at| now.saturating_duration_since(started_at) < OPENING_DURATION)
    }
}

pub fn opening_frame(elapsed: Duration) -> usize {
    let mut remaining = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    for (frame, dwell_ms) in OPENING_SEQUENCE {
        if remaining < dwell_ms {
            return frame;
        }
        remaining = remaining.saturating_sub(dwell_ms);
    }
    0
}

pub fn render_welcome(
    buf: &mut Buffer,
    area: Rect,
    elapsed: Duration,
    model: &str,
    preset: &str,
    theme: &Theme,
) -> WelcomeLayout {
    buf.set_style(area, Style::default().bg(theme.bg_base));
    if area.width >= WIDE_MIN_WIDTH && area.height >= WIDE_MIN_HEIGHT {
        render_wide(buf, area, opening_frame(elapsed), model, preset, theme);
        WelcomeLayout::Wide
    } else {
        render_compact(buf, area, model, preset, theme);
        WelcomeLayout::Compact
    }
}

pub fn format_cwd(cwd: &str, home: Option<&str>, max_width: usize) -> String {
    let cwd = cwd.trim();
    let display = home
        .filter(|home| !home.is_empty())
        .and_then(|home| {
            if cwd == home {
                Some("~".to_string())
            } else {
                cwd.strip_prefix(home)
                    .filter(|suffix| suffix.starts_with('/'))
                    .map(|suffix| format!("~{suffix}"))
            }
        })
        .unwrap_or_else(|| {
            if cwd.is_empty() {
                ".".to_string()
            } else {
                cwd.to_string()
            }
        });
    elide_middle(&display, max_width)
}

fn render_wide(
    buf: &mut Buffer,
    area: Rect,
    frame_index: usize,
    model: &str,
    preset: &str,
    theme: &Theme,
) {
    let group_width = WIDE_MIN_WIDTH.min(area.width);
    let group_x = area
        .x
        .saturating_add(area.width.saturating_sub(group_width) / 2);
    let group_y = area
        .y
        .saturating_add(area.height.saturating_sub(WHALE_HEIGHT) / 2);
    let whale_area = Rect::new(group_x, group_y, WHALE_WIDTH as u16, WHALE_HEIGHT);
    paint_whale(buf, whale_area, frame_index, theme.bg_base);

    let text_x = group_x.saturating_add(WHALE_WIDTH as u16 + 2);
    let text_width = area.right().saturating_sub(text_x);
    let title = Line::from(Span::styled(
        "✦ DeepSeek Harness",
        Style::default()
            .fg(BELLY)
            .bg(theme.bg_base)
            .add_modifier(Modifier::BOLD),
    ));
    buf.set_line(text_x, group_y.saturating_add(1), &title, text_width);

    let value_style = Style::default().fg(theme.text_primary).bg(theme.bg_base);
    let label_style = Style::default().fg(theme.gray).bg(theme.bg_base);
    let model_line = Line::from(vec![
        Span::styled("model  ", label_style),
        Span::styled(model, value_style),
    ]);
    let preset_line = Line::from(vec![
        Span::styled("preset ", label_style),
        Span::styled(preset, value_style),
    ]);
    buf.set_line(text_x, group_y.saturating_add(4), &model_line, text_width);
    buf.set_line(text_x, group_y.saturating_add(5), &preset_line, text_width);

    let tip = Line::from(vec![
        Span::styled("Tip: ", label_style),
        Span::styled(
            "/preset changes agent · Shift+Tab plan/sandbox",
            value_style,
        ),
    ]);
    buf.set_line(text_x, group_y.saturating_add(8), &tip, text_width);

    render_centered(
        buf,
        Rect::new(group_x, group_y.saturating_add(12), WHALE_WIDTH as u16, 1),
        "Explore the uncharted!",
        Style::default().fg(BELLY).bg(theme.bg_base),
    );
}

fn render_compact(buf: &mut Buffer, area: Rect, model: &str, preset: &str, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let rows = area.height.min(3);
    let y = area.y.saturating_add(area.height.saturating_sub(rows) / 2);
    render_centered(
        buf,
        Rect::new(area.x, y, area.width, 1),
        "✦ DeepSeek Harness",
        Style::default()
            .fg(BELLY)
            .bg(theme.bg_base)
            .add_modifier(Modifier::BOLD),
    );
    if rows >= 2 {
        render_centered(
            buf,
            Rect::new(area.x, y.saturating_add(1), area.width, 1),
            &format!("{model} · {preset}"),
            Style::default().fg(theme.gray).bg(theme.bg_base),
        );
    }
    if rows >= 3 {
        render_centered(
            buf,
            Rect::new(area.x, y.saturating_add(2), area.width, 1),
            "/preset changes agent",
            Style::default().fg(theme.gray_dim).bg(theme.bg_base),
        );
    }
}

fn paint_whale(buf: &mut Buffer, area: Rect, frame_index: usize, background: Color) {
    let frame = &WHALE_FRAMES[frame_index.min(FRAME_COUNT - 1)];
    for output_y in 0..WHALE_HEIGHT as usize {
        let upper = frame.get(output_y * 2);
        let lower = frame.get(output_y * 2 + 1);
        for x in 0..WHALE_WIDTH {
            let Some(cell) = buf.cell_mut((
                area.x.saturating_add(x as u16),
                area.y.saturating_add(output_y as u16),
            )) else {
                continue;
            };
            let upper = upper.and_then(|row| palette(row[x]));
            let lower = lower.and_then(|row| palette(row[x]));
            match (upper, lower) {
                (Some(foreground), Some(background_pixel)) => {
                    cell.set_char('▀');
                    cell.set_style(Style::default().fg(foreground).bg(background_pixel));
                }
                (Some(foreground), None) => {
                    cell.set_char('▀');
                    cell.set_style(Style::default().fg(foreground).bg(background));
                }
                (None, Some(foreground)) => {
                    cell.set_char('▄');
                    cell.set_style(Style::default().fg(foreground).bg(background));
                }
                (None, None) => {
                    cell.set_char(' ');
                    cell.set_style(Style::default().bg(background));
                }
            }
        }
    }
}

fn palette(pixel: u8) -> Option<Color> {
    match pixel {
        b'D' => Some(OUTLINE),
        b'B' => Some(BODY),
        b'L' => Some(BELLY),
        b'W' => Some(MOUTH),
        _ => None,
    }
}

fn render_centered(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let text = elide_middle(text, area.width as usize);
    let width = UnicodeWidthStr::width(text.as_str()) as u16;
    let x = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    buf.set_stringn(x, area.y, text, width as usize, style);
}

fn elide_middle(text: &str, max_width: usize) -> String {
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
    let mut left_width = 0;
    for character in text.chars() {
        let width = character.to_string().width();
        if left_width + width > left_budget {
            break;
        }
        left.push(character);
        left_width += width;
    }
    let mut right = String::new();
    let mut right_width = 0;
    for character in text.chars().rev() {
        let width = character.to_string().width();
        if right_width + width > right_budget {
            break;
        }
        right.insert(0, character);
        right_width += width;
    }
    format!("{left}…{right}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_frames_keep_the_fixed_sprite_dimensions() {
        assert_eq!(WHALE_FRAMES.len(), 13);
        assert!(WHALE_FRAMES.iter().all(|frame| frame.len() == 25));
        assert!(WHALE_FRAMES.iter().flatten().all(|row| row.len() == 40));
    }

    #[test]
    fn opening_sequence_advances_on_exact_dwell_boundaries_and_settles() {
        assert_eq!(opening_frame(Duration::ZERO), 0);
        assert_eq!(opening_frame(Duration::from_millis(399)), 0);
        assert_eq!(opening_frame(Duration::from_millis(400)), 1);
        assert_eq!(opening_frame(Duration::from_millis(649)), 1);
        assert_eq!(opening_frame(Duration::from_millis(650)), 0);
        assert_eq!(opening_frame(Duration::from_millis(950)), 4);
        assert_eq!(opening_frame(OPENING_DURATION), 0);
        assert_eq!(opening_frame(Duration::from_secs(30)), 0);
    }

    #[test]
    fn half_block_painter_preserves_upper_and_lower_palette_pixels() {
        let theme = Theme::current();
        let area = Rect::new(0, 0, 40, 13);
        let mut buffer = Buffer::empty(area);
        paint_whale(&mut buffer, area, 0, theme.bg_base);

        let lower_only = &buffer[(23, 1)];
        assert_eq!(lower_only.symbol(), "▄");
        assert_eq!(lower_only.fg, OUTLINE);
        assert_eq!(lower_only.bg, theme.bg_base);

        let paired = &buffer[(24, 1)];
        assert_eq!(paired.symbol(), "▀");
        assert_eq!(paired.fg, OUTLINE);
        assert_eq!(paired.bg, BODY);
    }

    #[test]
    fn welcome_uses_wide_and_compact_layouts_at_the_contract_boundary() {
        let theme = Theme::current();
        let wide_area = Rect::new(0, 0, 64, 13);
        let mut wide = Buffer::empty(wide_area);
        assert_eq!(
            render_welcome(
                &mut wide,
                wide_area,
                Duration::ZERO,
                "deepseek",
                "标准模式",
                theme,
            ),
            WelcomeLayout::Wide
        );
        let compact_area = Rect::new(0, 0, 63, 13);
        let mut compact = Buffer::empty(compact_area);
        assert_eq!(
            render_welcome(
                &mut compact,
                compact_area,
                Duration::ZERO,
                "deepseek",
                "标准模式",
                theme,
            ),
            WelcomeLayout::Compact
        );
    }

    #[test]
    fn cwd_shortens_home_and_elides_the_middle() {
        assert_eq!(
            format_cwd(
                "/home/leo/aidreamschool/dsh-tui-grok",
                Some("/home/leo"),
                80
            ),
            "~/aidreamschool/dsh-tui-grok"
        );
        assert_eq!(
            format_cwd(
                "/home/leo/aidreamschool/dsh-tui-grok",
                Some("/home/leo"),
                16
            ),
            "~/aidrea…ui-grok"
        );
        assert_eq!(format_cwd("", None, 8), ".");
    }

    #[test]
    fn animation_restarts_only_when_the_attached_session_changes() {
        let start = Instant::now();
        let mut animation = WelcomeAnimation::default();
        animation.observe_session("one");
        assert_eq!(animation.elapsed(start), Duration::ZERO);
        assert!(animation.is_animating(start + Duration::from_secs(1)));
        animation.observe_session("one");
        assert_eq!(
            animation.elapsed(start + Duration::from_secs(1)),
            Duration::from_secs(1)
        );
        animation.observe_session("two");
        assert_eq!(
            animation.elapsed(start + Duration::from_secs(1)),
            Duration::ZERO
        );
    }
}
