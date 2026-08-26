#[path = "../../vendor/grok/xai-grok-pager/src/input/key.rs"]
pub mod key;
#[path = "../../vendor/grok/xai-grok-pager/src/input/line_editor.rs"]
#[allow(dead_code)]
pub(crate) mod line_editor;
#[path = "../../vendor/grok/xai-grok-pager/src/input/mouse.rs"]
pub mod mouse;
#[path = "../../vendor/grok/xai-grok-pager/src/input/scroll_log.rs"]
pub(crate) mod scroll_log;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use xai_ratatui_textarea::{TextArea, classify_key_event};

use line_editor::LineEditOutcome;

/// Multiline prompt seam backed by Grok's production TextArea. Newlines and
/// trailing spaces are preserved for DSH submit.
#[derive(Debug)]
pub(crate) struct PromptEditor {
    textarea: TextArea,
}

impl PromptEditor {
    pub(crate) fn text(&self) -> &str {
        self.textarea.text()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }
    pub(crate) fn reset(&mut self) {
        self.textarea.set_text("");
        self.textarea.set_cursor(0);
        self.textarea.clear_history();
    }
    pub(crate) fn replace_text(&mut self, text: &str) -> LineEditOutcome {
        let accepted = sanitize_prompt_text(text);
        if accepted == self.text() {
            let cursor_changed = self.textarea.cursor() != self.textarea.text().len();
            self.textarea.set_cursor(self.textarea.text().len());
            return if cursor_changed {
                LineEditOutcome::CursorChanged
            } else {
                LineEditOutcome::HandledNoChange
            };
        }
        self.textarea.set_text(&accepted);
        self.textarea.set_cursor(self.textarea.text().len());
        self.textarea.clear_history();
        LineEditOutcome::TextChanged
    }
    pub(crate) fn insert_newline(&mut self) -> LineEditOutcome {
        self.apply_textarea_input(|textarea| textarea.insert_str_replacing_selection("\n"))
    }
    pub(crate) fn insert_paste(&mut self, text: &str) -> LineEditOutcome {
        let accepted = sanitize_prompt_text(text);
        if accepted.is_empty() {
            return LineEditOutcome::HandledNoChange;
        }
        self.apply_textarea_input(|textarea| textarea.insert_str_replacing_selection(&accepted))
    }
    pub(crate) fn handle_key(&mut self, key: &KeyEvent) -> LineEditOutcome {
        if key.kind == KeyEventKind::Release {
            return LineEditOutcome::Unhandled;
        }
        if classify_key_event(key).is_none() && !matches!(key.code, KeyCode::Up | KeyCode::Down) {
            return LineEditOutcome::Unhandled;
        }
        self.apply_textarea_input(|textarea| textarea.input(*key))
    }
    pub(crate) fn desired_height(&self, width: u16) -> u16 {
        self.textarea.desired_height(width.max(1))
    }
    pub(crate) fn textarea(&self) -> &TextArea {
        &self.textarea
    }
    pub(crate) fn cursor(&self) -> usize {
        self.textarea.cursor()
    }
    /// Grok `PromptWidget::can_send`: trim-empty drafts and trailing `\`
    /// continuations are not sendable, so the shortcuts bar hides Enter:send.
    pub(crate) fn can_send(&self) -> bool {
        let text = self.textarea.text();
        if text.trim().is_empty() {
            return false;
        }
        let cursor = self.textarea.cursor();
        if cursor > 0 && text.as_bytes().get(cursor - 1) == Some(&b'\\') {
            return false;
        }
        true
    }
    pub(crate) fn textarea_mut(&mut self) -> &mut TextArea {
        &mut self.textarea
    }
    fn apply_textarea_input(&mut self, input: impl FnOnce(&mut TextArea)) -> LineEditOutcome {
        let before_text = self.textarea.text().to_owned();
        let before_cursor = self.textarea.cursor();
        input(&mut self.textarea);
        if self.textarea.text() != before_text {
            LineEditOutcome::TextChanged
        } else if self.textarea.cursor() != before_cursor {
            LineEditOutcome::CursorChanged
        } else {
            LineEditOutcome::HandledNoChange
        }
    }
}

impl Default for PromptEditor {
    fn default() -> Self {
        let mut textarea = TextArea::new();
        // DSH submits the exact draft; tabs must not be expanded into spaces.
        textarea.set_tab_width(0);
        Self { textarea }
    }
}

fn normalize_prompt_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn sanitize_prompt_text(text: &str) -> String {
    let mut accepted = String::new();
    for character in normalize_prompt_text(text).chars() {
        if character == '\n' || character == '\t' || !character.is_control() {
            if accepted.len().saturating_add(character.len_utf8()) > 64 * 1024 {
                break;
            }
            accepted.push(character);
        }
    }
    accepted
}

#[cfg(test)]
mod prompt_tests {
    use super::*;

    #[test]
    fn prompt_paste_preserves_newlines_and_trailing_spaces() {
        let mut editor = PromptEditor::default();
        assert_eq!(
            editor.insert_paste("one\r\ntwo  "),
            LineEditOutcome::TextChanged
        );
        assert_eq!(editor.text(), "one\ntwo  ");
    }

    #[test]
    fn prompt_textarea_height_uses_unicode_soft_wrap() {
        let mut editor = PromptEditor::default();
        let _ = editor.insert_paste("中abcdef\nsecond");
        assert!(editor.desired_height(4) >= 4);
    }

    #[test]
    fn prompt_paste_filters_controls_but_keeps_tabs_and_line_breaks() {
        let mut editor = PromptEditor::default();
        let _ = editor.insert_paste("a\0b\t\r\nc");
        assert_eq!(editor.text(), "ab\t\nc");
    }

    #[test]
    fn can_send_matches_grok_empty_and_backslash_rules() {
        let mut editor = PromptEditor::default();
        assert!(!editor.can_send());
        let _ = editor.insert_paste("hello");
        assert!(editor.can_send());
        editor.reset();
        let _ = editor.insert_paste("   ");
        assert!(!editor.can_send());
        editor.reset();
        let _ = editor.insert_paste("line\\");
        assert!(!editor.can_send());
    }

    #[test]
    fn prompt_replace_reports_cursor_only_change_and_starts_fresh_history() {
        let mut editor = PromptEditor::default();
        let _ = editor.insert_paste("draft");
        let _ = editor.handle_key(&KeyEvent::from(KeyCode::Left));
        assert_eq!(editor.replace_text("draft"), LineEditOutcome::CursorChanged);
        let _ = editor.handle_key(&KeyEvent::new(
            KeyCode::Char('z'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        assert_eq!(editor.text(), "draft");
    }
}
