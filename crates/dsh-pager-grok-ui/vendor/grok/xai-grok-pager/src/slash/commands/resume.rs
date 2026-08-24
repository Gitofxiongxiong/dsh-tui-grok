//! `/resume` -- open session picker overlay to resume a previous session.

use crate::slash::{Action, CommandResult, SlashCommand};

pub struct ResumeCommand;

impl SlashCommand for ResumeCommand {
    fn name(&self) -> &str {
        "resume"
    }

    fn description(&self) -> &str {
        "Resume a previous session"
    }

    fn usage(&self) -> &str {
        "/resume"
    }

    fn run(&self, _args: &str) -> CommandResult {
        CommandResult::Action(Action::ShowSessionPicker)
    }
}
