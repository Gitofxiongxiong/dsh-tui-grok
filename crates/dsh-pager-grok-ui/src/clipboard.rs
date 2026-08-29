//! Desktop clipboard write policy adapted from Grok's native+OSC 52 route.
//!
//! Grok sources (not vendored):
//! - `xai-grok-shared/src/clipboard.rs`: macOS `pbcopy` (not arboard/AppKit),
//!   Linux `wl-copy` then `xclip`, Windows arboard, bounded wait
//! - `xai-grok-pager-render/src/clipboard/mod.rs`: `clipboard_write_with_route`
//!   fires native **and** OSC 52 (not XOR)
//!
//! OSC 52 encoding stays in `dsh-pager-render::TerminalSurface::copy_text`.

use std::io::{self, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const CLIPBOARD_COMMAND_DEADLINE: Duration = Duration::from_secs(2);

pub fn system_clipboard_get() -> Option<String> {
    match native_clipboard_kind() {
        NativeClipboardKind::Pbcopy => capture_cli("pbpaste", &["-Prefer", "txt"]),
        NativeClipboardKind::WlCopy => capture_cli("wl-paste", &["--no-newline", "-t", "text"]),
        NativeClipboardKind::Xclip => capture_cli("xclip", &["-o", "-selection", "clipboard"]),
        NativeClipboardKind::Arboard => get_arboard(),
        NativeClipboardKind::Unavailable => None,
    }
    .filter(|text| !text.is_empty())
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

/// OS family used by the unit-testable native-write selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Macos,
    Linux,
    Windows,
    Other,
}

/// Native write target. `Arboard` is a library path, not a CLI binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeClipboardKind {
    Pbcopy,
    WlCopy,
    Xclip,
    Arboard,
    Unavailable,
}

impl NativeClipboardKind {
    /// CLI binary to spawn, if any. `None` for arboard and unavailable.
    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::Pbcopy => Some("pbcopy"),
            Self::WlCopy => Some("wl-copy"),
            Self::Xclip => Some("xclip"),
            Self::Arboard | Self::Unavailable => None,
        }
    }

    /// Grok `native_tool_name` label: `"arboard"` on Windows, CLI names elsewhere.
    pub fn tool_name(self) -> Option<&'static str> {
        match self {
            Self::Pbcopy => Some("pbcopy"),
            Self::WlCopy => Some("wl-copy"),
            Self::Xclip => Some("xclip"),
            Self::Arboard => Some("arboard"),
            Self::Unavailable => None,
        }
    }
}

pub fn current_host_os() -> HostOs {
    if cfg!(target_os = "macos") {
        HostOs::Macos
    } else if cfg!(target_os = "linux") {
        HostOs::Linux
    } else if cfg!(target_os = "windows") {
        HostOs::Windows
    } else {
        HostOs::Other
    }
}

/// Grok native-write selection. Pure: no PATH probe, no live pasteboard.
///
/// WSL without WSLg is Linux with neither display var (`host::is_wsl()` plus
/// no `WAYLAND_DISPLAY`/`DISPLAY`) and returns [`NativeClipboardKind::Unavailable`].
/// WSLg sets a display env and uses the same Linux CLI path.
pub fn select_native_clipboard(
    os: HostOs,
    wayland_display: bool,
    x11_display: bool,
) -> NativeClipboardKind {
    match os {
        HostOs::Macos => NativeClipboardKind::Pbcopy,
        HostOs::Linux => {
            if wayland_display {
                NativeClipboardKind::WlCopy
            } else if x11_display {
                NativeClipboardKind::Xclip
            } else {
                NativeClipboardKind::Unavailable
            }
        }
        HostOs::Windows => NativeClipboardKind::Arboard,
        HostOs::Other => NativeClipboardKind::Unavailable,
    }
}

pub fn native_clipboard_kind() -> NativeClipboardKind {
    select_native_clipboard(
        current_host_os(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
    )
}

/// Return the desktop clipboard command selected by the environment. The
/// command is intentionally explicit and never shells out through `sh -c`.
/// Windows uses arboard, so this is `None` there even when native writes work.
pub fn system_clipboard_command() -> Option<&'static str> {
    native_clipboard_kind().command()
}

pub fn system_clipboard_set(text: &str) -> io::Result<ClipboardResult> {
    set_with_kind(native_clipboard_kind(), text)
}

fn set_with_kind(kind: NativeClipboardKind, text: &str) -> io::Result<ClipboardResult> {
    match kind {
        NativeClipboardKind::Pbcopy => pipe_to_command("pbcopy", &[], text),
        NativeClipboardKind::WlCopy => pipe_to_command("wl-copy", &[], text),
        NativeClipboardKind::Xclip => pipe_to_command("xclip", &["-selection", "clipboard"], text),
        NativeClipboardKind::Arboard => set_arboard(text),
        NativeClipboardKind::Unavailable => Ok(unavailable_result()),
    }
}

fn unavailable_result() -> ClipboardResult {
    ClipboardResult {
        backend: ClipboardBackend::Unavailable,
        message: "Clipboard unavailable",
    }
}

fn copied_system_result() -> ClipboardResult {
    ClipboardResult {
        backend: ClipboardBackend::System,
        message: "Copied to system clipboard",
    }
}

/// Combine native and optional OSC 52 legs. Success if **either** works.
///
/// Mirrors Grok `clipboard_write_with_route`: native and OSC 52 are AND-fired,
/// not XOR. `osc52 == None` means the OSC leg was not attempted.
pub fn merge_copy_legs(
    native: io::Result<ClipboardResult>,
    osc52: Option<io::Result<()>>,
) -> ClipboardResult {
    let native_ok = matches!(
        native,
        Ok(ClipboardResult {
            backend: ClipboardBackend::System,
            ..
        })
    );
    let osc_ok = matches!(osc52, Some(Ok(())));
    if native_ok || osc_ok {
        ClipboardResult {
            backend: if native_ok {
                ClipboardBackend::System
            } else {
                ClipboardBackend::Osc52
            },
            message: "Selection copied",
        }
    } else {
        unavailable_result()
    }
}

fn pipe_to_command(bin: &str, args: &[&str], text: &str) -> io::Result<ClipboardResult> {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let write = if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).and_then(|_| stdin.flush())
    } else {
        Ok(())
    };
    let status = wait_with_deadline(&mut child, CLIPBOARD_COMMAND_DEADLINE);
    write?;
    let status = status?;
    Ok(if status.success() {
        copied_system_result()
    } else {
        ClipboardResult {
            backend: ClipboardBackend::Unavailable,
            message: "Clipboard command failed",
        }
    })
}

fn capture_cli(bin: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).ok()?;
        Some(buf)
    });
    let status = wait_with_deadline(&mut child, CLIPBOARD_COMMAND_DEADLINE);
    let buf = match reader.join() {
        Ok(Some(buf)) => buf,
        _ => return None,
    };
    let status = status.ok()?;
    if !status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Poll `try_wait` so a wedged compositor cannot freeze the UI thread.
/// Adapted from Grok `wait_with_deadline` (~15 ms, kill on expiry).
fn wait_with_deadline(child: &mut Child, deadline: Duration) -> io::Result<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if start.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "clipboard command timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

#[cfg(windows)]
fn set_arboard(text: &str) -> io::Result<ClipboardResult> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .map(|_| copied_system_result())
        .map_err(io::Error::other)
}

#[cfg(not(windows))]
fn set_arboard(_text: &str) -> io::Result<ClipboardResult> {
    Ok(unavailable_result())
}

#[cfg(windows)]
fn get_arboard() -> Option<String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_text())
        .ok()
}

#[cfg(not(windows))]
fn get_arboard() -> Option<String> {
    None
}

pub fn clipboard_text_is_pasteable(text: Option<&str>) -> bool {
    text.is_some_and(|value| !value.is_empty())
}

pub fn log_paste_key_empty_host_clipboard(_surface: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn linux_headless() -> NativeClipboardKind {
        select_native_clipboard(HostOs::Linux, false, false)
    }

    #[test]
    fn no_display_on_linux_has_no_cli_command() {
        let kind = linux_headless();
        assert_eq!(kind, NativeClipboardKind::Unavailable);
        assert_eq!(kind.command(), None);
        assert_eq!(kind.tool_name(), None);
    }

    #[test]
    fn wsl_without_display_is_native_unavailable() {
        // Headless WSL (`host::is_wsl()` without WSLg) is Linux with no display.
        assert_eq!(linux_headless(), NativeClipboardKind::Unavailable);
    }

    #[test]
    fn wsl_with_wslg_uses_linux_cli() {
        assert_eq!(
            select_native_clipboard(HostOs::Linux, true, false),
            NativeClipboardKind::WlCopy
        );
        assert_eq!(
            select_native_clipboard(HostOs::Linux, false, true),
            NativeClipboardKind::Xclip
        );
    }

    #[test]
    fn macos_uses_pbcopy_even_without_display() {
        let kind = select_native_clipboard(HostOs::Macos, false, false);
        assert_eq!(kind, NativeClipboardKind::Pbcopy);
        assert_eq!(kind.command(), Some("pbcopy"));
        assert_eq!(kind.tool_name(), Some("pbcopy"));
    }

    #[test]
    fn macos_ignores_xquartz_display_env() {
        let kind = select_native_clipboard(HostOs::Macos, false, true);
        assert_eq!(kind, NativeClipboardKind::Pbcopy);
        assert_eq!(kind.command(), Some("pbcopy"));
    }

    #[test]
    fn wayland_display_selects_wl_copy() {
        let kind = select_native_clipboard(HostOs::Linux, true, false);
        assert_eq!(kind, NativeClipboardKind::WlCopy);
        assert_eq!(kind.command(), Some("wl-copy"));
    }

    #[test]
    fn wayland_wins_over_x11_display() {
        let kind = select_native_clipboard(HostOs::Linux, true, true);
        assert_eq!(kind, NativeClipboardKind::WlCopy);
    }

    #[test]
    fn x11_display_selects_xclip() {
        let kind = select_native_clipboard(HostOs::Linux, false, true);
        assert_eq!(kind, NativeClipboardKind::Xclip);
        assert_eq!(kind.command(), Some("xclip"));
    }

    #[test]
    fn windows_native_path_is_arboard_not_a_cli() {
        let kind = select_native_clipboard(HostOs::Windows, false, false);
        assert_eq!(kind, NativeClipboardKind::Arboard);
        assert_eq!(kind.command(), None);
        assert_eq!(kind.tool_name(), Some("arboard"));
        assert_ne!(kind, NativeClipboardKind::Unavailable);
    }

    #[test]
    fn merge_osc52_only_success() {
        let result = merge_copy_legs(Ok(unavailable_result()), Some(Ok(())));
        assert_eq!(result.backend, ClipboardBackend::Osc52);
        assert_eq!(result.message, "Selection copied");
    }

    #[test]
    fn merge_native_only_success() {
        let result = merge_copy_legs(Ok(copied_system_result()), None);
        assert_eq!(result.backend, ClipboardBackend::System);
        assert_eq!(result.message, "Selection copied");
    }

    #[test]
    fn merge_native_success_despite_osc52_error() {
        let osc_err = io::Error::other("osc failed");
        let result = merge_copy_legs(Ok(copied_system_result()), Some(Err(osc_err)));
        assert_eq!(result.backend, ClipboardBackend::System);
        assert_eq!(result.message, "Selection copied");
    }

    #[test]
    fn merge_osc52_success_despite_native_error() {
        let native_err = io::Error::other("native failed");
        let result = merge_copy_legs(Err(native_err), Some(Ok(())));
        assert_eq!(result.backend, ClipboardBackend::Osc52);
        assert_eq!(result.message, "Selection copied");
    }

    #[test]
    fn merge_both_fail_is_unavailable() {
        let osc_err = io::Error::other("osc failed");
        let result = merge_copy_legs(Ok(unavailable_result()), Some(Err(osc_err)));
        assert_eq!(result.backend, ClipboardBackend::Unavailable);
        assert_eq!(result.message, "Clipboard unavailable");
    }

    #[test]
    fn merge_native_error_without_osc52_is_unavailable() {
        let native_err = io::Error::other("native failed");
        let result = merge_copy_legs(Err(native_err), None);
        assert_eq!(result.backend, ClipboardBackend::Unavailable);
        assert_eq!(result.message, "Clipboard unavailable");
    }

    #[test]
    fn merge_both_ok_prefers_system_backend() {
        let result = merge_copy_legs(Ok(copied_system_result()), Some(Ok(())));
        assert_eq!(result.backend, ClipboardBackend::System);
        assert_eq!(result.message, "Selection copied");
    }

    #[test]
    fn headless_native_set_is_unavailable_without_live_pasteboard() {
        let result = set_with_kind(NativeClipboardKind::Unavailable, "copy me").unwrap();
        assert_eq!(result.backend, ClipboardBackend::Unavailable);
    }

    #[cfg(unix)]
    fn spawn_sleep(seconds: &str) -> Child {
        Command::new("sleep")
            .arg(seconds)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleep")
    }

    #[cfg(unix)]
    #[test]
    fn wait_with_deadline_reaps_fast_child() {
        let mut child = spawn_sleep("0");
        let status = wait_with_deadline(&mut child, Duration::from_secs(2)).unwrap();
        assert!(status.success());
    }

    #[cfg(unix)]
    #[test]
    fn capture_cli_drains_large_stdout_without_deadlock() {
        let started = Instant::now();
        let text = capture_cli(
            "node",
            &["-e", "process.stdout.write('x'.repeat(2 * 1024 * 1024))"],
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "stdout must be drained while waiting: {:?}",
            started.elapsed()
        );
        assert_eq!(text.as_deref().map(str::len), Some(2 * 1024 * 1024));
    }

    #[cfg(unix)]
    #[test]
    fn wait_with_deadline_kills_hung_child() {
        let mut child = spawn_sleep("30");
        let started = Instant::now();
        let err = wait_with_deadline(&mut child, Duration::from_millis(50)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(child.try_wait().unwrap().is_some());
    }

    struct DisplayEnvGuard {
        wayland: Option<std::ffi::OsString>,
        display: Option<std::ffi::OsString>,
    }

    impl DisplayEnvGuard {
        fn apply(wayland: Option<&str>, display: Option<&str>) -> Self {
            let wayland_prev = std::env::var_os("WAYLAND_DISPLAY");
            let display_prev = std::env::var_os("DISPLAY");
            unsafe {
                match wayland {
                    Some(value) => std::env::set_var("WAYLAND_DISPLAY", value),
                    None => std::env::remove_var("WAYLAND_DISPLAY"),
                }
                match display {
                    Some(value) => std::env::set_var("DISPLAY", value),
                    None => std::env::remove_var("DISPLAY"),
                }
            }
            Self {
                wayland: wayland_prev,
                display: display_prev,
            }
        }
    }

    impl Drop for DisplayEnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.wayland {
                    Some(value) => std::env::set_var("WAYLAND_DISPLAY", value),
                    None => std::env::remove_var("WAYLAND_DISPLAY"),
                }
                match &self.display {
                    Some(value) => std::env::set_var("DISPLAY", value),
                    None => std::env::remove_var("DISPLAY"),
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[serial]
    fn live_wayland_env_selects_wl_copy() {
        let _guard = DisplayEnvGuard::apply(Some("wayland-0"), None);
        assert_eq!(system_clipboard_command(), Some("wl-copy"));
        assert_eq!(native_clipboard_kind(), NativeClipboardKind::WlCopy);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[serial]
    fn live_x11_env_selects_xclip() {
        let _guard = DisplayEnvGuard::apply(None, Some(":0"));
        assert_eq!(system_clipboard_command(), Some("xclip"));
        assert_eq!(native_clipboard_kind(), NativeClipboardKind::Xclip);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[serial]
    fn live_headless_env_has_no_command() {
        let _guard = DisplayEnvGuard::apply(None, None);
        assert_eq!(system_clipboard_command(), None);
        assert_eq!(native_clipboard_kind(), NativeClipboardKind::Unavailable);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn live_macos_command_is_pbcopy() {
        assert_eq!(system_clipboard_command(), Some("pbcopy"));
        assert_eq!(native_clipboard_kind(), NativeClipboardKind::Pbcopy);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn live_windows_kind_is_arboard() {
        assert_eq!(native_clipboard_kind(), NativeClipboardKind::Arboard);
        assert_eq!(native_clipboard_kind().tool_name(), Some("arboard"));
        assert_eq!(system_clipboard_command(), None);
    }
}
