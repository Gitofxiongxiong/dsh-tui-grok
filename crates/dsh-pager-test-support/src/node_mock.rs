//! Portable Node stdio JSON-RPC mocks for pager protocol tests.
//!
//! DSH's product backend is `node` plus a JS file. Protocol tests that once
//! spawned `sh` + `backend.sh` should write a `.mjs` here and call
//! `RpcTransport::spawn("node", &[script])`, matching
//! `crates/dsh-pager/tests/spawn_contract.rs` and
//! `crates/dsh-pager-bin/tests/mock-server.mjs`. Grok's ConPTY pager harness
//! is not copied.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::TestSandbox;

/// Hermetic temp `.mjs` plus the `node` argv used to spawn it.
///
/// Keep this value alive for as long as the child runs: dropping it deletes
/// the sandbox, including the script file.
pub struct NodeStdioMock {
    sandbox: TestSandbox,
    script: PathBuf,
}

impl NodeStdioMock {
    pub const PROGRAM: &'static str = "node";

    /// Write `source` as `backend.mjs` after checking that `node` is on PATH.
    pub fn write(source: impl AsRef<[u8]>) -> io::Result<Self> {
        require_node()?;
        let sandbox = TestSandbox::new()?;
        let script = sandbox.root().join("backend.mjs");
        fs::write(&script, source)?;
        Ok(Self { sandbox, script })
    }

    /// Reply to every stdin line with the same JSON-RPC line.
    ///
    /// Writes a raw `\n` terminator (not `\r\n`); the pager reader already
    /// strips `\r`.
    pub fn echo_line(response: &str) -> io::Result<Self> {
        let literal = serde_json::to_string(response)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Self::write(format!(
            r#"
import {{ createInterface }} from 'node:readline'
const response = {literal}
const rl = createInterface({{ input: process.stdin }})
rl.on('line', () => {{
  process.stdout.write(response)
  process.stdout.write('\n')
}})
"#
        ))
    }

    pub fn program(&self) -> &'static str {
        Self::PROGRAM
    }

    pub fn script_arg(&self) -> String {
        utf8_path_arg(&self.script)
    }

    pub fn sandbox(&self) -> &TestSandbox {
        &self.sandbox
    }
}

/// Convert a test path to a `--backend-arg` / spawn argv string.
///
/// Cargo target dirs and sandbox roots are UTF-8 in normal CI.
pub fn utf8_path_arg(path: &Path) -> String {
    path.to_str()
        .unwrap_or_else(|| {
            panic!(
                "test path must be UTF-8 for node/backend argv (Cargo target dirs usually are): {}",
                path.display()
            )
        })
        .to_owned()
}

/// Fail with a clear message when Node is missing from PATH.
pub fn require_node() -> io::Result<()> {
    match Command::new("node")
        .args(["-e", "process.exit(0)"])
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(io::Error::other(format!(
            "node is required for protocol mock tests; `node -e process.exit(0)` exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))),
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!(
                "node is required for protocol mock tests (install Node.js and put `node` on PATH): {error}"
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_mjs_under_sandbox() {
        let mock = NodeStdioMock::write("process.exit(0)\n").expect("write mock");
        assert_eq!(mock.program(), "node");
        assert_eq!(
            mock.script_arg(),
            mock.sandbox().root().join("backend.mjs").to_string_lossy()
        );
        assert!(mock.sandbox().root().join("backend.mjs").is_file());
    }
}
