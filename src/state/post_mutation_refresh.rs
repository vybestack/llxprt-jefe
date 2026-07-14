//! Deterministic coalescing state for post-mutation list/detail refreshes.
//!
//! A successful mutation records one requested refresh. The orchestration layer
//! starts it only after both relevant request channels become idle, then sends a
//! reducer event that consumes the request before launching side effects.

/// Coalesces repeated readiness checks into one eventual refresh start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostMutationRefresh {
    requested: bool,
}

impl PostMutationRefresh {
    /// Record that a refresh must eventually run.
    pub fn request(&mut self) {
        self.requested = true;
    }

    /// Whether the requested refresh can begin with the current request state.
    #[must_use]
    pub const fn is_ready(&self, list_pending: bool, detail_pending: bool) -> bool {
        self.requested && !list_pending && !detail_pending
    }

    /// Consume the requested refresh immediately before orchestration starts it.
    pub fn started(&mut self) {
        self.requested = false;
    }
}
