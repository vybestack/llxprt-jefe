//! Issue #198 scrollback history-capture tests, extracted from
//! `manager.rs` to keep that file under the 1000-line source-file limit.
//!
//! As a child module of the `manager` module, `use super::*` grants access to
//! private items (`HistoryCache`) the same way an inline `mod tests` block does.

use super::*;
use crate::runtime::stub_manager::StubRuntimeManager;

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

// ── output_generation ───────────────────────────────────────────────

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
                code_puppy_model: String::new(),
                code_puppy_yolo: Some(false),
                code_puppy_quick_resume: false,
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
