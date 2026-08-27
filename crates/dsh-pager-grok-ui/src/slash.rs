//! Minimal local slash-command seam.
//!
//! Grok owns command names, descriptions and actions. DSH owns the prompt
//! draft and decides whether a local command is consumed before a prompt
//! effect is compiled.

#[path = "../vendor/grok/xai-grok-pager/src/slash/commands/resume.rs"]
mod resume;

use resume::ResumeCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ShowSessionPicker,
    ToggleTimestamps,
    SetTimestamps(bool),
    ShowPresetPicker,
    SelectPreset(String),
    PresetStatus,
    NewSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    Action(Action),
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
}

const RESUME: ResumeCommand = ResumeCommand;
const TIMESTAMPS_USAGE: &str = "/timestamps";
const TIMESTAMPS_DESCRIPTION: &str = "Show or hide transcript timestamps";
const TIMESTAMPS_USAGE_TEXT: &str = "/timestamps [on|off]";
const PRESET_USAGE: &str = "/preset";
const PRESET_DESCRIPTION: &str = "Choose the agent preset for this blank session";
const PRESET_USAGE_TEXT: &str = "/preset [status|<id>]";
const NEW_USAGE: &str = "/new";
const NEW_DESCRIPTION: &str = "Start a blank session and choose its agent preset";
const NEW_USAGE_TEXT: &str = "/new";

pub fn dispatch(input: &str) -> DispatchResult {
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
    for extra in [PRESET_USAGE, NEW_USAGE, TIMESTAMPS_USAGE] {
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
        assert_eq!(dispatch("/model"), DispatchResult::NotLocal);
        assert_eq!(dispatch("explain /resume"), DispatchResult::NotLocal);
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
            vec!["/help", "/resume", "/preset", "/new", "/timestamps"]
        );

        let mut missing = vec!["/help".to_string()];
        merge_builtin_suggestions(&mut missing);
        assert_eq!(
            missing,
            vec!["/resume", "/help", "/preset", "/new", "/timestamps"]
        );
        assert_eq!(
            command_description("/resume"),
            Some("Resume a previous session")
        );
        assert_eq!(command_description("/preset"), Some(PRESET_DESCRIPTION));
        assert_eq!(command_description("/new"), Some(NEW_DESCRIPTION));
    }
}
