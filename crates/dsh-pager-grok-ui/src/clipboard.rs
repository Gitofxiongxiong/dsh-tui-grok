pub fn system_clipboard_get() -> Option<String> {
    None
}

pub fn clipboard_text_is_pasteable(text: Option<&str>) -> bool {
    text.is_some_and(|value| !value.is_empty())
}

pub fn log_paste_key_empty_host_clipboard(_surface: &str) {}
