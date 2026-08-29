//! Provider-neutral login modal backed by the Host credential seam.
//!
//! Grok owns the `/login` command name, modal chrome and focus/Esc behavior.
//! DeepSeek Harness owns credential state and persistence. This module keeps
//! only ephemeral masked input and never receives a credential value back.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use dsh_pager_protocol::CredentialInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::theme::Theme;
use crate::views::modal_window::{
    ModalSizing, ModalWindowConfig, ModalWindowOutcome, Shortcut, handle_modal_key,
    handle_modal_mouse, render_modal_window,
};

const MAX_SECRET_CHARS: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMethod {
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginProvider {
    pub id: &'static str,
    pub display_name: &'static str,
    pub method: LoginMethod,
    pub credential_ref: &'static str,
}

pub const DEEPSEEK_LOGIN_PROVIDER: LoginProvider = LoginProvider {
    id: "deepseek",
    display_name: "DeepSeek",
    method: LoginMethod::ApiKey,
    credential_ref: "DEEPSEEK_API_KEY",
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoginActivity {
    Checking,
    Ready,
    Saving,
}

struct SecretInput {
    chars: Box<[char]>,
    len: usize,
    cursor: usize,
}

impl std::fmt::Debug for SecretInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretInput")
            .field("chars", &"[REDACTED]")
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl Default for SecretInput {
    fn default() -> Self {
        Self {
            chars: vec!['\0'; MAX_SECRET_CHARS].into_boxed_slice(),
            len: 0,
            cursor: 0,
        }
    }
}

impl Drop for SecretInput {
    fn drop(&mut self) {
        self.clear();
    }
}

impl SecretInput {
    fn clear(&mut self) {
        self.chars.fill('\0');
        self.len = 0;
        self.cursor = 0;
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn value(&self) -> String {
        self.chars[..self.len].iter().collect()
    }

    fn insert(&mut self, text: &str) -> bool {
        let mut changed = false;
        for character in text.chars() {
            if character.is_control() || character.is_whitespace() {
                continue;
            }
            if self.len >= MAX_SECRET_CHARS {
                break;
            }
            self.chars
                .copy_within(self.cursor..self.len, self.cursor.saturating_add(1));
            self.chars[self.cursor] = character;
            self.len += 1;
            self.cursor += 1;
            changed = true;
        }
        changed
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        match key.code {
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let changed = !self.is_empty();
                self.clear();
                changed
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(&character.to_string())
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.chars
                    .copy_within(self.cursor + 1..self.len, self.cursor);
                self.len -= 1;
                self.chars[self.len] = '\0';
                true
            }
            KeyCode::Delete if self.cursor < self.len => {
                self.chars
                    .copy_within(self.cursor + 1..self.len, self.cursor);
                self.len -= 1;
                self.chars[self.len] = '\0';
                true
            }
            KeyCode::Left => {
                let previous = self.cursor;
                self.cursor = self.cursor.saturating_sub(1);
                self.cursor != previous
            }
            KeyCode::Right => {
                let previous = self.cursor;
                self.cursor = (self.cursor + 1).min(self.len);
                self.cursor != previous
            }
            KeyCode::Home => {
                let changed = self.cursor != 0;
                self.cursor = 0;
                changed
            }
            KeyCode::End => {
                let changed = self.cursor != self.len;
                self.cursor = self.len;
                changed
            }
            _ => false,
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum LoginOutcome {
    Close,
    Submit(String),
    Changed,
    Unchanged,
}

impl std::fmt::Debug for LoginOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Close => f.write_str("Close"),
            Self::Submit(_) => f.write_str("Submit([REDACTED])"),
            Self::Changed => f.write_str("Changed"),
            Self::Unchanged => f.write_str("Unchanged"),
        }
    }
}

#[derive(Debug)]
pub struct LoginModalState {
    provider: LoginProvider,
    activity: LoginActivity,
    info: Option<CredentialInfo>,
    error: Option<String>,
    input: SecretInput,
    window: crate::modal_window_state::ModalWindowState,
}

impl Default for LoginModalState {
    fn default() -> Self {
        Self {
            provider: DEEPSEEK_LOGIN_PROVIDER,
            activity: LoginActivity::Checking,
            info: None,
            error: None,
            input: SecretInput::default(),
            window: crate::modal_window_state::ModalWindowState::new(),
        }
    }
}

impl LoginModalState {
    pub fn open(&mut self, provider: LoginProvider) {
        self.provider = provider;
        self.activity = LoginActivity::Checking;
        self.info = None;
        self.error = None;
        self.input.clear();
        self.window = crate::modal_window_state::ModalWindowState::new();
    }

    pub fn provider(&self) -> LoginProvider {
        self.provider
    }

    pub fn apply_info(&mut self, info: CredentialInfo) {
        self.activity = LoginActivity::Ready;
        self.info = Some(info);
        self.error = None;
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.activity = LoginActivity::Ready;
        self.error = Some(message.into());
    }

    pub fn mark_saving(&mut self) {
        self.activity = LoginActivity::Saving;
        self.error = None;
        self.input.clear();
    }

    pub fn clear_secret(&mut self) {
        self.input.clear();
    }

    pub fn handle_event(&mut self, event: Event) -> LoginOutcome {
        let shortcuts = Self::shortcuts();
        let config = self.config(&shortcuts, false);
        match event {
            Event::Key(key) => match handle_modal_key(&mut self.window, &key, &config) {
                ModalWindowOutcome::CloseRequested => {
                    self.input.clear();
                    LoginOutcome::Close
                }
                ModalWindowOutcome::Unhandled => self.handle_content_key(key),
                _ => LoginOutcome::Changed,
            },
            Event::Paste(text) => {
                if self.can_edit() && self.input.insert(&text) {
                    self.error = None;
                    LoginOutcome::Changed
                } else {
                    LoginOutcome::Unchanged
                }
            }
            Event::Mouse(MouseEvent {
                kind, column, row, ..
            }) => match handle_modal_mouse(&mut self.window, kind, column, row) {
                ModalWindowOutcome::CloseRequested | ModalWindowOutcome::ShortcutActivated(2) => {
                    self.input.clear();
                    LoginOutcome::Close
                }
                ModalWindowOutcome::ShortcutActivated(1) => self.submit_outcome(),
                ModalWindowOutcome::Unhandled => LoginOutcome::Unchanged,
                _ => LoginOutcome::Changed,
            },
            _ => LoginOutcome::Unchanged,
        }
    }

    fn handle_content_key(&mut self, key: KeyEvent) -> LoginOutcome {
        if key.code == KeyCode::Enter {
            return self.submit_outcome();
        }
        if self.can_edit() && self.input.handle_key(key) {
            self.error = None;
            LoginOutcome::Changed
        } else {
            LoginOutcome::Unchanged
        }
    }

    fn submit_outcome(&self) -> LoginOutcome {
        if !self.can_edit() || self.input.is_empty() {
            LoginOutcome::Unchanged
        } else {
            LoginOutcome::Submit(self.input.value())
        }
    }

    fn can_edit(&self) -> bool {
        self.activity == LoginActivity::Ready
            && self.info.as_ref().is_some_and(|info| info.writable)
    }

    fn shortcuts() -> [Shortcut<'static>; 2] {
        [
            Shortcut {
                label: "Enter save",
                clickable: true,
                id: 1,
            },
            Shortcut {
                label: "Esc cancel",
                clickable: true,
                id: 2,
            },
        ]
    }

    fn config<'a>(&self, shortcuts: &'a [Shortcut<'a>], compact: bool) -> ModalWindowConfig<'a> {
        ModalWindowConfig {
            title: "Log in to DeepSeek",
            tabs: None,
            shortcuts,
            sizing: ModalSizing {
                width_pct: 0.60,
                max_width: 80,
                min_width: 44,
                v_margin: if compact { 0 } else { 7 },
                h_pad: if compact { 1 } else { 2 },
                v_pad: if compact { 0 } else { 1 },
                footer_lines: 2,
            },
            fold_info: None,
        }
    }

    pub fn render(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        theme: &Theme,
        compact: bool,
    ) -> Option<(u16, u16)> {
        let shortcuts = Self::shortcuts();
        let config = self.config(&shortcuts, compact);
        let content = render_modal_window(buf, area, &mut self.window, &config, theme)?.content;
        if content.width == 0 || content.height == 0 {
            return None;
        }

        let text_style = Style::default().fg(theme.text_primary).bg(theme.bg_base);
        let muted_style = Style::default().fg(theme.gray).bg(theme.bg_base);
        let error_style = Style::default().fg(theme.accent_error).bg(theme.bg_base);
        let instruction = self.instruction();
        Paragraph::new(instruction)
            .style(text_style)
            .wrap(Wrap { trim: false })
            .render(
                Rect::new(content.x, content.y, content.width, content.height.min(2)),
                buf,
            );

        if self.activity == LoginActivity::Checking || self.info.is_none() {
            if let Some(error) = self.error.as_deref() {
                Paragraph::new(error)
                    .style(error_style)
                    .wrap(Wrap { trim: false })
                    .render(
                        Rect::new(
                            content.x,
                            content.y.saturating_add(2),
                            content.width,
                            content.height.saturating_sub(2),
                        ),
                        buf,
                    );
            }
            return None;
        }

        let info = self.info.as_ref().expect("checked above");
        if !info.writable {
            let detail = format!(
                "Remove {} from the launching environment and restart to replace it.",
                self.provider.credential_ref
            );
            Paragraph::new(detail)
                .style(muted_style)
                .wrap(Wrap { trim: false })
                .render(
                    Rect::new(
                        content.x,
                        content.y.saturating_add(2),
                        content.width,
                        content.height.saturating_sub(2),
                    ),
                    buf,
                );
            return None;
        }

        let field_y = content.y.saturating_add(2);
        if field_y >= content.bottom() {
            return None;
        }
        let field = Rect::new(
            content.x,
            field_y,
            content.width,
            3.min(content.bottom() - field_y),
        );
        let border = if self.activity == LoginActivity::Saving {
            theme.prompt_border
        } else {
            theme.prompt_border_active
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border).bg(theme.bg_base))
            .style(Style::default().bg(theme.bg_base));
        let inner = block.inner(field);
        block.render(field, buf);

        let available = inner.width as usize;
        if available > 0 {
            let start = self
                .input
                .cursor
                .saturating_sub(available.saturating_sub(1));
            let visible = self.input.len.saturating_sub(start).min(available);
            let bullets = "•".repeat(visible);
            buf.set_string(
                inner.x,
                inner.y,
                bullets,
                Style::default()
                    .fg(theme.text_primary)
                    .bg(theme.bg_base)
                    .add_modifier(Modifier::BOLD),
            );
            if self.activity != LoginActivity::Saving {
                let cursor = self.input.cursor.saturating_sub(start).min(available - 1) as u16;
                if inner.height > 0 {
                    return Some((inner.x + cursor, inner.y));
                }
            }
        }

        if let Some(error) = self.error.as_deref() {
            let error_y = field.bottom();
            if error_y < content.bottom() {
                Paragraph::new(error)
                    .style(error_style)
                    .wrap(Wrap { trim: false })
                    .render(
                        Rect::new(
                            content.x,
                            error_y,
                            content.width,
                            content.bottom() - error_y,
                        ),
                        buf,
                    );
            }
        }
        None
    }

    fn instruction(&self) -> String {
        match self.activity {
            LoginActivity::Checking => "Checking the current credential source…".into(),
            LoginActivity::Saving => "Saving the DeepSeek API key…".into(),
            LoginActivity::Ready => match self.info.as_ref() {
                Some(info) if !info.writable => {
                    "DeepSeek API key is supplied by the launch environment.".into()
                }
                Some(info) if info.configured => "Replace the current DeepSeek API key:".into(),
                Some(_) => "Enter your DeepSeek API key:".into(),
                None => "Credential state is unavailable.".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn writable(configured: bool) -> CredentialInfo {
        CredentialInfo {
            configured,
            source: configured.then(|| "file".into()),
            writable: true,
        }
    }

    fn buffer_text(buffer: &Buffer) -> String {
        let area = buffer.area;
        let mut text = String::new();
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn masked_input_never_renders_or_debugs_the_secret() {
        let mut login = LoginModalState::default();
        login.open(DEEPSEEK_LOGIN_PROVIDER);
        login.apply_info(writable(false));
        assert_eq!(
            login.handle_event(Event::Paste("sk-test-secret\n".into())),
            LoginOutcome::Changed
        );
        let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 30));
        let area = buffer.area;
        let cursor = login.render(&mut buffer, area, Theme::current(), false);
        let rendered = buffer_text(&buffer);
        let debug = format!("{login:?}");
        assert!(cursor.is_some());
        assert!(rendered.contains("•••"));
        assert!(!rendered.contains("sk-test-secret"));
        assert!(!debug.contains("sk-test-secret"));
    }

    #[test]
    fn read_only_environment_blocks_submit_with_actionable_copy() {
        let mut login = LoginModalState::default();
        login.open(DEEPSEEK_LOGIN_PROVIDER);
        login.apply_info(CredentialInfo {
            configured: true,
            source: Some("env".into()),
            writable: false,
        });
        assert_eq!(
            login.handle_event(Event::Paste("sk-ignored".into())),
            LoginOutcome::Unchanged
        );
        assert_eq!(
            login.handle_event(key(KeyCode::Enter)),
            LoginOutcome::Unchanged
        );
        let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 30));
        let area = buffer.area;
        assert!(
            login
                .render(&mut buffer, area, Theme::current(), false)
                .is_none()
        );
        let rendered = buffer_text(&buffer);
        assert!(rendered.contains("launch"));
        assert!(rendered.contains("environment"));
        assert!(rendered.contains("DEEPSEEK_API_KEY"));
    }

    #[test]
    fn enter_submits_once_and_escape_clears_the_draft() {
        let mut login = LoginModalState::default();
        login.open(DEEPSEEK_LOGIN_PROVIDER);
        login.apply_info(writable(true));
        let _ = login.handle_event(Event::Paste("sk-one".into()));
        assert_eq!(
            login.handle_event(key(KeyCode::Enter)),
            LoginOutcome::Submit("sk-one".into())
        );
        assert_eq!(
            format!("{:?}", login.handle_event(key(KeyCode::Enter))),
            "Submit([REDACTED])"
        );
        login.mark_saving();
        assert_eq!(
            login.handle_event(key(KeyCode::Enter)),
            LoginOutcome::Unchanged
        );

        login.apply_info(writable(true));
        let _ = login.handle_event(Event::Paste("sk-two".into()));
        assert_eq!(login.handle_event(key(KeyCode::Esc)), LoginOutcome::Close);
        assert!(login.input.is_empty());
    }
}
