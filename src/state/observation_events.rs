//! Runtime-only JSP observation lifecycle events.
//!
//! Split out of `events.rs` so the observation payload types do not consume
//! space in that file, which sits at the source-size gate.
//!
//! These events are runtime-only: they are never persisted, and they carry the
//! reduced observation payload verbatim rather than a rendered projection.

use crate::domain::AgentId;
use crate::domain::observation::AgentObservation;

/// Payload-preserving observation lifecycle for a single agent.
///
/// Both variants carry the lifecycle generation so a document produced by a
/// superseded agent generation can be rejected rather than applied.
#[derive(Debug, Clone)]
pub enum ObservationEvent {
    /// Replace the agent's observation with a newly reduced state.
    Updated(AgentId, u64, Box<AgentObservation>),
    /// Drop the agent's observation, for example on revocation or relaunch.
    Cleared(AgentId, u64),
}
