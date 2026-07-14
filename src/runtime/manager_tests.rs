//! Tests for the runtime manager, kept in a sibling file so `manager.rs`
//! stays under the source-file size hard limit.

use super::*;
use crate::runtime::stub_manager::StubRuntimeManager;

// The `dead_signatures` field is private and the real mutating methods
// (`mark_session_dead`, `kill`) require a live tmux session to exercise
// end-to-end, which is not unit-test friendly. Instead this test targets
// the bound directly: it constructs an `LruCache` with the production
// capacity constant and proves that exceeding it evicts the oldest entries
// while never growing past the cap. This is the property the field relies
// on to prevent unbounded memory growth from repeated kill/recreate cycles.
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
                llxprt_version: String::new(),
                code_puppy_yolo: None,
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
fn prefix_passthrough_tracking_memoizes_per_session() {
    let mut mgr = TmuxRuntimeManager::new(40, 120);

    // Initially nothing is enforced — a pre-fix session has not been
    // remediated, which is exactly the reattach gap #200 closes.
    assert!(!mgr.prefix_passthrough_enforced("jefe-agent-a"));
    assert!(!mgr.prefix_passthrough_enforced("jefe-agent-b"));

    // Recording a session marks only that session.
    mgr.record_prefix_passthrough("jefe-agent-a");
    assert!(mgr.prefix_passthrough_enforced("jefe-agent-a"));
    assert!(!mgr.prefix_passthrough_enforced("jefe-agent-b"));

    // Recording again is idempotent (HashSet dedup).
    mgr.record_prefix_passthrough("jefe-agent-a");
    assert!(mgr.prefix_passthrough_enforced("jefe-agent-a"));

    // A second session is tracked independently.
    mgr.record_prefix_passthrough("jefe-agent-b");
    assert!(mgr.prefix_passthrough_enforced("jefe-agent-a"));
    assert!(mgr.prefix_passthrough_enforced("jefe-agent-b"));
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

#[test]
fn tmux_take_dirty_returns_false_without_viewer() {
    let mgr = TmuxRuntimeManager::new(40, 120);
    // No viewer attached → take_dirty must return false (not panic).
    assert!(
        !mgr.take_dirty(),
        "take_dirty should return false when no viewer is attached"
    );
}

// ── Pre-destructive validation: invalid selector causes no kill ────────────
//
// spawn_session_fresh must validate the version selector BEFORE the force-fresh
// pre-kill. An invalid selector (embedded NUL) must return
// InvalidVersionSelector without any destruction. This proves the validation
// ordering: the error is returned before the kill/kill_remote commands run.

/// Build a LaunchSignature with an embedded NUL byte in the version selector.
fn signature_with_nul_selector() -> LaunchSignature {
    LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp"),
        profile: "default".into(),
        code_puppy_model: String::new(),
        llxprt_version: "0.9.0\x00; rm -rf /".to_owned(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: vec![],
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: crate::domain::SandboxEngine::Podman,
        sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        agent_kind: crate::domain::AgentKind::Llxprt,
    }
}

/// `spawn_session_fresh` with an embedded-NUL selector must return
/// `InvalidVersionSelector` without reaching the force-fresh pre-kill. Since
/// the kill runs tmux commands that don't exist in the test environment, a
/// non-`InvalidVersionSelector` error proves the validation did NOT fire first.
#[test]
fn spawn_session_fresh_validates_selector_before_kill() {
    let mut mgr = TmuxRuntimeManager::new(40, 120);
    let agent_id = AgentId("test-nul-selector".into());
    let signature = signature_with_nul_selector();

    let result = mgr.spawn_session_fresh(&agent_id, signature.work_dir.as_path(), &signature);

    let Err(error) = result else {
        panic!("invalid selector must be rejected by spawn_session_fresh");
    };

    // Must be InvalidVersionSelector — NOT KillFailed, CapabilityProbeFailed,
    // or any tmux-related error that would prove the kill ran first.
    assert!(
        matches!(error, RuntimeError::InvalidVersionSelector(_)),
        "expected InvalidVersionSelector before kill, got {error:?}"
    );

    // The agent must NOT be tracked as a session.
    assert!(
        !mgr.is_alive(&agent_id),
        "invalid-selector agent must not be tracked after rejection"
    );
}

/// `spawn_session` (reattach path) with a valid selector must NOT return
/// InvalidVersionSelector — proving the validation only rejects truly
/// invalid selectors, not valid ones.
#[test]
fn spawn_session_valid_selector_does_not_trigger_validation_error() {
    let mut mgr = TmuxRuntimeManager::new(40, 120);
    let agent_id = AgentId("test-valid-selector".into());

    let signature = LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp"),
        profile: "default".into(),
        code_puppy_model: String::new(),
        llxprt_version: "0.9.0".to_owned(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: vec![],
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: crate::domain::SandboxEngine::Podman,
        sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        agent_kind: crate::domain::AgentKind::Llxprt,
    };

    let result = mgr.spawn_session(&agent_id, signature.work_dir.as_path(), &signature);

    // We expect an error (no tmux in test env), but it must NOT be
    // InvalidVersionSelector — the selector is valid.
    if let Err(error) = result {
        assert!(
            !matches!(error, RuntimeError::InvalidVersionSelector(_)),
            "valid selector must not trigger InvalidVersionSelector: {error:?}"
        );
    }
}
