pub fn system_clipboard_get() -> Option<String> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardBackend {
    Osc52,
    System,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardResult {
    pub backend: ClipboardBackend,
    pub message: &'static str,
}

/// Return the desktop clipboard command selected by the environment. The
/// command is intentionally explicit and never shells out through `sh -c`.
pub fn system_clipboard_command() -> Option<&'static str> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Some("wl-copy")
    } else if std::env::var_os("DISPLAY").is_some() {
        Some("xclip")
    } else if cfg!(target_os = "macos") {
        Some("pbcopy")
    } else {
        None
    }
}

pub fn system_clipboard_set(text: &str) -> std::io::Result<ClipboardResult> {
    let Some(command) = system_clipboard_command() else {
        return Ok(ClipboardResult {
            backend: ClipboardBackend::Unavailable,
            message: "Clipboard unavailable",
        });
    };
    let mut child = std::process::Command::new(command);
    if command == "xclip" {
        child.args(["-selection", "clipboard"]);
    }
    let mut process = child.stdin(std::process::Stdio::piped()).spawn()?;
    if let Some(stdin) = process.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(text.as_bytes())?;
    }
    let status = process.wait()?;
    Ok(if status.success() {
        ClipboardResult {
            backend: ClipboardBackend::System,
            message: "Copied to system clipboard",
        }
    } else {
        ClipboardResult {
            backend: ClipboardBackend::Unavailable,
            message: "Clipboard command failed",
        }
    })
}

pub fn clipboard_text_is_pasteable(text: Option<&str>) -> bool {
    text.is_some_and(|value| !value.is_empty())
}

pub fn log_paste_key_empty_host_clipboard(_surface: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_capability_is_explicit_when_no_display_is_present() {
        // CI normally has neither variable; the assertion also documents the
        // typed result contract without depending on an external clipboard.
        if system_clipboard_command().is_none() {
            let result = system_clipboard_set("copy me").unwrap();
            assert_eq!(result.backend, ClipboardBackend::Unavailable);
        }
    }
}
