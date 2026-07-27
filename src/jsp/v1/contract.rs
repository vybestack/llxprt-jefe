//! Public typed JSP/1 snapshot contract (issue #476, J1 slice).
//!
//! [`Snapshot`] is the validated, strongly typed result of parsing a JSP/1
//! snapshot document. It wraps the transport-neutral domain observation values
//! from [`crate::domain::observation`] together with the wire-level identity
//! key and ordering fields. Construction goes through the strict parse layer;
//! callers cannot build a partially validated snapshot.

use crate::domain::observation::{
    CurrentTurnField, CurrentWaitField, LastMessageField, LastToolField, NativeActivityField,
    NativeSession, ObservationIdentity, ProcessBindingField, SourceErrorField, SourceTerminalField,
    TodosField,
};

/// Monotonic source sequence consumed by a record (decision 10).
pub type SourceSequence = u64;

/// Cursor reflecting all effects through it (decision 10).
pub type Cursor = u64;

/// The live observation key. This is the identity that distinguishes two
/// independent observation streams.
///
/// Repository/path/kind/pid/display-name are descriptive metadata on the
/// [`Snapshot`] and never participate in this key (decision 2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObservationKey(pub(crate) ObservationIdentity);

impl ObservationKey {
    /// The underlying observation identity.
    #[must_use]
    pub fn identity(&self) -> &ObservationIdentity {
        &self.0
    }
}

/// A fully parsed and validated JSP/1 snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The live observation key (agent/generation/epoch).
    pub identity: ObservationIdentity,
    /// Monotonic source sequence consumed by this snapshot.
    pub source_sequence: SourceSequence,
    /// Cursor reflecting all effects applied through it.
    pub cursor: Cursor,
    /// Bridge observation timestamp (bounded UTC epoch milliseconds).
    pub bridge_observed_ms: u64,
    /// Source/native-session descriptive identity.
    pub native_session: NativeSession,
    /// Producer-reported process binding metadata (not liveness).
    pub process_binding: ProcessBindingField,
    /// Native activity (source-owned).
    pub native_activity: NativeActivityField,
    /// Current explicit wait state.
    pub current_wait: CurrentWaitField,
    /// Current turn runtime.
    pub current_turn: CurrentTurnField,
    /// Full-replacement todo list.
    pub todos: TodosField,
    /// Last displayed assistant message.
    pub last_displayed_assistant_message: LastMessageField,
    /// Last-created tool call.
    pub last_created_tool_call: LastToolField,
    /// Source terminal state.
    pub source_terminal_state: SourceTerminalField,
    /// Source error state.
    pub source_error_state: SourceErrorField,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_key_wraps_identity() {
        let identity = ObservationIdentity {
            agent_id: crate::domain::observation::OpaqueId("a".to_string()),
            lifecycle_generation: 1,
            source_epoch: crate::domain::observation::OpaqueId("e".to_string()),
        };
        let key = ObservationKey(identity);
        assert_eq!(key.identity().agent_id.as_str(), "a");
    }
}
