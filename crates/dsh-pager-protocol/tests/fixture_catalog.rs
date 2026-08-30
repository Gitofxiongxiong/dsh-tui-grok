use dsh_pager_protocol::{JsonRpcLine, TUI_PROTOCOL_VERSION, parse_line};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    serde_json::from_str(&fs::read_to_string(path).expect("fixture file")).expect("fixture json")
}

#[test]
fn canonical_catalog_matches_rust_protocol_facts() {
    let version = fixture("protocol-version.json");
    assert_eq!(
        version["tuiProtocolVersion"].as_u64(),
        Some(u64::from(TUI_PROTOCOL_VERSION))
    );

    for name in ["hello-request.json", "hello-result.json"] {
        let expected = fixture(name);
        let parsed: JsonRpcLine = parse_line(&expected.to_string()).expect("hello line");
        assert_eq!(
            serde_json::to_value(parsed).expect("serialize hello"),
            expected
        );
    }

    let catalog = fixture("method-catalog.json");
    let mut methods = BTreeSet::new();
    for group in ["unary", "control", "notification"] {
        for method in catalog[group].as_array().expect("method group") {
            methods.insert(method.as_str().expect("method string"));
        }
    }
    assert_eq!(methods.len(), 65);

    // Every method currently emitted or consumed by crates/dsh-pager must be
    // present in the canonical catalog; Rust need not duplicate all 65 names.
    for method in [
        "agentPreset.list",
        "agentPreset.select",
        "commands/execute",
        "commands/list",
        "credentials.describe",
        "credentials.set",
        "events.host",
        "events.mux",
        "fileReferences.list",
        "session.attachment",
        "session.cancel",
        "session.create",
        "session.fork",
        "session.history",
        "session.list",
        "session.models",
        "session.prompt",
        "session.rename",
        "session.search",
        "session.selectModel",
        "session.updateQueue",
        "subagent.history",
        "subagent.interrupt",
        "subagent.list",
        "subagent.prompt",
        "tui.attach",
        "tui.controlPlaneBaseline",
        "tui.detach",
        "tui.hello",
        "tui.respond",
        "tui.serverDraining",
        "tui.subscribe",
        "workspace.archiveSession",
        "workspace.insertBefore",
        "workspace.insertSessionBefore",
        "workspace.list",
    ] {
        assert!(
            methods.contains(method),
            "missing Rust-used method {method}"
        );
    }
}
