use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Data-only scenario description shared by deterministic integration tests.
/// Keeping the steps serializable makes it possible to promote a regression
/// from a Rust test into a reviewed JSONL fixture without changing semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub steps: Vec<ScenarioStep>,
}

impl Scenario {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    pub fn push(mut self, step: ScenarioStep) -> Self {
        self.steps.push(step);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScenarioStep {
    Send { message: Value },
    ExpectMethod { method: String },
    ExpectText { text: String },
    Resize { rows: u16, cols: u16 },
    WaitMs { milliseconds: u64 },
    Note { text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_is_stable_json() {
        let scenario = Scenario::new("reconnect").push(ScenarioStep::ExpectMethod {
            method: "tui.hello".into(),
        });
        let encoded = serde_json::to_value(&scenario).expect("scenario JSON");
        assert_eq!(encoded["name"], "reconnect");
        assert_eq!(encoded["steps"][0]["kind"], "expect_method");
    }
}
