//! `/model` (alias `/m`) — switch model + (optionally) reasoning effort.
//! Chained autocomplete: pick a reasoning-supported model → trailing space
//! re-opens the dropdown into the model's advertised effort sub-menu.
//!
//! Copied from Grok `slash/commands/model.rs`. `SetDefaultModel` is the
//! bare-name path; `SwitchModel` carries an effort. Both compile to DSH
//! `session.selectModel`.

use crate::model_state::ModelState;
use crate::slash::{Action, ArgItem, CommandResult, SlashCommand};

use super::effort_levels::build_effort_arg_items;

/// Switch the active model (and optionally its reasoning effort).
pub struct ModelCommand;

impl SlashCommand for ModelCommand {
    fn name(&self) -> &str {
        "model"
    }

    fn description(&self) -> &str {
        "Switch the active model"
    }

    fn usage(&self) -> &str {
        "/model <name> [effort]"
    }

    fn run(&self, args: &str) -> CommandResult {
        if args.trim().is_empty() {
            CommandResult::Error("Usage: /model <name> [effort]".into())
        } else {
            CommandResult::Error("Usage: /model <name> [effort]".into())
        }
    }
}

impl ModelCommand {
    pub fn aliases(&self) -> &[&str] {
        &["m"]
    }

    pub fn suggest_args(&self, models: &ModelState, args_query: &str) -> Option<Vec<ArgItem>> {
        if models.is_empty() {
            return None;
        }

        // Effort phase if input is "<reasoning-model> ", else model phase.
        if let Some(model_id) = detect_effort_phase(models, args_query) {
            return Some(build_effort_items(models, &model_id));
        }
        Some(build_model_items(models))
    }

    pub fn run_with_models(&self, models: &ModelState, args: &str) -> CommandResult {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return CommandResult::Error("Usage: /model <name> [effort]".into());
        }

        // Prefer an exact full-string catalog match first. Model display names
        // often contain spaces; if we split on the last token first, a shorter
        // catalog entry would steal the prefix and treat the rest as effort.
        if let Some(id) = models.resolve_by_name_or_id(trimmed) {
            return CommandResult::Action(Action::SetDefaultModel(id));
        }

        // Trailing effort token + reasoning model → session-scoped switch.
        if let Some((prefix, token)) = split_trailing_token(trimmed)
            && let Some(id) = models.resolve_by_name_or_id(prefix)
            && models
                .find(&id)
                .is_some_and(|info| info.supports_reasoning_effort())
        {
            return match models.resolve_effort_for_model(&id, token) {
                Ok(effort) => CommandResult::Action(Action::SwitchModel {
                    model_id: id,
                    effort: Some(effort),
                }),
                Err(err) => CommandResult::Error(err.message()),
            };
        }

        CommandResult::Error(format!("Unknown model: {trimmed}"))
    }
}

/// Split `args` into `(prefix, last_token)` on the final whitespace run.
/// Returns `None` when there is no interior whitespace to split on.
fn split_trailing_token(args: &str) -> Option<(&str, &str)> {
    let (prefix, last) = args.rsplit_once(char::is_whitespace)?;
    let prefix = prefix.trim_end();
    if prefix.is_empty() || last.is_empty() {
        return None;
    }
    Some((prefix, last))
}

/// Returns the matched model id when `args_query` is `"<reasoning-model> ..."`.
/// Longest-name-first to disambiguate names that share a prefix.
fn detect_effort_phase(
    models: &ModelState,
    args_query: &str,
) -> Option<crate::model_state::ModelId> {
    let mut candidates: Vec<_> = models
        .available
        .iter()
        .filter(|info| info.supports_reasoning_effort())
        .map(|info| (info.id.clone(), info.name.as_str()))
        .collect();
    candidates.sort_by_key(|(_, name)| std::cmp::Reverse(name.len()));

    for (id, name) in candidates {
        if args_query.len() > name.len()
            && args_query.is_char_boundary(name.len())
            && args_query[..name.len()].eq_ignore_ascii_case(name)
            && args_query[name.len()..].starts_with(char::is_whitespace)
        {
            return Some(id);
        }
    }
    None
}

/// One row per logical model. Reasoning models get a trailing space in
/// `insert_text` so the prompt widget chains into the effort sub-menu.
fn build_model_items(models: &ModelState) -> Vec<ArgItem> {
    let current_id = models.current.as_ref();
    let mut items: Vec<ArgItem> = Vec::with_capacity(models.available.len());
    for info in &models.available {
        let is_current = current_id == Some(&info.id);
        let supports = info.supports_reasoning_effort();

        let display = if is_current {
            format!("{} (current)", info.name)
        } else {
            info.name.clone()
        };

        // Trailing space on reasoning models: signals "more input
        // expected" so Enter advances to effort phase instead of submitting.
        let insert_text = if supports {
            format!("{} ", info.name)
        } else {
            info.name.clone()
        };

        items.push(ArgItem {
            display,
            match_text: info.name.clone(),
            insert_text,
            description: info.description.clone().unwrap_or_else(|| info.id.key()),
        });
    }
    items
}

/// One row per effort level for the `/model` chained effort phase.
/// `insert_text` is `"ModelName high"` so selecting a row completes both tokens.
fn build_effort_items(models: &ModelState, model_id: &crate::model_state::ModelId) -> Vec<ArgItem> {
    let info = match models.find(model_id) {
        Some(info) => info,
        None => return Vec::new(),
    };
    let model_name = info.name.clone();
    let is_current_model = models.current.as_ref() == Some(model_id);
    let options = models.reasoning_effort_options_for(model_id);
    build_effort_arg_items(
        &options,
        models.reasoning_effort.as_deref(),
        is_current_model,
        |option| format!("{model_name} {}", option.id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_state::{ModelId, ModelInfo, ReasoningEffortOption};

    fn model_with_reasoning(provider: &str, id: &str, name: &str) -> ModelInfo {
        ModelInfo {
            id: ModelId::new(provider, id),
            name: name.to_string(),
            description: None,
            reasoning: Some(vec![
                ReasoningEffortOption {
                    id: "xhigh".into(),
                    name: "xhigh".into(),
                    description: Some("Extended reasoning".into()),
                },
                ReasoningEffortOption {
                    id: "high".into(),
                    name: "high".into(),
                    description: Some("Heavy reasoning".into()),
                },
                ReasoningEffortOption {
                    id: "medium".into(),
                    name: "medium".into(),
                    description: Some("Balanced reasoning".into()),
                },
                ReasoningEffortOption {
                    id: "low".into(),
                    name: "low".into(),
                    description: Some("Faster, lighter reasoning".into()),
                },
            ]),
            default_effort: Some("high".into()),
        }
    }

    fn plain_model(provider: &str, id: &str, name: &str) -> ModelInfo {
        ModelInfo {
            id: ModelId::new(provider, id),
            name: name.to_string(),
            description: None,
            reasoning: None,
            default_effort: None,
        }
    }

    #[test]
    fn split_trailing_token_splits_on_final_whitespace() {
        assert_eq!(
            split_trailing_token("Reasoning X high"),
            Some(("Reasoning X", "high"))
        );
        assert_eq!(
            split_trailing_token("reasoning-x  xhigh"),
            Some(("reasoning-x", "xhigh"))
        );
        assert!(split_trailing_token("reasoning-x-pro").is_none());
    }

    #[test]
    fn empty_query_returns_one_row_per_logical_model() {
        let mut state = ModelState::default();
        state.available.push(model_with_reasoning(
            "deepseek-official",
            "reasoning-x",
            "Reasoning X",
        ));
        state
            .available
            .push(plain_model("deepseek-official", "grok-4.5", "Grok 4.5"));

        let cmd = ModelCommand;
        let items = cmd.suggest_args(&state, "").unwrap();
        assert_eq!(items.len(), 2, "model phase: one row per logical model");

        let reasoning = items
            .iter()
            .find(|i| i.match_text == "Reasoning X")
            .unwrap();
        assert_eq!(reasoning.insert_text, "Reasoning X ");

        let plain = items.iter().find(|i| i.match_text == "Grok 4.5").unwrap();
        assert_eq!(plain.insert_text, "Grok 4.5");
    }

    #[test]
    fn trailing_space_after_reasoning_model_enters_effort_phase() {
        let mut state = ModelState::default();
        state.available.push(model_with_reasoning(
            "deepseek-official",
            "reasoning-x",
            "Reasoning X",
        ));

        let cmd = ModelCommand;
        let items = cmd.suggest_args(&state, "Reasoning X ").unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].insert_text, "Reasoning X xhigh");
        assert_eq!(items[1].insert_text, "Reasoning X high");
        assert_eq!(items[2].insert_text, "Reasoning X medium");
        assert_eq!(items[3].insert_text, "Reasoning X low");
        assert_eq!(items[0].display, "xhigh");
        assert!(items[0].match_text.starts_with("a "));
        assert!(items[3].match_text.starts_with("d "));
    }

    #[test]
    fn run_parses_model_plus_effort_when_supported() {
        let mut state = ModelState::default();
        state.available.push(model_with_reasoning(
            "deepseek-official",
            "reasoning-x",
            "Reasoning X",
        ));
        let result = ModelCommand.run_with_models(&state, "Reasoning X xhigh");
        match result {
            CommandResult::Action(Action::SwitchModel { model_id, effort }) => {
                assert_eq!(model_id.model, "reasoning-x");
                assert_eq!(effort.as_deref(), Some("xhigh"));
            }
            other => panic!("expected SwitchModel with effort, got {other:?}"),
        }
    }

    #[test]
    fn run_rejects_unoffered_effort_with_effort_error_not_unknown_model() {
        let mut state = ModelState::default();
        state.available.push(model_with_reasoning(
            "deepseek-official",
            "reasoning-x",
            "Reasoning X",
        ));
        let result = ModelCommand.run_with_models(&state, "Reasoning X none");
        match result {
            CommandResult::Error(msg) => {
                assert!(
                    msg.contains("unknown effort level 'none'"),
                    "expected effort error, got {msg}"
                );
                assert!(
                    msg.contains("use one of:"),
                    "expected offered levels in message, got {msg}"
                );
                assert!(
                    !msg.to_lowercase().contains("unknown model"),
                    "must not misreport as unknown model: {msg}"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn run_prefers_full_multi_word_model_name_over_prefix_plus_effort() {
        let mut state = ModelState::default();
        state
            .available
            .push(model_with_reasoning("deepseek-official", "grok", "Grok"));
        state.available.push(model_with_reasoning(
            "deepseek-official",
            "grok-4.5",
            "Grok 4.5",
        ));
        let result = ModelCommand.run_with_models(&state, "Grok 4.5");
        match result {
            CommandResult::Action(Action::SetDefaultModel(resolved_id)) => {
                assert_eq!(resolved_id.model, "grok-4.5");
            }
            other => panic!("expected SetDefaultModel(Grok 4.5), got {other:?}"),
        }
    }

    #[test]
    fn run_bare_model_name_dispatches_set_default_model() {
        let mut state = ModelState::default();
        state
            .available
            .push(plain_model("deepseek-official", "grok-4.5", "Grok 4.5"));
        let result = ModelCommand.run_with_models(&state, "Grok 4.5");
        match result {
            CommandResult::Action(Action::SetDefaultModel(resolved_id)) => {
                assert_eq!(resolved_id.model, "grok-4.5");
            }
            other => panic!("expected Action::SetDefaultModel(<id>), got {other:?}"),
        }
    }

    #[test]
    fn run_set_default_model_resolves_case_insensitively() {
        let mut state = ModelState::default();
        state
            .available
            .push(plain_model("deepseek-official", "grok-4.5", "Grok 4.5"));
        let result = ModelCommand.run_with_models(&state, "grok 4.5");
        match result {
            CommandResult::Action(Action::SetDefaultModel(resolved_id)) => {
                assert_eq!(resolved_id.model, "grok-4.5");
            }
            other => panic!("expected Action::SetDefaultModel(<id>), got {other:?}"),
        }
    }
}
