//! Runtime orchestration layer - tmux/PTY session management.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-004
//!
//! Pseudocode reference: component-002 lines 01-35

mod attach;
mod commands;
mod errors;
mod liveness;
mod manager;
mod preflight;
mod session;

pub use errors::RuntimeError;
pub use manager::{RuntimeManager, StubRuntimeManager, TmuxRuntimeManager};
pub use preflight::sandbox_ssh_agent_warning;
pub use session::{RuntimeSession, TerminalCell, TerminalCellStyle, TerminalSnapshot};

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use std::path::PathBuf;

    use super::*;
    use crate::domain::{AgentId, LaunchSignature};

    #[test]
    fn stub_spawn_and_attach() {
        let mut mgr = StubRuntimeManager::default();
        let agent_id = AgentId("test-1".into());
        let work_dir = PathBuf::from("/tmp");
        let signature = LaunchSignature {
            work_dir: work_dir.clone(),
            profile: "default".into(),
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: crate::domain::SandboxEngine::Podman,
            sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        };

        mgr.spawn_session(&agent_id, &work_dir, &signature)
            .expect("spawn should succeed");
        assert!(mgr.is_alive(&agent_id));

        mgr.attach(&agent_id).expect("attach should succeed");
        assert_eq!(mgr.attached_agent(), Some(&agent_id));
    }

    #[test]
    fn stub_kill_removes_session() {
        let mut mgr = StubRuntimeManager::default();
        let agent_id = AgentId("test-1".into());
        let work_dir = PathBuf::from("/tmp");
        let signature = LaunchSignature {
            work_dir: work_dir.clone(),
            profile: "default".into(),
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: crate::domain::SandboxEngine::Podman,
            sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        };

        mgr.spawn_session(&agent_id, &work_dir, &signature)
            .expect("spawn should succeed");
        mgr.kill(&agent_id).expect("kill should succeed");
        assert!(!mgr.is_alive(&agent_id));
    }

    #[test]
    fn stub_write_requires_attached() {
        let mut mgr = StubRuntimeManager::default();
        let result = mgr.write_input(b"test");
        assert!(result.is_err());
    }

    #[test]
    fn stub_duplicate_spawn_fails() {
        let mut mgr = StubRuntimeManager::default();
        let agent_id = AgentId("test-1".into());
        let work_dir = PathBuf::from("/tmp");
        let signature = LaunchSignature {
            work_dir: work_dir.clone(),
            profile: "default".into(),
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: crate::domain::SandboxEngine::Podman,
            sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        };

        mgr.spawn_session(&agent_id, &work_dir, &signature)
            .expect("first spawn should succeed");
        let result = mgr.spawn_session(&agent_id, &work_dir, &signature);
        assert!(result.is_err());
    }

    #[test]
    fn stub_spawn_session_fresh_matches_spawn_semantics() {
        let mut mgr = StubRuntimeManager::default();
        let agent_id = AgentId("fresh-test".into());
        let work_dir = PathBuf::from("/tmp");
        let signature = LaunchSignature {
            work_dir: work_dir.clone(),
            profile: "default".into(),
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: crate::domain::SandboxEngine::Podman,
            sandbox_flags: crate::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
        };

        mgr.spawn_session_fresh(&agent_id, &work_dir, &signature)
            .expect("fresh spawn should succeed");
        assert!(mgr.is_alive(&agent_id));

        let duplicate = mgr.spawn_session_fresh(&agent_id, &work_dir, &signature);
        assert!(duplicate.is_err());
    }

    #[test]
    fn session_name_for_agent() {
        let agent_id = AgentId("my-agent".into());
        let name = RuntimeSession::session_name_for(&agent_id);
        assert_eq!(name, "jefe-my-agent");
    }
}
