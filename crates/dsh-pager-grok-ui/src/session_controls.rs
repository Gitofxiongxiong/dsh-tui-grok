//! Project DSH's independent Plan and permission-preset controls.
//!
//! Plan is not a sandbox mode. The TUI therefore keeps the two host
//! projections separate and never derives a synthetic combined mode.

use dsh_pager::SessionState;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const YOLO_PRESET: &str = "danger-full-access";
pub const DEFAULT_PERMISSION_PRESET: &str = "workspace-write";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlanModeSnapshot {
    pub active: bool,
    pub pending: bool,
}

impl PlanModeSnapshot {
    /// DSH's pending flag means a transition away from the current value.
    pub const fn target_active(self) -> bool {
        if self.pending {
            !self.active
        } else {
            self.active
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PermissionPresetSnapshot {
    pub current_value: Option<String>,
    pub options: Vec<String>,
}

impl PermissionPresetSnapshot {
    pub fn is_yolo(&self) -> bool {
        self.current_value.as_deref() == Some(YOLO_PRESET)
    }

    pub fn supports(&self, preset: &str) -> bool {
        self.options.iter().any(|option| option == preset)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionControlsSnapshot {
    pub plan: PlanModeSnapshot,
    pub permission: PermissionPresetSnapshot,
}

pub fn derive_session_controls(session: &SessionState) -> SessionControlsSnapshot {
    SessionControlsSnapshot {
        plan: derive_plan(session),
        permission: derive_permission(session),
    }
}

fn derive_plan(session: &SessionState) -> PlanModeSnapshot {
    if let Some(value) = session.projection("plan") {
        return PlanModeSnapshot {
            active: value
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            pending: value
                .get("pending")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
    }
    PlanModeSnapshot {
        active: fold_last_string(session, "plan/mode", "active") == Some("true")
            || fold_last_bool(session, "plan/mode", "active") == Some(true),
        pending: false,
    }
}

fn derive_permission(session: &SessionState) -> PermissionPresetSnapshot {
    if let Some(value) = session.projection("permissions") {
        let current_value = value
            .get("currentValue")
            .or_else(|| value.get("current_value"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let options = value
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                option
                    .as_str()
                    .or_else(|| option.get("value").and_then(Value::as_str))
                    .map(str::to_string)
            })
            .collect();
        return PermissionPresetSnapshot {
            current_value,
            options,
        };
    }

    let sandbox = fold_last_string(session, "sandbox/mode", "mode");
    let approval = fold_last_string(session, "approval/policy", "policy");
    let current_value = match (sandbox, approval) {
        (Some("danger-full-access"), Some("never")) => Some(YOLO_PRESET.to_string()),
        (Some("workspace-write"), Some("ask")) => Some(DEFAULT_PERMISSION_PRESET.to_string()),
        (Some("read-only"), Some("ask")) => Some("read-only".to_string()),
        _ => None,
    };
    PermissionPresetSnapshot {
        current_value,
        options: Vec::new(),
    }
}

fn fold_last_string<'a>(
    session: &'a SessionState,
    event_type: &str,
    field: &str,
) -> Option<&'a str> {
    session.history().iter().rev().find_map(|entry| {
        (entry.event.event_type == event_type)
            .then(|| entry.event.data.get(field).and_then(Value::as_str))
            .flatten()
    })
}

fn fold_last_bool(session: &SessionState, event_type: &str, field: &str) -> Option<bool> {
    session.history().iter().rev().find_map(|entry| {
        (entry.event.event_type == event_type)
            .then(|| entry.event.data.get(field).and_then(Value::as_bool))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager_protocol::{
        HistoryEntry, SessionEvent, SessionHistoryValue, SessionProjectionsBlock,
    };
    use serde_json::json;

    fn session_with(projections: serde_json::Map<String, Value>) -> SessionState {
        let mut state = SessionState::new("s".into(), 1);
        state
            .install_initial(SessionHistoryValue {
                events: Vec::<HistoryEntry>::new(),
                has_more: false,
                projections: Some(SessionProjectionsBlock {
                    as_of_seq: 0,
                    values: projections,
                }),
            })
            .expect("install");
        state
    }

    #[test]
    fn plan_and_yolo_are_independent() {
        let mut values = serde_json::Map::new();
        values.insert("plan".into(), json!({"active": true, "pending": false}));
        values.insert(
            "permissions".into(),
            json!({
                "currentValue": "danger-full-access",
                "options": [
                    {"value": "workspace-write"},
                    {"value": "danger-full-access"}
                ]
            }),
        );
        let controls = derive_session_controls(&session_with(values));
        assert!(controls.plan.target_active());
        assert!(controls.permission.is_yolo());
        assert!(controls.permission.supports(DEFAULT_PERMISSION_PRESET));
    }

    #[test]
    fn pending_plan_projects_the_transition_target() {
        assert!(
            PlanModeSnapshot {
                active: false,
                pending: true
            }
            .target_active()
        );
        assert!(
            !PlanModeSnapshot {
                active: true,
                pending: true
            }
            .target_active()
        );
    }

    #[test]
    fn raw_events_are_read_only_fallback_not_capability_proof() {
        let mut state = SessionState::new("s".into(), 1);
        state
            .install_initial(SessionHistoryValue {
                events: vec![
                    HistoryEntry {
                        event: SessionEvent {
                            event_type: "sandbox/mode".into(),
                            seq: 1,
                            time: 1.0,
                            data: json!({"mode": "danger-full-access"}),
                            source_event_seqs: None,
                            surface_op: None,
                            ignorable: None,
                        },
                        view: None,
                    },
                    HistoryEntry {
                        event: SessionEvent {
                            event_type: "approval/policy".into(),
                            seq: 2,
                            time: 2.0,
                            data: json!({"policy": "never"}),
                            source_event_seqs: None,
                            surface_op: None,
                            ignorable: None,
                        },
                        view: None,
                    },
                ],
                has_more: false,
                projections: None,
            })
            .expect("install");
        let permission = derive_session_controls(&state).permission;
        assert!(permission.is_yolo());
        assert!(permission.options.is_empty());
    }
}
