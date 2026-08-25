//! File-backed diagnostics for sudden interactive exits.
//!
//! Panic text painted on the alternate screen disappears when the surface is
//! restored. This module writes the same payload to a file so the next shell
//! prompt still has evidence.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

const ENV_PATH: &str = "DSH_PAGER_DIAG";
const DEFAULT_PANIC_NAME: &str = "dsh-pager-panic.log";

static HOOK: OnceLock<()> = OnceLock::new();

/// Optional trace file. `DSH_PAGER_DIAG=1` uses a temp default; any other
/// non-empty value is treated as a path.
pub fn log_path() -> Option<PathBuf> {
    match std::env::var(ENV_PATH) {
        Ok(value) if value == "1" || value.eq_ignore_ascii_case("true") => {
            Some(std::env::temp_dir().join("dsh-pager-diag.log"))
        }
        Ok(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => None,
    }
}

/// Panic payload always lands here, even when tracing is off.
pub fn panic_log_path() -> PathBuf {
    log_path().unwrap_or_else(|| std::env::temp_dir().join(DEFAULT_PANIC_NAME))
}

pub fn log(stage: &str, message: impl AsRef<str>) {
    let Some(path) = log_path() else {
        return;
    };
    write_line(&path, stage, message.as_ref());
}

pub fn log_always(stage: &str, message: impl AsRef<str>) {
    let message = message.as_ref();
    write_line(&panic_log_path(), stage, message);
    if log_path().as_deref() != Some(panic_log_path().as_path()) {
        log(stage, message);
    }
}

fn write_line(path: &Path, stage: &str, message: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{ts} {stage} {message}");
        let _ = file.flush();
    }
}

/// Install once. Later panics append to [`panic_log_path`] before the previous
/// hook runs, so alternate-screen restore cannot erase the only copy.
pub fn install_panic_hook() {
    HOOK.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let backtrace = std::backtrace::Backtrace::force_capture();
            log_always("panic", format!("{info}\n{backtrace}"));
            previous(info);
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    fn unique_path(label: &str) -> PathBuf {
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "dsh-pager-diag-{label}-{seq}-{}.log",
            std::process::id()
        ))
    }

    #[test]
    #[serial]
    fn env_path_writes_stage_line() {
        let path = unique_path("trace");
        let _ = fs::remove_file(&path);
        // SAFETY: serial_test holds DSH_PAGER_DIAG for this assertion only.
        unsafe {
            std::env::set_var(ENV_PATH, &path);
        }
        log("catalog", "entries=4");
        let body = fs::read_to_string(&path).expect("diag file");
        unsafe {
            std::env::remove_var(ENV_PATH);
        }
        let _ = fs::remove_file(&path);
        assert!(body.contains("catalog entries=4"), "{body}");
    }

    #[test]
    #[serial]
    fn panic_hook_appends_payload() {
        let path = unique_path("panic");
        let _ = fs::remove_file(&path);
        // SAFETY: serial_test holds DSH_PAGER_DIAG for this assertion only.
        unsafe {
            std::env::set_var(ENV_PATH, &path);
        }
        install_panic_hook();
        let caught = std::panic::catch_unwind(|| {
            panic!("diag-hook-probe");
        });
        let after = fs::read_to_string(&path).unwrap_or_default();
        unsafe {
            std::env::remove_var(ENV_PATH);
        }
        let _ = fs::remove_file(&path);
        assert!(caught.is_err());
        assert!(after.contains("diag-hook-probe"), "{after}");
        assert!(after.contains("panic"), "{after}");
    }
}
