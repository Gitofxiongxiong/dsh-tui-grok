use std::collections::BTreeSet;
use std::fs;

use serde_json::Value;

#[test]
fn m0_parity_manifest_contains_all_required_fallback_scenarios() {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../dsh-pager-test-support/fixtures/parity/"
    );
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(format!("{root}manifest.json")).expect("read parity manifest"),
    )
    .expect("parse parity manifest");
    assert_eq!(manifest["status"], "fallback-baseline");
    let names = manifest["scenarios"]
        .as_array()
        .expect("scenario array")
        .iter()
        .map(|scenario| scenario["name"].as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    let expected = [
        "empty",
        "user-assistant",
        "streaming-tool",
        "queue",
        "approval-question",
        "two-sessions",
        "reconnect",
        "narrow-terminal",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<BTreeSet<_>>();
    assert_eq!(names, expected);
}

#[test]
fn fallback_screen_baselines_cover_wide_and_narrow_terminal_sizes() {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../dsh-pager-test-support/fixtures/parity/"
    );
    for (name, width, height) in [("80x24", 80, 24), ("40x12", 40, 12)] {
        let path = format!("{root}fallback-screen-{name}.json");
        let baseline: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(baseline["status"], "fallback-baseline");
        assert_eq!(baseline["terminal"]["width"], width);
        assert_eq!(baseline["terminal"]["height"], height);
        assert_eq!(baseline["focusOwner"], "prompt");
        assert!(baseline["semanticRows"].as_array().unwrap().len() >= 4);
    }
}
