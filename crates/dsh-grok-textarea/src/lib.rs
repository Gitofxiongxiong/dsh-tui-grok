//! Text editing widget derived from xAI's `xai-ratatui-textarea`.
//!
//! Source: Grok Build mirror commit `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`,
//! `SOURCE_REV` `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`.
//! The source is Apache-2.0; see [`../NOTICE`](../NOTICE) and the repository
//! `LICENSE-APACHE`. DSH changes are limited to crate/package boundaries.
#![allow(clippy::new_without_default)]
// The upstream test suite contains a few intentionally explicit imports and
// one-element range fixtures. Keep those tests byte-close to the source while
// allowing the DSH workspace's `-D warnings` policy to run them unchanged.
#![allow(unused_imports)]
#![allow(clippy::single_range_in_vec_init)]
#![allow(clippy::useless_borrows_in_formatting)]

pub mod editor;
pub mod render;
pub mod textarea;
pub mod wrapping;

pub use editor::{
    ApplyEditPlanError, EditBuffer, EditCommand, EditDelta, EditOutcome, EditPlan,
    PostEditCursorAffinity, SingleLineViewport, WordStyle, classify_key_event,
};
pub use textarea::{
    ClipboardProvider, ElementId, ElementKind, InternalClipboard, MouseAction, TextArea,
    TextAreaState, TextElement, TextElementEvent, TextElementEventKind, is_undo_input,
};

use crossterm::event::KeyModifiers;

// On Windows, AltGr arrives as Ctrl+Alt; on other platforms it's composed before reaching us.
#[cfg(target_os = "windows")]
#[inline]
pub fn is_altgr(modifiers: KeyModifiers) -> bool {
    let without_shift = modifiers & !KeyModifiers::SHIFT;
    without_shift == (KeyModifiers::CONTROL | KeyModifiers::ALT)
}

#[cfg(not(target_os = "windows"))]
#[inline]
pub fn is_altgr(_modifiers: KeyModifiers) -> bool {
    false
}
