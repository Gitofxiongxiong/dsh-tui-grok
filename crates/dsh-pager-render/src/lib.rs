use std::io::{self, Stdout, Write};
use std::sync::OnceLock;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use dsh_grok_inline::{LinkSpan, Terminal as InlineTerminal};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

/// Semantic palette shared by every Grok-derived view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub accent_user: Color,
    pub bg_base: Color,
    pub bg_highlight: Color,
    pub bg_hover: Color,
    pub bg_light: Color,
    pub bg_visual: Color,
    pub gray: Color,
    pub gray_bright: Color,
    pub gray_dim: Color,
    pub fuzzy_accent: Color,
    pub text_secondary: Color,
    pub text_primary: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent_user: Color::Rgb(120, 180, 255),
            bg_base: Color::Rgb(17, 19, 24),
            bg_highlight: Color::Rgb(30, 34, 43),
            bg_hover: Color::Rgb(38, 43, 54),
            bg_light: Color::Rgb(27, 31, 40),
            bg_visual: Color::Rgb(45, 52, 68),
            gray: Color::Rgb(160, 168, 182),
            gray_bright: Color::Rgb(218, 224, 235),
            gray_dim: Color::Rgb(93, 101, 116),
            fuzzy_accent: Color::Rgb(120, 190, 255),
            text_secondary: Color::Rgb(223, 229, 240),
            text_primary: Color::Rgb(235, 239, 247),
        }
    }
}

impl Theme {
    pub fn current() -> &'static Self {
        static THEME: OnceLock<Theme> = OnceLock::new();
        THEME.get_or_init(Self::default)
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
}
