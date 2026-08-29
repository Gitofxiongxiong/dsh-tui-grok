//! Grok Build's slash dropdown state machine with a DSH command roster.
//!
//! Rendering, fuzzy matching, command/argument phases, selection carry and
//! completion insertion follow Grok's `slash` module. DSH supplies only the
//! commands it owns plus the per-session model catalog/effects.

use std::ops::Range;

#[path = "../vendor/grok/xai-grok-pager/src/slash/commands/effort_levels.rs"]
mod effort_levels;
#[path = "../vendor/grok/xai-grok-pager/src/slash/matcher.rs"]
mod matcher;
#[path = "../vendor/grok/xai-grok-pager/src/slash/commands/model.rs"]
mod model;
#[path = "../vendor/grok/xai-grok-pager/src/slash/commands/resume.rs"]
mod resume;

use matcher::FuzzyMatcher;
use resume::ResumeCommand;

use crate::model_state::{ModelId, ModelState};
use dsh_pager_protocol::CommandDescriptor;
pub use model::ModelCommand;

/// Maximum number of visible rows in the dropdown (scroll beyond this).
pub const MAX_VISIBLE_SUGGESTIONS: usize = 6;

/// Origin of a slash command. Copied from Grok `slash/command.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandProvenance {
    Builtin,
    Shell,
    Skill { source: String },
}

impl CommandProvenance {
    pub fn badge(&self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::Builtin | Self::Shell => std::borrow::Cow::Borrowed("built-in"),
            Self::Skill { source } => std::borrow::Cow::Owned(format!("skill · {source}")),
        }
    }
}

/// A suggestion item for command argument completion.
/// Copied from Grok `slash/command.rs`.
#[derive(Debug, Clone)]
pub struct ArgItem {
    pub display: String,
    pub match_text: String,
    pub insert_text: String,
    pub description: String,
}

/// A row consumed verbatim by Grok's `views/slash_dropdown.rs`.
#[derive(Debug, Clone)]
pub struct SuggestionRow {
    pub display: String,
    pub description: String,
    pub insert_text: String,
    pub indices: Vec<u32>,
    pub tag: Option<String>,
    pub provenance: Option<CommandProvenance>,
}

impl SuggestionRow {
    fn from_arg(item: &ArgItem) -> Self {
        Self {
            display: item.display.clone(),
            description: item.description.clone(),
            insert_text: item.insert_text.clone(),
            indices: Vec::new(),
            tag: None,
            provenance: None,
        }
    }

    pub(crate) fn command_name(&self) -> &str {
        self.display.strip_prefix('/').unwrap_or(&self.display)
    }
}

/// Immutable slash state rendered by Grok's dropdown.
#[derive(Debug, Clone, Default)]
pub struct SlashSnapshot {
    pub active: bool,
    pub open: bool,
    pub query: String,
    pub matches: Vec<SuggestionRow>,
    pub selected: usize,
    pub command_range: Option<Range<usize>>,
    pub args_range: Option<Range<usize>>,
    pub cursor_in_command: bool,
    pub args_placeholder: Option<String>,
    pub args_query_is_empty: bool,
    pub is_skill: bool,
    pub command_recognized: bool,
    pub inline_ghost: Option<InlineGhost>,
    pub recognized_tokens: Vec<Range<usize>>,
}

#[derive(Debug, Clone)]
pub struct InlineGhost {
    pub text: String,
    pub token_range: Range<usize>,
    pub full_name: String,
}

impl SlashSnapshot {
    pub fn selection(&self) -> Option<&SuggestionRow> {
        if self.matches.is_empty() {
            None
        } else {
            self.matches
                .get(self.selected.min(self.matches.len().saturating_sub(1)))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ShowSessionPicker,
    ToggleTimestamps,
    SetTimestamps(bool),
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
const NEW_USAGE: &str = "/new";
const NEW_DESCRIPTION: &str = "Start a blank session and choose its agent preset";
const NEW_USAGE_TEXT: &str = "/new";
const MODEL_USAGE: &str = "/model";
const MODEL_USAGE_TEXT: &str = "/model <name> [effort]";
const MODEL_DESCRIPTION: &str = "Switch the active model";

#[derive(Clone, Copy)]
struct CommandSpec {
    name: &'static str,
    aliases: &'static [&'static str],
    description: &'static str,
    takes_args: bool,
    args_required: bool,
    placeholder: Option<&'static str>,
}

const COMMANDS: [CommandSpec; 4] = [
    CommandSpec {
        name: "resume",
        aliases: &[],
        description: "Resume a previous session",
        takes_args: false,
        args_required: false,
        placeholder: None,
    },
    CommandSpec {
        name: "new",
        aliases: &[],
        description: NEW_DESCRIPTION,
        takes_args: false,
        args_required: false,
        placeholder: None,
    },
    CommandSpec {
        name: "model",
        aliases: &["m"],
        description: MODEL_DESCRIPTION,
        takes_args: true,
        args_required: true,
        placeholder: Some("<model> [effort]"),
    },
    CommandSpec {
        name: "timestamps",
        aliases: &[],
        description: TIMESTAMPS_DESCRIPTION,
        takes_args: true,
        args_required: false,
        placeholder: Some("[on|off]"),
    },
];

fn command_spec(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS
        .iter()
        .find(|spec| spec.name == name || spec.aliases.contains(&name))
}

/// Grok-derived command/argument completion controller.
#[derive(Debug, Default)]
pub struct SlashController {
    matcher: FuzzyMatcher,
    snapshot: SlashSnapshot,
    dismissed_text: Option<String>,
    host_commands: Vec<CommandDescriptor>,
    permission_presets: Vec<String>,
}

impl SlashController {
    pub fn snapshot(&self) -> SlashSnapshot {
        self.snapshot.clone()
    }

    pub fn is_open(&self) -> bool {
        self.snapshot.open
    }

    /// Recompute the snapshot from prompt text + cursor position.
    /// This is the leading-slash tranche copied from Grok's controller; DSH
    /// injects its small command roster and any host-advertised commands.
    pub fn refresh(
        &mut self,
        text: &str,
        cursor: usize,
        models: &ModelState,
        host_commands: &[CommandDescriptor],
        permission_presets: &[String],
    ) {
        self.host_commands = host_commands.to_vec();
        self.permission_presets = permission_presets.to_vec();
        if self.dismissed_text.as_deref() == Some(text) {
            self.snapshot.open = false;
            return;
        }
        self.dismissed_text = None;
        let previous = self.snapshot.clone();
        let Some(input) = analyze_input(text, cursor) else {
            self.snapshot = SlashSnapshot::default();
            return;
        };

        let args_text_empty = input
            .args_range
            .as_ref()
            .is_some_and(|range| text[range.clone()].trim().is_empty());
        let mut snapshot = SlashSnapshot {
            active: true,
            open: false,
            query: input.query.clone(),
            matches: Vec::new(),
            selected: 0,
            command_range: Some(input.command_range.clone()),
            args_range: input.args_range.clone(),
            cursor_in_command: input.cursor_in_command,
            args_placeholder: None,
            args_query_is_empty: args_text_empty,
            is_skill: false,
            command_recognized: false,
            inline_ghost: None,
            recognized_tokens: Vec::new(),
        };

        if input.cursor_in_command {
            let matches = self.command_suggestions(&input.query);
            snapshot.selected = carry_selection(&previous, &matches, true, &input);
            snapshot.open = !matches.is_empty();
            snapshot.matches = matches;
        } else if input.args_range.is_some() {
            let matches = self.arg_suggestions_for_input(text, &input, models);
            snapshot.selected = carry_selection(&previous, &matches, false, &input);
            snapshot.open = !matches.is_empty();
            snapshot.matches = matches;
        }

        if let Some(invocation) = parse_invocation(text) {
            if let Some(spec) = command_spec(invocation.token) {
                snapshot.command_recognized = true;
                if args_text_empty {
                    snapshot.args_placeholder = spec.placeholder.map(str::to_string);
                }
            } else if let Some(command) = self.host_command(invocation.token) {
                snapshot.command_recognized = true;
                if args_text_empty {
                    snapshot.args_placeholder =
                        command.input.as_ref().map(|input| input.hint.clone());
                }
            }
        }
        self.snapshot = snapshot;
    }

    pub fn move_selection(&mut self, delta: isize) {
        let len = self.snapshot.matches.len();
        if len == 0 {
            return;
        }
        let current = self.snapshot.selected.min(len - 1) as isize;
        self.snapshot.selected = (current + delta).rem_euclid(len as isize) as usize;
    }

    pub fn scroll_selection(&mut self, delta: isize) {
        let len = self.snapshot.matches.len();
        if len == 0 {
            return;
        }
        let current = self.snapshot.selected.min(len - 1) as isize;
        self.snapshot.selected = (current + delta).clamp(0, len as isize - 1) as usize;
    }

    pub fn dismiss(&mut self, text: &str) {
        self.dismissed_text = Some(text.to_string());
        self.close();
    }

    pub fn close(&mut self) {
        self.snapshot.open = false;
        self.snapshot.matches.clear();
        self.snapshot.args_range = None;
        self.snapshot.args_placeholder = None;
        self.snapshot.args_query_is_empty = false;
    }

    pub fn reset(&mut self) {
        self.snapshot = SlashSnapshot::default();
        self.dismissed_text = None;
    }

    /// Same replacement contract as Grok `PromptWidget::accept_slash_completion`.
    pub fn accepted_text(&self, text: &str) -> Option<String> {
        let row = self.snapshot.selection()?;
        let range = if self.snapshot.cursor_in_command {
            self.snapshot.command_range.clone()?
        } else {
            self.snapshot.args_range.clone()?
        };
        if range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            return None;
        }
        let mut end = range.end;
        if self.snapshot.cursor_in_command
            && row.insert_text.ends_with(' ')
            && text[range.end..].starts_with(' ')
        {
            end += 1;
        }
        let mut accepted = text.to_string();
        accepted.replace_range(range.start..end, &row.insert_text);
        Some(accepted)
    }

    pub fn selected_chains(&self) -> bool {
        self.snapshot
            .selection()
            .is_some_and(|row| row.insert_text.ends_with(' '))
    }

    /// Grok's exact-command Enter rule: complete typed commands execute
    /// directly instead of first accepting the visually identical row.
    pub fn typed_complete_selected(&self, text: &str) -> bool {
        if !self.snapshot.cursor_in_command {
            return false;
        }
        let Some(invocation) = parse_invocation(text) else {
            return false;
        };
        let canonical = if let Some(spec) = command_spec(invocation.token) {
            if spec.takes_args && spec.args_required && invocation.args.trim().is_empty() {
                return false;
            }
            spec.name
        } else if let Some(command) = self.host_command(invocation.token) {
            command.name.as_str()
        } else {
            return false;
        };
        match self.snapshot.selection() {
            None => true,
            Some(row) => command_spec(row.command_name())
                .map(|selected| selected.name)
                .or_else(|| {
                    self.host_command(row.command_name())
                        .map(|command| command.name.as_str())
                })
                .is_some_and(|selected| selected == canonical),
        }
    }

    fn command_suggestions(&mut self, query: &str) -> Vec<SuggestionRow> {
        let mut candidates = COMMANDS
            .iter()
            .map(|spec| SuggestionRow {
                display: format!("/{}", spec.name),
                description: spec.description.to_string(),
                insert_text: if spec.takes_args {
                    format!("/{} ", spec.name)
                } else {
                    format!("/{}", spec.name)
                },
                indices: Vec::new(),
                tag: None,
                provenance: None,
            })
            .collect::<Vec<_>>();
        for command in &self.host_commands {
            let name = command.name.trim();
            let display = format!("/{name}");
            if !visible_host_command_name(name)
                || candidates.iter().any(|row| row.display == display)
            {
                continue;
            }
            candidates.push(SuggestionRow {
                display: display.clone(),
                description: command.description.clone(),
                insert_text: if command.input.is_some() {
                    format!("{display} ")
                } else {
                    display
                },
                indices: Vec::new(),
                tag: None,
                provenance: None,
            });
        }

        let trimmed = query.trim();
        if trimmed.is_empty() {
            return candidates;
        }
        if trimmed.contains('/') {
            return Vec::new();
        }
        let hits = self
            .matcher
            .rank(&candidates, trimmed, candidates.len(), |row| {
                row.display.strip_prefix('/').unwrap_or(&row.display)
            });
        hits.into_iter()
            .map(|(index, _)| {
                let mut row = candidates[index].clone();
                row.indices = self.matcher.indices(row.display.as_str());
                row
            })
            .collect()
    }

    fn arg_suggestions_for_input(
        &mut self,
        text: &str,
        input: &SlashInput,
        models: &ModelState,
    ) -> Vec<SuggestionRow> {
        let Some(invocation) = parse_invocation(text) else {
            return Vec::new();
        };
        let items = match command_spec(invocation.token).map(|spec| spec.name) {
            Some("model") => MODEL.suggest_args(models, &input.args_query),
            Some("timestamps") => Some(vec![
                ArgItem {
                    display: "on".into(),
                    match_text: "on".into(),
                    insert_text: "on".into(),
                    description: "Show transcript timestamps".into(),
                },
                ArgItem {
                    display: "off".into(),
                    match_text: "off".into(),
                    insert_text: "off".into(),
                    description: "Hide transcript timestamps".into(),
                },
            ]),
            _ if invocation.token == "permission" && self.host_command("permission").is_some() => {
                Some(
                    self.permission_presets
                        .iter()
                        .map(|preset| ArgItem {
                            display: preset.clone(),
                            match_text: preset.clone(),
                            insert_text: preset.clone(),
                            description: "DSH permission preset".into(),
                        })
                        .collect(),
                )
            }
            _ => None,
        }
        .unwrap_or_default();
        if items.is_empty() {
            return Vec::new();
        }
        let trimmed = input.args_query.trim();
        if trimmed.is_empty() {
            return items.iter().map(SuggestionRow::from_arg).collect();
        }
        let hits = self.matcher.rank(&items, trimmed, items.len(), |item| {
            item.match_text.as_str()
        });
        hits.into_iter()
            .map(|(index, _)| {
                let mut row = SuggestionRow::from_arg(&items[index]);
                row.indices = self.argument_highlight_indices(trimmed, &row.display);
                row
            })
            .collect()
    }

    fn argument_highlight_indices(&mut self, query: &str, display: &str) -> Vec<u32> {
        let token = query.split_whitespace().next_back().unwrap_or("");
        let fragment = token.rsplit(['/', '\\']).next().unwrap_or(token);
        self.matcher
            .indices_for(fragment, display)
            .or_else(|| {
                fragment
                    .rsplit_once('.')
                    .and_then(|(_, suffix)| self.matcher.indices_for(suffix, display))
            })
            .unwrap_or_default()
    }

    fn host_command(&self, name: &str) -> Option<&CommandDescriptor> {
        self.host_commands.iter().find(|command| {
            command.name == name && command_spec(name).is_none() && visible_host_command_name(name)
        })
    }
}

fn valid_command_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

fn visible_host_command_name(name: &str) -> bool {
    valid_command_name(name) && name != "preset"
}

struct SlashInput {
    command_range: Range<usize>,
    query: String,
    cursor_in_command: bool,
    args_range: Option<Range<usize>>,
    args_query: String,
}

/// Copied from Grok `slash::analyze_input`.
fn analyze_input(text: &str, cursor: usize) -> Option<SlashInput> {
    if text.is_empty() || !text.starts_with('/') {
        return None;
    }
    let cursor = cursor.min(text.len());
    if text[1..].chars().all(char::is_whitespace) {
        return Some(SlashInput {
            command_range: 0..1,
            query: String::new(),
            cursor_in_command: true,
            args_range: None,
            args_query: String::new(),
        });
    }
    let mut command_end = text.len();
    for (index, character) in text.char_indices() {
        if index > 0 && character.is_whitespace() {
            command_end = index;
            break;
        }
    }
    let query_end = cursor.clamp(1, command_end);
    let query = if query_end <= 1 {
        String::new()
    } else {
        text[1..query_end].to_string()
    };
    let cursor_in_command = cursor <= command_end;
    let mut args_range = None;
    let mut args_query = String::new();
    if !cursor_in_command {
        let mut start = command_end;
        while start < text.len() {
            let Some(character) = text[start..].chars().next() else {
                break;
            };
            if character.is_whitespace() {
                start += character.len_utf8();
            } else {
                break;
            }
        }
        let end = text.len();
        let query_end = cursor.clamp(start, end);
        if query_end > start {
            args_query = text[start..query_end].to_string();
        }
        args_range = Some(start..end);
    }
    Some(SlashInput {
        command_range: 0..command_end,
        query,
        cursor_in_command,
        args_range,
        args_query,
    })
}

fn carry_selection(
    previous: &SlashSnapshot,
    matches: &[SuggestionRow],
    cursor_in_command: bool,
    input: &SlashInput,
) -> usize {
    if matches.is_empty() {
        return 0;
    }
    let same_context = if cursor_in_command {
        previous.cursor_in_command && previous.query == input.query
    } else {
        !previous.cursor_in_command && previous.args_range == input.args_range
    };
    if !same_context || previous.matches.is_empty() {
        return 0;
    }
    let previous_index = previous
        .selected
        .min(previous.matches.len().saturating_sub(1));
    if let Some(previous_row) = previous.matches.get(previous_index)
        && let Some(index) = matches
            .iter()
            .position(|row| row.insert_text == previous_row.insert_text)
    {
        return index;
    }
    previous.selected.min(matches.len().saturating_sub(1))
}

pub struct SlashInvocation<'a> {
    pub token: &'a str,
    pub args: &'a str,
}

/// Copied from Grok `slash::parse_invocation`.
pub fn parse_invocation(line: &str) -> Option<SlashInvocation<'_>> {
    let remainder = line.strip_prefix('/')?;
    if remainder.is_empty() {
        return None;
    }
    let mut command_end = remainder.len();
    for (index, character) in remainder.char_indices() {
        if character.is_whitespace() {
            command_end = index;
            break;
        }
    }
    let token = remainder[..command_end].trim();
    if token.is_empty() {
        return None;
    }
    let args = if command_end < remainder.len() {
        remainder[command_end..].trim_start()
    } else {
        ""
    };
    Some(SlashInvocation { token, args })
}

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
        "preset" => DispatchResult::Error(
            "Agent preset is chosen before the first turn; press Shift+Tab or click the preset label"
                .into(),
        ),
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

pub fn command_description(command: &str) -> Option<&'static str> {
    if command == RESUME.usage() {
        Some(RESUME.description())
    } else if command == TIMESTAMPS_USAGE || command == TIMESTAMPS_USAGE_TEXT {
        Some(TIMESTAMPS_DESCRIPTION)
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
    use crate::model_state::{ModelInfo, ReasoningEffortOption};

    fn models() -> ModelState {
        let mut models = ModelState::default();
        models.available.push(ModelInfo {
            id: ModelId::new("deepseek-official", "deepseek-v4-flash"),
            name: "DeepSeek-V4-Flash".into(),
            description: Some("Fast model".into()),
            reasoning: Some(vec![ReasoningEffortOption {
                id: "high".into(),
                name: "high".into(),
                description: Some("Heavy reasoning".into()),
            }]),
            default_effort: Some("high".into()),
        });
        models.available.push(ModelInfo {
            id: ModelId::new("deepseek-official", "deepseek-v4-pro"),
            name: "DeepSeek-V4-Pro".into(),
            description: Some("Capable model".into()),
            reasoning: None,
            default_effort: None,
        });
        models
    }

    #[test]
    fn bare_slash_uses_grok_rows_and_descriptions() {
        let mut controller = SlashController::default();
        controller.refresh("/", 1, &models(), &[], &[]);
        let snapshot = controller.snapshot();
        assert!(snapshot.open);
        assert_eq!(snapshot.matches.len(), 4);
        assert_eq!(snapshot.matches[0].display, "/resume");
        assert!(!snapshot.matches[0].description.is_empty());
        assert!(snapshot.matches.iter().any(|row| row.display == "/model"));
    }

    #[test]
    fn model_accept_chains_into_real_catalog_then_effort() {
        let models = models();
        let mut controller = SlashController::default();
        controller.refresh("/mod", 4, &models, &[], &[]);
        assert_eq!(controller.snapshot().selection().unwrap().display, "/model");
        let command = controller.accepted_text("/mod").unwrap();
        assert_eq!(command, "/model ");

        controller.refresh(&command, command.len(), &models, &[], &[]);
        let snapshot = controller.snapshot();
        assert_eq!(snapshot.matches.len(), 2);
        assert_eq!(snapshot.matches[0].display, "DeepSeek-V4-Flash");
        let model = controller.accepted_text(&command).unwrap();
        assert_eq!(model, "/model DeepSeek-V4-Flash ");

        controller.refresh(&model, model.len(), &models, &[], &[]);
        let snapshot = controller.snapshot();
        assert_eq!(snapshot.matches.len(), 1);
        assert_eq!(snapshot.matches[0].display, "high");
        assert_eq!(
            controller.accepted_text(&model).unwrap(),
            "/model DeepSeek-V4-Flash high"
        );
    }

    #[test]
    fn exact_required_model_command_does_not_submit_empty_args() {
        let mut controller = SlashController::default();
        controller.refresh("/model", 6, &models(), &[], &[]);
        assert!(!controller.typed_complete_selected("/model"));
        assert!(controller.selected_chains());
    }

    fn official_commands() -> Vec<CommandDescriptor> {
        vec![
            CommandDescriptor {
                name: "permission".into(),
                description: "Set the permission preset".into(),
                input: Some(dsh_pager_protocol::CommandInputDescriptor {
                    hint: "<preset>".into(),
                    images: None,
                }),
            },
            CommandDescriptor {
                name: "plan".into(),
                description: "Enter or leave plan mode".into(),
                input: Some(dsh_pager_protocol::CommandInputDescriptor {
                    hint: "[off|message]".into(),
                    images: Some(true),
                }),
            },
            CommandDescriptor {
                name: "model".into(),
                description: "Host collision must not replace local model".into(),
                input: None,
            },
            CommandDescriptor {
                name: "preset".into(),
                description: "Host collision must stay hidden".into(),
                input: None,
            },
        ]
    }

    #[test]
    fn official_catalog_merges_with_local_commands_and_keeps_metadata() {
        let mut controller = SlashController::default();
        controller.refresh("/p", 2, &models(), &official_commands(), &[]);
        let snapshot = controller.snapshot();
        assert!(snapshot.matches.iter().any(|row| {
            row.display == "/permission" && row.description == "Set the permission preset"
        }));
        assert!(snapshot.matches.iter().any(|row| {
            row.display == "/plan" && row.description == "Enter or leave plan mode"
        }));

        controller.refresh("/", 1, &models(), &official_commands(), &[]);
        let snapshot = controller.snapshot();
        assert_eq!(
            snapshot
                .matches
                .iter()
                .filter(|row| row.display == "/model")
                .count(),
            1
        );
        assert_eq!(
            snapshot
                .matches
                .iter()
                .find(|row| row.display == "/model")
                .unwrap()
                .description,
            MODEL_DESCRIPTION
        );
        assert!(!snapshot.matches.iter().any(|row| row.display == "/preset"));
    }

    #[test]
    fn official_command_is_executable_and_uses_its_input_hint() {
        let mut controller = SlashController::default();
        controller.refresh("/plan", 5, &models(), &official_commands(), &[]);
        assert!(controller.typed_complete_selected("/plan"));

        controller.refresh("/plan ", 6, &models(), &official_commands(), &[]);
        let snapshot = controller.snapshot();
        assert!(snapshot.command_recognized);
        assert_eq!(snapshot.args_placeholder.as_deref(), Some("[off|message]"));
    }

    #[test]
    fn permission_arguments_come_from_authoritative_projection() {
        let mut controller = SlashController::default();
        let presets = vec!["workspace-write".into(), "danger-full-access".into()];
        controller.refresh(
            "/permission ",
            12,
            &models(),
            &official_commands(),
            &presets,
        );
        let snapshot = controller.snapshot();
        assert_eq!(snapshot.matches.len(), 2);
        assert_eq!(snapshot.matches[0].display, "workspace-write");
        assert_eq!(snapshot.matches[1].display, "danger-full-access");
    }

    #[test]
    fn dispatch_model_resolves_catalog_name_and_effort() {
        let models = models();
        assert!(matches!(
            dispatch_with_models("/model DeepSeek-V4-Pro", &models),
            DispatchResult::Action(Action::SetDefaultModel(id)) if id.model == "deepseek-v4-pro"
        ));
        assert!(matches!(
            dispatch_with_models("/model DeepSeek-V4-Flash high", &models),
            DispatchResult::Action(Action::SwitchModel { model_id, effort })
                if model_id.model == "deepseek-v4-flash" && effort.as_deref() == Some("high")
        ));
    }

    #[test]
    fn timestamps_stays_local_and_preset_is_not_a_slash_command() {
        assert_eq!(
            dispatch("/timestamps off"),
            DispatchResult::Action(Action::SetTimestamps(false))
        );
        assert!(matches!(dispatch("/preset"), DispatchResult::Error(_)));
        assert_eq!(dispatch("plain prompt"), DispatchResult::NotLocal);
    }
}
