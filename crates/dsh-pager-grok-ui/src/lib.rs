//! Grok Build-derived UI surface for DSH.
//!
//! The files under `vendor/grok` are intentionally kept close to their
//! upstream module paths.  DSH-specific knowledge lives in `host_adapter` and
//! `runtime`, so the same view layer can later be attached to another harness.

#[path = "../vendor/grok/xai-grok-pager/src/actions/mod.rs"]
pub mod actions;
pub mod app;
pub mod appearance;
pub mod clipboard;
pub mod glyphs;
pub mod host;
pub mod input;
#[path = "../vendor/grok/xai-grok-pager-render/src/modal_window_state.rs"]
pub mod modal_window_state;
pub mod render;
pub mod slash;
pub mod terminal;
pub mod theme;
pub mod views;

pub mod effects;
pub mod geometry;
pub mod host_adapter;
pub mod media;
pub mod parity;
pub mod runtime;
pub mod scheduler;
pub mod scrollback;
pub mod scrollback_adapter;
pub mod selection;
pub mod session_mode;

pub use app::{
    AppShell, KeyOwner, Overlay, REPLACEMENT_MAP, ReplacementEntry, ShellAction, ShellEvent,
    ShellLayout, ShellSnapshot,
};
pub use dsh_pager_render::{TerminalCapabilities, Theme};
pub use effects::{
    DshEffectSink, OperationKey, UiContext, UiEffect, UiEffectReceipt, UiEffectSink,
    UiEffectStatus, UiIntent, compile_intent,
};
pub use geometry::{GeometryLine, HitMap, HitRegion, HitTarget, LinkTarget};
pub use runtime::run_interactive;
pub use scheduler::{
    BoundedScheduler, GenerationGuard, ReconnectPolicy, ReconnectState, SchedulerStats,
};
pub use selection::{ResolvedSelection, SelectionModel, SelectionPoint};
pub use views::agent::{AgentView, AgentViewLayout};
