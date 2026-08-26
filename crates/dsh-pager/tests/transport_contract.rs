use std::thread;
use std::time::Duration;

use dsh_pager::RpcTransport;
use dsh_pager_test_support::NodeStdioMock;

fn spawn_mock(source: &str) -> (RpcTransport, NodeStdioMock) {
    let mock = NodeStdioMock::write(source).expect("write node protocol mock");
    let transport = RpcTransport::spawn(mock.program(), &[mock.script_arg()])
        .expect("spawn node protocol mock");
    (transport, mock)
}

#[test]
fn transport_ignores_malformed_noise_and_preserves_notifications() {
    let (mut transport, _mock) = spawn_mock(
        r#"
import { createInterface } from 'node:readline'

process.stdout.write('not-json\n')
process.stdout.write('{"jsonrpc":"2.0","method":"events.host","params":{"type":"host/session-status","sessionId":"s","running":true}}\n')

const rl = createInterface({ input: process.stdin })
rl.on('line', (line) => {
  if (line.includes('tui.hello')) {
    process.stdout.write('{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"clientId":"client-test","generation":1,"resumeClass":"baseline-required","serverInfo":{"name":"deepseek-harness-tui","version":"test"}}}\n')
  }
})
"#,
    );
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
    let (mut transport, _mock) = spawn_mock(
        r#"
import { createInterface } from 'node:readline'

let count = 0
const rl = createInterface({ input: process.stdin })
rl.on('line', () => {
  count += 1
  if (count === 2) {
    process.stdout.write('{"jsonrpc":"2.0","id":2,"result":{"value":"second"}}\n')
    process.stdout.write('{"jsonrpc":"2.0","method":"events.host","params":{"type":"host/session-status","sessionId":"s","running":false}}\n')
    process.stdout.write('{"jsonrpc":"2.0","id":1,"result":{"value":"first"}}\n')
  }
})
"#,
    );
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
