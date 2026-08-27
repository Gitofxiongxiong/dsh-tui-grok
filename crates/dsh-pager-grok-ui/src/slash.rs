//! Minimal local slash-command seam.
//!
//! Grok owns command names, descriptions and actions. DSH owns the prompt
//! draft and decides whether a local command is consumed before a prompt
//! effect is compiled.

#[path = "../vendor/grok/xai-grok-pager/src/slash/commands/effort_levels.rs"]
mod effort_levels;
#[path = "../vendor/grok/xai-grok-pager/src/slash/commands/model.rs"]
mod model;
#[path = "../vendor/grok/xai-grok-pager/src/slash/commands/resume.rs"]
mod resume;

use crate::model_state::{ModelId, ModelState};
pub use model::ModelCommand;
use resume::ResumeCommand;

/// A suggestion item for command argument completion.
/// Copied from Grok `slash/command.rs`.
#[derive(Debug, Clone)]
pub struct ArgItem {
    /// Display text shown in the dropdown.
    pub display: String,
    /// Text used for fuzzy matching.
    pub match_text: String,
    /// Text inserted into the prompt on acceptance.
    pub insert_text: String,
    /// Description shown alongside the item.
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ShowSessionPicker,
    ToggleTimestamps,
    SetTimestamps(bool),
    ShowPresetPicker,
    SelectPreset(String),
    PresetStatus,
    NewSession,
    ShowModelPicker,
    SetDefaultModel(ModelId),
    SwitchModel {
        model_id: ModelId,
        effort: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    Action(Action),
    Error(String),
}

pub trait SlashCommand {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn usage(&self) -> &str;
    fn run(&self, args: &str) -> CommandResult;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchResult {
    NotLocal,
    Action(Action),
    InvalidUsage(&'static str),
    Error(String),
}

const RESUME: ResumeCommand = ResumeCommand;
const MODEL: ModelCommand = ModelCommand;
const TIMESTAMPS_USAGE: &str = "/timestamps";
const TIMESTAMPS_DESCRIPTION: &str = "Show or hide transcript timestamps";
const TIMESTAMPS_USAGE_TEXT: &str = "/timestamps [on|off]";
const PRESET_USAGE: &str = "/preset";
const PRESET_DESCRIPTION: &str = "Choose the agent preset for this blank session";
const PRESET_USAGE_TEXT: &str = "/preset [status|<id>]";
const NEW_USAGE: &str = "/new";
const NEW_DESCRIPTION: &str = "Start a blank session and choose its agent preset";
const NEW_USAGE_TEXT: &str = "/new";
const MODEL_USAGE: &str = "/model";
const MODEL_USAGE_TEXT: &str = "/model <name> [effort]";
const MODEL_DESCRIPTION: &str = "Switch the active model";

pub fn dispatch(input: &str) -> DispatchResult {
    dispatch_with_models(input, &ModelState::default())
}

pub fn dispatch_with_models(input: &str, models: &ModelState) -> DispatchResult {
    let trimmed = input.trim();
    let Some(command) = trimmed.strip_prefix('/') else {
        return DispatchResult::NotLocal;
    };
    let split_at = command.find(char::is_whitespace).unwrap_or(command.len());
    let (name, args) = command.split_at(split_at);
    match name {
        "timestamps" => match args.trim() {
            "" => DispatchResult::Action(Action::ToggleTimestamps),
            "on" => DispatchResult::Action(Action::SetTimestamps(true)),
            "off" => DispatchResult::Action(Action::SetTimestamps(false)),
            _ => DispatchResult::InvalidUsage(TIMESTAMPS_USAGE_TEXT),
        },
        "preset" => match args.trim() {
            "" => DispatchResult::Action(Action::ShowPresetPicker),
            "status" => DispatchResult::Action(Action::PresetStatus),
            id if is_preset_id(id) => DispatchResult::Action(Action::SelectPreset(id.to_string())),
            _ => DispatchResult::InvalidUsage(PRESET_USAGE_TEXT),
        },
        "new" => {
            if args.trim().is_empty() {
                DispatchResult::Action(Action::NewSession)
            } else {
                DispatchResult::InvalidUsage(NEW_USAGE_TEXT)
            }
        }
        _ if name == RESUME.name() => {
            if !args.trim().is_empty() {
                return DispatchResult::InvalidUsage("/resume");
            }
            match RESUME.run(args) {
                CommandResult::Action(action) => DispatchResult::Action(action),
                CommandResult::Error(message) => DispatchResult::Error(message),
            }
        }
        _ if name == MODEL.name() || MODEL.aliases().contains(&name) => {
            match MODEL.run_with_models(models, args) {
                CommandResult::Action(action) => DispatchResult::Action(action),
                CommandResult::Error(message) => DispatchResult::Error(message),
            }
        }
        _ => DispatchResult::NotLocal,
    }
}

fn is_preset_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

pub fn merge_builtin_suggestions(items: &mut Vec<String>) {
    let usage = RESUME.usage();
    if !items.iter().any(|item| item == usage) {
        items.insert(0, usage.to_string());
    }
    for extra in [PRESET_USAGE, NEW_USAGE, MODEL_USAGE, TIMESTAMPS_USAGE] {
        if !items.iter().any(|item| item == extra) {
            items.push(extra.to_string());
        }
    }
}

pub fn command_description(command: &str) -> Option<&'static str> {
    if command == RESUME.usage() {
        Some(RESUME.description())
    } else if command == TIMESTAMPS_USAGE || command == TIMESTAMPS_USAGE_TEXT {
        Some(TIMESTAMPS_DESCRIPTION)
    } else if command == PRESET_USAGE || command == PRESET_USAGE_TEXT {
        Some(PRESET_DESCRIPTION)
    } else if command == NEW_USAGE || command == NEW_USAGE_TEXT {
        Some(NEW_DESCRIPTION)
    } else if command == MODEL_USAGE || command == MODEL_USAGE_TEXT || command == "/m" {
        Some(MODEL_DESCRIPTION)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_is_local_and_accepts_no_foreign_selector() {
        assert_eq!(
            dispatch("  /resume  "),
            DispatchResult::Action(Action::ShowSessionPicker)
        );
        assert_eq!(
            dispatch("/resume claude"),
            DispatchResult::InvalidUsage("/resume")
        );
        assert_eq!(
            dispatch("/model"),
            DispatchResult::Error("Usage: /model <name> [effort]".into())
        );
        assert_eq!(dispatch("explain /resume"), DispatchResult::NotLocal);
    }

    #[test]
    fn model_resolves_catalog_name_and_effort() {
        let mut models = ModelState::default();
        models.available.push(crate::model_state::ModelInfo {
            id: crate::model_state::ModelId::new("deepseek-official", "deepseek-chat"),
            name: "DeepSeek Chat".into(),
            description: None,
            reasoning: None,
            default_effort: None,
        });
        models.available.push(crate::model_state::ModelInfo {
            id: crate::model_state::ModelId::new("deepseek-official", "deepseek-reasoner"),
            name: "DeepSeek Reasoner".into(),
            description: None,
            reasoning: Some(vec![crate::model_state::ReasoningEffortOption {
                id: "high".into(),
                name: "high".into(),
                description: None,
            }]),
            default_effort: Some("high".into()),
        });
        assert!(matches!(
            dispatch_with_models("/model DeepSeek Chat", &models),
            DispatchResult::Action(Action::SetDefaultModel(id)) if id.model == "deepseek-chat"
        ));
        assert!(matches!(
            dispatch_with_models("/model DeepSeek Reasoner high", &models),
            DispatchResult::Action(Action::SwitchModel { model_id, effort })
                if model_id.model == "deepseek-reasoner" && effort.as_deref() == Some("high")
        ));
        assert!(matches!(
            dispatch_with_models("/m DeepSeek Chat", &models),
            DispatchResult::Action(Action::SetDefaultModel(_))
        ));
    }

    #[test]
    fn timestamps_command_is_local_and_supports_explicit_state() {
        assert_eq!(
            dispatch("/timestamps"),
            DispatchResult::Action(Action::ToggleTimestamps)
        );
        assert_eq!(
            dispatch("/timestamps on"),
            DispatchResult::Action(Action::SetTimestamps(true))
        );
        assert_eq!(
            dispatch("/timestamps off"),
            DispatchResult::Action(Action::SetTimestamps(false))
        );
        assert_eq!(
            dispatch("/timestamps maybe"),
            DispatchResult::InvalidUsage(TIMESTAMPS_USAGE_TEXT)
        );
        assert_eq!(
            command_description("/timestamps"),
            Some(TIMESTAMPS_DESCRIPTION)
        );
    }

    #[test]
    fn preset_and_new_are_local() {
        assert_eq!(
            dispatch("/preset"),
            DispatchResult::Action(Action::ShowPresetPicker)
        );
        assert_eq!(
            dispatch("/preset status"),
            DispatchResult::Action(Action::PresetStatus)
        );
        assert_eq!(
            dispatch("/preset code"),
            DispatchResult::Action(Action::SelectPreset("code".into()))
        );
        assert_eq!(
            dispatch("/preset Not An Id"),
            DispatchResult::InvalidUsage(PRESET_USAGE_TEXT)
        );
        assert_eq!(dispatch("/new"), DispatchResult::Action(Action::NewSession));
        assert_eq!(
            dispatch("/new extra"),
            DispatchResult::InvalidUsage(NEW_USAGE_TEXT)
        );
    }

    #[test]
    fn resume_suggestion_is_pinned_once() {
        let mut items = vec!["/help".to_string(), "/resume".to_string()];
        merge_builtin_suggestions(&mut items);
        assert_eq!(
            items,
            vec![
                "/help",
                "/resume",
                "/preset",
                "/new",
                "/model",
                "/timestamps"
            ]
        );

        let mut missing = vec!["/help".to_string()];
        merge_builtin_suggestions(&mut missing);
        assert_eq!(
            missing,
            vec![
                "/resume",
                "/help",
                "/preset",
                "/new",
                "/model",
                "/timestamps"
            ]
        );
        assert_eq!(
            command_description("/resume"),
            Some("Resume a previous session")
        );
        assert_eq!(command_description("/preset"), Some(PRESET_DESCRIPTION));
        assert_eq!(command_description("/new"), Some(NEW_DESCRIPTION));
    }
}
