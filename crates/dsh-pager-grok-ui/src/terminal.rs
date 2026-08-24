//! Terminal-brand stub for the vendored Grok action registry and shortcuts bar.
//!
//! B seam: this is not Grok's full `xai-grok-pager-render` terminal tree. It
//! copies the product-facing predicates `build_hints` / `default_actions`
//! actually read: VS Code family, Apple Terminal, SSH, `Ctrl+.` reliability,
//! and Shift+Enter availability.

use std::sync::OnceLock;

/// Known terminal emulator categories used by Grok action defaults.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalName {
    AppleTerminal,
    VsCode,
    Cursor,
    Windsurf,
    Zed,
    #[default]
    Unknown,
    Other,
}

impl TerminalName {
    /// VS Code integrated terminal and xterm.js-based IDE embeds (including forks).
    pub fn is_vscode_family(self) -> bool {
        matches!(
            self,
            Self::VsCode | Self::Cursor | Self::Windsurf | Self::Zed
        )
    }
}

/// Process-lifetime terminal facts the registry and hint builder consult.
#[derive(Clone, Copy, Debug)]
pub struct TerminalContext {
    pub brand: TerminalName,
    pub is_ssh: bool,
    multiplexer: bool,
    vte_version: Option<u32>,
}

impl TerminalContext {
    /// True when `Ctrl+.` cannot be delivered reliably as a shortcuts primary.
    ///
    /// Conservative subset of Grok's `kitty_skip_reason`: vscode-family embeds
    /// and unidentified hosts without a multiplexer skip KKP, so the bar
    /// advertises `Ctrl+X` as the ShortcutsHelp primary.
    pub fn ctrl_dot_unreliable(&self) -> bool {
        self.brand.is_vscode_family() || (self.brand == TerminalName::Unknown && !self.multiplexer)
    }

    /// True when Shift+Enter is not distinguishable from Enter.
    ///
    /// Mirrors Grok's product rule: legacy VTE, VS Code-family xterm.js, and
    /// unidentified hosts with no multiplexer advertise `Alt+Enter:newline`.
    pub fn shift_enter_unavailable(&self) -> bool {
        if let Some(ver) = self.vte_version {
            return ver < 8200;
        }
        if self.brand.is_vscode_family() {
            return true;
        }
        self.brand == TerminalName::Unknown && !self.multiplexer
    }
}

/// Process-wide terminal context, detected once from the environment.
pub fn terminal_context() -> TerminalContext {
    static CONTEXT: OnceLock<TerminalContext> = OnceLock::new();
    *CONTEXT.get_or_init(detect_terminal_context)
}

fn detect_terminal_context() -> TerminalContext {
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();
    let brand = if term_program.eq_ignore_ascii_case("Apple_Terminal") {
        TerminalName::AppleTerminal
    } else if term_program.eq_ignore_ascii_case("vscode")
        || std::env::var_os("VSCODE_INJECTION").is_some()
    {
        TerminalName::VsCode
    } else if term_program.eq_ignore_ascii_case("cursor") {
        TerminalName::Cursor
    } else if term_program.eq_ignore_ascii_case("Windsurf") {
        TerminalName::Windsurf
    } else if term_program.eq_ignore_ascii_case("zed") {
        TerminalName::Zed
    } else if term_program.is_empty() {
        TerminalName::Unknown
    } else {
        TerminalName::Other
    };
    let multiplexer = std::env::var_os("TMUX").is_some()
        || std::env::var_os("STY").is_some()
        || term.starts_with("screen")
        || term.starts_with("tmux");
    let is_ssh = std::env::var_os("SSH_TTY").is_some()
        || std::env::var_os("SSH_CONNECTION").is_some()
        || std::env::var_os("SSH_CLIENT").is_some();
    let vte_version = std::env::var("VTE_VERSION")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());
    TerminalContext {
        brand,
        is_ssh,
        multiplexer,
        vte_version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_family_is_shift_enter_unavailable() {
        let ctx = TerminalContext {
            brand: TerminalName::Cursor,
            is_ssh: false,
            multiplexer: false,
            vte_version: None,
        };
        assert!(ctx.shift_enter_unavailable());
        assert!(ctx.ctrl_dot_unreliable());
    }

    #[test]
    fn unknown_without_multiplexer_is_conservative() {
        let ctx = TerminalContext {
            brand: TerminalName::Unknown,
            is_ssh: false,
            multiplexer: false,
            vte_version: None,
        };
        assert!(ctx.shift_enter_unavailable());
        assert!(ctx.ctrl_dot_unreliable());
    }

    #[test]
    fn other_brand_keeps_shift_enter() {
        let ctx = TerminalContext {
            brand: TerminalName::Other,
            is_ssh: false,
            multiplexer: false,
            vte_version: None,
        };
        assert!(!ctx.shift_enter_unavailable());
        assert!(!ctx.ctrl_dot_unreliable());
    }
}
