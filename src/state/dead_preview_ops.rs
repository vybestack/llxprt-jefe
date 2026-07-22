//! Runtime-only dead-agent preview cache (issue #374 S4).
//!
//! The dead-pane preview is captured once during off-lock liveness detection
//! and stored here as a runtime-only snapshot keyed by agent id. The render
//! path reads from this cache without shelling out to tmux per-frame. The
//! cache is never persisted: it is rebuilt from liveness on each run.

use std::collections::HashMap;

use crate::domain::AgentId;
use crate::state::AppState;

/// Runtime-only cache of dead-agent pane previews (issue #374 S4).
///
/// Keyed by agent id. Populated once by the off-lock liveness worker when an
/// agent is confirmed dead, and read by the pure render projection. Cleared
/// on revival, restart, or deletion.
#[derive(Debug, Clone, Default)]
pub struct DeadAgentPreview {
    previews: HashMap<AgentId, Vec<String>>,
}

impl DeadAgentPreview {
    /// Whether the cache holds no previews.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.previews.is_empty()
    }
}

impl AppState {
    /// Store captured dead-pane lines for `agent_id`.
    pub fn store_dead_preview(&mut self, agent_id: AgentId, lines: Vec<String>) {
        self.dead_preview.previews.insert(agent_id, lines);
    }

    /// Read the cached dead-pane lines for `agent_id` without runtime I/O.
    #[must_use]
    pub fn dead_preview(&self, agent_id: &AgentId) -> Option<&[String]> {
        self.dead_preview.previews.get(agent_id).map(Vec::as_slice)
    }

    /// Remove the cached dead-pane preview for `agent_id` (issue #374 S4).
    /// Called on revival/restart so a stale preview does not leak.
    pub fn clear_dead_preview(&mut self, agent_id: &AgentId) {
        self.dead_preview.previews.remove(agent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_preview_cache_stores_replaces_and_clears_lines() {
        let mut state = AppState::default();
        let agent_id = AgentId("dead-agent".to_owned());

        state.store_dead_preview(agent_id.clone(), vec!["first".to_owned()]);
        assert_eq!(
            state.dead_preview(&agent_id),
            Some(["first".to_owned()].as_slice())
        );

        state.store_dead_preview(agent_id.clone(), vec!["second".to_owned()]);
        assert_eq!(
            state.dead_preview(&agent_id),
            Some(["second".to_owned()].as_slice())
        );

        state.clear_dead_preview(&agent_id);
        assert!(state.dead_preview(&agent_id).is_none());
    }
}
