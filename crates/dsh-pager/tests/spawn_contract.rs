use std::fs;
use std::time::{Duration, Instant};

use dsh_pager::{validate_backend_program, RpcTransport};
use dsh_pager_test_support::TestSandbox;

const HELLO_SCRIPT: &str = r#"
import { writeSync } from 'node:fs'
import { createInterface } from 'node:readline'

writeSync(2, 'Y'.repeat(128 * 1024))
writeSync(2, '\nSTDERR_FLOOD_DONE\n')

const rl = createInterface({ input: process.stdin })
rl.on('line', (line) => {
  if (line.includes('tui.hello')) {
    process.stdout.write(
      '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"clientId":"client-stderr","generation":1,"resumeClass":"baseline-required","serverInfo":{"name":"deepseek-harness-tui","version":"test"}}}\n',
    )
  }
})
"#;

#[test]
fn validate_backend_rejects_cmd_bat_and_nested_pager() {
    for program in ["dsh.cmd", "run.bat", r"C:\npm\dsh.cmd"] {
        let error = validate_backend_program(program, false).expect_err(program);
        assert!(
            error.to_string().contains("Windows script backend"),
            "{program}: {error}"
        );
    }
    let nested = validate_backend_program("dsh-pager.exe", false).expect_err("nested");
    assert!(nested.to_string().contains("nested dsh-pager"), "{nested}");
    validate_backend_program("node.exe", false).expect("node.exe is the product backend");
}

#[test]
fn stderr_flood_is_drained_and_does_not_deadlock_hello() {
    let sandbox = TestSandbox::new().expect("sandbox");
    let script = sandbox.root().join("stderr-flood.mjs");
    fs::write(&script, HELLO_SCRIPT).expect("write flood backend");
    let started = Instant::now();
    let mut transport = RpcTransport::spawn("node", &[script.to_string_lossy().into_owned()])
        .expect("spawn node flood backend");
    let hello = transport
        .hello("/work".into())
        .expect("hello despite stderr flood");
    assert_eq!(hello.client_id, "client-stderr");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "stderr pipe must be drained; hello took {:?}",
        started.elapsed()
    );
    let tail = transport.backend_stderr_tail();
    assert!(
        tail.contains("STDERR_FLOOD_DONE"),
        "bounded tail should keep the end of the flood, got {} bytes",
        tail.len()
    );
}
