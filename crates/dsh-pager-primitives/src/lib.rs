//! DSH-neutral presentation helpers derived from Grok Build's pager renderer.
//!
//! The original renderer's theme and pager-specific safe-buffer modules are
//! deliberately outside this crate. Callers supply styles and use the
//! generic wrapping/scrollbar functions directly.

mod line_utils;
pub mod scrollbar;
pub mod wrapping;

pub use line_utils::{fit_line_to_width, line_to_static, push_owned_lines};
