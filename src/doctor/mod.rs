//! Read-only local readiness diagnostics for `jefe doctor` (issue #264).
//!
//! This module owns the typed diagnostic domain model and the pure rendering
//! pipeline. It is deliberately split across small focused files so no single
//! file grows past the project's 750-line source ceiling and each concern
//! (classification, redaction, persistence probing, report rendering, and
//! runtime collection) stays independently testable.
//!
//! # Decision context
//!
//! - Exit contract (D-04): required startup blockers (`Multiplexer`,
//!   `ConPty` on Windows, `Persistence`) map to exit 2; optional findings
//!   (`Git`, `GhAuth`, agent runtimes) warn and still exit 0; an internal
//!   diagnostic failure (`DiagnosticsInternal`) dominates with exit 1.
//! - Read-only (D-05): the persistence probe never initializes a missing
//!   config directory and never mutates real settings/state files.
//! - Redaction (AC-09): every evidence string flows through [`redact_value`]
//!   before it reaches rendered output.
//!
//! The runtime collection in [`collect`] consumes existing public primitives
//! (multiplexer plan, local-tool resolver, GitHub auth check, agent-runtime
//! resolver) without widening their ownership boundaries.

mod classification;
mod collection;
mod persistence_probe;
mod redaction;
mod report;
mod types;
mod windows_probe;

pub use classification::{DoctorOutcome, ExitCode, classify_doctor};
pub use collection::collect;
pub use persistence_probe::{PersistenceProbeOutcome, probe_persistence};
pub use redaction::redact_value;
pub use report::{DoctorReport, render_report};
pub use types::{DiagnosticFinding, DiagnosticStatus, FindingKind};
pub use windows_probe::{LongPathPolicy, long_path_finding, terminal_host_evidence};
