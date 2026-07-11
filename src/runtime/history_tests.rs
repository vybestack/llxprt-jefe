//! Issue #198 scrollback history-capture tests, extracted from
//! `manager.rs` to keep that file under the 1000-line source-file limit.
//!
//! As a child module of the `manager` module, `use super::*` grants access to
//! private items (`HistoryCache`) the same way an inline `mod tests` block does.

use super::*;

// ── is_dirty (non-consuming dirty check) ────────────────────────────────

#[test]
fn tmux_take_dirty_returns_false_without_viewer() {
    let mgr = TmuxRuntimeManager::new(40, 120);
    // No viewer attached → take_dirty must return false (not panic).
    assert!(
        !mgr.take_dirty(),
        "take_dirty should return false when no viewer is attached"
    );
}

#[test]
fn stub_is_dirty_always_returns_false() {
    // Issue #198: the non-consuming is_dirty() mirrors take_dirty for the stub
    // (no PTY). Used by the history cache so it never steals the render-decision
    // dirty flag.
    let mgr = StubRuntimeManager::default();
    assert!(
        !mgr.is_dirty(),
        "StubRuntimeManager is_dirty must be false (no PTY)"
    );
}

#[test]
fn tmux_is_dirty_returns_false_without_viewer() {
    let mgr = TmuxRuntimeManager::new(40, 120);
    assert!(
        !mgr.is_dirty(),
        "is_dirty should return false when no viewer is attached"
    );
}

// ── capture_history ─────────────────────────────────────────────────────

#[test]
fn stub_capture_history_returns_none() {
    let mut mgr = StubRuntimeManager::default();
    assert!(
        mgr.capture_history().is_none(),
        "StubRuntimeManager has no PTY, so capture_history must return None"
    );
}

#[test]
fn tmux_capture_history_returns_none_without_attached_session() {
    let mut mgr = TmuxRuntimeManager::new(40, 120);
    assert!(
        mgr.capture_history().is_none(),
        "no attached session → capture_history must return None (not panic)"
    );
}

// ── HistoryCache ────────────────────────────────────────────────────────

#[test]
fn history_cache_get_returns_none_for_different_agent() {
    let mut cache = HistoryCache::default();
    let agent_a = AgentId("agent-a".into());
    let agent_b = AgentId("agent-b".into());
    cache.store(&agent_a, 0, Some(vec!["line1".to_owned()]));
    assert!(
        cache.get(&agent_b, 0).is_none(),
        "cache for agent-a must not serve agent-b"
    );
}

#[test]
fn history_cache_get_returns_cached_for_same_agent_and_generation() {
    let mut cache = HistoryCache::default();
    let agent = AgentId("agent-a".into());
    cache.store(
        &agent,
        5,
        Some(vec!["line1".to_owned(), "line2".to_owned()]),
    );
    let cached = cache.get(&agent, 5);
    let Some(cached) = cached else {
        panic!("same agent + generation must hit cache");
    };
    assert_eq!(*cached, ["line1", "line2"]);
}

#[test]
fn history_cache_get_returns_none_for_stale_generation() {
    // Issue #198 review fix #2: a stale generation must NOT serve the cache.
    let mut cache = HistoryCache::default();
    let agent = AgentId("agent-a".into());
    cache.store(&agent, 3, Some(vec!["old".to_owned()]));
    assert!(
        cache.get(&agent, 4).is_none(),
        "stale generation must invalidate cache"
    );
}

#[test]
fn history_cache_store_overwrites_previous() {
    let mut cache = HistoryCache::default();
    let agent = AgentId("agent-a".into());
    cache.store(&agent, 0, Some(vec!["old".to_owned()]));
    cache.store(&agent, 1, Some(vec!["new".to_owned()]));
    let cached = cache.get(&agent, 1);
    let Some(cached) = cached else {
        panic!("same agent + generation must hit cache");
    };
    assert_eq!(*cached, ["new"]);
}

#[test]
fn history_cache_fallback_returns_lines_regardless_of_generation() {
    // Issue #198 review fix #9: on transient capture failure, the prior cache
    // should still be available regardless of generation mismatch.
    let mut cache = HistoryCache::default();
    let agent = AgentId("agent-a".into());
    cache.store(&agent, 3, Some(vec!["prior".to_owned()]));
    assert!(
        cache.get(&agent, 99).is_none(),
        "stale generation must not serve via get()"
    );
    let fallback = cache.get_fallback(&agent);
    let Some(fallback) = fallback else {
        panic!("get_fallback must return prior cache for same agent");
    };
    assert_eq!(*fallback, ["prior"]);
}

// ── Review fix #7: cache empty captures ──────────────────────────────────

#[test]
fn history_cache_stores_and_serves_empty_capture() {
    // Some(vec![]) = cached empty capture (review fix #7). This must be
    // served by get() so we don't shell out on every frame.
    let mut cache = HistoryCache::default();
    let agent = AgentId("agent-a".into());
    cache.store(&agent, 0, Some(vec![]));
    let Some(cached) = cache.get(&agent, 0) else {
        panic!("cached empty capture must be served by get()");
    };
    assert!(cached.is_empty(), "cached empty must be empty");
}

#[test]
fn history_cache_none_lines_means_no_cache() {
    // Default cache: lines=None, must not serve.
    let cache = HistoryCache::default();
    let agent = AgentId("agent-a".into());
    assert!(
        cache.get(&agent, 0).is_none(),
        "default cache (lines=None) must not serve"
    );
    assert!(
        cache.get_fallback(&agent).is_none(),
        "default cache fallback must not serve"
    );
}

// ── Review fix #8: cache invalidation on kill ────────────────────────────

#[test]
fn history_cache_clear_invalidates_for_agent() {
    let mut cache = HistoryCache::default();
    let agent_a = AgentId("agent-a".into());
    let agent_b = AgentId("agent-b".into());
    cache.store(&agent_a, 0, Some(vec!["a1".to_owned()]));
    cache.store(&agent_b, 0, Some(vec!["b1".to_owned()]));

    // Clear agent_a only.
    cache.clear(&agent_a);
    assert!(
        cache.get(&agent_a, 0).is_none(),
        "cleared agent must not serve cache"
    );
    assert!(
        cache.get_fallback(&agent_a).is_none(),
        "cleared agent fallback must not serve"
    );
    // Agent_b unaffected.
    assert!(
        cache.get(&agent_b, 0).is_some(),
        "other agent cache must be unaffected by clear"
    );
}

// ── Review fix #9: trailing blank rows preserved ─────────────────────────

#[test]
fn strip_trailing_rows_preserves_blank_content_rows() {
    // After stripping the visible pane rows, remaining trailing blank lines
    // are real history content and must NOT be stripped (review fix #9).
    // strip_trailing_rows removes the last N lines (the visible pane), not
    // content-blank lines.
    let input: Vec<String> = vec![
        "line1".to_owned(),
        String::new(), // real blank line in history
        String::new(), // real blank line in history
        "visible1".to_owned(),
        "visible2".to_owned(),
    ];
    let result = strip_trailing_rows(input, 2); // strip 2 visible rows
    assert_eq!(
        result.len(),
        3,
        "must keep 3 history rows (including trailing blanks)"
    );
    assert_eq!(result[0], "line1");
    assert_eq!(result[1], "");
    assert_eq!(result[2], "");
}

// ── Review fix #10: is_dirty treated as cache miss ───────────────────────
//
// The capture_history method now checks is_dirty() and treats a dirty viewer
// as a cache miss. This is tested at the method level, not at the cache level.
// The stub returns false for is_dirty, so this is a contract assertion.

#[test]
fn stub_is_dirty_always_false_no_dirty_race() {
    let mgr = StubRuntimeManager::default();
    assert!(
        !mgr.is_dirty(),
        "stub is_dirty must be false (no dirty race in test)"
    );
}

// ── strip_trailing_rows (issue #198 review fix #1) ──────────────────────
//
// tmux's `capture-pane -E -` includes the visible pane rows at the tail. The
// live snapshot already represents the visible pane, so the history capture
// must strip exactly `live_rows` trailing lines to avoid duplicating every
// visible row.

#[test]
fn strip_trailing_rows_removes_last_n() {
    let input: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
    let result = strip_trailing_rows(input, 3);
    assert_eq!(result.len(), 7);
    assert_eq!(result[0], "line0");
    assert_eq!(result[6], "line6");
}

#[test]
fn strip_trailing_rows_empty_when_n_exceeds_len() {
    let input: Vec<String> = vec!["a".to_owned(), "b".to_owned()];
    let result = strip_trailing_rows(input, 5);
    assert!(result.is_empty(), "stripping more rows than exist → empty");
}

#[test]
fn strip_trailing_rows_zero_is_identity() {
    let input: Vec<String> = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
    let result = strip_trailing_rows(input, 0);
    assert_eq!(result, ["a", "b", "c"]);
}

// ── output_generation (issue #198 review fix #2) ────────────────────────

#[test]
fn tmux_output_generation_starts_at_zero() {
    let mgr = TmuxRuntimeManager::new(40, 120);
    assert_eq!(mgr.output_generation(), 0);
}

#[test]
fn stub_output_generation_is_zero() {
    let mgr = StubRuntimeManager::default();
    assert_eq!(mgr.output_generation(), 0);
}

// ── Tests moved from inline `mod tests` to keep manager.rs under 1000 lines ─

#[test]
fn dead_signatures_cache_is_bounded_by_max_dead_signatures() {
    let cap = MAX_DEAD_SIGNATURES.get();
    let mut cache: LruCache<AgentId, LaunchSignature> = LruCache::new(MAX_DEAD_SIGNATURES);

    // Insert well beyond the capacity.
    for i in 0..cap + 10 {
        let id = AgentId(format!("agent-{i}"));
        let _ = cache.put(
            id,
            LaunchSignature {
                work_dir: std::path::PathBuf::from("/tmp"),
                profile: "default".into(),
                mode_flags: vec![],
                llxprt_debug: String::new(),
                pass_continue: true,
                sandbox_enabled: false,
                sandbox_engine: crate::domain::SandboxEngine::Podman,
                sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
                remote: crate::domain::RemoteRepositorySettings::default(),
                agent_kind: crate::domain::AgentKind::Llxprt,
            },
        );
    }

    // The cache must never exceed the configured bound.
    assert_eq!(cache.len(), cap);

    // The oldest entries (agent-0 .. agent-9) were evicted; the most recent
    // entries survive because they are the ones most likely to be relaunched.
    assert!(cache.peek(&AgentId("agent-0".into())).is_none());
    assert!(cache.peek(&AgentId("agent-9".into())).is_none());
    assert!(
        cache
            .peek(&AgentId(format!("agent-{}", cap + 10 - 1)))
            .is_some()
    );
}

#[test]
fn clipboard_passthrough_tracking_memoizes_per_session() {
    let mut mgr = TmuxRuntimeManager::new(40, 120);

    // Initially nothing is enforced.
    assert!(!mgr.clipboard_passthrough_enforced("jefe-agent-a"));
    assert!(!mgr.clipboard_passthrough_enforced("jefe-agent-b"));

    // Recording a session marks only that session.
    mgr.record_clipboard_passthrough("jefe-agent-a");
    assert!(mgr.clipboard_passthrough_enforced("jefe-agent-a"));
    assert!(!mgr.clipboard_passthrough_enforced("jefe-agent-b"));

    // Recording again is idempotent (HashSet dedup).
    mgr.record_clipboard_passthrough("jefe-agent-a");
    assert!(mgr.clipboard_passthrough_enforced("jefe-agent-a"));

    // A second session is tracked independently.
    mgr.record_clipboard_passthrough("jefe-agent-b");
    assert!(mgr.clipboard_passthrough_enforced("jefe-agent-a"));
    assert!(mgr.clipboard_passthrough_enforced("jefe-agent-b"));
}

#[test]
fn stub_take_dirty_always_returns_false() {
    let mgr = StubRuntimeManager::default();
    // The stub has no real PTY, so the dirty flag is always false.
    assert!(
        !mgr.take_dirty(),
        "StubRuntimeManager should never be dirty"
    );
}
