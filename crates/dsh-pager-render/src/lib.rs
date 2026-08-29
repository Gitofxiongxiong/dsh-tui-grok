use std::io::{self, Stdout, Write};
use std::sync::OnceLock;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use dsh_grok_inline::{LinkSpan, Terminal as InlineTerminal};
use ratatui::Frame;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

/// Semantic palette shared by every Grok-derived view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    // Background roles.
    pub accent_user: Color,
    pub bg_base: Color,
    pub bg_highlight: Color,
    pub bg_hover: Color,
    pub bg_light: Color,
    pub bg_dark: Color,
    pub bg_terminal: Color,
    pub bg_visual: Color,

    // Agent and lifecycle accents.
    pub accent_assistant: Color,
    pub accent_thinking: Color,
    pub accent_tool: Color,
    pub accent_system: Color,
    pub accent_error: Color,
    pub accent_success: Color,
    pub accent_running: Color,
    pub accent_skill: Color,

    // Semantic text roles.
    pub gray: Color,
    pub gray_bright: Color,
    pub gray_dim: Color,
    pub command: Color,
    pub path: Color,
    pub running: Color,
    pub warning: Color,
    pub fuzzy_accent: Color,
    pub accent_plan: Color,
    pub accent_verify: Color,
    pub accent_remember: Color,
    pub selection_border: Color,
    pub hover_border: Color,
    pub prompt_border: Color,
    pub prompt_border_active: Color,
    pub accent_model: Color,
    pub scrollbar_bg: Color,
    pub scrollbar_fg: Color,

    // Diff roles.
    pub diff_delete_bg: Color,
    pub diff_delete_fg: Color,
    pub diff_insert_bg: Color,
    pub diff_insert_fg: Color,
    pub diff_equal_fg: Color,
    pub diff_gutter_fg: Color,

    // Prompt image/attachment roles.
    pub paste_bg: Color,
    pub paste_fg: Color,
    pub paste_dim: Color,

    // Markdown roles. Modifier fields preserve the upstream style contract.
    pub md_heading_h1: Color,
    pub md_heading_h1_mod: Modifier,
    pub md_heading_h2: Color,
    pub md_heading_h2_mod: Modifier,
    pub md_heading_h3: Color,
    pub md_heading_h3_mod: Modifier,
    pub md_heading_h4: Color,
    pub md_heading_h4_mod: Modifier,
    pub md_heading_h5: Color,
    pub md_heading_h5_mod: Modifier,
    pub md_heading_h6: Color,
    pub md_heading_h6_mod: Modifier,
    pub md_code: Color,
    pub md_task_checked: Color,
    pub md_task_unchecked: Color,
    pub md_muted: Color,
    pub md_code_bg: Color,
    pub md_text: Color,
    pub link_fg: Color,
    pub text_secondary: Color,
    pub text_primary: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_base: Color::Rgb(20, 20, 20),
            bg_light: Color::Rgb(36, 36, 36),
            bg_dark: Color::Rgb(28, 28, 28),
            bg_highlight: Color::Rgb(36, 36, 36),
            bg_hover: Color::Rgb(44, 44, 44),
            bg_terminal: Color::Rgb(10, 10, 10),
            accent_user: Color::Rgb(200, 200, 200),
            accent_assistant: Color::Rgb(187, 154, 247),
            accent_thinking: Color::Rgb(187, 154, 247),
            accent_tool: Color::Rgb(120, 120, 120),
            accent_system: Color::Rgb(122, 162, 247),
            accent_error: Color::Rgb(247, 118, 142),
            accent_success: Color::Rgb(158, 206, 106),
            accent_running: Color::Rgb(187, 154, 247),
            accent_skill: Color::Rgb(122, 162, 247),
            text_primary: Color::Rgb(225, 225, 225),
            text_secondary: Color::Rgb(200, 200, 200),
            gray_dim: Color::Rgb(88, 88, 88),
            gray: Color::Rgb(108, 108, 108),
            gray_bright: Color::Rgb(120, 120, 120),
            command: Color::Rgb(224, 175, 104),
            path: Color::Rgb(255, 158, 100),
            running: Color::Rgb(125, 207, 255),
            warning: Color::Rgb(224, 175, 104),
            fuzzy_accent: Color::Rgb(122, 162, 247),
            accent_plan: Color::Rgb(255, 219, 141),
            accent_verify: Color::Rgb(187, 154, 247),
            accent_remember: Color::Rgb(139, 195, 74),
            selection_border: Color::Rgb(60, 60, 65),
            hover_border: Color::Rgb(30, 30, 34),
            prompt_border: Color::Rgb(50, 50, 55),
            prompt_border_active: Color::Rgb(80, 80, 88),
            accent_model: Color::Rgb(26, 188, 156),
            scrollbar_bg: Color::Rgb(17, 17, 17),
            scrollbar_fg: Color::Rgb(36, 36, 36),
            diff_delete_bg: Color::Rgb(66, 14, 20),
            diff_delete_fg: Color::Rgb(247, 118, 142),
            diff_insert_bg: Color::Rgb(6, 56, 6),
            diff_insert_fg: Color::Rgb(158, 206, 106),
            diff_equal_fg: Color::Rgb(108, 108, 108),
            diff_gutter_fg: Color::Rgb(108, 108, 108),
            bg_visual: Color::Rgb(54, 54, 54),
            paste_bg: Color::Rgb(17, 17, 17),
            paste_fg: Color::Rgb(200, 200, 200),
            paste_dim: Color::Rgb(65, 65, 65),
            md_heading_h1: Color::Rgb(26, 188, 156),
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: Color::Rgb(122, 162, 247),
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: Color::Rgb(157, 124, 216),
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: Color::Rgb(120, 120, 120),
            md_heading_h4_mod: Modifier::BOLD,
            md_heading_h5: Color::Rgb(108, 108, 108),
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: Color::Rgb(90, 90, 90),
            md_heading_h6_mod: Modifier::empty(),
            md_code: Color::Rgb(58, 149, 171),
            md_task_checked: Color::Rgb(158, 206, 106),
            md_task_unchecked: Color::Rgb(200, 200, 200),
            md_muted: Color::Rgb(108, 108, 108),
            md_code_bg: Color::Rgb(28, 28, 28),
            md_text: Color::Rgb(200, 200, 200),
            link_fg: Color::Rgb(122, 166, 218),
        }
    }
}

impl Theme {
    pub fn current() -> &'static Self {
        static THEME: OnceLock<Theme> = OnceLock::new();
        THEME.get_or_init(Self::default)
    }

    /// Foreground style using a palette color, matching Grok `Theme::fg`.
    pub const fn fg(&self, color: Color) -> Style {
        Style::new().fg(color)
    }

    /// Muted text. `Color::Reset` uses DIM so terminal-native palettes stay
    /// polarity-safe; RGB themes keep an explicit gray foreground.
    pub const fn muted(&self) -> Style {
        match self.gray {
            Color::Reset => Style::new().add_modifier(Modifier::DIM),
            c => Style::new().fg(c),
        }
    }

    /// Dim text (gray_dim). Same Reset→DIM rule as [`Self::muted`].
    pub const fn dim(&self) -> Style {
        match self.gray_dim {
            Color::Reset => Style::new().add_modifier(Modifier::DIM),
            c => Style::new().fg(c),
        }
    }

    /// Primary body text.
    pub const fn primary(&self) -> Style {
        Style::new().fg(self.text_primary)
    }
}

/// Capabilities negotiated by the terminal surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCapabilities {
    pub alternate_screen: bool,
    pub bracketed_paste: bool,
    pub mouse: bool,
    pub osc52: bool,
    pub hyperlinks: bool,
    pub cursor: bool,
    pub cell_diff: bool,
}

impl TerminalCapabilities {
    pub fn probe() -> Self {
        let disabled = |name: &str| std::env::var_os(name).is_some();
        Self {
            // TERM is frequently `dumb` in hermetic PTYs even though the
            // terminal still accepts crossterm mode controls. Explicit
            // DSH_TUI_DISABLE_* switches provide deterministic fallbacks.
            alternate_screen: !disabled("DSH_TUI_DISABLE_ALT_SCREEN"),
            bracketed_paste: !disabled("DSH_TUI_DISABLE_PASTE"),
            mouse: !disabled("DSH_TUI_DISABLE_MOUSE"),
            osc52: !disabled("DSH_TUI_DISABLE_OSC52"),
            hyperlinks: !disabled("DSH_TUI_DISABLE_HYPERLINKS"),
            cursor: !disabled("DSH_TUI_DISABLE_CURSOR"),
            cell_diff: true,
        }
    }
}

impl Default for TerminalCapabilities {
    fn default() -> Self {
        Self::probe()
    }
}

/// Owns raw mode and alternate-screen restoration, including error unwinding.
pub struct TerminalSurface {
    terminal: InlineTerminal<CrosstermBackend<Stdout>>,
    restored: bool,
    suspended: bool,
    capabilities: TerminalCapabilities,
    resize_epoch: u64,
}

impl TerminalSurface {
    pub fn enter() -> io::Result<Self> {
        let capabilities = TerminalCapabilities::probe();
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = enter_modes(&mut stdout, capabilities) {
            let _ = leave_modes(&mut stdout, capabilities);
            let _ = disable_raw_mode();
            return Err(error);
        }
        match InlineTerminal::new(CrosstermBackend::new(stdout)) {
            Ok(mut terminal) => {
                if let Err(error) = terminal.clear() {
                    let mut stdout = io::stdout();
                    let _ = leave_modes(&mut stdout, capabilities);
                    let _ = disable_raw_mode();
                    return Err(error);
                }
                Ok(Self {
                    terminal,
                    restored: false,
                    suspended: false,
                    capabilities,
                    resize_epoch: 0,
                })
            }
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = leave_modes(&mut stdout, capabilities);
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }

    pub fn size(&self) -> io::Result<Rect> {
        let size = self.terminal.size()?;
        Ok(Rect::new(0, 0, size.width, size.height))
    }

    pub fn capabilities(&self) -> TerminalCapabilities {
        self.capabilities
    }

    /// Synchronize internal diff buffers with terminal geometry and report a
    /// changed area so the UI can invalidate layout and hit maps.
    pub fn sync_size(&mut self) -> io::Result<Option<Rect>> {
        let before = self.terminal.last_known_area();
        self.terminal.autoresize()?;
        let after = self.terminal.last_known_area();
        if before == after {
            Ok(None)
        } else {
            self.resize_epoch = self.resize_epoch.wrapping_add(1);
            Ok(Some(after))
        }
    }

    pub fn resize_epoch(&self) -> u64 {
        self.resize_epoch
    }

    pub fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.terminal.draw(render).map(|_| ())
    }

    /// Draw with the same cell diff as [`Self::draw`] and an explicit link
    /// map. Passing an empty map removes links from the previous frame.
    pub fn draw_with_links<F>(&mut self, links: &[LinkSpan], render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.terminal.draw_with_links(render, links).map(|_| ())
    }

    /// Write a terminal-native clipboard update without coupling the pager to
    /// a desktop clipboard library. OSC 52 is understood by modern terminal
    /// emulators and also works when the pager is running over SSH or inside a
    /// multiplexer that forwards the sequence.
    pub fn copy_text(&mut self, text: &str) -> io::Result<()> {
        if !self.capabilities.osc52 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "terminal does not support OSC 52 clipboard",
            ));
        }
        let encoded = base64_encode(text.as_bytes());
        let sequence = format!("\x1b]52;c;{encoded}\x07");
        self.terminal.backend_mut().write_all(sequence.as_bytes())?;
        self.terminal.backend_mut().flush()
    }

    /// Leave raw/alternate-screen mode while an external editor or pager owns
    /// the user's terminal. The underlying backend remains alive so the same
    /// surface can be resumed without rebuilding view state.
    pub fn suspend(&mut self) -> io::Result<()> {
        if self.restored || self.suspended {
            return Ok(());
        }
        leave_modes(self.terminal.backend_mut(), self.capabilities)?;
        disable_raw_mode()?;
        self.suspended = true;
        Ok(())
    }

    /// Re-enter the pager surface after an external process returns.
    pub fn resume(&mut self) -> io::Result<()> {
        if self.restored || !self.suspended {
            return Ok(());
        }
        enable_raw_mode()?;
        if let Err(error) = enter_modes(self.terminal.backend_mut(), self.capabilities) {
            let _ = leave_modes(self.terminal.backend_mut(), self.capabilities);
            let _ = disable_raw_mode();
            return Err(error);
        }
        if let Err(error) = self.terminal.clear() {
            let _ = leave_modes(self.terminal.backend_mut(), self.capabilities);
            let _ = disable_raw_mode();
            return Err(error);
        }
        self.suspended = false;
        Ok(())
    }

    pub fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        if self.suspended {
            self.suspended = false;
            return Ok(());
        }
        let terminal_result = leave_modes(self.terminal.backend_mut(), self.capabilities);
        let raw_result = disable_raw_mode();
        match (terminal_result, raw_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
        }
    }
}

fn enter_modes<W: Write>(writer: &mut W, capabilities: TerminalCapabilities) -> io::Result<()> {
    if capabilities.alternate_screen {
        execute!(writer, EnterAlternateScreen)?;
    }
    if capabilities.bracketed_paste {
        execute!(writer, EnableBracketedPaste)?;
    }
    if capabilities.mouse {
        execute!(writer, EnableMouseCapture)?;
    }
    if capabilities.cursor {
        execute!(writer, Hide)?;
    }
    Ok(())
}

fn leave_modes<W: Write>(writer: &mut W, capabilities: TerminalCapabilities) -> io::Result<()> {
    let mut first_error = None;
    macro_rules! leave {
        ($command:expr) => {
            if first_error.is_none() {
                if let Err(error) = execute!(writer, $command) {
                    first_error = Some(error);
                }
            }
        };
    }
    if capabilities.cursor {
        leave!(Show);
    }
    if capabilities.mouse {
        leave!(DisableMouseCapture);
    }
    if capabilities.bracketed_paste {
        leave!(DisableBracketedPaste);
    }
    if capabilities.alternate_screen {
        leave!(LeaveAlternateScreen);
    }
    first_error.map_or(Ok(()), Err)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[((a & 0x03) << 4 | b >> 4) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((b & 0x0f) << 2 | c >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

impl Drop for TerminalSurface {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_encoding_matches_osc52_examples() {
        assert_eq!(super::base64_encode(b""), "");
        assert_eq!(super::base64_encode(b"f"), "Zg==");
        assert_eq!(super::base64_encode(b"fo"), "Zm8=");
        assert_eq!(super::base64_encode(b"foo"), "Zm9v");
        assert_eq!(super::base64_encode("中文".as_bytes()), "5Lit5paH");
    }

    #[test]
    fn terminal_capabilities_keep_cell_diff_available_for_fallback_rendering() {
        let capabilities = super::TerminalCapabilities::probe();
        assert!(capabilities.cell_diff);
    }

    #[test]
    fn grok_theme_exposes_full_renderer_role_closure() {
        let theme = super::Theme::default();
        assert_ne!(theme.prompt_border, theme.prompt_border_active);
        assert_ne!(theme.diff_delete_fg, theme.diff_insert_fg);
        assert_ne!(theme.md_code, theme.md_text);
        assert!(theme.md_heading_h1_mod.contains(super::Modifier::BOLD));
    }

    #[test]
    fn style_helpers_match_grok_reset_and_rgb_rules() {
        use super::{Color, Modifier, Theme};

        let theme = Theme::default();
        assert_eq!(theme.primary().fg, Some(theme.text_primary));
        assert_eq!(theme.muted().fg, Some(theme.gray));
        assert_eq!(theme.dim().fg, Some(theme.gray_dim));
        assert_eq!(theme.fg(theme.path).fg, Some(theme.path));

        let mut native = theme;
        native.gray = Color::Reset;
        native.gray_dim = Color::Reset;
        assert_eq!(native.muted().fg, None);
        assert!(native.muted().add_modifier.contains(Modifier::DIM));
        assert!(native.dim().add_modifier.contains(Modifier::DIM));
    }
}
