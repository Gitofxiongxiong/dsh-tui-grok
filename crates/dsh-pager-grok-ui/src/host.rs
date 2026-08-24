//! Host-OS probes used by the vendored Grok action registry.
//!
//! B seam: Grok's `crate::host` is a larger process/OS layer. The registry only
//! needs WSL detection for `Ctrl+.` reliability.

/// True when this Linux binary is running under Win32's WSL pipeline.
pub fn is_wsl() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    std::fs::read_to_string("/proc/version")
        .map(|version| version.to_ascii_lowercase().contains("microsoft"))
        .unwrap_or(false)
}
