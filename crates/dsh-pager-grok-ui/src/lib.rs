//! Grok Build-derived UI surface for DSH.
//!
//! The files under `vendor/grok` are intentionally kept close to their
//! upstream module paths.  DSH-specific knowledge lives in `host_adapter` and
//! `runtime`, so the same view layer can later be attached to another harness.

pub mod appearance;
pub mod clipboard;
pub mod glyphs;
pub mod input;
#[path = "../vendor/grok/xai-grok-pager-render/src/modal_window_state.rs"]
pub mod modal_window_state;
pub mod render;
pub mod theme;
pub mod views;

pub mod effects;
pub mod host_adapter;
pub mod runtime;

pub use effects::{
    DshEffectSink, OperationKey, UiContext, UiEffect, UiEffectReceipt, UiEffectSink,
    UiEffectStatus, UiIntent, compile_intent,
};
pub use runtime::run_interactive;
