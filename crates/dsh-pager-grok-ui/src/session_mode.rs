//! Derive the three DSH session modes from host projections and session events.

use dsh_pager::SessionState;
use dsh_pager_protocol::SessionModeId;
use serde_json::Value;

pub use dsh_pager_protocol::SessionModeId as ModeId;

/// Project the authoritative session mode. Pending UI switches are applied by
/// the runtime; this fold only reads host truth.
pub fn derive_session_mode(session: &SessionState) -> SessionModeId {
    if plan_active(session) {
        return SessionModeId::Plan;
    }
    if permission_value(session) == Some("danger-full-access")
        || matches_danger_full_access_events(session)
    {
        return SessionModeId::DangerFullAccess;
    }
    SessionModeId::Normal
}

fn plan_active(session: &SessionState) -> bool {
    if let Some(value) = session.projection("plan") {
        let active = value
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let pending = value
            .get("pending")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if active || pending {
            return true;
        }
    }
    fold_event(session, "plan/mode", |data| {
        data.get("active").and_then(Value::as_bool) == Some(true)
    })
}

fn permission_value(session: &SessionState) -> Option<&str> {
    session
        .projection("permissions")
        .and_then(|value| {
            value
                .get("currentValue")
                .or_else(|| value.get("current_value"))
        })
        .and_then(Value::as_str)
}

fn matches_danger_full_access_events(session: &SessionState) -> bool {
    fold_last_string(session, "sandbox/mode", "mode") == Some("danger-full-access")
        && fold_last_string(session, "approval/policy", "policy") == Some("never")
}

fn fold_event(session: &SessionState, event_type: &str, matches: impl Fn(&Value) -> bool) -> bool {
    session
        .history()
        .iter()
        .rev()
        .find(|entry| entry.event.event_type == event_type)
        .is_some_and(|entry| matches(&entry.event.data))
}

fn fold_last_string<'a>(
    session: &'a SessionState,
    event_type: &str,
    field: &str,
) -> Option<&'a str> {
    session.history().iter().rev().find_map(|entry| {
        if entry.event.event_type == event_type {
            entry.event.data.get(field).and_then(Value::as_str)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsh_pager::SessionState;
    use dsh_pager_protocol::{HistoryEntry, SessionEvent, SessionHistoryValue};
    use serde_json::json;

    fn session_with(
        projections: serde_json::Map<String, Value>,
        events: Vec<SessionEvent>,
    ) -> SessionState {
        let mut state = SessionState::new("s".into(), 1);
        state
            .install_initial(SessionHistoryValue {
                events: events
                    .into_iter()
                    .map(|event| HistoryEntry { event, view: None })
                    .collect(),
                has_more: false,
                projections: Some(dsh_pager_protocol::SessionProjectionsBlock {
                    as_of_seq: 0,
                    values: projections,
                }),
            })
            .expect("install");
        state
    }

    #[test]
    fn missing_projections_are_normal() {
        let session = SessionState::new("s".into(), 1);
        assert_eq!(derive_session_mode(&session), SessionModeId::Normal);
    }

    #[test]
    fn plan_projection_wins_over_permissions() {
        let mut values = serde_json::Map::new();
        values.insert("plan".into(), json!({"active": true, "pending": false}));
        values.insert(
            "permissions".into(),
            json!({"currentValue": "danger-full-access"}),
        );
        let session = session_with(values, Vec::new());
        assert_eq!(derive_session_mode(&session), SessionModeId::Plan);
    }

    #[test]
    fn danger_full_access_uses_permission_select() {
        let mut values = serde_json::Map::new();
        values.insert("plan".into(), json!({"active": false, "pending": false}));
        values.insert(
            "permissions".into(),
            json!({"currentValue": "danger-full-access"}),
        );
        let session = session_with(values, Vec::new());
        assert_eq!(
            derive_session_mode(&session),
            SessionModeId::DangerFullAccess
        );
    }

    #[test]
    fn event_fold_recovers_danger_full_access_without_projections() {
        let session = session_with(
            serde_json::Map::new(),
            vec![
                SessionEvent {
                    event_type: "sandbox/mode".into(),
                    seq: 1,
                    time: 1.0,
                    data: json!({"mode": "danger-full-access"}),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
                SessionEvent {
                    event_type: "approval/policy".into(),
                    seq: 2,
                    time: 2.0,
                    data: json!({"policy": "never"}),
                    source_event_seqs: None,
                    surface_op: None,
                    ignorable: None,
                },
            ],
        );
        assert_eq!(
            derive_session_mode(&session),
            SessionModeId::DangerFullAccess
        );
    }
}
