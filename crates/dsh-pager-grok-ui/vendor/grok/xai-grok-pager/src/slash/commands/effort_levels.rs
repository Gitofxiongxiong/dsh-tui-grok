//! Shared reasoning-effort dropdown levels for `/model` and `/effort`.
//!
//! Copied from Grok `slash/commands/effort_levels.rs`. Option ids come from
//! the DSH catalog instead of Grok's `ReasoningEffort` enum.

use crate::model_state::ReasoningEffortOption;
use crate::slash::ArgItem;

/// Build effort rows for autocomplete from a per-model option list.
///
/// - `mark_active` + `current_effort` mark the current session effort with `(active)`.
/// - `insert_text_for` controls what is inserted on select:
///   - `/effort`: the option id (`"deep"`)
///   - `/model` chained phase: `"ModelName deep"`
///
/// `match_text` gets an `a `/`b `/…` sort prefix so the matcher's alphabetical
/// tiebreak preserves the option order.
pub(crate) fn build_effort_arg_items(
    options: &[ReasoningEffortOption],
    current_effort: Option<&str>,
    mark_active: bool,
    insert_text_for: impl Fn(&ReasoningEffortOption) -> String,
) -> Vec<ArgItem> {
    options
        .iter()
        .enumerate()
        .map(|(idx, option)| {
            let active = mark_active && current_effort == Some(option.id.as_str());
            let active_suffix = if active { " (active)" } else { "" };
            let insert_text = insert_text_for(option);
            // Sort-key prefix: 'a' for top row, 'b' for next, etc. Only
            // affects matcher tiebreak ordering, never rendered.
            let sort_prefix = char::from(b'a' + idx as u8);
            ArgItem {
                display: format!("{}{active_suffix}", option.name),
                match_text: format!("{sort_prefix} {insert_text}"),
                insert_text,
                description: option.description.clone().unwrap_or_default(),
            }
        })
        .collect()
}
