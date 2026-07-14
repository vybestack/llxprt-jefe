//! Tutorial-capture workflow: reusable agent-driven documentation capture.
//!
//! This crate builds on the `jefe` harness to provide a safe, repeatable
//! documentation-capture workflow. It is a standalone workspace member with a
//! one-way path dependency on the `jefe` library crate — it consumes jefe's
//! public APIs but does not pollute the root package with tutorial-specific
//! code or dependencies.
//!
//! ## Architecture boundaries
//!
//! - `manifest`, `path_shim`, `allowlist`, `redaction`, `report` are pure
//!   data/planning layers with no I/O.
//! - `orchestration` owns filesystem setup/teardown and delegates pure
//!   decisions to the above modules.
//! - The CLI binary (`jefe-tutorial-capture`) composes orchestration with the
//!   harness runner.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-001

pub mod tutorial_capture;

pub use tutorial_capture::*;
