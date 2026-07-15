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
                code_puppy_version: String::new(),
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
                llxprt_version: None,
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
    dead_signature_retains_selector_for_relaunch();
    failed_relaunch_retains_exact_selector_for_successful_retry();
}

fn dead_signature_retains_selector_for_relaunch() {
    let agent_id = AgentId("selector-agent".to_owned());
    let selector = crate::domain::LlxprtNpmPackageSelector::normalize("nightly");
    let signature = LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp"),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: false,
        sandbox_enabled: false,
        sandbox_engine: crate::domain::SandboxEngine::Podman,
        sandbox_flags: String::new(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        agent_kind: crate::domain::AgentKind::Llxprt,
        llxprt_version: selector.clone(),
    };
    let mut manager = TmuxRuntimeManager::new(24, 80);
    manager.sessions.insert(
        agent_id.clone(),
        RuntimeSession::new(agent_id.clone(), "jefe-selector".to_owned(), signature),
    );

    assert!(manager.mark_session_dead(&agent_id));
    assert_eq!(
        manager
            .dead_signatures
            .peek(&agent_id)
            .and_then(|value| value.llxprt_version.as_ref()),
        selector.as_ref()
    );
}
fn failed_relaunch_retains_exact_selector_for_successful_retry() {
    let agent_id = AgentId("retry-selector-agent".to_owned());
    let selector = crate::domain::LlxprtNpmPackageSelector::normalize("next@canary");
    let signature = LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp/retry-selector"),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: false,
        sandbox_enabled: false,
        sandbox_engine: crate::domain::SandboxEngine::Podman,
        sandbox_flags: String::new(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        agent_kind: crate::domain::AgentKind::Llxprt,
        llxprt_version: selector.clone(),
    };
    let mut cache = LruCache::new(MAX_DEAD_SIGNATURES);
    let _ = cache.put(agent_id.clone(), signature);

    let first_attempt = retained_relaunch_signature(&mut cache, &agent_id)
        .unwrap_or_else(|error| panic!("first relaunch should find signature: {error}"));
    let failure = RuntimeError::SpawnFailed("npm package disappeared".to_owned());
    assert!(complete_relaunch_attempt(&mut cache, &agent_id, Err(failure)).is_err());

    let retry = retained_relaunch_signature(&mut cache, &agent_id)
        .unwrap_or_else(|error| panic!("retry should retain signature: {error}"));
    assert_eq!(retry.llxprt_version, selector);
    assert_eq!(retry.work_dir, first_attempt.work_dir);
    assert!(complete_relaunch_attempt(&mut cache, &agent_id, Ok(())).is_ok());
    assert!(cache.peek(&agent_id).is_none());
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
