//! Schema-1 deterministic real-process TUI harness (issue #380).
//!
//! This module owns the closed schema-1 scenario contract end to end:
//! bounded strict JSON parsing, typed validation, `${workspace}`
//! interpolation, contained workspaces, capture shims, the synchronous
//! runner, and the deterministic report. Schema-1 input is the only accepted
//! format; there is no legacy adapter or compatibility shim, by explicit
//! project decision (see issue #380 and the CW-00b migration issue #397).

pub mod action_capture;
pub mod action_capture_sink;
#[cfg(test)]
#[path = "action_capture_tests.rs"]
mod action_capture_tests;
// Serves the Unix PTY runner only; gated with it so the Windows build does
// not carry a module whose tests need the Unix-only workspace.
#[cfg(unix)]
pub mod app_socket;
pub mod capture;
pub mod contract;
pub mod env;
pub mod error;
pub mod fields;
pub mod interp;
pub mod json;
pub mod keys;
pub mod limits;
pub mod parse;
pub mod parse_step;
#[cfg(unix)]
pub mod pty;
pub mod redact;
pub mod report;
#[cfg(unix)]
pub mod runner;
pub mod semantic;
#[cfg(unix)]
pub mod signal_cleanup;
pub mod validate;
#[cfg(unix)]
pub mod workspace;

pub use contract::{ScenarioV1, Step};
pub use error::{HarCode, HarnessError};
pub use parse::parse_scenario_v1;
pub use report::Report;
#[cfg(unix)]
pub use runner::{RunOutcome, RunnerConfig, run};
