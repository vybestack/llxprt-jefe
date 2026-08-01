//! Tests for the runtime manager, kept in a sibling file so `manager.rs`
//! stays under the source-file size hard limit.

use super::existing::ExistingLocalSessionObservation;
use super::*;
use crate::domain::agent_definition::AgentLaunchPlan;
use crate::runtime::stub_manager::StubRuntimeManager;

// The `dead_plans` field is private and the real mutating methods
// (`mark_session_dead`, `kill`) require a live tmux session to exercise
// end-to-end, which is not unit-test friendly. Instead this test targets
// the bound directly: it constructs an `LruCache` with the production
// capacity constant and proves that exceeding it evicts the oldest entries
// while never growing past the cap. This is the property the field relies
// on to prevent unbounded memory growth from repeated kill/recreate cycles.
#[test]
fn dead_signatures_cache_is_bounded_by_max_dead_signatures() {
    let cap = MAX_DEAD_SIGNATURES.get();
    let mut cache: LruCache<AgentId, RetainedLaunch> = LruCache::new(MAX_DEAD_SIGNATURES);

    // Insert well beyond the capacity.
    for i in 0..cap + 10 {
        let id = AgentId(format!("agent-{i}"));
        let _ = cache.put(id, RetainedLaunch);
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
    failed_relaunch_retains_dead_marker_for_successful_retry();
}

fn dead_signature_retains_selector_for_relaunch() {
    let agent_id = AgentId("selector-agent".to_owned());
    let mut manager = TmuxRuntimeManager::new(24, 80);
    manager.sessions.insert(
        agent_id.clone(),
        RuntimeSession::new(
            agent_id.clone(),
            "jefe-selector".to_owned(),
            AgentLaunchPlan::default(),
            None,
        ),
    );

    assert!(manager.mark_session_dead(&agent_id));
    assert!(manager.dead_plans.peek(&agent_id).is_some());
}
fn failed_relaunch_retains_dead_marker_for_successful_retry() {
    // relaunch now supplies its own authorized plan, so the retained entry is a
    // dead-marker only. `complete_relaunch_attempt` must retain the marker on
    // failure (allowing retry) and clear it on success.
    let agent_id = AgentId("retry-selector-agent".to_owned());
    let mut cache = LruCache::new(MAX_DEAD_SIGNATURES);
    let _ = cache.put(agent_id.clone(), RetainedLaunch);

    let failure = RuntimeError::SpawnFailed("npm package disappeared".to_owned());
    assert!(complete_relaunch_attempt(&mut cache, &agent_id, Err(failure)).is_err());
    // A failed relaunch retains the dead marker so the caller may retry.
    assert!(cache.peek(&agent_id).is_some());

    assert!(complete_relaunch_attempt(&mut cache, &agent_id, Ok(())).is_ok());
    // A successful relaunch clears the dead marker.
    assert!(cache.peek(&agent_id).is_none());
}

#[test]
fn observed_existing_session_returns_complete_authoritative_binding() {
    let agent_id = AgentId("existing-agent".to_owned());
    let mut manager = TmuxRuntimeManager::new(24, 80);
    let signature = crate::domain::LaunchSignatureV1::default();
    // A Windows-shaped observation: the pane leader is the session host (42) and
    // the agent worker is a distinct process below it (43) (issue #543).
    let pane = crate::domain::PaneProcessIdentity::new(42, 900);
    let worker = crate::domain::WorkerProcessIdentity::new(43, 901);

    let binding = manager.register_observed_local_session(
        &agent_id,
        Path::new("/tmp/existing"),
        signature.clone(),
        RuntimeSession::session_name_for(&agent_id),
        ExistingLocalSessionObservation {
            pane_identity: pane,
            worker_identity: Some(worker),
            worker_identities: vec![worker],
        },
    );

    assert_eq!(binding.launch_signature, signature);
    assert_eq!(
        binding.pane_identity,
        Some(pane),
        "the observed pane leader must be recorded in the pane role"
    );
    assert_eq!(
        binding.worker_identity,
        Some(worker),
        "the observed worker must be recorded in the worker role, not the pane's PID"
    );
    assert_eq!(binding.worker_identities, vec![worker]);
    assert!(binding.lifecycle_generation > 0);
    let target = manager
        .liveness_targets()
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("registered session must have a liveness target"));
    assert_eq!(target.lifecycle_generation, binding.lifecycle_generation);
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

// ── Issue #467 Slice 2: explicit session-host root ownership ───────────────
//
// The default `TmuxRuntimeManager::new` preserves every existing caller. The
// new `with_session_host_root` constructor records the caller-supplied path
// authority without mutating process environment, and exposes it so the local
// launch path can stage the host image below it on Windows.

#[test]
fn new_default_manager_has_no_session_host_root() {
    let mgr = TmuxRuntimeManager::new(40, 120);
    assert!(
        mgr.session_host_root().is_none(),
        "default manager must not own a session-host root"
    );
}

#[test]
fn with_session_host_root_records_explicit_authority() {
    let root = std::path::PathBuf::from("/state/session-hosts");
    let mgr = TmuxRuntimeManager::with_session_host_root(40, 120, root.clone());
    assert_eq!(
        mgr.session_host_root(),
        Some(root.as_path()),
        "explicit-root constructor must expose the supplied authority verbatim"
    );
}

#[test]
fn with_session_host_root_preserves_dimensions_and_default_state() {
    let root = std::path::PathBuf::from("/state/session-hosts");
    let mgr = TmuxRuntimeManager::with_session_host_root(24, 80, root);
    assert_eq!(mgr.rows, 24);
    assert_eq!(mgr.cols, 80);
    assert!(
        !mgr.take_dirty(),
        "explicit-root manager must start without a dirty viewer"
    );
    assert!(
        mgr.attached_agent().is_none(),
        "explicit-root manager must start without an attached agent"
    );
}

/// AC7 contract surface: the manager exposes the session-host root so the kill
/// path can derive the per-session directory. The actual filesystem removal is
/// exercised in `session_host_tests` because the manager's `kill` requires a
/// live psmux/tmux session this unit harness cannot create.
#[test]
fn session_host_root_is_readable_for_kill_path_authority() {
    let root = std::path::PathBuf::from("/state/session-hosts");
    let mgr = TmuxRuntimeManager::with_session_host_root(40, 120, root.clone());
    assert_eq!(mgr.session_host_root(), Some(root.as_path()));
}
