use dsh_pager::{
    DshPresentationAdapter, DshRenderBlock, DshRenderEntry, DshRenderEntryId, DshRenderKind,
    DshRenderUpdate, SessionState,
};
use dsh_pager_protocol::{HistoryEntry, JsonRpcNotification, SessionEvent, SessionHistoryValue};
use serde_json::{json, Value};

fn history(seq: i64, event_type: &str, data: Value) -> HistoryEntry {
    HistoryEntry {
        event: SessionEvent {
            event_type: event_type.into(),
            seq,
            time: seq as f64,
            data,
            source_event_seqs: None,
            surface_op: None,
            ignorable: None,
        },
        view: None,
    }
}

fn mux(
    session_id: &str,
    generation: Option<u64>,
    frame_type: &str,
    mut frame: Value,
) -> JsonRpcNotification {
    frame["type"] = Value::String(frame_type.into());
    frame["sessionId"] = Value::String(session_id.into());
    if let Some(generation) = generation {
        frame["generation"] = json!(generation);
    }
    JsonRpcNotification {
        jsonrpc: "2.0".into(),
        method: "events.mux".into(),
        params: Some(frame),
    }
}

#[test]
fn structured_fixture_preserves_markdown_reasoning_image_tool_and_unknown_blocks() {
    let mut adapter = DshPresentationAdapter::default();
    let updates = adapter.adapt_event(&history(
        7,
        "assistant/message",
        json!({
            "message": { "content": [
                { "type": "text", "text": "answer" },
                { "type": "reasoning", "text": "thinking" },
                { "type": "image", "attachment": { "attachmentId": "img-1", "mediaType": "image/png", "name": "plot" } },
                { "type": "tool-call", "id": "call-1", "name": "edit", "arguments": "{\"path\":\"a.rs\",\"old_string\":\"old\",\"new_string\":\"new\"}" },
                { "type": "future-block", "payload": { "x": 1 } }
            ] }
        }),
    ));
    let DshRenderUpdate::Upsert(entry) = &updates[0] else {
        panic!("expected assistant block");
    };
    assert_eq!(entry.kind, DshRenderKind::Assistant);
    assert!(matches!(
        entry.content.blocks[0],
        DshRenderBlock::Markdown { .. }
    ));
    assert!(matches!(
        entry.content.blocks[1],
        DshRenderBlock::Reasoning { .. }
    ));
    assert!(matches!(
        entry.content.blocks[2],
        DshRenderBlock::Image { .. }
    ));
    assert!(matches!(
        entry.content.blocks[3],
        DshRenderBlock::ToolCall { edit: Some(_), .. }
    ));
    assert!(matches!(
        entry.content.blocks[4],
        DshRenderBlock::Unknown { .. }
    ));
    assert!(entry.text.contains("answer"));
}

#[test]
fn partial_stream_and_final_message_keep_one_stable_surface() {
    let mut adapter = DshPresentationAdapter::default();
    let updates = adapter.adapt_history(&[
        history(
            0,
            "assistant/chunk",
            json!({
                "turn": 1, "step": 1,
                "chunk": { "type": "block-start", "index": 0, "blockType": "text" }
            }),
        ),
        history(
            1,
            "assistant/chunk",
            json!({
                "turn": 1, "step": 1,
                "chunk": { "type": "text-delta", "index": 0, "text": "draft" }
            }),
        ),
        history(
            2,
            "assistant/message",
            json!({
                "turn": 1, "step": 1,
                "message": { "content": [{ "type": "text", "text": "final" }] }
            }),
        ),
    ]);
    assert!(updates.iter().any(|update| matches!(
        update,
        DshRenderUpdate::Upsert(DshRenderEntry { id: DshRenderEntryId::Partial { .. }, text, .. }) if text == "draft"
    )));
    assert!(updates.iter().any(|update| matches!(
        update,
        DshRenderUpdate::Remove(DshRenderEntryId::Partial { turn: 1, step: 1 })
    )));
    assert!(updates.iter().any(|update| matches!(
        update,
        DshRenderUpdate::Upsert(DshRenderEntry { id: DshRenderEntryId::Event { seq: 2 }, text, .. }) if text == "final"
    )));
}

#[test]
fn session_fixture_deduplicates_replay_live_and_rejects_stale_generation() {
    let mut state = SessionState::new("s".into(), 4);
    state
        .install_initial(SessionHistoryValue {
            events: vec![history(
                0,
                "assistant/message",
                json!({ "message": { "content": [{ "type": "text", "text": "baseline" }] } }),
            )],
            has_more: false,
            projections: None,
        })
        .unwrap();

    let duplicate = mux(
        "s",
        Some(4),
        "session/event",
        json!({ "event": history(0, "assistant/message", json!({ "message": { "content": [{ "type": "text", "text": "duplicate" }] } })).event }),
    );
    assert!(!state.accept_notification(duplicate).unwrap().changed);
    assert_eq!(state.history().len(), 1);

    let live = mux(
        "s",
        Some(4),
        "session/event",
        json!({ "event": history(1, "assistant/message", json!({ "message": { "content": [{ "type": "text", "text": "live" }] } })).event }),
    );
    assert!(state.accept_notification(live).unwrap().changed);
    assert_eq!(state.history().len(), 2);

    let stale = mux("s", Some(3), "session/queue", json!({ "items": [] }));
    assert!(!state.accept_notification(stale).unwrap().changed);
    assert!(state
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "stale-generation"));
}

#[test]
fn authoritative_queue_fixture_advances_revision_for_identical_snapshots() {
    let mut state = SessionState::new("s".into(), 1);
    let note = mux(
        "s",
        Some(1),
        "session/queue",
        json!({
            "items": [{
                "id": "q1",
                "placement": "queued",
                "message": { "content": [{ "type": "text", "text": "queued" }] }
            }]
        }),
    );
    assert!(state.accept_notification(note.clone()).unwrap().changed);
    assert_eq!(state.queue_revision(), 1);
    assert!(!state.accept_notification(note).unwrap().changed);
    assert_eq!(state.queue_revision(), 2);
    assert_eq!(
        state.presentation_controls().queue[0]
            .content
            .editable_text
            .as_deref(),
        Some("queued")
    );
}
