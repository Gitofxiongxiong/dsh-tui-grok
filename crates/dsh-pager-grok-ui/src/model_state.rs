//! Model state — tracks available models and current selection.
//!
//! Copied from Grok `acp/model_state.rs`. Catalog identity is DSH's
//! `{provider, model}` pair; Grok's single `ModelId` is `provider/model`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use dsh_pager_protocol::{ModelCatalogFailure, ModelSelection, SessionModelsValue};

const MODEL_LABEL_ALIASES_JSON: &str = include_str!("../model-label-aliases.json");
static MODEL_LABEL_ALIASES: OnceLock<BTreeMap<String, String>> = OnceLock::new();

/// Compact model label used only by space-constrained prompt/welcome chrome.
///
/// Catalog names and wire identities stay untouched: pickers and RPC effects
/// continue to use the Host-published value. Aliases are project-owned data;
/// unknown names stay intact so the renderer can elide them to its real width.
pub fn compact_model_label(name: &str) -> String {
    model_label_aliases()
        .get(&name.to_ascii_lowercase())
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

/// Compact current-model label with its Host-authoritative reasoning effort.
///
/// Keep effort attached to the model, matching Grok's prompt/welcome contract.
/// The model alias must be resolved before adding the effort suffix because the
/// checked-in alias table intentionally matches exact catalog display names.
pub fn compact_model_effort_label(name: &str, effort: Option<&str>) -> String {
    let model = compact_model_label(name);
    match effort {
        Some(effort) => format!("{model} ({effort})"),
        None => model,
    }
}

fn model_label_aliases() -> &'static BTreeMap<String, String> {
    MODEL_LABEL_ALIASES.get_or_init(|| {
        let aliases = serde_json::from_str::<BTreeMap<String, String>>(MODEL_LABEL_ALIASES_JSON)
            .expect("model-label-aliases.json must be a string-to-string JSON object");
        aliases
            .into_iter()
            .map(|(name, alias)| (name.to_ascii_lowercase(), alias))
            .collect()
    })
}

/// Why an effort token could not be applied to a model. Shared by `/model`'s
/// effort phase so typed input classifies the same way as the ArgPicker rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EffortTokenError {
    /// The target model does not advertise reasoning efforts.
    Unsupported,
    /// The token is not a menu id offered by this model's menu.
    UnknownToken { token: String, offered: Vec<String> },
    /// No active model to resolve the effort against. Kept for `/effort` parity
    /// with Grok `EffortTokenError`.
    #[allow(dead_code)]
    NoActiveModel,
}

impl EffortTokenError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Unsupported => "current model does not support reasoning effort".to_string(),
            Self::UnknownToken { token, offered } => {
                if offered.is_empty() {
                    format!(
                        "unknown effort level '{token}'; this model has no selectable effort levels"
                    )
                } else {
                    format!(
                        "unknown effort level '{token}'; use one of: {}",
                        offered.join(", ")
                    )
                }
            }
            Self::NoActiveModel => "no active model to apply effort to".to_string(),
        }
    }
}

/// Session-scoped `{provider, model}` identity. Display/key form is
/// `provider/model`, matching Grok's single catalog key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelId {
    pub provider: String,
    pub model: String,
}

impl ModelId {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }

    pub fn key(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let (provider, model) = key.split_once('/')?;
        if provider.is_empty() || model.is_empty() {
            return None;
        }
        Some(Self::new(provider, model))
    }
}

impl From<&ModelSelection> for ModelId {
    fn from(selection: &ModelSelection) -> Self {
        Self::new(&selection.provider, &selection.model)
    }
}

/// One adapter-owned reasoning effort shown in the `/model` effort phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningEffortOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// One catalog row. `id` is the provider-owned model id; `name` is displayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: ModelId,
    pub name: String,
    pub description: Option<String>,
    pub reasoning: Option<Vec<ReasoningEffortOption>>,
    pub default_effort: Option<String>,
}

impl ModelInfo {
    pub fn supports_reasoning_effort(&self) -> bool {
        self.reasoning
            .as_ref()
            .is_some_and(|efforts| !efforts.is_empty())
    }
}

/// Per-session model state. Mirrors Grok `ModelState`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelState {
    pub available: Vec<ModelInfo>,
    pub current: Option<ModelId>,
    pub reasoning_effort: Option<String>,
    pub routable: Option<bool>,
    pub failures: Vec<ModelCatalogFailure>,
}

impl ModelState {
    pub fn is_empty(&self) -> bool {
        self.available.is_empty()
    }

    /// Display name for the current model.
    pub fn current_model_name(&self) -> Option<String> {
        let current = self.current.as_ref()?;
        if let Some(model_info) = self.find(current) {
            Some(model_info.name.clone())
        } else {
            Some(current.model.clone())
        }
    }

    pub fn find(&self, id: &ModelId) -> Option<&ModelInfo> {
        self.available.iter().find(|info| &info.id == id)
    }

    /// Replace the available-model list. Leaves `current` and
    /// `reasoning_effort` alone — those change only via `/model` / create / load.
    pub fn update_catalog(&mut self, new_available: Vec<ModelInfo>) {
        self.available = new_available;
    }

    /// Set the current model and resolve reasoning effort from catalog meta.
    pub fn set_current(&mut self, model_id: ModelId, effort_override: Option<String>) {
        self.current = Some(model_id.clone());
        self.reasoning_effort = effort_override.or_else(|| {
            self.find(&model_id)
                .and_then(|info| info.default_effort.clone())
        });
        self.routable = Some(true);
    }

    /// Apply a host `session.models` value. Current is kept even when it is
    /// absent from the advisory groups (Grok catalog membership is not a
    /// whitelist).
    pub fn apply_session_models(&mut self, value: SessionModelsValue) {
        let current = ModelId::from(&value.current);
        let effort = value.current.reasoning_effort.clone();
        self.available = flatten_groups(&value);
        self.failures = value.failures;
        self.routable = Some(value.routable);
        self.current = Some(current);
        self.reasoning_effort = effort;
    }

    /// Menu for a specific catalog model id (used by `/model`'s effort phase).
    pub(crate) fn reasoning_effort_options_for(&self, id: &ModelId) -> Vec<ReasoningEffortOption> {
        self.find(id)
            .and_then(|info| info.reasoning.clone())
            .unwrap_or_default()
    }

    pub fn reasoning_effort_options(&self) -> Vec<ReasoningEffortOption> {
        match self.current.as_ref() {
            Some(id) => self.reasoning_effort_options_for(id),
            None => Vec::new(),
        }
    }

    /// Canonical effort-token policy: gate on the model's support flag first,
    /// then resolve the token against that model's advertised ids.
    pub(crate) fn resolve_effort_for_model(
        &self,
        id: &ModelId,
        token: &str,
    ) -> Result<String, EffortTokenError> {
        let supports = self
            .find(id)
            .is_some_and(ModelInfo::supports_reasoning_effort);
        if !supports {
            return Err(EffortTokenError::Unsupported);
        }
        let options = self.reasoning_effort_options_for(id);
        options
            .iter()
            .find(|opt| opt.id.eq_ignore_ascii_case(token) || opt.name.eq_ignore_ascii_case(token))
            .map(|opt| opt.id.clone())
            .ok_or_else(|| EffortTokenError::UnknownToken {
                token: token.to_string(),
                offered: options.into_iter().map(|opt| opt.id).collect(),
            })
    }

    /// Resolve a user-supplied name to a `ModelId` via case-insensitive
    /// ASCII match against the catalog.
    pub fn resolve_by_name_or_id(&self, query: &str) -> Option<ModelId> {
        self.available.iter().find_map(|info| {
            if info.name.eq_ignore_ascii_case(query)
                || info.id.model.eq_ignore_ascii_case(query)
                || info.id.key().eq_ignore_ascii_case(query)
            {
                Some(info.id.clone())
            } else {
                None
            }
        })
    }

    /// Look up the display name for a `ModelId` in the catalog.
    pub fn display_name_for(&self, id: &ModelId) -> String {
        self.find(id)
            .map(|info| info.name.clone())
            .unwrap_or_else(|| id.model.clone())
    }
}

fn flatten_groups(value: &SessionModelsValue) -> Vec<ModelInfo> {
    let mut available = Vec::new();
    for group in &value.groups {
        for model in &group.models {
            let reasoning = model.reasoning.as_ref().map(|meta| {
                meta.efforts
                    .iter()
                    .map(|effort| ReasoningEffortOption {
                        id: effort.id.clone(),
                        name: effort.name.clone(),
                        description: effort.description.clone(),
                    })
                    .collect()
            });
            available.push(ModelInfo {
                id: ModelId::new(&group.id, &model.id),
                name: model.name.clone(),
                description: model.description.clone(),
                reasoning,
                default_effort: model
                    .reasoning
                    .as_ref()
                    .and_then(|meta| meta.default_effort.clone()),
            });
        }
    }
    available
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::{ModelCatalogModel, ModelProviderGroup};

    fn sample() -> ModelState {
        let mut state = ModelState::default();
        state.available.push(ModelInfo {
            id: ModelId::new("deepseek-official", "deepseek-chat"),
            name: "DeepSeek Chat".into(),
            description: None,
            reasoning: None,
            default_effort: None,
        });
        state.available.push(ModelInfo {
            id: ModelId::new("deepseek-official", "deepseek-reasoner"),
            name: "DeepSeek Reasoner".into(),
            description: None,
            reasoning: Some(vec![ReasoningEffortOption {
                id: "high".into(),
                name: "High".into(),
                description: Some("Heavy reasoning".into()),
            }]),
            default_effort: Some("high".into()),
        });
        state.current = Some(ModelId::new("deepseek-official", "deepseek-chat"));
        state
    }

    #[test]
    fn resolve_matches_name_id_and_key() {
        let state = sample();
        assert_eq!(
            state.resolve_by_name_or_id("DeepSeek Chat").unwrap().model,
            "deepseek-chat"
        );
        assert_eq!(
            state
                .resolve_by_name_or_id("deepseek-reasoner")
                .unwrap()
                .model,
            "deepseek-reasoner"
        );
        assert_eq!(
            state
                .resolve_by_name_or_id("deepseek-official/deepseek-chat")
                .unwrap()
                .model,
            "deepseek-chat"
        );
    }

    #[test]
    fn compact_prompt_label_uses_the_checked_in_alias_table() {
        assert_eq!(compact_model_label("DeepSeek-V4-Flash"), "dsv4 flash");
        assert_eq!(
            compact_model_label("DeepSeek-V4-Flash-Vision-Exp"),
            "dsv4 flash-v"
        );
        assert_eq!(
            compact_model_label("DeepSeek-V4-Flash-Vision-Preview"),
            "dsv4 flash-v"
        );
        assert_eq!(compact_model_label("DeepSeek-V4-Pro"), "dsv4 pro");
        assert_eq!(compact_model_label("deepseek-v4-pro"), "dsv4 pro");
        assert_eq!(
            compact_model_label("Preview-DeepSeek-V4-Flash"),
            "Preview-DeepSeek-V4-Flash"
        );
        assert_eq!(
            compact_model_label("DeepSeek V4 Flash"),
            "DeepSeek V4 Flash"
        );
        assert_eq!(compact_model_label("private-preview"), "private-preview");
    }

    #[test]
    fn compact_prompt_label_appends_authoritative_effort_after_aliasing() {
        assert_eq!(
            compact_model_effort_label("DeepSeek-V4-Flash", Some("high")),
            "dsv4 flash (high)"
        );
        assert_eq!(
            compact_model_effort_label("DeepSeek-V4-Flash", Some("off")),
            "dsv4 flash (off)"
        );
        assert_eq!(
            compact_model_effort_label("DeepSeek-V4-Pro", None),
            "dsv4 pro"
        );
        assert_eq!(
            compact_model_effort_label("private-preview", Some("max")),
            "private-preview (max)"
        );
    }

    #[test]
    fn checked_in_alias_table_has_nonempty_unique_case_insensitive_keys() {
        let raw = serde_json::from_str::<BTreeMap<String, String>>(MODEL_LABEL_ALIASES_JSON)
            .expect("checked-in aliases parse");
        assert!(!raw.is_empty());
        assert!(
            raw.iter()
                .all(|(name, alias)| !name.is_empty() && !alias.is_empty())
        );
        assert_eq!(raw.len(), model_label_aliases().len());
    }

    #[test]
    fn apply_session_models_keeps_current_out_of_groups() {
        let mut state = ModelState::default();
        state.apply_session_models(SessionModelsValue {
            current: ModelSelection {
                provider: "deepseek-official".into(),
                model: "private-preview".into(),
                reasoning_effort: None,
            },
            routable: true,
            groups: vec![ModelProviderGroup {
                id: "deepseek-official".into(),
                name: "DeepSeek".into(),
                models: vec![ModelCatalogModel {
                    id: "deepseek-chat".into(),
                    name: "DeepSeek Chat".into(),
                    description: None,
                    reasoning: None,
                }],
            }],
            failures: Vec::new(),
        });
        assert_eq!(state.current.as_ref().unwrap().model, "private-preview");
        assert_eq!(state.available.len(), 1);
        assert_eq!(
            state.current_model_name().as_deref(),
            Some("private-preview")
        );
    }

    #[test]
    fn effort_gate_rejects_plain_model() {
        let state = sample();
        let id = ModelId::new("deepseek-official", "deepseek-chat");
        assert!(matches!(
            state.resolve_effort_for_model(&id, "high"),
            Err(EffortTokenError::Unsupported)
        ));
    }
}
