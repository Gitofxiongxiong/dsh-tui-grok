//! Minimal local slash-command seam.
//!
//! Grok owns command names, descriptions and actions. DSH owns the prompt
//! draft and decides whether a local command is consumed before a prompt
//! effect is compiled.

#[path = "../vendor/grok/xai-grok-pager/src/slash/commands/resume.rs"]
mod resume;

use resume::ResumeCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ShowSessionPicker,
    ToggleTimestamps,
    SetTimestamps(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandResult {
    Action(Action),
}

pub trait SlashCommand {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn usage(&self) -> &str;
    fn run(&self, args: &str) -> CommandResult;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchResult {
    NotLocal,
    Action(Action),
    InvalidUsage(&'static str),
}

const RESUME: ResumeCommand = ResumeCommand;
const TIMESTAMPS_USAGE: &str = "/timestamps";
const TIMESTAMPS_DESCRIPTION: &str = "Show or hide transcript timestamps";
const TIMESTAMPS_USAGE_TEXT: &str = "/timestamps [on|off]";

pub fn dispatch(input: &str) -> DispatchResult {
    let trimmed = input.trim();
    let Some(command) = trimmed.strip_prefix('/') else {
        return DispatchResult::NotLocal;
    };
    let split_at = command.find(char::is_whitespace).unwrap_or(command.len());
    let (name, args) = command.split_at(split_at);
    if name == "timestamps" {
        return match args.trim() {
            "" => DispatchResult::Action(Action::ToggleTimestamps),
            "on" => DispatchResult::Action(Action::SetTimestamps(true)),
            "off" => DispatchResult::Action(Action::SetTimestamps(false)),
            _ => DispatchResult::InvalidUsage(TIMESTAMPS_USAGE_TEXT),
        };
    }
    if name != RESUME.name() {
        return DispatchResult::NotLocal;
    }
    if !args.trim().is_empty() {
        return DispatchResult::InvalidUsage("/resume");
    }
    match RESUME.run(args) {
        CommandResult::Action(action) => DispatchResult::Action(action),
    }
}

pub fn merge_builtin_suggestions(items: &mut Vec<String>) {
    let usage = RESUME.usage();
    if !items.iter().any(|item| item == usage) {
        items.insert(0, usage.to_string());
    }
    if !items.iter().any(|item| item == TIMESTAMPS_USAGE) {
        items.push(TIMESTAMPS_USAGE.to_string());
    }
}

pub fn command_description(command: &str) -> Option<&'static str> {
    if command == RESUME.usage() {
        Some(RESUME.description())
    } else if command == TIMESTAMPS_USAGE || command == TIMESTAMPS_USAGE_TEXT {
        Some(TIMESTAMPS_DESCRIPTION)
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
    fn resume_suggestion_is_pinned_once() {
        let mut items = vec!["/help".to_string(), "/resume".to_string()];
        merge_builtin_suggestions(&mut items);
        assert_eq!(items, vec!["/help", "/resume", "/timestamps"]);

        let mut missing = vec!["/help".to_string()];
        merge_builtin_suggestions(&mut missing);
        assert_eq!(missing, vec!["/resume", "/help", "/timestamps"]);
        assert_eq!(
            command_description("/resume"),
            Some("Resume a previous session")
        );
    }
}
