//! Schema-1 deterministic real-process TUI harness (issue #380).
//!
//! This module owns the closed schema-1 scenario contract end to end:
//! bounded strict JSON parsing, typed validation, `${workspace}`
//! interpolation, contained workspaces, capture shims, the synchronous
//! runner, and the deterministic report. Schema-1 input is the only accepted
//! format; there is no legacy adapter or compatibility shim, by explicit
//! project decision (see issue #380 and the CW-00b migration issue #397).

pub mod capture;
pub mod contract;
pub mod env;
pub mod error;
pub mod fields;
pub mod interp;
pub mod json;
pub mod limits;
pub mod parse;
pub mod parse_step;
pub mod redact;
pub mod semantic;
pub mod validate;
#[cfg(unix)]
pub mod workspace;

pub use contract::{ScenarioV1, Step};
pub use error::{HarCode, HarnessError};
pub use parse::parse_scenario_v1;
