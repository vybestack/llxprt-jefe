//! Tests for the immutable shell-operation snapshot boundary (issue #374 S1).
//!
//! These tests verify the typed `ShellWindowInputs` snapshot/free-execute
//! boundary: a snapshot is taken under a short lock, subprocess execution
//! happens off-lock, and revalidation rejects stale owners. The tests target
//! the pure decision seams (snapshot construction, owner matching, remote
//! rejection) without live tmux.

use std::collections::HashMap;

use super::super::manager::TmuxRuntimeManager;
use super::super::*;
use super::{ShellWindowInputs, shell_window_inputs_for};
use crate::domain::{AgentId, LaunchSignature};
use crate::runtime::RuntimeSession;

fn local_signature(work_dir: &str) -> LaunchSignature {
    LaunchSignature {
        work_dir: std::path::PathBuf::from(work_dir),
        profile: "default".into(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
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
        llxprt_version: None,
    }
}

fn remote_signature() -> LaunchSignature {
    let mut sig = local_signature("/tmp/remote");
    sig.remote.enabled = true;
    sig.remote.host = "example.com".into();
    sig
}

fn manager_with_session(agent_id: &AgentId, signature: LaunchSignature) -> TmuxRuntimeManager {
    let mut mgr = TmuxRuntimeManager::new(24, 80);
    let session_name = RuntimeSession::session_name_for(agent_id);
    let mut session = RuntimeSession::new(agent_id.clone(), session_name, signature);
    session.lifecycle_generation = 7;
    mgr.sessions.insert(agent_id.clone(), session);
    mgr
}

// --- S1: snapshot construction (local/missing/remote) ---

#[test]
fn snapshot_inputs_carries_session_name_and_local_owner_for_local_agent() {
    let agent_id = AgentId("local-agent".into());
    let mgr = manager_with_session(&agent_id, local_signature("/tmp/work"));
    let inputs = shell_window_inputs_for(&mgr.sessions, &agent_id);
    assert!(inputs.is_some(), "local session yields snapshot inputs");
    let inputs = inputs.map(|i| {
        (
            i.session_name.clone(),
            i.owner.clone(),
            i.lifecycle_generation,
            i.remote_enabled,
        )
    });
    assert_eq!(
        inputs,
        Some(("jefe-local-agent".to_owned(), agent_id.clone(), 7, false))
    );
}

#[test]
fn snapshot_inputs_missing_for_unknown_agent() {
    let mgr = TmuxRuntimeManager::new(24, 80);
    let agent_id = AgentId("ghost".into());
    let inputs = shell_window_inputs_for(&mgr.sessions, &agent_id);
    assert!(
        inputs.is_none(),
        "snapshot must be None for an untracked agent"
    );
}

#[test]
fn snapshot_inputs_reports_remote_enabled_for_remote_agent() {
    let agent_id = AgentId("remote-agent".into());
    let mgr = manager_with_session(&agent_id, remote_signature());
    let inputs = shell_window_inputs_for(&mgr.sessions, &agent_id);
    assert!(
        inputs.is_some(),
        "remote session still yields snapshot inputs for owner/generation checks"
    );
    let inputs = inputs.map(|i| (i.remote_enabled, i.owner.clone(), i.session_name.clone()));
    assert_eq!(
        inputs,
        Some((true, agent_id.clone(), "jefe-remote-agent".to_owned()))
    );
}

// --- S1: execute rejection (remote, missing session) ---

#[test]
fn execute_open_rejects_remote_snapshot_without_subprocess() {
    let inputs = ShellWindowInputs {
        owner: AgentId("r".into()),
        session_name: "jefe-r".to_owned(),
        lifecycle_generation: 0,
        remote_enabled: true,
        work_dir: std::path::PathBuf::from("/tmp/remote"),
    };
    let result = inputs.execute_open();
    assert!(result.is_err(), "open must reject a remote snapshot");
    match result {
        Err(RuntimeError::SpawnFailed(msg)) => {
            assert!(
                msg.contains("remote"),
                "remote rejection message should mention remote: {msg}"
            );
        }
        other => panic!("expected SpawnFailed for remote open, got {other:?}"),
    }
}

#[test]
fn execute_select_rejects_remote_snapshot_without_subprocess() {
    let inputs = ShellWindowInputs {
        owner: AgentId("r".into()),
        session_name: "jefe-r".to_owned(),
        lifecycle_generation: 0,
        remote_enabled: true,
        work_dir: std::path::PathBuf::from("/tmp/remote"),
    };
    let result = inputs.execute_select();
    assert!(result.is_err(), "select must reject a remote snapshot");
}

// --- S1: owner revalidation (stale owner rejected) ---

#[test]
fn revalidate_accepts_matching_owner() {
    let agent_id = AgentId("owner-1".into());
    let mgr = manager_with_session(&agent_id, local_signature("/tmp/w"));
    let sessions: HashMap<AgentId, RuntimeSession> = mgr.sessions.clone();
    let inputs = shell_window_inputs_for(&mgr.sessions, &agent_id)
        .unwrap_or_else(|| panic!("expected matching shell inputs"));
    assert!(
        inputs.owner_still_matches(&sessions),
        "matching owner and session name must revalidate"
    );
}

#[test]
fn revalidate_rejects_when_owner_changed() {
    let original = AgentId("owner-orig".into());
    let next = AgentId("owner-next".into());
    let mut mgr = manager_with_session(&original, local_signature("/tmp/w"));
    // Simulate the background attach swapping the attached session to a
    // different owner while the shell operation was off-lock.
    mgr.sessions.clear();
    let session_name = RuntimeSession::session_name_for(&next);
    let session = RuntimeSession::new(next.clone(), session_name, local_signature("/tmp/w"));
    mgr.sessions.insert(next, session);
    let sessions = mgr.sessions.clone();

    let inputs = ShellWindowInputs {
        owner: original,
        session_name: "jefe-owner-orig".to_owned(),
        lifecycle_generation: 0,
        remote_enabled: false,
        work_dir: std::path::PathBuf::from("/tmp/w"),
    };
    assert!(
        !inputs.owner_still_matches(&sessions),
        "a snapshot whose owner is no longer tracked must be stale"
    );
}

#[test]
fn revalidate_rejects_when_session_name_differs() {
    // The owner agent id is tracked but its session name changed (rebind).
    let agent_id = AgentId("owner-rebind".into());
    let mut sessions = HashMap::new();
    let session = RuntimeSession::new(
        agent_id.clone(),
        "jefe-different-name".to_owned(),
        local_signature("/tmp/w"),
    );
    sessions.insert(agent_id, session);

    let inputs = ShellWindowInputs {
        owner: AgentId("owner-rebind".into()),
        session_name: "jefe-owner-rebind".to_owned(),
        lifecycle_generation: 0,
        remote_enabled: false,

        work_dir: std::path::PathBuf::from("/tmp/w"),
    };
    assert!(
        !inputs.owner_still_matches(&sessions),
        "a snapshot whose session name no longer matches must be stale"
    );
}

#[test]
fn revalidate_rejects_when_lifecycle_generation_changes() {
    let agent_id = AgentId("owner-generation".into());
    let mut mgr = manager_with_session(&agent_id, local_signature("/tmp/w"));
    let inputs = shell_window_inputs_for(&mgr.sessions, &agent_id)
        .unwrap_or_else(|| panic!("expected matching shell inputs"));
    if let Some(session) = mgr.sessions.get_mut(&agent_id) {
        session.lifecycle_generation = session.lifecycle_generation.wrapping_add(1);
    }
    assert!(!inputs.owner_still_matches(&mgr.sessions));
}

#[test]
fn dead_preview_capture_targets_agent_window_zero() {
    assert_eq!(
        crate::runtime::capture_pane_lines_args("jefe-owner:0"),
        ["capture-pane", "-p", "-t", "jefe-owner:0"]
    );
}
