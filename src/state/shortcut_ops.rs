//! Agent shortcut-slot selection helpers (extracted from `mod.rs` to keep
//! that file under the source-file size hard limit).
//!
//! Pure read/mutate operations on `AppState` that derive and enforce the
//! Option-digit shortcut slot uniqueness invariant.

use crate::domain::AgentId;

use super::AppState;

impl AppState {
    /// First Option-digit slot (1-9) not claimed by another agent.
    pub(super) fn first_unused_shortcut_slot(&self, ignore_agent: Option<&AgentId>) -> Option<u8> {
        (1u8..=9u8).find(|slot| {
            self.agents.iter().all(|agent| {
                if ignore_agent.is_some_and(|id| &agent.id == id) {
                    true
                } else {
                    agent.shortcut_slot != Some(*slot)
                }
            })
        })
    }

    /// Clear `slot` from any agent other than `owner_id` so the slot stays unique.
    pub(super) fn enforce_shortcut_uniqueness(&mut self, owner_id: &AgentId, slot: Option<u8>) {
        let Some(slot) = slot else {
            return;
        };
        for agent in &mut self.agents {
            if agent.id != *owner_id && agent.shortcut_slot == Some(slot) {
                agent.shortcut_slot = None;
            }
        }
    }
}
