//! Per-agent scrollback history cache for `TmuxRuntimeManager` (issue #198).
//!
//! Extracted from `manager.rs` to keep that file under the source-file-size
//! limit. Pure data + methods: no I/O, no tmux calls.

use crate::domain::AgentId;

/// Cached scrollback history for the currently attached session (issue #198).
///
/// Invalidated (re-captured) only when the output generation advances or the
/// attached session changes. `lines` is `Option<Vec<String>>`:
/// - `None` = no cache (never captured or invalidated).
/// - `Some(vec![])` = cached empty capture (review fix #7).
/// - `Some(non-empty)` = cached lines.
///
/// Caching the empty result avoids shelling out to `capture-pane` every render
/// frame for a session with no scrollback.
#[derive(Debug, Clone, Default)]
pub struct HistoryCache {
    pub cached_agent: Option<AgentId>,
    pub generation: u64,
    pub lines: Option<Vec<String>>,
}

impl HistoryCache {
    pub fn get(&self, agent_id: &AgentId, generation: u64) -> Option<&Vec<String>> {
        if self.cached_agent.as_ref() == Some(agent_id)
            && self.generation == generation
            && let Some(ref lines) = self.lines
        {
            Some(lines)
        } else {
            None
        }
    }

    pub fn get_fallback(&self, agent_id: &AgentId) -> Option<&Vec<String>> {
        if self.cached_agent.as_ref() == Some(agent_id)
            && let Some(ref lines) = self.lines
        {
            Some(lines)
        } else {
            None
        }
    }

    pub fn store(&mut self, agent_id: &AgentId, generation: u64, lines: Option<Vec<String>>) {
        self.cached_agent = Some(agent_id.clone());
        self.generation = generation;
        self.lines = lines;
    }

    /// Invalidate the cache for `agent_id` (review fix #8).
    pub fn clear(&mut self, agent_id: &AgentId) {
        if self.cached_agent.as_ref() == Some(agent_id) {
            self.lines = None;
            self.cached_agent = None;
        }
    }
}

/// Strip the last `n` lines from `lines`, returning the remaining prefix
/// (issue #198 review fix #1). The live snapshot already represents the
/// visible pane, so the history capture must exclude those rows.
#[must_use]
pub fn strip_trailing_rows(lines: Vec<String>, n: usize) -> Vec<String> {
    let keep = lines.len().saturating_sub(n);
    lines.into_iter().take(keep).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(s: &str) -> AgentId {
        AgentId(s.to_owned())
    }

    #[test]
    fn get_returns_none_for_empty_cache() {
        let cache = HistoryCache::default();
        assert!(cache.get(&agent("a"), 0).is_none());
    }

    #[test]
    fn store_then_get_returns_lines() {
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, Some(vec!["x".to_owned()]));
        let got = cache.get(&agent("a"), 3);
        assert_eq!(got, Some(&vec!["x".to_owned()]));
    }

    #[test]
    fn get_misses_on_generation_mismatch() {
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, Some(vec!["x".to_owned()]));
        assert!(cache.get(&agent("a"), 4).is_none());
    }

    #[test]
    fn get_misses_on_agent_mismatch() {
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, Some(vec!["x".to_owned()]));
        assert!(cache.get(&agent("b"), 3).is_none());
    }

    #[test]
    fn get_misses_on_cached_none_lines() {
        // A stored `None` lines value means "no cache" (invalidated), not an
        // empty capture. get() must not return a hit for it.
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, None);
        assert!(cache.get(&agent("a"), 3).is_none());
    }

    #[test]
    fn get_fallback_ignores_generation() {
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, Some(vec!["x".to_owned()]));
        // Different generation but same agent → fallback hit.
        assert_eq!(cache.get_fallback(&agent("a")), Some(&vec!["x".to_owned()]));
    }

    #[test]
    fn get_fallback_misses_on_agent_mismatch() {
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, Some(vec!["x".to_owned()]));
        assert!(cache.get_fallback(&agent("b")).is_none());
    }

    #[test]
    fn get_fallback_misses_on_cached_none_lines() {
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, None);
        assert!(cache.get_fallback(&agent("a")).is_none());
    }

    #[test]
    fn store_empty_vec_is_cached_and_returned() {
        // Review fix #7: an empty capture is cached as Some(vec![]), which
        // get() returns as a hit so we don't re-shell-out every frame.
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 1, Some(Vec::new()));
        assert_eq!(cache.get(&agent("a"), 1), Some(&Vec::<String>::new()));
    }

    #[test]
    fn clear_invalidates_matching_agent() {
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, Some(vec!["x".to_owned()]));
        cache.clear(&agent("a"));
        assert!(cache.get(&agent("a"), 3).is_none());
        assert!(cache.cached_agent.is_none());
        assert!(cache.lines.is_none());
    }

    #[test]
    fn clear_leaves_other_agents_untouched() {
        let mut cache = HistoryCache::default();
        cache.store(&agent("a"), 3, Some(vec!["x".to_owned()]));
        cache.clear(&agent("b"));
        assert_eq!(cache.get(&agent("a"), 3), Some(&vec!["x".to_owned()]));
    }

    #[test]
    fn strip_trailing_rows_removes_last_n() {
        let lines = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(
            strip_trailing_rows(lines, 1),
            vec!["a".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn strip_trailing_rows_saturates_at_zero() {
        let lines = vec!["a".to_owned()];
        assert!(strip_trailing_rows(lines, 5).is_empty());
    }

    #[test]
    fn strip_trailing_rows_zero_n_returns_all() {
        let lines = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(
            strip_trailing_rows(lines, 0),
            vec!["a".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn strip_trailing_rows_preserves_blank_content_rows() {
        // After stripping the visible pane rows, remaining trailing blank lines
        // are real history content and must NOT be stripped (review fix #9).
        // strip_trailing_rows removes the last N lines (the visible pane), not
        // content-blank lines.
        let input: Vec<String> = vec![
            "line1".to_owned(),
            String::new(),
            String::new(),
            "visible1".to_owned(),
            "visible2".to_owned(),
        ];
        let result = strip_trailing_rows(input, 2);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "line1");
        assert_eq!(result[1], "");
        assert_eq!(result[2], "");
    }
}
