use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
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

enum ReaderMessage {
    Frame(JsonRpcLine),
    Error(String),
    Closed,
}

/// A spawned backend with a persistent JSON-RPC reader.
pub struct RpcTransport {
    program: String,
    program_args: Vec<String>,
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<ReaderMessage>,
    reader: Option<JoinHandle<()>>,
    notifications: VecDeque<JsonRpcNotification>,
    next_id: u64,
    client_id: Option<String>,
    control_plane: ControlPlaneRouter,
}

impl RpcTransport {
    pub fn spawn(program: &str, args: &[String]) -> PagerResult<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
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
        let (tx, rx) = mpsc::channel();
        let reader = thread::Builder::new()
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
            })
            .map_err(|error| PagerError::new(format!("failed to start RPC reader: {error}")))?;
        Ok(Self {
            program: program.to_string(),
            program_args: args.to_vec(),
            child,
            stdin,
            rx,
            reader: Some(reader),
            notifications: VecDeque::new(),
            next_id: 1,
            client_id: None,
            control_plane: ControlPlaneRouter::default(),
        })
    }

    /// Replace a failed backend process with a fresh stdio connection.
    ///
    /// The caller owns the protocol baseline: after this returns it must send
    /// `tui.hello`, observe `baseline-required`, attach, and reload history.
    pub fn reconnect(&mut self) -> PagerResult<()> {
        let client_id = self.client_id.clone();
        let replacement = Self::spawn(&self.program, &self.program_args)?;
        let old_reader = self.reader.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = old_reader {
            let _ = reader.join();
        }
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
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let request = rpc_request(id, method, Some(serde_json::to_value(params)?));
        self.stdin
            .write_all(encode_request_line(&request)?.as_bytes())?;
        self.stdin.flush()?;

        loop {
            let message = match self.rx.recv_timeout(RPC_TIMEOUT) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(PagerError::new(format!(
                        "timed out waiting for {method} response"
                    )))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(PagerError::new(format!(
                        "backend disconnected while waiting for {method}"
                    )))
                }
            };
            match message {
                ReaderMessage::Frame(JsonRpcLine::Notification(note)) => {
                    self.notifications.push_back(note);
                }
                ReaderMessage::Frame(JsonRpcLine::Success(success)) if success.id == request.id => {
                    return serde_json::from_value(success.result).map_err(PagerError::from);
                }
                ReaderMessage::Frame(JsonRpcLine::Failure(failure)) if failure.id == request.id => {
                    return Err(PagerError::new(format!(
                        "{method} failed: {}",
                        failure.error
                    )))
                }
                ReaderMessage::Frame(other) => {
                    return Err(PagerError::new(format!(
                        "unexpected frame while waiting for {method}: {other:?}"
                    )))
                }
                ReaderMessage::Error(error) => return Err(PagerError::new(error)),
                ReaderMessage::Closed => {
                    return Err(PagerError::new(format!(
                        "backend closed stdout while waiting for {method}"
                    )))
                }
            }
        }
    }

    pub fn try_notification(&mut self) -> PagerResult<Option<JsonRpcNotification>> {
        if let Some(note) = self.notifications.pop_front() {
            return Ok(Some(note));
        }
        match self.rx.try_recv() {
            Ok(ReaderMessage::Frame(JsonRpcLine::Notification(note))) => Ok(Some(note)),
            Ok(ReaderMessage::Frame(other)) => Err(PagerError::new(format!(
                "unexpected frame without an outstanding request: {other:?}"
            ))),
            Ok(ReaderMessage::Error(error)) => Err(PagerError::new(error)),
            Ok(ReaderMessage::Closed) => Err(PagerError::new("backend closed stdout")),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(PagerError::new("backend reader stopped")),
        }
    }

    pub fn call_value(&mut self, method: &str, params: Value) -> PagerResult<Value> {
        self.call(method, &params)
    }
}

impl Drop for RpcTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
