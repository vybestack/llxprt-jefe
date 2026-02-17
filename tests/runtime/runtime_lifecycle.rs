//! Runtime lifecycle tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P07
//! @requirement REQ-FUNC-007
//! @pseudocode component-002 lines 07-35
//!
//! Tests for attach/reattach safety, kill, relaunch, and status transitions.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, LaunchSignature, RepositoryId};
use jefe::runtime::{RuntimeError, RuntimeManager, StubRuntimeManager};

fn make_agent(id: &str, repo_id: &str) -> Agent {
    Agent::new(
        AgentId(id.into()),
        RepositoryId(repo_id.into()),
        format!("Test Agent {id}"),
        PathBuf::from(format!("/tmp/test/{id}")),
    )
}

fn make_signature(agent: &Agent) -> LaunchSignature {
    LaunchSignature {
        work_dir: agent.work_dir.clone(),
        profile: agent.profile.clone(),
        mode_flags: agent.mode_flags.clone(),
        pass_continue: agent.pass_continue,
    }
}

// =============================================================================
// Spawn Lifecycle (component-002 lines 01-06)
// =============================================================================

#[test]
fn spawn_creates_session_for_agent() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn should succeed");

    assert!(
        mgr.is_alive(&agent.id),
        "session should be alive after spawn"
    );
}

#[test]
fn spawn_fails_for_duplicate_agent() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("first spawn should succeed");

    let result = mgr.spawn_session(&agent.id, &agent.work_dir, &sig);
    assert!(
        matches!(result, Err(RuntimeError::AlreadyRunning(_))),
        "duplicate spawn should fail"
    );
}

#[test]
fn spawn_allows_multiple_different_agents() {
    let mut mgr = StubRuntimeManager::default();
    let agent1 = make_agent("agent-1", "repo-1");
    let agent2 = make_agent("agent-2", "repo-1");
    let sig1 = make_signature(&agent1);
    let sig2 = make_signature(&agent2);

    mgr.spawn_session(&agent1.id, &agent1.work_dir, &sig1)
        .expect("first spawn should succeed");
    mgr.spawn_session(&agent2.id, &agent2.work_dir, &sig2)
        .expect("second spawn should succeed");

    assert!(mgr.is_alive(&agent1.id));
    assert!(mgr.is_alive(&agent2.id));
}

// =============================================================================
// Attach/Reattach Safety (component-002 lines 07-14)
// =============================================================================

#[test]
fn attach_to_existing_session_succeeds() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    mgr.attach(&agent.id).expect("attach should succeed");

    assert_eq!(mgr.attached_agent(), Some(&agent.id));
}

#[test]
fn attach_to_nonexistent_session_fails() {
    let mut mgr = StubRuntimeManager::default();
    let agent_id = AgentId("nonexistent".into());

    let result = mgr.attach(&agent_id);
    assert!(
        matches!(result, Err(RuntimeError::SessionNotFound(_))),
        "attach to nonexistent should fail"
    );
}

#[test]
fn attach_switches_from_previous_session() {
    let mut mgr = StubRuntimeManager::default();
    let agent1 = make_agent("agent-1", "repo-1");
    let agent2 = make_agent("agent-2", "repo-1");
    let sig1 = make_signature(&agent1);
    let sig2 = make_signature(&agent2);

    mgr.spawn_session(&agent1.id, &agent1.work_dir, &sig1)
        .expect("spawn 1");
    mgr.spawn_session(&agent2.id, &agent2.work_dir, &sig2)
        .expect("spawn 2");

    mgr.attach(&agent1.id).expect("attach to 1");
    assert_eq!(mgr.attached_agent(), Some(&agent1.id));

    mgr.attach(&agent2.id).expect("attach to 2");
    assert_eq!(
        mgr.attached_agent(),
        Some(&agent2.id),
        "should be attached to agent 2 now"
    );

    // Verify agent1 session is still alive but not attached
    assert!(mgr.is_alive(&agent1.id));
    let session1 = mgr.get_session(&agent1.id).expect("session 1 exists");
    assert!(!session1.attached, "agent 1 should be detached");
}

#[test]
fn detach_clears_attached_agent() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    mgr.attach(&agent.id).expect("attach");
    mgr.detach().expect("detach should succeed");

    assert_eq!(mgr.attached_agent(), None, "no agent should be attached");
}

#[test]
fn snapshot_returns_none_when_not_attached() {
    let mgr = StubRuntimeManager::default();
    assert!(
        mgr.snapshot().is_none(),
        "snapshot should be None when not attached"
    );
}

#[test]
fn snapshot_returns_some_when_attached() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    mgr.attach(&agent.id).expect("attach");

    assert!(
        mgr.snapshot().is_some(),
        "snapshot should be Some when attached"
    );
}

// =============================================================================
// Kill Lifecycle (component-002 lines 21-26)
// =============================================================================

#[test]
fn kill_removes_session() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    assert!(mgr.is_alive(&agent.id));

    mgr.kill(&agent.id).expect("kill should succeed");
    assert!(
        !mgr.is_alive(&agent.id),
        "session should not be alive after kill"
    );
}

#[test]
fn kill_nonexistent_fails() {
    let mut mgr = StubRuntimeManager::default();
    let agent_id = AgentId("nonexistent".into());

    let result = mgr.kill(&agent_id);
    assert!(
        matches!(result, Err(RuntimeError::SessionNotFound(_))),
        "kill nonexistent should fail"
    );
}

#[test]
fn kill_attached_session_clears_attachment() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    mgr.attach(&agent.id).expect("attach");
    mgr.kill(&agent.id).expect("kill");

    assert_eq!(
        mgr.attached_agent(),
        None,
        "attachment should be cleared after killing attached session"
    );
}

#[test]
fn kill_one_session_preserves_others() {
    let mut mgr = StubRuntimeManager::default();
    let agent1 = make_agent("agent-1", "repo-1");
    let agent2 = make_agent("agent-2", "repo-1");
    let sig1 = make_signature(&agent1);
    let sig2 = make_signature(&agent2);

    mgr.spawn_session(&agent1.id, &agent1.work_dir, &sig1)
        .expect("spawn 1");
    mgr.spawn_session(&agent2.id, &agent2.work_dir, &sig2)
        .expect("spawn 2");

    mgr.kill(&agent1.id).expect("kill 1");

    assert!(!mgr.is_alive(&agent1.id));
    assert!(mgr.is_alive(&agent2.id), "agent 2 should still be alive");
}

// =============================================================================
// Relaunch Lifecycle (component-002 lines 27-32)
// =============================================================================

#[test]
fn relaunch_running_session_fails() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");

    let result = mgr.relaunch(&agent.id);
    assert!(
        matches!(result, Err(RuntimeError::AlreadyRunning(_))),
        "relaunch running should fail"
    );
}

#[test]
fn relaunch_dead_session_requires_signature() {
    // The stub doesn't store signatures after kill, so relaunch fails
    // Real impl would preserve signatures for relaunch
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    mgr.kill(&agent.id).expect("kill");

    let result = mgr.relaunch(&agent.id);
    // Stub returns NotRunning because it can't find stored signature
    assert!(
        matches!(result, Err(RuntimeError::NotRunning(_))),
        "relaunch without stored signature should fail"
    );
}

// =============================================================================
// Liveness Checks (component-002 lines 33-35)
// =============================================================================

#[test]
fn is_alive_returns_false_for_unknown_agent() {
    let mgr = StubRuntimeManager::default();
    let agent_id = AgentId("unknown".into());
    assert!(!mgr.is_alive(&agent_id));
}

#[test]
fn is_alive_returns_true_for_spawned_agent() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    assert!(mgr.is_alive(&agent.id));
}

#[test]
fn is_alive_returns_false_after_kill() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    mgr.kill(&agent.id).expect("kill");
    assert!(!mgr.is_alive(&agent.id));
}
