//! Runtime-only shell-window inventory and hide/resume reducer operations
//! (issue #361 PR A).
//!
//! Pure state transitions for the embedded agent-shell inventory. The
//! inventory mirrors which agents currently own a live `jefe-shell` window
//! (visible or hidden). Ground truth remains the multiplexer runtime; this
//! typed mirror is updated only after runtime operations succeed so failures
//! leave state intact (see invariants in `project-plans/issue361-plan.md`).
//!
//! Reducers here perform no I/O. The runtime boundary modules
//! (`app_input::shell_overlay`, `runtime::shell_window`) drive the actual
//! tmux/psmux commands and then apply the deterministic transitions here.

use crate::domain::AgentId;

/// Runtime-only inventory of agents that own a live `jefe-shell` window
/// (issue #361).
///
/// Membership means "a `jefe-shell` window exists for this agent in the
/// multiplexer". It does NOT mean the overlay is visible — visibility is
/// tracked separately by [`crate::state::ShellOverlayState::agent_id`].
/// Ground truth is the runtime multiplexer; this mirror is updated only
/// after a runtime operation succeeds, and removed only after a runtime
/// disappearance/success, so a transient probe failure cannot corrupt the
/// inventory.
///
/// At most one shell per agent is enforced structurally by the fixed
/// `jefe-shell` window name.
///
/// Backed by a sorted `Vec<AgentId>` (sorted by the inner `String`) so
/// iteration order is deterministic without requiring `AgentId: Ord`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellInventory {
    agents: Vec<AgentId>,
}

impl ShellInventory {
    /// Construct an empty inventory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `agent_id` owns a tracked shell window.
    #[must_use]
    pub fn contains(&self, agent_id: &AgentId) -> bool {
        self.agents.iter().any(|entry| entry == agent_id)
    }

    /// Number of tracked shell windows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Whether no shell windows are tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Iterate over the agent IDs owning tracked shell windows, in stable
    /// lexicographic order of the underlying string.
    pub fn iter(&self) -> impl Iterator<Item = &AgentId> {
        self.agents.iter()
    }

    /// Snapshot the tracked agent IDs as a `Vec`.
    #[must_use]
    pub fn to_vec(&self) -> Vec<AgentId> {
        self.agents.clone()
    }

    /// Record a shell window for `agent_id` after a successful runtime
    /// open/resume. Idempotent: re-recording an existing entry is a no-op so
    /// resume of a hidden shell does not duplicate.
    pub fn record(&mut self, agent_id: AgentId) {
        let position = self.agents.partition_point(|entry| entry.0 < agent_id.0);
        if self
            .agents
            .get(position)
            .is_some_and(|existing| existing == &agent_id)
        {
            return;
        }
        self.agents.insert(position, agent_id);
    }

    /// Remove `agent_id` from the inventory after a runtime close/disappearance.
    /// Returns whether an entry was actually removed.
    pub fn remove(&mut self, agent_id: &AgentId) -> bool {
        let position = self.agents.partition_point(|entry| entry.0 < agent_id.0);
        if self
            .agents
            .get(position)
            .is_some_and(|existing| existing == agent_id)
        {
            self.agents.remove(position);
            true
        } else {
            false
        }
    }

    /// Replace the entire inventory with `agents`. Used by startup adoption
    /// and batched reconciliation which observe runtime ground truth. The
    /// resulting inventory is deduplicated and sorted.
    pub fn replace(&mut self, agents: impl IntoIterator<Item = AgentId>) {
        self.agents = agents.into_iter().collect();
        self.agents.sort_by(|a, b| a.0.cmp(&b.0));
        self.agents.dedup_by(|a, b| a == b);
    }

    /// Clear every entry. Used by graceful shutdown.
    pub fn clear(&mut self) {
        self.agents.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> AgentId {
        AgentId(name.to_owned())
    }

    #[test]
    fn new_inventory_is_empty() {
        let inventory = ShellInventory::new();
        assert!(inventory.is_empty());
        assert_eq!(inventory.len(), 0);
    }

    #[test]
    fn record_then_contains() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("a"));
        assert!(inventory.contains(&id("a")));
        assert!(!inventory.contains(&id("b")));
        assert_eq!(inventory.len(), 1);
    }

    #[test]
    fn record_is_idempotent_so_resume_does_not_duplicate() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("a"));
        inventory.record(id("a"));
        assert_eq!(inventory.len(), 1);
    }

    #[test]
    fn remove_returns_whether_entry_existed() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("a"));
        assert!(inventory.remove(&id("a")));
        assert!(!inventory.remove(&id("a")));
        assert!(inventory.is_empty());
    }

    #[test]
    fn iter_is_stable_lexicographic_order() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("b"));
        inventory.record(id("a"));
        inventory.record(id("c"));
        let ordered: Vec<_> = inventory.iter().cloned().collect();
        assert_eq!(ordered, vec![id("a"), id("b"), id("c")]);
    }

    #[test]
    fn replace_overwrites_membership_and_sorts() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("old"));
        inventory.replace([id("b"), id("a")]);
        assert!(!inventory.contains(&id("old")));
        assert!(inventory.contains(&id("a")));
        assert!(inventory.contains(&id("b")));
        assert_eq!(inventory.len(), 2);
        let ordered: Vec<_> = inventory.iter().cloned().collect();
        assert_eq!(ordered, vec![id("a"), id("b")]);
    }

    #[test]
    fn replace_deduplicates() {
        let mut inventory = ShellInventory::new();
        inventory.replace([id("a"), id("a"), id("b")]);
        assert_eq!(inventory.len(), 2);
    }

    #[test]
    fn clear_empties_inventory() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("a"));
        inventory.record(id("b"));
        inventory.clear();
        assert!(inventory.is_empty());
    }

    #[test]
    fn to_vec_snapshots_membership() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("b"));
        inventory.record(id("a"));
        let snapshot = inventory.to_vec();
        assert_eq!(snapshot, vec![id("a"), id("b")]);
        // Mutating the inventory after snapshot does not affect the snapshot.
        inventory.remove(&id("a"));
        assert_eq!(snapshot.len(), 2);
    }
}
