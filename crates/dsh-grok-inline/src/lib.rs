//! Inline terminal implementation derived from xAI's `xai-ratatui-inline`.
//!
//! Source: Grok Build mirror commit `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`,
//! `SOURCE_REV` `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa`.
//! The source is Apache-2.0; see [`../NOTICE`](../NOTICE) and the repository
//! `LICENSE-APACHE`. DSH changes are limited to crate/package boundaries.

mod common;
mod resize;
mod scrollback;
mod segment;
mod terminal;

#[cfg(test)]
mod tests;

pub use self::{
    common::{TerminalLike, with_synchronized_output},
    resize::{resize_purge_rerender, resize_viewport_height},
    scrollback::emit_to_scrollback,
    segment::split_into_line_segments,
    terminal::{LinkSpan, Terminal},
};
