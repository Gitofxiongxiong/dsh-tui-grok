//! Read/Edit tool-path display helpers extracted from Grok Build.
//!
//! A1 slice of `xai-grok-pager-render/src/render/tool_paths.rs` at
//! `19d42e35c07a9c9244f03f6df0c4c353f970d4f9`. This keeps `shorten_path`
//! (fish-style component shortening) and its tests. Tilde expansion, cwd
//! resolution, `xai_grok_paths` and OSC8 targets stay excluded because DSH
//! presenters already supply display paths.

use unicode_width::UnicodeWidthStr;

use super::line_utils::truncate_str;

/// Shorten a file path to fit within `budget` display columns using fish-style
/// component shortening.
pub fn shorten_path(path: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    if path.width() <= budget {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 1 {
        return truncate_str(path, budget);
    }

    let mut shortened: Vec<String> = parts.iter().map(|part| part.to_string()).collect();
    let last_idx = shortened.len() - 1;
    for i in 0..last_idx {
        if shortened.iter().map(String::len).sum::<usize>() + shortened.len() - 1 <= budget {
            break;
        }
        if let Some(first) = parts[i].chars().next() {
            shortened[i] = first.to_string();
        }
    }

    let joined = shortened.join("/");
    if joined.width() <= budget {
        return joined;
    }

    let mut tail_start = 0;
    for (i, _) in path.char_indices() {
        if i == 0 {
            continue;
        }
        if path.as_bytes().get(i.wrapping_sub(1)) == Some(&b'/') {
            let candidate = format!("\u{2026}{}", &path[i - 1..]);
            if candidate.width() <= budget {
                tail_start = i - 1;
                break;
            }
        }
    }
    if tail_start > 0 {
        let result = format!("\u{2026}{}", &path[tail_start..]);
        if result.width() <= budget {
            return result;
        }
    }
    truncate_str(path, budget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_path_fits() {
        assert_eq!(shorten_path("src/main.rs", 20), "src/main.rs");
    }

    #[test]
    fn shorten_path_fish_style() {
        let result = shorten_path("crates/codegen/xai-grok-pager/src/views/foo.rs", 25);
        assert!(result.width() <= 25, "got: {result}");
        assert!(result.ends_with("foo.rs"), "got: {result}");
    }

    #[test]
    fn shorten_path_front_truncate() {
        let result = shorten_path(
            "crates/codegen/xai-grok-pager/src/views/very_long_filename.rs",
            20,
        );
        assert!(result.width() <= 20, "got: {result}");
    }

    #[test]
    fn shorten_path_no_separator() {
        assert_eq!(shorten_path("verylongfilename.rs", 10), "verylongf\u{2026}");
    }

    #[test]
    fn shorten_path_zero_budget() {
        assert_eq!(shorten_path("src/main.rs", 0), "");
    }
}
