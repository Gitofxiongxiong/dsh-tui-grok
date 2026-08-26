//! Shared, deterministic test infrastructure for the native DSH pager.
//!
//! The helpers in this crate are intentionally production-independent. They
//! own test resources and make fixture/scenario behavior explicit so tests can
//! be reused by unit, binary, and PTY layers without process-global state.

mod fixture;
mod node_mock;
mod parity;
mod process;
mod sandbox;
mod scenario;
mod screen;

pub use fixture::{read_jsonl, write_jsonl, JsonlFixture};
pub use node_mock::{require_node, utf8_path_arg, NodeStdioMock};
pub use parity::{ParityManifest, ParityReport, ParityScenario, ReferenceMatrix};
pub use process::{run_with_timeout, CommandOutput, ProcessError};
pub use sandbox::TestSandbox;
pub use scenario::{Scenario, ScenarioStep};
pub use screen::{normalize_ansi, visible_lines};
