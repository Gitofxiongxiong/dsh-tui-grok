use std::io;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum ProcessError {
    Spawn(io::Error),
    Io(io::Error),
    Timeout {
        command: String,
        output: Option<Output>,
    },
}

impl std::fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "spawn test process: {error}"),
            Self::Io(error) => write!(formatter, "test process I/O: {error}"),
            Self::Timeout { command, .. } => write!(formatter, "test process timed out: {command}"),
        }
    }
}

impl std::error::Error for ProcessError {}

/// Run a bounded test process and always reap it. This helper deliberately
/// keeps the contract small; PTY/process-tree tests should use their richer
/// harness and attach descendants explicitly.
pub fn run_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<CommandOutput, ProcessError> {
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let description = format_command(command);
    let mut child = command.spawn().map_err(ProcessError::Spawn)?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().map_err(ProcessError::Io)? {
            Some(_) => {
                return child
                    .wait_with_output()
                    .map(CommandOutput::from)
                    .map_err(ProcessError::Io);
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let output = child.wait_with_output().ok();
                return Err(ProcessError::Timeout {
                    command: description,
                    output,
                });
            }
            None => thread::sleep(Duration::from_millis(5)),
        }
    }
}

fn format_command(command: &Command) -> String {
    let program = command.get_program().to_string_lossy();
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    if args.is_empty() {
        program.into_owned()
    } else {
        format!("{program} {args}")
    }
}

#[derive(Debug)]
pub struct CommandOutput {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl From<Output> for CommandOutput {
    fn from(output: Output) -> Self {
        Self {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn bounded_runner_reaps_successful_child_and_captures_output() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf ok"]).current_dir("/");
        let output = run_with_timeout(&mut command, Duration::from_secs(2)).expect("command");
        assert!(output.status.success());
        assert_eq!(output.stdout, "ok");
        assert!(output.stderr.is_empty());
    }
}
