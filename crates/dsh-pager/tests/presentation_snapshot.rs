use std::fs;

use dsh_pager::SessionState;
use dsh_pager_protocol::SessionHistoryValue;
use dsh_pager_test_support::read_jsonl;
use serde_json::Value;

#[test]
fn replay_presentation_matches_reviewed_semantic_snapshot() {
    let fixture = read_jsonl(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/replay-live.jsonl"
    ))
    .expect("read fixture");
    let events = fixture
        .records
        .into_iter()
        .map(|record| serde_json::from_value(record).expect("history entry"))
        .collect();
    let mut state = SessionState::new("fixture-session".into(), 3);
    state
        .install_initial(SessionHistoryValue {
            events,
            has_more: false,
            projections: None,
        })
        .expect("install fixture");

    let actual = serde_json::to_value(state.presentation_model()).expect("serialize model");
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/snapshots/replay-presentation.json"
        ))
        .expect("read semantic snapshot"),
    )
    .expect("parse semantic snapshot");
    assert_eq!(actual, expected);
}
