#![cfg(unix)]

use std::fs;
use std::thread;
use std::time::Duration;

use dsh_pager::RpcTransport;
use dsh_pager_test_support::TestSandbox;

#[test]
fn transport_ignores_malformed_noise_and_preserves_notifications() {
    let sandbox = TestSandbox::new().expect("sandbox");
    let script = sandbox.root().join("backend.sh");
    fs::write(
        &script,
        r#"#!/bin/sh
printf '%s\n' 'not-json'
printf '%s\n' '{"jsonrpc":"2.0","method":"events.host","params":{"type":"host/session-status","sessionId":"s","running":true}}'
while IFS= read -r line; do
  case "$line" in
    *tui.hello*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"clientId":"client-test","generation":1,"resumeClass":"baseline-required","serverInfo":{"name":"deepseek-harness-tui","version":"test"}}}'
      ;;
  esac
done
"#,
    )
    .expect("write backend");

    let mut transport = RpcTransport::spawn("sh", &[script.to_string_lossy().into_owned()])
        .expect("spawn scripted backend");
    let hello = transport.hello("/work".into()).expect("hello response");
    assert_eq!(hello.client_id, "client-test");
    let notification = transport
        .try_notification()
        .expect("notification read")
        .expect("buffered notification");
    assert_eq!(notification.method, "events.host");
}

#[test]
fn transport_polls_out_of_order_completions_without_losing_notifications() {
    let sandbox = TestSandbox::new().expect("sandbox");
    let script = sandbox.root().join("backend.sh");
    fs::write(
        &script,
        r#"#!/bin/sh
count=0
while IFS= read -r line; do
  count=$((count + 1))
  if [ "$count" -eq 2 ]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"value":"second"}}'
    printf '%s\n' '{"jsonrpc":"2.0","method":"events.host","params":{"type":"host/session-status","sessionId":"s","running":false}}'
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"value":"first"}}'
  fi
done
"#,
    )
    .expect("write backend");
    let mut transport = RpcTransport::spawn("sh", &[script.to_string_lossy().into_owned()])
        .expect("spawn scripted backend");
    let first = transport
        .begin_call_value("first", serde_json::json!({}))
        .expect("begin first");
    let second = transport
        .begin_call_value("second", serde_json::json!({}))
        .expect("begin second");
    let mut first_value = None;
    let mut second_value = None;
    for _ in 0..100 {
        if first_value.is_none() {
            first_value = transport.poll_call_value(first).expect("poll first");
        }
        if second_value.is_none() {
            second_value = transport.poll_call_value(second).expect("poll second");
        }
        if first_value.is_some() && second_value.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(first_value.expect("first completion")["value"], "first");
    assert_eq!(second_value.expect("second completion")["value"], "second");
    assert_eq!(
        transport
            .try_notification()
            .expect("notification")
            .expect("buffered notification")
            .method,
        "events.host"
    );
}
