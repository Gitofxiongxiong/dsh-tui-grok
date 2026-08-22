//! Minimal appearance state seam used by the copied modal chrome.

pub mod cache {
    use std::sync::atomic::{AtomicBool, Ordering};

    static VIM_MODE: AtomicBool = AtomicBool::new(false);

    pub fn load_vim_mode() -> bool {
        VIM_MODE.load(Ordering::Relaxed)
    }

    pub fn set_vim_mode(enabled: bool) {
        VIM_MODE.store(enabled, Ordering::Relaxed);
    }
}
