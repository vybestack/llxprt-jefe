//! JSP — Jefe Stream Protocol external wire boundary (issue #476).
//!
//! JSP is the closed JSON wire contract between a Jefe agent observer and
//! external producers/brokers. The `v1` module owns the schema-1 snapshot
//! parser/validator. It depends only on `domain::observation`, the standard
//! library, and existing `serde`/`serde_json`. Private closed wire DTOs convert
//! to typed domain values only after complete validation.
//!
//! This crate root re-exports the public parsing surface. The internal wire
//! DTOs, limits, and helpers are intentionally not re-exported: the public API
//! is minimal and strongly typed.

pub mod v1;

pub use v1::Snapshot;
pub use v1::error::{JspCode, JspError};
pub use v1::parse_snapshot;
