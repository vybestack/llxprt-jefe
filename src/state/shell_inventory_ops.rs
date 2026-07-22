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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellInventoryEntry {
    agent_id: AgentId,
    focus_ordinal: u64,
}

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
/// Backed by entries sorted on `AgentId`'s inner `String` so
/// iteration order is deterministic without requiring `AgentId: Ord`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellInventory {
    entries: Vec<ShellInventoryEntry>,
    next_focus_ordinal: u64,
}

impl ShellInventory {
    /// Construct an empty inventory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn position(&self, agent_id: &AgentId) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|entry| entry.agent_id.0.cmp(&agent_id.0))
    }

    /// Whether `agent_id` owns a tracked shell window.
    #[must_use]
    pub fn contains(&self, agent_id: &AgentId) -> bool {
        self.position(agent_id).is_ok()
    }

    /// Number of tracked shell windows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no shell windows are tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over owner IDs in stable lexicographic order.
    pub fn iter(&self) -> impl Iterator<Item = &AgentId> {
        self.entries.iter().map(|entry| &entry.agent_id)
    }

    #[must_use]
    pub fn focus_ordinal(&self, agent_id: &AgentId) -> u64 {
        self.position(agent_id)
            .ok()
            .and_then(|position| self.entries.get(position))
            .map_or(0, |entry| entry.focus_ordinal)
    }

    pub fn record_focus(&mut self, agent_id: &AgentId) {
        let Ok(position) = self.position(agent_id) else {
            return;
        };
        if self.next_focus_ordinal == u64::MAX {
            self.rebase_focus_ordinals();
        }
        self.next_focus_ordinal += 1;
        if let Some(entry) = self.entries.get_mut(position) {
            entry.focus_ordinal = self.next_focus_ordinal;
        }
    }

    fn rebase_focus_ordinals(&mut self) {
        let mut focused = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                (entry.focus_ordinal > 0).then_some((index, entry.focus_ordinal))
            })
            .collect::<Vec<_>>();
        focused.sort_by_key(|(_, ordinal)| *ordinal);
        for (ordinal, (index, _)) in focused.iter().enumerate() {
            if let Some(entry) = self.entries.get_mut(*index) {
                entry.focus_ordinal = ordinal as u64 + 1;
            }
        }
        self.next_focus_ordinal = focused.len() as u64;
    }

    /// Snapshot the tracked owner IDs.
    #[must_use]
    pub fn to_vec(&self) -> Vec<AgentId> {
        self.iter().cloned().collect()
    }

    /// Record an observed shell without changing existing focus recency.
    pub fn record(&mut self, agent_id: AgentId) {
        let Err(position) = self.position(&agent_id) else {
            return;
        };
        self.entries.insert(
            position,
            ShellInventoryEntry {
                agent_id,
                focus_ordinal: 0,
            },
        );
    }

    pub fn remove(&mut self, agent_id: &AgentId) -> bool {
        let Ok(position) = self.position(agent_id) else {
            return false;
        };
        self.entries.remove(position);
        true
    }

    /// Replace observed membership while preserving recency for survivors.
    pub fn replace(&mut self, agents: impl IntoIterator<Item = AgentId>) {
        let mut next = agents
            .into_iter()
            .map(|agent_id| ShellInventoryEntry {
                focus_ordinal: self.focus_ordinal(&agent_id),
                agent_id,
            })
            .collect::<Vec<_>>();
        next.sort_by(|left, right| left.agent_id.0.cmp(&right.agent_id.0));
        next.dedup_by(|left, right| left.agent_id == right.agent_id);
        self.entries = next;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
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
    fn replace_preserves_focus_recency_for_survivors() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("a"));
        inventory.record(id("b"));
        inventory.record_focus(&id("b"));
        let ordinal = inventory.focus_ordinal(&id("b"));

        inventory.replace([id("b"), id("c")]);

        assert_eq!(inventory.focus_ordinal(&id("b")), ordinal);
        assert_eq!(inventory.focus_ordinal(&id("c")), 0);
    }

    #[test]
    fn focus_ordinal_overflow_rebases_without_inverting_recency() {
        let mut inventory = ShellInventory::new();
        inventory.record(id("a"));
        inventory.record(id("b"));
        inventory.entries[0].focus_ordinal = u64::MAX - 1;
        inventory.entries[1].focus_ordinal = u64::MAX;
        inventory.next_focus_ordinal = u64::MAX;

        inventory.record_focus(&id("a"));

        assert!(inventory.focus_ordinal(&id("a")) > inventory.focus_ordinal(&id("b")));
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
