use std::io::{self, Stdout, Write};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use dsh_grok_inline::Terminal as InlineTerminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub chrome: Color,
    pub muted: Color,
    pub user: Color,
    pub assistant: Color,
    pub thinking: Color,
    pub tool: Color,
    pub result: Color,
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            chrome: Color::Cyan,
            muted: Color::DarkGray,
            user: Color::Green,
            assistant: Color::White,
            thinking: Color::Magenta,
            tool: Color::Yellow,
            result: Color::Blue,
            error: Color::Red,
        }
    }
}

/// Owns raw mode and alternate-screen restoration, including error unwinding.
pub struct TerminalSurface {
    terminal: InlineTerminal<CrosstermBackend<Stdout>>,
    restored: bool,
    suspended: bool,
}

impl TerminalSurface {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture,
            Hide
        ) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match InlineTerminal::new(CrosstermBackend::new(stdout)) {
            Ok(mut terminal) => {
                if let Err(error) = terminal.clear() {
                    let mut stdout = io::stdout();
                    let _ = execute!(
                        stdout,
                        Show,
                        DisableMouseCapture,
                        DisableBracketedPaste,
                        LeaveAlternateScreen
                    );
                    let _ = disable_raw_mode();
                    return Err(error);
                }
                Ok(Self {
                    terminal,
                    restored: false,
                    suspended: false,
                })
            }
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(
                    stdout,
                    Show,
                    DisableMouseCapture,
                    DisableBracketedPaste,
                    LeaveAlternateScreen
                );
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }

    pub fn size(&self) -> io::Result<Rect> {
        let size = self.terminal.size()?;
        Ok(Rect::new(0, 0, size.width, size.height))
    }

    pub fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.terminal.draw(render).map(|_| ())
    }

    /// Write a terminal-native clipboard update without coupling the pager to
    /// a desktop clipboard library. OSC 52 is understood by modern terminal
    /// emulators and also works when the pager is running over SSH or inside a
    /// multiplexer that forwards the sequence.
    pub fn copy_text(&mut self, text: &str) -> io::Result<()> {
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
        execute!(
            self.terminal.backend_mut(),
            Show,
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
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
        if let Err(error) = execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture,
            Hide
        ) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        if let Err(error) = self.terminal.clear() {
            let _ = execute!(
                self.terminal.backend_mut(),
                Show,
                DisableMouseCapture,
                DisableBracketedPaste,
                LeaveAlternateScreen
            );
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
        let terminal_result = execute!(
            self.terminal.backend_mut(),
            Show,
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let raw_result = disable_raw_mode();
        match (terminal_result, raw_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
        }
    }
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
}
