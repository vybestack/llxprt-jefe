//! Attach debounce state machine.
//!
//! Decouples the render/input path from the expensive `runtime.attach()`
//! call. The scheduler records the *desired* attachment target and emits a
//! [`AttachAction::Perform`] only after the debounce window has elapsed with
//! no further changes. Rapid selection changes collapse to a single perform
//! for the final target.

use std::time::{Duration, Instant};

use crate::domain::AgentId;

/// Default debounce window (100 ms).
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(100);

/// Action returned by [`AttachScheduler::poll`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachAction {
    /// Desired and attached targets match — nothing to do.
    Stable,
    /// Debounce window still running — caller should keep polling.
    Waiting,
    /// Debounce elapsed — caller should perform the attach/detach now.
    Perform(Option<AgentId>),
}

/// Pure debounce state machine for scheduling attach/detach operations.
///
/// The scheduler is `Clone` (for snapshotting under hooks) and owns no
/// runtime resources — callers perform the actual attach when
/// [`AttachAction::Perform`] is returned.
#[derive(Debug, Clone)]
pub struct AttachScheduler {
    desired: Option<AgentId>,
    attached: Option<AgentId>,
    debounce_target: Option<AgentId>,
    debounce_started: Option<Instant>,
    debounce_duration: Duration,
}

impl AttachScheduler {
    /// Create a new scheduler with the given debounce window.
    #[must_use]
    pub fn new(debounce_duration: Duration) -> Self {
        Self {
            desired: None,
            attached: None,
            debounce_target: None,
            debounce_started: None,
            debounce_duration,
        }
    }

    /// Borrow the current desired target.
    #[must_use]
    pub fn desired(&self) -> Option<&AgentId> {
        self.desired.as_ref()
    }

    /// Record the desired target from the render/input path.
    pub fn set_desired(&mut self, desired: Option<AgentId>) {
        self.desired = desired;
    }

    /// Record a completed attach/detach. Clears debounce state.
    pub fn mark_attached(&mut self, attached: Option<AgentId>) {
        self.attached = attached;
        self.debounce_target = None;
        self.debounce_started = None;
    }

    /// Core debounce logic. See module docs and unit tests for semantics.
    pub fn poll(&mut self, now: Instant) -> AttachAction {
        // 1. Stable: desired matches what is already attached.
        if self.desired == self.attached {
            self.debounce_target = None;
            self.debounce_started = None;
            return AttachAction::Stable;
        }

        // 2. No debounce in progress (or tracking a stale target): start one.
        let debounce_matches = match (&self.debounce_target, &self.desired) {
            (Some(target), Some(desired)) => target == desired,
            (None, None) => true,
            _ => false,
        };
        if !debounce_matches {
            self.debounce_target = self.desired.clone();
            self.debounce_started = Some(now);
            return AttachAction::Waiting;
        }

        // 3. Debounce target matches but no start time (inconsistent): restart.
        let Some(started) = self.debounce_started else {
            self.debounce_started = Some(now);
            return AttachAction::Waiting;
        };

        // 4. Debounce elapsed: perform the attach for the desired target.
        if now.duration_since(started) >= self.debounce_duration {
            self.debounce_target = None;
            self.debounce_started = None;
            return AttachAction::Perform(self.desired.clone());
        }

        // 5. Still within the debounce window.
        AttachAction::Waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(name: &str) -> AgentId {
        AgentId(name.to_owned())
    }

    #[test]
    fn stable_when_desired_equals_attached() {
        let mut scheduler = AttachScheduler::new(DEFAULT_DEBOUNCE);
        let a = agent("A");
        scheduler.set_desired(Some(a.clone()));
        scheduler.mark_attached(Some(a.clone()));
        let t0 = Instant::now();
        assert_eq!(scheduler.poll(t0), AttachAction::Stable);
    }

    #[test]
    fn starts_debounce_on_first_mismatch() {
        let mut scheduler = AttachScheduler::new(DEFAULT_DEBOUNCE);
        let a = agent("A");
        scheduler.set_desired(Some(a.clone()));
        scheduler.mark_attached(None);
        let t0 = Instant::now();
        assert_eq!(scheduler.poll(t0), AttachAction::Waiting);
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(99)),
            AttachAction::Waiting
        );
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(100)),
            AttachAction::Perform(Some(a.clone()))
        );
    }

    #[test]
    fn restarts_debounce_when_desired_changes() {
        let mut scheduler = AttachScheduler::new(DEFAULT_DEBOUNCE);
        let a = agent("A");
        let b = agent("B");
        scheduler.set_desired(Some(a.clone()));
        let t0 = Instant::now();
        assert_eq!(scheduler.poll(t0), AttachAction::Waiting);
        scheduler.set_desired(Some(b.clone()));
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(50)),
            AttachAction::Waiting
        );
        // 99 ms since B's debounce started — not expired yet.
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(149)),
            AttachAction::Waiting
        );
        // 100 ms since B's debounce started — now expired.
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(150)),
            AttachAction::Perform(Some(b.clone()))
        );
    }

    #[test]
    fn rapid_changes_collapse_to_single_perform() {
        let mut scheduler = AttachScheduler::new(DEFAULT_DEBOUNCE);
        let a = agent("A");
        let b = agent("B");
        let c = agent("C");
        let t0 = Instant::now();
        scheduler.set_desired(Some(a.clone()));
        assert_eq!(scheduler.poll(t0), AttachAction::Waiting);
        scheduler.set_desired(Some(b.clone()));
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(10)),
            AttachAction::Waiting
        );
        scheduler.set_desired(Some(c.clone()));
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(20)),
            AttachAction::Waiting
        );
        // Only one Perform for the final target C.
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(120)),
            AttachAction::Perform(Some(c.clone()))
        );
    }

    #[test]
    fn handles_detach_to_none() {
        let mut scheduler = AttachScheduler::new(DEFAULT_DEBOUNCE);
        let a = agent("A");
        scheduler.set_desired(Some(a.clone()));
        scheduler.mark_attached(Some(a.clone()));
        let t0 = Instant::now();
        assert_eq!(scheduler.poll(t0), AttachAction::Stable);
        scheduler.set_desired(None);
        assert_eq!(scheduler.poll(t0), AttachAction::Waiting);
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(100)),
            AttachAction::Perform(None)
        );
    }

    #[test]
    fn handles_first_attach_from_none() {
        let mut scheduler = AttachScheduler::new(DEFAULT_DEBOUNCE);
        let a = agent("A");
        scheduler.set_desired(Some(a.clone()));
        let t0 = Instant::now();
        assert_eq!(scheduler.poll(t0), AttachAction::Waiting);
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(100)),
            AttachAction::Perform(Some(a.clone()))
        );
    }

    #[test]
    fn clears_debounce_when_becoming_stable() {
        let mut scheduler = AttachScheduler::new(DEFAULT_DEBOUNCE);
        let a = agent("A");
        scheduler.set_desired(Some(a.clone()));
        scheduler.mark_attached(None);
        let t0 = Instant::now();
        assert_eq!(scheduler.poll(t0), AttachAction::Waiting);
        scheduler.mark_attached(Some(a.clone()));
        // No leftover Perform — should be Stable immediately.
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(50)),
            AttachAction::Stable
        );
    }

    #[test]
    fn does_not_perform_before_debounce_expires() {
        let mut scheduler = AttachScheduler::new(DEFAULT_DEBOUNCE);
        let a = agent("A");
        scheduler.set_desired(Some(a.clone()));
        let t0 = Instant::now();
        assert_eq!(scheduler.poll(t0), AttachAction::Waiting);
        // 99 ms — one millisecond short of the 100 ms window.
        assert_eq!(
            scheduler.poll(t0 + Duration::from_millis(99)),
            AttachAction::Waiting
        );
    }
}
