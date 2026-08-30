//! Display names for DSH agent presets (session-bound compositions).

use dsh_pager_protocol::AgentPresetEntry;

/// Built-in labels for the four shipped roster ids.
pub fn builtin_agent_preset_label(id: &str) -> Option<&'static str> {
    match id {
        "standard" => Some("标准模式"),
        "code" => Some("PTC 模式"),
        "minimal" => Some("极简模式"),
        "cordis" => Some("创造模式"),
        _ => None,
    }
}

/// Prefer the roster's published name, then the shipped Chinese label, then the id.
pub fn agent_preset_label(id: &str, roster: &[AgentPresetEntry]) -> String {
    if let Some(entry) = roster.iter().find(|entry| entry.id == id)
        && let Some(name) = entry.name.as_deref().filter(|name| !name.trim().is_empty())
    {
        return name.to_string();
    }
    builtin_agent_preset_label(id).unwrap_or(id).to_string()
}

/// Label for the current session, including the host-plane fallback.
pub fn current_agent_preset_label(
    agent_preset: Option<&str>,
    roster: &[AgentPresetEntry],
) -> String {
    match agent_preset.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => agent_preset_label(id, roster),
        None => roster
            .iter()
            .find(|entry| entry.is_default)
            .map(|entry| agent_preset_label(&entry.id, roster))
            .unwrap_or_else(|| "标准模式".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::AgentPresetTrust;

    fn entry(id: &str, name: &str, is_default: bool) -> AgentPresetEntry {
        AgentPresetEntry {
            id: id.to_string(),
            trust: AgentPresetTrust::System,
            is_default,
            name: Some(name.to_string()),
            description: None,
            broken: None,
        }
    }

    #[test]
    fn shipped_ids_have_chinese_labels() {
        assert_eq!(builtin_agent_preset_label("standard"), Some("标准模式"));
        assert_eq!(builtin_agent_preset_label("code"), Some("PTC 模式"));
        assert_eq!(builtin_agent_preset_label("minimal"), Some("极简模式"));
        assert_eq!(builtin_agent_preset_label("cordis"), Some("创造模式"));
    }

    #[test]
    fn roster_name_wins_over_builtin() {
        let roster = vec![entry("standard", "My Standard", true)];
        assert_eq!(agent_preset_label("standard", &roster), "My Standard");
        assert_eq!(current_agent_preset_label(None, &roster), "My Standard");
    }

    #[test]
    fn roster_loading_still_shows_the_standard_default() {
        assert_eq!(current_agent_preset_label(None, &[]), "标准模式");
    }
}
