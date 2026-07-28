//! JSP/1 schema-1 snapshot parser and validator (issue #476, J1 slice).
//!
//! This module freezes the external semantic and wire contract for JSP/1
//! snapshots. It provides a strict typed snapshot validator over a closed JSON
//! envelope. The public entry point is [`parse_snapshot`], which returns a
//! strongly typed [`Snapshot`] or a coded [`JspError`].
//!
//! Architecture:
//! - [`error`] owns the stable error code taxonomy (`JSP-E001`..`JSP-E006`).
//! - [`limits`] owns the inclusive bounds.
//! - [`wire`] owns the private closed wire DTOs.
//! - [`contract`] owns the public typed snapshot and identity.
//! - [`validate`] owns the validation/conversion from wire DTOs to domain.
//! - [`parse`] owns the parser entry point and serde orchestration.
//!
//! The parser performs no I/O and no logging. Diagnostics carry stable
//! code/path/location and never echo producer payload values.

pub mod contract;
pub mod error;
mod event;
mod event_wire;
mod limits;
mod parse;
mod validate;
mod wire;

pub use contract::{Cursor, ObservationKey, Snapshot, SourceSequence};
pub use error::{JspCode, JspError};
pub use event::{parse_event, parse_heartbeat};
pub use parse::parse_snapshot;
