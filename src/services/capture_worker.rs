//! Async capture request/result flow (issue #301, Phase 2).
//!
//! Moves `tmux capture-pane` off the render hot path. The render path calls
//! [`CaptureHandle::request`] (cheap, stores under a short lock — no I/O). A
//! background `smol::unblock` drains the request, calls the existing
//! `capture_pane_history`, and stores the result in the runtime's
//! `HistoryCache`. The renderer reads the cache only.

use std::sync::{Arc, Mutex};

use crate::domain::AgentId;

/// A capture request keyed by `(agent_id, output_generation)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRequest {
    pub agent_id: AgentId,
    pub session_name: String,
    pub generation: u64,
}

/// Shared handle for the render path to request a background capture.
#[derive(Clone)]
pub struct CaptureHandle {
    inner: Arc<Inner>,
}

struct Inner {
    pending: Mutex<Option<CaptureRequest>>,
}

impl std::fmt::Debug for CaptureHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureHandle").finish_non_exhaustive()
    }
}

impl CaptureHandle {
    /// Create a new capture worker handle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                pending: Mutex::new(None),
            }),
        }
    }

    /// Request a capture for `(agent_id, session_name, generation)`.
    ///
    /// This is the render-path call — it stores the request under a short
    /// lock and returns immediately. No I/O occurs here.
    ///
    /// **Dedup:** if the pending request has the same `(agent_id, generation)`,
    /// the request is a no-op (the in-flight capture will satisfy it).
    ///
    /// # Panics
    ///
    /// Recovers from a poisoned mutex by taking the inner guard (logs an
    /// error). Does not panic on poison.
    pub fn request(&self, agent_id: AgentId, session_name: String, generation: u64) {
        let req = CaptureRequest {
            agent_id,
            session_name,
            generation,
        };
        let mut pending = lock_or_recover(&self.inner.pending, "capture slot");
        if let Some(existing) = &*pending
            && existing.agent_id == req.agent_id
            && existing.generation == req.generation
            && existing.session_name == req.session_name
        {
            return;
        }
        *pending = Some(req);
    }

    /// Take the pending capture request, if any. The worker calls this to
    /// start a capture. Returns `None` if no request is pending.
    ///
    /// # Panics
    ///
    /// Recovers from a poisoned mutex by taking the inner guard (logs an
    /// error). Does not panic on poison.
    #[must_use]
    pub fn take_pending(&self) -> Option<CaptureRequest> {
        let mut pending = lock_or_recover(&self.inner.pending, "capture slot");
        pending.take()
    }

    /// Peek at the pending request without taking it (for testing).
    ///
    /// # Panics
    ///
    /// Recovers from a poisoned mutex by taking the inner guard (logs an
    /// error). Does not panic on poison.
    #[cfg(test)]
    #[must_use]
    pub fn peek_pending(&self) -> Option<CaptureRequest> {
        lock_or_recover(&self.inner.pending, "capture slot").clone()
    }
}

impl Default for CaptureHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Lock a mutex, recovering from poison by taking the inner guard.
fn lock_or_recover<'a, T>(mutex: &'a Mutex<T>, label: &str) -> std::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::error!(label, "mutex poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

/// Decide whether a completed capture result should be stored.
///
/// Returns `true` if the result's `(agent_id, generation)` matches the
/// `current` request — i.e., the result is not stale.
#[must_use]
pub fn should_store_result(
    result_agent: &AgentId,
    result_generation: u64,
    current_agent: Option<&AgentId>,
    current_generation: Option<u64>,
) -> bool {
    match (current_agent, current_generation) {
        (Some(agent), Some(generation)) => agent == result_agent && generation == result_generation,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(s: &str) -> AgentId {
        AgentId(s.to_owned())
    }

    fn require_pending(handle: &CaptureHandle) -> CaptureRequest {
        let Some(pending) = handle.peek_pending() else {
            panic!("pending should exist");
        };
        pending
    }

    #[test]
    fn request_stores_pending_request() {
        let handle = CaptureHandle::new();
        handle.request(agent("a"), "session-a".to_string(), 5);
        let pending = require_pending(&handle);
        assert_eq!(pending.agent_id, agent("a"));
        assert_eq!(pending.session_name, "session-a");
        assert_eq!(pending.generation, 5);
    }

    #[test]
    fn request_deduplicates_same_agent_and_generation() {
        let handle = CaptureHandle::new();
        handle.request(agent("a"), "session-a".to_string(), 5);
        handle.request(agent("a"), "session-a-different".to_string(), 5);
        let pending = require_pending(&handle);
        assert_eq!(
            pending.session_name, "session-a-different",
            "dedup must now include session_name: a different session replaces"
        );
    }

    #[test]
    fn request_deduplicates_same_agent_generation_and_session() {
        let handle = CaptureHandle::new();
        handle.request(agent("a"), "session-a".to_string(), 5);
        handle.request(agent("a"), "session-a".to_string(), 5);
        let pending = require_pending(&handle);
        assert_eq!(pending.generation, 5, "exact dup is a no-op");
        assert_eq!(pending.session_name, "session-a");
    }

    #[test]
    fn request_replaces_with_newer_generation() {
        let handle = CaptureHandle::new();
        handle.request(agent("a"), "session-a".to_string(), 5);
        handle.request(agent("a"), "session-a".to_string(), 6);
        let pending = require_pending(&handle);
        assert_eq!(pending.generation, 6, "newer generation must replace");
    }

    #[test]
    fn request_replaces_with_different_agent() {
        let handle = CaptureHandle::new();
        handle.request(agent("a"), "session-a".to_string(), 5);
        handle.request(agent("b"), "session-b".to_string(), 5);
        let pending = require_pending(&handle);
        assert_eq!(pending.agent_id, agent("b"), "different agent must replace");
    }

    #[test]
    fn take_pending_clears_slot() {
        let handle = CaptureHandle::new();
        handle.request(agent("a"), "session-a".to_string(), 5);
        let Some(taken) = handle.take_pending() else {
            panic!("pending should exist");
        };
        assert_eq!(taken.agent_id, agent("a"));
        assert!(
            handle.take_pending().is_none(),
            "slot must be empty after take"
        );
    }

    #[test]
    fn take_pending_returns_none_when_empty() {
        let handle = CaptureHandle::new();
        assert!(handle.take_pending().is_none());
    }

    #[test]
    fn should_store_when_agent_and_generation_match() {
        assert!(should_store_result(
            &agent("a"),
            5,
            Some(&agent("a")),
            Some(5)
        ));
    }

    #[test]
    fn should_not_store_when_generation_mismatches() {
        assert!(!should_store_result(
            &agent("a"),
            3,
            Some(&agent("a")),
            Some(5)
        ));
    }

    #[test]
    fn should_not_store_when_agent_mismatches() {
        assert!(!should_store_result(
            &agent("a"),
            5,
            Some(&agent("b")),
            Some(5)
        ));
    }

    #[test]
    fn should_not_store_when_no_current_target() {
        assert!(!should_store_result(&agent("a"), 5, None, None));
    }

    #[test]
    fn should_not_store_when_current_generation_missing() {
        assert!(!should_store_result(
            &agent("a"),
            5,
            Some(&agent("a")),
            None
        ));
    }
}
