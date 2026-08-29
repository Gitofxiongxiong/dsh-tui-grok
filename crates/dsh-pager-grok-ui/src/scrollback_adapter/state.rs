//! Grok-derived scrollback viewport and layout state.
//!
//! Source closure:
//! - `xai-grok-pager/src/scrollback/state/mod.rs::prepare_layout`
//! - `xai-grok-pager/src/scrollback/state/nav.rs`
//! - `xai-grok-pager/src/scrollback/state/layout.rs::settle_visible_measurements`
//!
//! DSH owns canonical entries and protocol generations.  This type is the
//! single UI owner of scroll offset, total height, viewport height, follow and
//! the parked top anchor.  The host pane supplies Grok-rendered exact heights;
//! neither the runtime nor the DSH Fenwick cache owns a second viewport state.

use dsh_pager::scrollback::{ScrollAnchor, Scrollback};

use super::{host_pane::DshScrollbackHost, materialize_entry::RichPaintLine};

/// Unified Grok-compatible state for the production transcript viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrokScrollbackState {
    /// Absolute virtual row from the top of the rendered transcript.
    scroll_offset: usize,
    /// Exact/estimated content height settled by the most recent layout pass.
    total_height: usize,
    /// Terminal rows available to the scrollback content.
    viewport_height: u16,
    /// Whether new content keeps the viewport pinned to the tail.
    follow_mode: bool,
    /// Grok page-flip guard.  `goto_bottom` always clears it.
    follow_preserve_scroll: bool,
    /// Stable top-entry anchor captured after the last completed layout pass.
    viewport_anchor: Option<ScrollAnchor>,
    /// Width used by the most recent completed layout pass.
    last_width: usize,
    /// Sticky rows removed from a full-page navigation step.
    sticky_header_rows: u16,
}

impl Default for GrokScrollbackState {
    fn default() -> Self {
        Self {
            scroll_offset: 0,
            total_height: 0,
            viewport_height: 0,
            follow_mode: true,
            follow_preserve_scroll: false,
            viewport_anchor: None,
            last_width: 0,
            sticky_header_rows: 0,
        }
    }
}

impl GrokScrollbackState {
    /// Prepare the one authoritative layout for this frame.
    ///
    /// This is the DSH-hosted counterpart of Grok's `prepare_layout`: it is
    /// the only production call that may resolve estimates, change
    /// `total_height`, restore a parked anchor, clamp the viewport or re-pin a
    /// following viewport.  Rendering consumes the prepared window without a
    /// second materialization pass.
    pub fn prepare_layout(
        &mut self,
        host: &mut DshScrollbackHost,
        scrollback: &mut Scrollback,
        width: usize,
        height: u16,
    ) {
        self.viewport_height = height;
        if height == 0 || host.is_empty() {
            self.reset();
            self.viewport_height = height;
            self.last_width = width;
            return;
        }

        self.last_width = width;

        // The stable entry id survives content growth and structural shifts.
        // On an explicit navigation `viewport_anchor` is cleared, so the new
        // absolute offset wins for this pass and a fresh anchor is captured at
        // the end.  Width changes also restore by identity; the host clamps the
        // intra-entry row to the new wrapped span.
        let parked_anchor = (!self.follow_mode)
            .then_some(self.viewport_anchor)
            .flatten();
        if let Some(anchor) = parked_anchor
            && let Some(restored) = host.scroll_for_anchor(scrollback, anchor)
        {
            self.scroll_offset = restored;
        }

        // Measurement is monotonic for a frame but may reveal one more entry
        // at the lower edge.  Re-run the bounded host settle until both height
        // and anchor-derived top are stable.  This mirrors Grok's
        // `settle_visible_measurements` loop while keeping DSH entry identity.
        let max_iters = scrollback.entries().len().saturating_add(2).min(64);
        for _ in 0..max_iters {
            self.total_height = host.total_height(scrollback);
            let max_offset = self.max_scroll_offset();
            if self.follow_mode && !self.follow_preserve_scroll {
                self.scroll_offset = max_offset;
            }

            let before_top = self.scroll_offset;
            let before_height = self.total_height;
            self.scroll_offset = host.prepare_viewport(
                scrollback,
                self.scroll_offset,
                self.viewport_height,
                self.follow_mode,
            );
            self.total_height = host.total_height(scrollback);

            if self.follow_mode {
                if self.follow_preserve_scroll {
                    self.follow_preserve_scroll = false;
                    self.scroll_offset = self.scroll_offset.min(self.max_scroll_offset());
                } else {
                    self.scroll_offset = self.max_scroll_offset();
                }
            } else if let Some(anchor) = parked_anchor {
                self.scroll_offset = host
                    .scroll_for_anchor(scrollback, anchor)
                    .unwrap_or(self.scroll_offset)
                    .min(self.max_scroll_offset());
            } else {
                self.scroll_offset = self.scroll_offset.min(self.max_scroll_offset());
            }

            if self.scroll_offset == before_top && self.total_height == before_height {
                break;
            }
        }

        self.sticky_header_rows =
            host.sticky_header_rows(scrollback, self.scroll_offset, self.viewport_height);
        self.viewport_anchor = (!self.follow_mode)
            .then(|| host.anchor_at(scrollback, self.scroll_offset))
            .flatten();

        // On resize the restored stable anchor, rather than the previous
        // wrapped absolute row, determines the new top.
    }

    /// Paint only the window settled by [`Self::prepare_layout`].
    pub fn visible_lines(
        &mut self,
        host: &mut DshScrollbackHost,
        scrollback: &mut Scrollback,
    ) -> Vec<RichPaintLine> {
        host.visible_lines_prepared(scrollback, self.scroll_offset, self.viewport_height)
    }

    /// Scroll up by `rows`, copied from Grok's absolute-row navigation rule.
    pub fn scroll_up(&mut self, rows: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(rows as usize);
        self.follow_mode = false;
        self.follow_preserve_scroll = false;
        self.viewport_anchor = None;
    }

    /// Scroll down by `rows`.
    ///
    /// As in Grok, landing at the bottom remains manual.  A later downward
    /// event that starts fully clamped moves zero rows and re-engages follow.
    pub fn scroll_down(&mut self, rows: u16) {
        let max_offset = self.max_scroll_offset();
        let before = self.scroll_offset;
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(rows as usize)
            .min(max_offset);
        if rows > 0 && self.scroll_offset == before && self.scroll_offset >= max_offset {
            self.follow_mode = true;
            self.follow_preserve_scroll = false;
        }
        self.viewport_anchor = None;
    }

    fn page_scroll_rows(&self) -> u16 {
        self.viewport_height
            .saturating_sub(self.sticky_header_rows)
            .saturating_sub(2)
            .max(1)
    }

    pub fn page_up(&mut self) {
        self.scroll_up(self.page_scroll_rows());
    }

    pub fn page_down(&mut self) {
        self.scroll_down(self.page_scroll_rows());
    }

    pub fn half_page_up(&mut self) {
        self.scroll_up(self.viewport_height / 2);
    }

    pub fn half_page_down(&mut self) {
        self.scroll_down(self.viewport_height / 2);
    }

    pub fn goto_top(&mut self) {
        self.scroll_offset = 0;
        self.follow_mode = false;
        self.follow_preserve_scroll = false;
        self.viewport_anchor = None;
    }

    /// Go to the real settled bottom and resume follow.
    pub fn goto_bottom(&mut self) {
        self.scroll_offset = self.max_scroll_offset();
        self.follow_mode = true;
        self.follow_preserve_scroll = false;
        self.viewport_anchor = None;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn total_height(&self) -> usize {
        self.total_height
    }

    pub fn viewport_height(&self) -> u16 {
        self.viewport_height
    }

    pub fn is_following(&self) -> bool {
        self.follow_mode
    }

    pub fn max_scroll_offset(&self) -> usize {
        self.total_height
            .saturating_sub(self.viewport_height as usize)
    }
}

#[cfg(test)]
mod tests {
    use dsh_pager::scrollback::Scrollback;
    use dsh_pager_protocol::{HistoryEntry, SessionEvent};
    use serde_json::json;

    use super::GrokScrollbackState;
    use crate::{scrollback_adapter::host_pane::DshScrollbackHost, theme::Theme};

    fn history(seq: i64, event_type: &str, data: serde_json::Value) -> HistoryEntry {
        HistoryEntry {
            event: SessionEvent {
                event_type: event_type.into(),
                seq,
                time: seq as f64,
                data,
                source_event_seqs: None,
                surface_op: None,
                ignorable: None,
            },
            view: None,
        }
    }

    fn state_at_bottom(total_height: usize, viewport_height: u16) -> GrokScrollbackState {
        GrokScrollbackState {
            scroll_offset: total_height.saturating_sub(viewport_height as usize),
            total_height,
            viewport_height,
            ..GrokScrollbackState::default()
        }
    }

    #[test]
    fn landing_at_bottom_then_overscrolling_reengages_follow() {
        let mut state = state_at_bottom(120, 20);
        state.scroll_up(10);
        state.scroll_down(10);
        assert_eq!(state.scroll_offset(), 100);
        assert!(!state.is_following());

        state.scroll_down(1);
        assert!(state.is_following());
    }

    #[test]
    fn goto_bottom_clears_manual_position_and_follows() {
        let mut state = state_at_bottom(120, 20);
        state.scroll_up(30);
        state.goto_bottom();
        assert_eq!(state.scroll_offset(), 100);
        assert!(state.is_following());
    }

    #[test]
    fn page_rows_exclude_sticky_header_and_overlap() {
        let mut state = state_at_bottom(120, 20);
        state.sticky_header_rows = 4;
        state.page_up();
        assert_eq!(state.scroll_offset(), 86);
    }

    #[test]
    fn half_page_navigation_uses_real_viewport_height() {
        let mut state = state_at_bottom(120, 21);
        state.half_page_up();
        assert_eq!(state.scroll_offset(), 89);
        state.half_page_down();
        assert_eq!(state.scroll_offset(), 99);
    }

    #[test]
    fn parked_anchor_survives_streaming_height_growth() {
        let mut scrollback = Scrollback::default();
        for seq in 0..24 {
            scrollback.apply_event(&history(
                seq,
                "user/message",
                json!({
                    "source": { "kind": "user" },
                    "content": [{
                        "type": "text",
                        "text": format!("history {seq} alpha beta gamma delta epsilon")
                    }]
                }),
            ));
        }
        scrollback.apply_event(&history(
            24,
            "assistant/chunk",
            json!({
                "turn": 1,
                "step": 0,
                "chunk": {
                    "type": "text-delta",
                    "index": 0,
                    "text": "# live\n\nalpha beta gamma delta epsilon"
                }
            }),
        ));

        let mut host = DshScrollbackHost::default();
        let mut state = GrokScrollbackState::default();
        let theme = *Theme::current();
        host.sync(&mut scrollback, 42, theme);
        state.prepare_layout(&mut host, &mut scrollback, 42, 10);
        state.scroll_up(7);
        state.prepare_layout(&mut host, &mut scrollback, 42, 10);
        let before = host
            .anchor_at(&mut scrollback, state.scroll_offset())
            .expect("parked anchor");

        scrollback.apply_event(&history(
            25,
            "assistant/chunk",
            json!({
                "turn": 1,
                "step": 0,
                "chunk": {
                    "type": "text-delta",
                    "index": 0,
                    "text": " zeta eta theta iota kappa lambda mu nu xi omicron"
                }
            }),
        ));
        host.sync(&mut scrollback, 42, theme);
        state.prepare_layout(&mut host, &mut scrollback, 42, 10);
        let after = host
            .anchor_at(&mut scrollback, state.scroll_offset())
            .expect("restored anchor");

        assert_eq!(after, before);
        assert!(!state.is_following());
    }

    #[test]
    fn overscroll_follow_tracks_later_stream_growth() {
        let mut scrollback = Scrollback::default();
        scrollback.apply_event(&history(
            0,
            "assistant/chunk",
            json!({
                "turn": 1,
                "step": 0,
                "chunk": {
                    "type": "text-delta",
                    "index": 0,
                    "text": "one two three four five six seven eight nine ten eleven twelve"
                }
            }),
        ));
        let mut host = DshScrollbackHost::default();
        let mut state = GrokScrollbackState::default();
        let theme = *Theme::current();
        host.sync(&mut scrollback, 18, theme);
        state.prepare_layout(&mut host, &mut scrollback, 18, 4);
        state.scroll_up(2);
        state.scroll_down(2);
        assert!(!state.is_following());
        state.scroll_down(1);
        assert!(state.is_following());

        let old_bottom = state.scroll_offset();
        scrollback.apply_event(&history(
            1,
            "assistant/chunk",
            json!({
                "turn": 1,
                "step": 0,
                "chunk": {
                    "type": "text-delta",
                    "index": 0,
                    "text": " thirteen fourteen fifteen sixteen seventeen eighteen"
                }
            }),
        ));
        host.sync(&mut scrollback, 18, theme);
        state.prepare_layout(&mut host, &mut scrollback, 18, 4);
        assert!(state.scroll_offset() > old_bottom);
        assert_eq!(state.scroll_offset(), state.max_scroll_offset());
    }
}
