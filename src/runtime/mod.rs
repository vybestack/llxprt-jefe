//! Runtime orchestration layer - tmux/PTY session management.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-004
//!
//! Pseudocode reference: component-002 lines 01-35

mod attach;
mod attach_scheduler;
mod capabilities;
mod commands;
mod errors;
/// One-shot `gh auth login --web` device-code subprocess driver (issue #244).
mod gh_auth;
mod liveness;
mod manager;
mod multiplexer;
mod pane_capture;
mod preflight;
mod session;
mod socket;
mod stub_manager;

pub use attach_scheduler::{AttachAction, AttachScheduler, DEFAULT_DEBOUNCE};
pub use capabilities::{
    AgentRuntimeCapabilities, ModelDiscovery, code_puppy_help_supports_yolo, static_capabilities,
    validate_code_puppy_launch,
};
pub use errors::RuntimeError;
pub use gh_auth::{AuthRunResult, run_device_auth};
pub use liveness::{check_remote_session_alive, check_session_alive, pid_alive};
pub use manager::{LivenessCheck, RuntimeManager, TmuxRuntimeManager};
pub use multiplexer::{
    LocalPlatform, MultiplexerCapability, MultiplexerError, MultiplexerIsolation, MultiplexerPlan,
    MultiplexerVersion, ProbeObservation, classify_probe,
};
pub use preflight::{
    PreflightAction, PreflightIssue, execute_preflight_action, platform_engine_diagnostic,
    sandbox_preflight, sandbox_ssh_agent_warning,
};
pub use session::{RuntimeSession, TerminalCell, TerminalCellStyle, TerminalSnapshot};
pub use socket::jefe_tmux_socket_path;
pub use stub_manager::StubRuntimeManager;

#[cfg(test)]
#[path = "multiplexer_tests.rs"]
mod multiplexer_tests;

#[cfg(test)]
mod tests {
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
        };

        if let Err(error) = mgr.spawn_session(&agent_id, &work_dir, &signature) {
            panic!("spawn should succeed: {error}");
        }
        assert!(mgr.is_alive(&agent_id));

        if let Err(error) = mgr.attach(&agent_id) {
            panic!("attach should succeed: {error}");
        }
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
        };

        if let Err(error) = mgr.spawn_session(&agent_id, &work_dir, &signature) {
            panic!("spawn should succeed: {error}");
        }
        if let Err(error) = mgr.kill(&agent_id) {
            panic!("kill should succeed: {error}");
        }
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
        };

        if let Err(error) = mgr.spawn_session(&agent_id, &work_dir, &signature) {
            panic!("first spawn should succeed: {error}");
        }
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
        };

        if let Err(error) = mgr.spawn_session_fresh(&agent_id, &work_dir, &signature) {
            panic!("fresh spawn should succeed: {error}");
        }
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
