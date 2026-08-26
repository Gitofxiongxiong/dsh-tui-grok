use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dsh_pager_protocol::{
    embedded_hello_params, encode_request_line, parse_line, rpc_request, JsonRpcLine,
    JsonRpcNotification, TuiHelloResult, TUI_PROTOCOL_VERSION, TUI_SERVER_INFO_NAME,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::control_plane::ControlPlaneRouter;
use crate::error::{PagerError, PagerResult};
use crate::session::{SessionState, SessionUpdate};

const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const STDERR_TAIL_BYTES: usize = 32 * 1024;

const NESTED_BACKEND_NAMES: &[&str] = &[
    "dsh-pager",
    "dsh-pager.exe",
    "dsh-pager.cmd",
    "dsh-pager.js",
];

enum ReaderMessage {
    Frame(JsonRpcLine),
    Error(String),
    Closed,
}

/// Last N bytes of backend stderr. Live bytes never go to the user TTY.
#[derive(Debug, Default)]
struct StderrTail {
    bytes: Vec<u8>,
}

impl StderrTail {
    fn extend(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() > STDERR_TAIL_BYTES {
            let excess = self.bytes.len() - STDERR_TAIL_BYTES;
            self.bytes.drain(..excess);
        }
    }

    fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

fn lock_stderr_tail(tail: &Mutex<StderrTail>) -> std::sync::MutexGuard<'_, StderrTail> {
    tail.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn drain_backend_stderr(mut stderr: ChildStderr, tail: Arc<Mutex<StderrTail>>) {
    let mut buf = [0u8; 4096];
    loop {
        match stderr.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => lock_stderr_tail(&tail).extend(&buf[..n]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

fn emit_backend_stderr_tail(tail: &str) {
    let trimmed = tail.trim_end();
    if trimmed.is_empty() {
        return;
    }
    eprintln!("--- dsh-pager: backend stderr (tail) ---");
    eprintln!("{trimmed}");
}

/// Basename using both `/` and `\` so Windows paths validate on Unix tests too.
pub fn backend_basename(program: &str) -> &str {
    program
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(program)
}

/// `DSH_PAGER_ALLOW_NESTED=1` disables T4 already-pager and T5 nested-basename
/// checks only. It never allows `.cmd`/`.bat`.
pub fn nested_backend_allowed() -> bool {
    std::env::var("DSH_PAGER_ALLOW_NESTED").as_deref() == Ok("1")
}

/// Reject nested pager binaries and Windows script shims before `Command::new`.
///
/// Grok's `plan_stdio_spawn` (`xai-grok-mcp/src/servers.rs` ~4200) documents
/// that Windows `CreateProcessW` only appends `.exe`, ignores `PATHEXT`, and
/// that std will then run `.cmd`/`.bat` through `cmd.exe`. That is the recipe
/// for npm launchers such as `npx.cmd`. It is the reason this pager **refuses**
/// `.cmd`/`.bat` (CVE-2024-24576): the product backend is PE `node`/`node.exe`
/// plus an absolute `lib/bin.js`. Do not PATHEXT-search `.cmd`.
pub fn validate_backend_program(program: &str, allow_nested: bool) -> PagerResult<()> {
    let basename = backend_basename(program).to_ascii_lowercase();
    if !allow_nested && NESTED_BACKEND_NAMES.contains(&basename.as_str()) {
        return Err(PagerError::new(format!(
            "refusing nested dsh-pager backend `{program}`; pass node/node.exe and an absolute lib/bin.js (set DSH_PAGER_ALLOW_NESTED=1 only for tests)"
        )));
    }
    if basename.ends_with(".cmd") || basename.ends_with(".bat") {
        return Err(PagerError::new(format!(
            "refusing Windows script backend `{program}`; pass node.exe and an absolute lib/bin.js (Rust will not CreateProcess .cmd/.bat or wrap cmd.exe)"
        )));
    }
    Ok(())
}

/// A spawned backend with a persistent JSON-RPC reader.
pub struct RpcTransport {
    program: String,
    program_args: Vec<String>,
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<ReaderMessage>,
    reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
    stderr_tail: Arc<Mutex<StderrTail>>,
    failed: bool,
    notifications: VecDeque<JsonRpcNotification>,
    pending: HashMap<u64, String>,
    completed: HashMap<u64, PagerResult<Value>>,
    next_id: u64,
    client_id: Option<String>,
    control_plane: ControlPlaneRouter,
}

impl RpcTransport {
    pub fn spawn(program: &str, args: &[String]) -> PagerResult<Self> {
        validate_backend_program(program, nested_backend_allowed())?;
        // Never shell:true / cmd.exe. Product argv is PE node + absolute bin.js.
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                PagerError::new(format!(
                    "failed to spawn backend {program} {}: {error}",
                    args.join(" ")
                ))
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| PagerError::new("backend stdin was not piped"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PagerError::new("backend stdout was not piped"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| PagerError::new("backend stderr was not piped"))?;
        let stderr_tail = Arc::new(Mutex::new(StderrTail::default()));
        let tail_for_thread = Arc::clone(&stderr_tail);
        // Pipe + drain: inherit paints the alternate screen on every OS. An
        // undrained pipe deadlocks Node once the OS buffer fills. Grok's
        // `xai-tty-utils::restore_native_stderr` (pager `app/mod.rs`,
        // `signal_handler.rs`) restores the real stderr after leaving the
        // TUI; we apply the same idea here and only emit a bounded failure
        // tail from Drop after the terminal is released (or on --hello /
        // --load-only process exit).
        let stderr_reader = match thread::Builder::new()
            .name("dsh-pager-stderr-drain".into())
            .spawn(move || drain_backend_stderr(stderr, tail_for_thread))
        {
            Ok(handle) => handle,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PagerError::new(format!(
                    "failed to start stderr drain: {error}"
                )));
            }
        };
        let (tx, rx) = mpsc::channel();
        let reader = match thread::Builder::new()
            .name("dsh-pager-rpc-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => {
                            let _ = tx.send(ReaderMessage::Closed);
                            return;
                        }
                        Ok(_) => {
                            let wire_line = line.trim_end_matches(['\r', '\n']).trim();
                            if wire_line.is_empty() {
                                continue;
                            }
                            // The TypeScript carrier treats malformed lines as noise on a
                            // long-lived stream. Keep the connection alive so one diagnostic
                            // line cannot destroy an otherwise recoverable session.
                            let Ok(frame) = parse_line(wire_line) else {
                                continue;
                            };
                            if tx.send(ReaderMessage::Frame(frame)).is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            let _ = tx.send(ReaderMessage::Error(format!(
                                "failed reading backend stdout: {error}"
                            )));
                            return;
                        }
                    }
                }
            }) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_reader.join();
                return Err(PagerError::new(format!(
                    "failed to start RPC reader: {error}"
                )));
            }
        };
        Ok(Self {
            program: program.to_string(),
            program_args: args.to_vec(),
            child,
            stdin,
            rx,
            reader: Some(reader),
            stderr_reader: Some(stderr_reader),
            stderr_tail,
            failed: false,
            notifications: VecDeque::new(),
            pending: HashMap::new(),
            completed: HashMap::new(),
            next_id: 1,
            client_id: None,
            control_plane: ControlPlaneRouter::default(),
        })
    }

    /// Bounded stderr captured from the backend. Empty on the success path.
    pub fn backend_stderr_tail(&self) -> String {
        lock_stderr_tail(&self.stderr_tail).to_string_lossy()
    }

    fn reap_child(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }

    fn emit_failure_stderr_tail(&self) {
        if !self.failed {
            return;
        }
        emit_backend_stderr_tail(&self.backend_stderr_tail());
    }

    /// Replace a failed backend process with a fresh stdio connection.
    ///
    /// The caller owns the protocol baseline: after this returns it must send
    /// `tui.hello`, observe `baseline-required`, attach, and reload history.
    pub fn reconnect(&mut self) -> PagerResult<()> {
        let client_id = self.client_id.clone();
        let replacement = Self::spawn(&self.program, &self.program_args)?;
        self.reap_child();
        *self = replacement;
        self.client_id = client_id;
        Ok(())
    }

    pub fn hello(&mut self, cwd: String) -> PagerResult<TuiHelloResult> {
        let mut params = embedded_hello_params(cwd);
        params.client_id = self.client_id.clone();
        let result: TuiHelloResult = self.call("tui.hello", &params)?;
        if result.protocol_version != TUI_PROTOCOL_VERSION {
            return Err(PagerError::new(format!(
                "protocol version mismatch: got {}",
                result.protocol_version
            )));
        }
        if result.server_info.name != TUI_SERVER_INFO_NAME {
            return Err(PagerError::new(format!(
                "unexpected TUI server: {}",
                result.server_info.name
            )));
        }
        self.client_id = Some(result.client_id.clone());
        self.control_plane.set_generation(result.generation);
        Ok(result)
    }

    /// Client identity reused on reconnecting hello handshakes.
    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    /// Read the all-session control-plane mirror populated by every incoming
    /// mux/host notification, including frames for unattached sessions.
    pub fn control_plane(&self) -> &crate::control_plane::ControlPlaneStore {
        &self.control_plane.store
    }

    /// Mutable access for a loader baseline or explicit generation reset.
    pub fn control_plane_mut(&mut self) -> &mut ControlPlaneRouter {
        &mut self.control_plane
    }

    /// Route one queued notification through the store and current session.
    pub fn route_notification(
        &mut self,
        state: &mut SessionState,
        notification: JsonRpcNotification,
    ) -> PagerResult<SessionUpdate> {
        self.control_plane.route(notification, Some(state))
    }

    pub fn call<P, T>(&mut self, method: &str, params: &P) -> PagerResult<T>
    where
        P: Serialize,
        T: DeserializeOwned,
    {
        let id = self.begin_call_value(method, serde_json::to_value(params)?)?;

        loop {
            if let Some(result) = self.completed.remove(&id) {
                return result
                    .and_then(|value| serde_json::from_value(value).map_err(PagerError::from));
            }
            let message = match self.rx.recv_timeout(RPC_TIMEOUT) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => {
                    return self.fail(format!("timed out waiting for {method} response"));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return self.fail(format!(
                        "backend disconnected while waiting for {method}"
                    ));
                }
            };
            self.process_message(message)?;
        }
    }

    /// Start a JSON-RPC request without waiting for its response.
    ///
    /// The request id is local to this transport connection. Callers must
    /// retain it and use poll_call_value to collect the result.
    /// Notifications and responses may arrive in any order.
    pub fn begin_call_value(&mut self, method: &str, params: Value) -> PagerResult<u64> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let request = rpc_request(id, method, Some(params));
        let line = encode_request_line(&request)?;
        if let Err(error) = self
            .stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.flush())
        {
            self.mark_failed();
            return Err(error.into());
        }
        self.pending.insert(id, method.to_string());
        Ok(id)
    }

    fn mark_failed(&mut self) {
        self.failed = true;
    }

    fn fail<T>(&mut self, message: impl Into<String>) -> PagerResult<T> {
        self.mark_failed();
        Err(PagerError::new(message))
    }

    /// Poll one previously started request. This method never waits on the
    /// backend; None means the response has not arrived yet.
    pub fn poll_call_value(&mut self, id: u64) -> PagerResult<Option<Value>> {
        self.pump_nonblocking()?;
        let Some(result) = self.completed.remove(&id) else {
            return Ok(None);
        };
        result.map(Some)
    }

    fn pump_nonblocking(&mut self) -> PagerResult<()> {
        loop {
            match self.rx.try_recv() {
                Ok(message) => self.process_message(message)?,
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    return self.fail("backend reader stopped");
                }
            }
        }
    }

    fn process_message(&mut self, message: ReaderMessage) -> PagerResult<()> {
        match message {
            ReaderMessage::Frame(JsonRpcLine::Notification(note)) => {
                self.notifications.push_back(note);
                Ok(())
            }
            ReaderMessage::Frame(JsonRpcLine::Success(success)) => {
                let id = response_id(&success.id)?;
                let method = self.pending.remove(&id).ok_or_else(|| {
                    PagerError::new(format!("response received for unknown request id {id}"))
                })?;
                let _ = method;
                self.completed.insert(id, Ok(success.result));
                Ok(())
            }
            ReaderMessage::Frame(JsonRpcLine::Failure(failure)) => {
                let id = response_id(&failure.id)?;
                let method = self.pending.remove(&id).ok_or_else(|| {
                    PagerError::new(format!("response received for unknown request id {id}"))
                })?;
                self.completed.insert(
                    id,
                    Err(PagerError::new(format!(
                        "{} failed: {}",
                        method, failure.error
                    ))),
                );
                Ok(())
            }
            ReaderMessage::Frame(other) => Err(PagerError::new(format!(
                "unexpected request frame from backend: {other:?}"
            ))),
            ReaderMessage::Error(error) => self.fail(error),
            ReaderMessage::Closed => self.fail("backend closed stdout"),
        }
    }

    pub fn try_notification(&mut self) -> PagerResult<Option<JsonRpcNotification>> {
        if let Some(note) = self.notifications.pop_front() {
            return Ok(Some(note));
        }
        self.pump_nonblocking()?;
        Ok(self.notifications.pop_front())
    }

    pub fn call_value(&mut self, method: &str, params: Value) -> PagerResult<Value> {
        self.call(method, &params)
    }
}

fn response_id(value: &Value) -> PagerResult<u64> {
    value
        .as_u64()
        .ok_or_else(|| PagerError::new(format!("JSON-RPC response id is not an integer: {value}")))
}

impl Drop for RpcTransport {
    fn drop(&mut self) {
        self.reap_child();
        self.emit_failure_stderr_tail();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_splits_unix_and_windows_separators() {
        assert_eq!(backend_basename("dsh-pager"), "dsh-pager");
        assert_eq!(backend_basename("/usr/bin/dsh-pager.exe"), "dsh-pager.exe");
        assert_eq!(
            backend_basename(r"C:\Program Files\dsh-pager.cmd"),
            "dsh-pager.cmd"
        );
        assert_eq!(backend_basename("C:/tools/dsh.bat"), "dsh.bat");
    }

    #[test]
    fn nested_pager_basenames_fail_closed() {
        for program in [
            "dsh-pager",
            "dsh-pager.exe",
            "DSH-PAGER.EXE",
            "/usr/bin/dsh-pager.js",
            r"C:\npm\dsh-pager.cmd",
        ] {
            let error = validate_backend_program(program, false).expect_err(program);
            assert!(
                error.to_string().contains("nested dsh-pager"),
                "{program}: {error}"
            );
        }
    }

    #[test]
    fn allow_nested_does_not_open_cmd_or_bat() {
        validate_backend_program("dsh-pager", true).expect("nested name allowed");
        validate_backend_program("dsh-pager.exe", true).expect("exe nested name allowed");
        validate_backend_program("dsh-pager.js", true).expect("js nested name allowed");
        let cmd = validate_backend_program("dsh-pager.cmd", true)
            .expect_err("cmd remains refused under ALLOW_NESTED");
        assert!(cmd.to_string().contains("node.exe"), "{cmd}");
        let bat = validate_backend_program("backend.bat", true).expect_err("bat");
        assert!(bat.to_string().contains("lib/bin.js"), "{bat}");
    }

    #[test]
    fn cmd_and_bat_backends_are_rejected() {
        for program in [
            "dsh.cmd",
            "DSH.CMD",
            r"C:\npm\dsh.cmd",
            "run.bat",
            "/tmp/run.BAT",
        ] {
            let error = validate_backend_program(program, false).expect_err(program);
            assert!(
                error.to_string().contains("Windows script backend"),
                "{program}: {error}"
            );
        }
        validate_backend_program("node", false).expect("node");
        validate_backend_program("node.exe", false).expect("node.exe");
        validate_backend_program(r"C:\Program Files\nodejs\node.exe", false)
            .expect("absolute node.exe");
    }

    fn spawn_err(program: &str) -> PagerError {
        match RpcTransport::spawn(program, &[]) {
            Ok(_) => panic!("spawn should have been rejected: {program}"),
            Err(error) => error,
        }
    }

    #[test]
    fn spawn_rejects_cmd_without_createprocess() {
        let error = spawn_err("dsh.cmd");
        assert!(
            error.to_string().contains("Windows script backend"),
            "{error}"
        );
    }

    #[test]
    fn spawn_rejects_nested_pager_basename() {
        let error = spawn_err("dsh-pager");
        assert!(error.to_string().contains("nested dsh-pager"), "{error}");
    }
}
