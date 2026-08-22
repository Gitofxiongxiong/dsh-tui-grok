use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use dsh_pager_test_support::{run_with_timeout, TestSandbox};

fn mock_server() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mock-server.mjs")
}

fn pager_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dsh-pager"))
}

#[test]
fn hello_against_mock_server_exits_zero() {
    let output = Command::new(pager_bin())
        .args([
            "--hello",
            "--backend",
            "node",
            "--backend-arg",
            mock_server().to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("spawn dsh-pager");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.contains("tui.hello ok"), "{stderr}");
    assert!(stderr.contains("clientId=client-mock"), "{stderr}");
    assert!(stderr.contains("resumeClass=baseline-required"), "{stderr}");
}

#[test]
fn load_barrier_against_mock_server_exits_zero() {
    let output = Command::new(pager_bin())
        .args([
            "--load-only",
            "--backend",
            "node",
            "--backend-arg",
            mock_server().to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("spawn dsh-pager");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.contains("tui.hello ok"), "{stderr}");
    assert!(
        stderr.contains("SessionLoaded sessionId=session-mock events=4 seq=0..3"),
        "{stderr}"
    );
}

#[test]
fn prompt_approval_question_round_trip_against_mock_server_exits_zero() {
    let output = Command::new(pager_bin())
        .args([
            "--smoke-interactions",
            "--backend",
            "node",
            "--backend-arg",
            mock_server().to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("spawn dsh-pager");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.contains("interaction smoke ok"), "{stderr}");
}

#[test]
fn session_search_selects_the_host_returned_match() {
    let output = Command::new(pager_bin())
        .args([
            "--load-only",
            "--session-search",
            "history",
            "--backend",
            "node",
            "--backend-arg",
            mock_server().to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("spawn dsh-pager");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.contains("sessionId=session-mock"), "{stderr}");
}

#[test]
fn queue_mutations_converge_on_authoritative_snapshots() {
    let output = Command::new(pager_bin())
        .args([
            "--smoke-queue",
            "--backend",
            "node",
            "--backend-arg",
            mock_server().to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("spawn dsh-pager");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.contains("queue smoke ok"), "{stderr}");
}

#[test]
fn session_lifecycle_smoke_round_trips_rename_and_fork() {
    let output = Command::new(pager_bin())
        .args([
            "--smoke-lifecycle",
            "--backend",
            "node",
            "--backend-arg",
            mock_server().to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("spawn dsh-pager");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.contains("lifecycle smoke ok"), "{stderr}");
    assert!(stderr.contains("session-forked"), "{stderr}");
}

#[test]
fn dashboard_mode_lists_host_sessions_with_derived_title_and_status() {
    let output = Command::new(pager_bin())
        .args([
            "--dashboard",
            "--backend",
            "node",
            "--backend-arg",
            mock_server().to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("spawn dsh-pager");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.contains("Dashboard"), "{stderr}");
    assert!(stderr.contains("title=\"Mock session\""), "{stderr}");
    assert!(stderr.contains("status=idle"), "{stderr}");
}

#[test]
fn shared_test_support_runs_the_real_binary_in_a_hermetic_sandbox() {
    let sandbox = TestSandbox::new().expect("sandbox");
    let mut command = sandbox.command(pager_bin());
    command.args([
        "--hello",
        "--backend",
        "node",
        "--backend-arg",
        mock_server().to_str().expect("utf-8 path"),
    ]);
    let output = run_with_timeout(&mut command, Duration::from_secs(5))
        .expect("hello must finish within the test deadline");
    assert!(output.status.success(), "stderr: {}", output.stderr);
    assert!(output.stderr.contains("tui.hello ok"));
}
