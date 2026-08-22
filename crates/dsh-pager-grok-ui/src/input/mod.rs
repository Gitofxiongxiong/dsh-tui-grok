#[path = "../../vendor/grok/xai-grok-pager/src/input/key.rs"]
pub mod key;
#[path = "../../vendor/grok/xai-grok-pager/src/input/line_editor.rs"]
#[allow(dead_code)]
pub(crate) mod line_editor;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use unicode_width::UnicodeWidthStr;
use xai_ratatui_textarea::{EditBuffer, EditOutcome, classify_key_event};

use line_editor::LineEditOutcome;

/// Multiline prompt seam built on the same Grok EditBuffer as the vendored
/// line editor. Newlines and trailing spaces are preserved for DSH submit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PromptEditor {
    buffer: EditBuffer,
    preferred_column: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromptViewport {
    pub lines: Vec<String>,
    pub cursor_x: usize,
    pub cursor_y: usize,
}

impl PromptEditor {
    pub(crate) fn text(&self) -> &str {
        self.buffer.text()
    }
    pub(crate) fn cursor_byte(&self) -> usize {
        self.buffer.cursor_byte()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.text().is_empty()
    }
    pub(crate) fn reset(&mut self) {
        self.buffer = EditBuffer::new();
        self.preferred_column = None;
    }
    pub(crate) fn insert_newline(&mut self) -> LineEditOutcome {
        self.preferred_column = None;
        Self::from_edit_outcome(self.buffer.insert_str("\n"))
    }
    pub(crate) fn insert_paste(&mut self, text: &str) -> LineEditOutcome {
        let mut accepted = String::new();
        for character in normalize_prompt_text(text).chars() {
            if character == '\n' || character == '\t' || !character.is_control() {
                if accepted.len().saturating_add(character.len_utf8()) > 64 * 1024 {
                    break;
                }
                accepted.push(character);
            }
        }
        if accepted.is_empty() {
            return LineEditOutcome::HandledNoChange;
        }
        self.preferred_column = None;
        Self::from_edit_outcome(self.buffer.insert_str(&accepted))
    }
    pub(crate) fn handle_key(&mut self, key: &KeyEvent) -> LineEditOutcome {
        if key.kind == KeyEventKind::Release {
            return LineEditOutcome::Unhandled;
        }
        match key.code {
            KeyCode::Up => return self.move_vertical(-1),
            KeyCode::Down => return self.move_vertical(1),
            _ => {}
        }
        let Some(command) = classify_key_event(key) else {
            return LineEditOutcome::Unhandled;
        };
        self.preferred_column = None;
        Self::from_edit_outcome(self.buffer.apply(command))
    }
    pub(crate) fn viewport(&self, width: usize, height: usize) -> PromptViewport {
        let width = width.max(1);
        let height = height.max(1);
        let cursor = self.cursor_byte();
        let mut lines = Vec::new();
        let mut cursor_x = 0;
        let mut cursor_y = 0;
        let mut offset = 0usize;
        let logical_lines: Vec<&str> = self.text().split('\n').collect();
        for (line_index, logical) in logical_lines.iter().enumerate() {
            let cursor_in_line = cursor.saturating_sub(offset).min(logical.len());
            let chunks = wrap_prompt_line(logical, width);
            let mut consumed = 0usize;
            for chunk in chunks {
                let end = consumed.saturating_add(chunk.len());
                if cursor >= offset.saturating_add(consumed) && cursor <= offset.saturating_add(end)
                {
                    cursor_y = lines.len();
                    cursor_x = logical[..cursor_in_line.min(end)]
                        .get(consumed..cursor_in_line.min(end))
                        .map_or(0, UnicodeWidthStr::width);
                }
                consumed = end;
                lines.push(chunk);
            }
            if logical.is_empty() {
                cursor_y = lines.len();
                cursor_x = 0;
                lines.push(String::new());
            }
            offset = offset.saturating_add(logical.len());
            if line_index + 1 < logical_lines.len() {
                offset = offset.saturating_add(1);
            }
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        let first = cursor_y.saturating_add(1).saturating_sub(height);
        PromptViewport {
            lines: lines.into_iter().skip(first).take(height).collect(),
            cursor_x,
            cursor_y: cursor_y.saturating_sub(first),
        }
    }
    fn move_vertical(&mut self, delta: isize) -> LineEditOutcome {
        let text = self.text();
        let cursor = self.cursor_byte();
        let start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
        let current_col = text[start..cursor].width();
        let current_line = text[..start].bytes().filter(|byte| *byte == b'\n').count();
        let lines: Vec<&str> = text.split('\n').collect();
        let target_line = if delta < 0 {
            current_line.checked_sub(1)
        } else {
            (current_line + 1 < lines.len()).then_some(current_line + 1)
        };
        let Some(target_line) = target_line else {
            return LineEditOutcome::HandledNoChange;
        };
        let target_col = self.preferred_column.unwrap_or(current_col);
        let target = lines[target_line];
        let mut byte = 0usize;
        let mut width = 0usize;
        for (index, character) in target.char_indices() {
            let char_width = character.to_string().width();
            if width.saturating_add(char_width) > target_col {
                break;
            }
            width = width.saturating_add(char_width);
            byte = index + character.len_utf8();
        }
        let target_start = lines
            .iter()
            .take(target_line)
            .map(|line| line.len() + 1)
            .sum::<usize>();
        self.preferred_column = Some(target_col);
        Self::from_edit_outcome(self.buffer.set_cursor_byte(target_start + byte))
    }
    fn from_edit_outcome(outcome: EditOutcome) -> LineEditOutcome {
        match outcome {
            EditOutcome::Unchanged => LineEditOutcome::HandledNoChange,
            EditOutcome::CursorOnly => LineEditOutcome::CursorChanged,
            EditOutcome::TextOnly(_) | EditOutcome::TextAndCursor(_) => {
                LineEditOutcome::TextChanged
            }
        }
    }
}

fn normalize_prompt_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn wrap_prompt_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for character in line.chars() {
        let char_width = character.to_string().width();
        if !current.is_empty() && current_width.saturating_add(char_width) > width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(character);
        current_width = current_width.saturating_add(char_width);
    }
    lines.push(current);
    lines
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
    fn prompt_viewport_keeps_cursor_visible_after_soft_wrap() {
        let mut editor = PromptEditor::default();
        let _ = editor.insert_paste("中abcdef\nsecond");
        let viewport = editor.viewport(4, 2);
        assert_eq!(viewport.lines.len(), 2);
        assert!(viewport.cursor_y < 2);
        assert!(viewport.cursor_x <= 4);
    }
}
