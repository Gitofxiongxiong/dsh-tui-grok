//! Pure sticky-prompt layout adapted from Grok's AllTurns scrollback.
//!
//! DSH drives this coordinate contract from its host pane while keeping Grok
//! storage, selection, media, and process state outside the vendored core.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptDescriptor {
    pub entry_idx: usize,
    pub y_virtual: usize,
    pub full_height: u16,
    pub min_height: u16,
    pub sticky: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderedPrompt {
    pub entry_idx: usize,
    pub render_height: u16,
    pub clip_top: u16,
}

impl RenderedPrompt {
    pub fn visible_height(self) -> u16 {
        self.render_height.saturating_sub(self.clip_top)
    }

    pub fn needs_scratch_buffer(self) -> bool {
        self.clip_top > 0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickyHeaderLayout {
    pub pushed: Option<RenderedPrompt>,
    pub pinned: Option<RenderedPrompt>,
}

const HEADER_CONTENT_GAP: u16 = 1;

impl StickyHeaderLayout {
    fn header_content_height(&self) -> u16 {
        let pushed = self.pushed.map_or(0, RenderedPrompt::visible_height);
        let pinned = self.pinned.map_or(0, RenderedPrompt::visible_height);
        pushed
            .saturating_add(pinned)
            .saturating_add(u16::from(pushed > 0 && pinned > 0))
    }

    pub fn has_header(&self) -> bool {
        self.pushed.is_some() || self.pinned.is_some()
    }

    pub fn header_screen_rows(&self) -> u16 {
        if !self.has_header() {
            return 0;
        }
        let content = self.header_content_height();
        if self.pushed.is_some() && self.pinned.is_none() {
            content
        } else {
            content.saturating_add(HEADER_CONTENT_GAP)
        }
    }

    pub fn content_height(&self, viewport_height: u16) -> u16 {
        viewport_height.saturating_sub(self.header_screen_rows())
    }

    pub fn scroll_for_content(&self, scroll_offset: usize) -> usize {
        scroll_offset.saturating_add(self.header_screen_rows() as usize)
    }

    pub fn pinned_entry_idx(&self) -> Option<usize> {
        self.pinned.map(|prompt| prompt.entry_idx)
    }

    pub fn pinned_screen_row(&self) -> Option<u16> {
        self.pinned?;
        let pushed = self.pushed.map_or(0, RenderedPrompt::visible_height);
        Some(pushed.saturating_add(u16::from(pushed > 0)))
    }

    pub fn entry_at_header_row(&self, row: u16) -> Option<usize> {
        if row >= self.header_screen_rows() {
            return None;
        }
        if let Some(pushed) = self.pushed
            && row < pushed.visible_height()
        {
            return Some(pushed.entry_idx);
        }
        let pinned = self.pinned?;
        let start = self.pinned_screen_row()?;
        (row >= start && row < start.saturating_add(pinned.visible_height()))
            .then_some(pinned.entry_idx)
    }
}

pub fn compute_sticky_layout(
    scroll_offset: usize,
    viewport_height: u16,
    prompts: &[PromptDescriptor],
) -> StickyHeaderLayout {
    if prompts.is_empty() || scroll_offset == 0 || viewport_height == 0 {
        return StickyHeaderLayout::default();
    }
    let Some(pinned_idx) = prompts
        .iter()
        .rposition(|prompt| prompt.sticky && prompt.y_virtual < scroll_offset)
    else {
        return StickyHeaderLayout::default();
    };
    let prompt = prompts[pinned_idx];
    let render_height = calculate_render_height(prompt, scroll_offset, viewport_height);
    let next = prompts.get(pinned_idx.saturating_add(1)).and_then(|next| {
        let row = next.y_virtual.saturating_sub(scroll_offset);
        (row <= render_height.saturating_add(HEADER_CONTENT_GAP) as usize).then_some(row)
    });
    if let Some(next_row) = next {
        let visible = (next_row as u16).saturating_sub(1);
        if visible == 0 {
            return StickyHeaderLayout::default();
        }
        let height = prompt.full_height.min(render_height);
        return StickyHeaderLayout {
            pushed: Some(RenderedPrompt {
                entry_idx: prompt.entry_idx,
                render_height: height,
                clip_top: height.saturating_sub(visible),
            }),
            pinned: None,
        };
    }
    StickyHeaderLayout {
        pushed: None,
        pinned: Some(RenderedPrompt {
            entry_idx: prompt.entry_idx,
            render_height,
            clip_top: 0,
        }),
    }
}

fn calculate_render_height(
    prompt: PromptDescriptor,
    scroll_offset: usize,
    viewport_height: u16,
) -> u16 {
    let scroll_past = scroll_offset
        .saturating_sub(prompt.y_virtual)
        .min(u16::MAX as usize) as u16;
    let height = prompt.full_height.saturating_sub(scroll_past);
    let minimum = prompt.min_height.max(1).min(prompt.full_height.max(1));
    height.max(minimum).min(viewport_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompts() -> Vec<PromptDescriptor> {
        vec![
            PromptDescriptor {
                entry_idx: 2,
                y_virtual: 0,
                full_height: 8,
                min_height: 4,
                sticky: true,
            },
            PromptDescriptor {
                entry_idx: 9,
                y_virtual: 20,
                full_height: 6,
                min_height: 3,
                sticky: true,
            },
        ]
    }

    #[test]
    fn pinned_prompt_collapses_gradually_and_preserves_bottom_line() {
        for scroll in 1..=8 {
            let layout = compute_sticky_layout(scroll, 24, &prompts());
            let bottom = layout
                .scroll_for_content(scroll)
                .saturating_add(layout.content_height(24) as usize)
                .saturating_sub(1);
            assert_eq!(bottom, scroll + 23);
        }
    }

    #[test]
    fn approaching_prompt_pushes_and_clips_the_previous_header() {
        let layout = compute_sticky_layout(16, 24, &prompts());
        let pushed = layout.pushed.expect("pushed prompt");
        assert_eq!(pushed.entry_idx, 2);
        assert!(pushed.needs_scratch_buffer());
        assert_eq!(layout.entry_at_header_row(0), Some(2));
    }

    #[test]
    fn zero_scroll_has_no_sticky_header() {
        assert_eq!(
            compute_sticky_layout(0, 24, &prompts()),
            StickyHeaderLayout::default()
        );
    }
}
