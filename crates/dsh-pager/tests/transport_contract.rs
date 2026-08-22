#![cfg(unix)]

use std::fs;

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
