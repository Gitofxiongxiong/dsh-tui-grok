//! Grok's semantic palette is owned by the renderer crate.
#[path = "../vendor/grok/xai-grok-pager-render/src/theme/wave.rs"]
mod wave;

pub use dsh_pager_render::Theme;
pub use wave::{pulse_brightness, wave_brightness};
