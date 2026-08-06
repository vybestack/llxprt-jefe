//! Pure, closed JSONL action-provider wire protocol (issue #390 CW-10).
//!
//! This module is the I/O-free wire boundary shared by the (later) handle-free
//! request reducer and the (later) process supervisor. Slice A delivers the
//! closed envelope/payload DTOs, bounded line framing, and the pure progress
//! and lifecycle validators. Nothing here spawns a process, holds application
//! state, emits an effect, or persists anything.

pub mod error;
pub mod framing;
pub mod protocol;

// Private, cohesive implementation modules for the closed protocol layer. They
// are re-exported through `protocol` as the stable public API, and are never
// part of the crate's public surface.
mod dto;
mod identifiers;
mod lifecycle;
mod object_reader;
mod payload_reader;
mod progress;
mod typed_value;

pub use error::{
    FramingFault, PROGRESS_SEQUENCE_MAX, PROTOCOL_FAILURE_CODE, ProgressFault, ProviderError,
};

#[cfg(test)]
#[path = "framing_tests.rs"]
mod framing_tests;

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod protocol_tests;
