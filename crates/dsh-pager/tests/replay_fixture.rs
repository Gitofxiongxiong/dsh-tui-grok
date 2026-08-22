use dsh_pager::{ConnectionPhase, SessionState};
use dsh_pager_protocol::{HistoryEntry, JsonRpcNotification, SessionHistoryValue};
use dsh_pager_test_support::{read_jsonl, Scenario, ScenarioStep};
use serde_json::json;

fn fixture_entries() -> Vec<HistoryEntry> {
    let fixture = read_jsonl(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/replay-live.jsonl"
    ))
    .expect("read replay fixture");
    fixture
        .records
        .into_iter()
        .map(|record| serde_json::from_value(record).expect("history entry shape"))
        .collect()
}

fn live(session_id: &str, entry: &HistoryEntry) -> JsonRpcNotification {
    JsonRpcNotification {
        jsonrpc: "2.0".into(),
        method: "events.mux".into(),
        params: Some(json!({
            "type": "session/event",
            "sessionId": session_id,
            "event": entry.event,
        })),
    }
}

#[test]
fn replay_fixture_stitches_duplicate_and_out_of_order_live_events() {
    let scenario = Scenario::new("replay-live").push(ScenarioStep::Note {
        text: "history baseline, duplicate live frame, then gap repair".into(),
    });
    assert_eq!(scenario.name, "replay-live");
    assert_eq!(scenario.steps.len(), 1);
    let entries = fixture_entries();
    assert_eq!(entries.len(), 4);
    assert!(entries
        .windows(2)
        .all(|pair| { pair[0].event.seq.saturating_add(1) == pair[1].event.seq }));

    let mut state = SessionState::new("fixture-session".into(), 3);
    state
        .install_initial(SessionHistoryValue {
            events: entries[..2].to_vec(),
            has_more: false,
            projections: None,
        })
        .expect("install fixture baseline");

    let duplicate = state.accept_notification(live("fixture-session", &entries[1]));
    assert!(!duplicate.expect("duplicate accepted").changed);

    let gap = state.accept_notification(live("fixture-session", &entries[3]));
    assert!(!gap.expect("gap buffered").changed);
    assert!(state.needs_repair());

    let next = state.accept_notification(live("fixture-session", &entries[2]));
    assert!(next.expect("buffer drained").changed);
    assert_eq!(state.tail_seq(), Some(3));
    assert_eq!(state.connection_phase(), ConnectionPhase::Connected);
    assert!(state
        .presentation_model()
        .entries
        .iter()
        .any(|entry| { entry.text == "fixture answer" }));
}
