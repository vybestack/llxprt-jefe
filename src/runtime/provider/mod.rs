//! Pure, closed JSONL action-provider wire protocol (issue #390 CW-10).
//!
//! This module is the I/O-free wire boundary shared by the (later) handle-free
//! request reducer and the (later) process supervisor. Slice A delivers the
//! closed envelope/payload DTOs, bounded line framing, and the pure progress
//! and lifecycle validators. Nothing here spawns a process, holds application
//! state, emits an effect, or persists anything.

pub mod composition;
pub mod coordinator;
pub mod encode;
pub mod environment;
pub mod error;
pub mod framing;
pub mod outbound;
pub mod outcome;
pub mod persistent;
pub mod protocol;
pub mod supervisor;

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

// Private helpers owned by the supervisor (Slice C1): process plumbing, live
// pipe drains, incremental line framing, the lifecycle driver, and recursive
// secret redaction.
mod drains;
mod driver;
mod line_reader;
mod process_tree;
mod redaction;

// Private helper owned by the persistent supervisor (Slice C2): per-candidate
// startup and the closed handshake to `ready`.
mod candidate;

// Persistent invocation sessions (Remediation E): command-owner threads that
// drive repeated same-PID invocations on ready candidates.
mod persistent_session;

pub use persistent_session::{PersistentInvocation, PersistentInvokeError, PersistentSessionOwner};

pub use composition::{CompositionRequest, Containment, ProviderComposition, compose};
pub use coordinator::{ProviderActionDescriptor, ProviderCatalog, ProviderCoordinator};
pub use error::{
    FramingFault, PROGRESS_SEQUENCE_MAX, PROTOCOL_FAILURE_CODE, ProgressFault, ProviderError,
    RUNTIME_UNAVAILABLE_CODE,
};
pub use outbound::{MAX_QUEUED_ENVELOPES, OutboundError, OutboundQueue};
pub use outcome::{
    CleanupFailure, LifecycleTranscript, OneShotOutcome, OneShotResult, SupervisorFailure,
    TranscriptEntry,
};
pub use persistent::{
    CandidateHealth, CandidateHealthSnapshot, PersistentPublication, PersistentStartupFailure,
    PersistentStartupResult, PersistentSupervisor, ReadyCandidate, ReapedCandidate, StartupFailure,
};
pub use supervisor::{OneShotRequest, SupervisorBounds, run_one_shot};

#[cfg(test)]
#[path = "composition_tests.rs"]
mod composition_tests;

#[cfg(test)]
#[path = "framing_tests.rs"]
mod framing_tests;

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod protocol_tests;

#[cfg(test)]
#[path = "environment_tests.rs"]
mod environment_tests;

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod supervisor_tests;

#[cfg(test)]
#[path = "persistent_tests.rs"]
mod persistent_tests;
