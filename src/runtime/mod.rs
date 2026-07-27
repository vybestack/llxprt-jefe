//! Runtime orchestration layer - tmux/PTY session management.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-001
//! @requirement REQ-TECH-004
//!
//! Pseudocode reference: component-002 lines 01-35

mod agent_executable;
/// Pure execution authorization guard (issue #382 CW02-12 / S8).
pub mod agent_execution_guard;
mod agent_launcher;
/// Definition-driven immutable local launch plan generation (issue #382 S7).
pub mod agent_plan;
mod agent_probe;
mod agent_probe_parse;
mod agent_probe_process;
/// Definition-driven immutable remote launch plan generation (issue #382 S9).
pub mod agent_remote_plan;
mod async_attach;
mod attach;
mod attach_mode_recovery;
mod attach_scheduler;
mod capabilities;
mod capture_ops;
mod command_capture;
mod commands;
mod errors;
/// External terminal launch boundary (issue #222).
mod external_terminal;
/// One-shot `gh auth login --web` device-code subprocess driver (issue #244).
mod gh_auth;
mod identity;
/// Narrow safe wrapper over Windows Job Object containment (issue #467 Slice 3).
#[cfg(windows)]
mod job_object;
mod liveness;
/// Jefe-managed install cache for selector-backed LLxprt launches (issue #425).
mod llxprt_install;
mod manager;
mod manager_passthrough;
mod multiplexer;
/// Non-interactive (single-prompt, capture-stdout) agent execution (issue #214).
mod non_interactive;
mod orphan;
mod package_probe;
mod pane_capture;
mod preflight;
mod process;
/// Pure server-health classification contract (issue #493 Slice 1).
mod server_health;
mod server_health_io;
mod session;
/// Per-session, content-addressed Windows host staging (issue #467 Slice 1).
pub(crate) mod session_host;
/// Embedded shell-window tmux operations (issue #222).
mod shell_window;
mod socket;
mod stub_manager;

pub use agent_executable::{
    AgentExecutableError, AgentExecutablePlatform, AgentExecutableResolver, AgentExecutableTarget,
    AgentWrapperKind, CanonicalScriptLaunchPlan, ResolvedAgentExecutable,
};
pub use agent_execution_guard::{
    AuthorizationRejection, AuthorizationResult, AuthorizedExecution, ExecutionEvidence,
    StaleDimension, authorize_execution,
};
pub use agent_launcher::{AgentLauncherError, INTERNAL_LAUNCH_ARGUMENT, run_launch_plan};
pub use agent_probe::{AgentProbeResult, AgentProbeTarget, run_local_agent_probe};
pub use attach::AttachedViewer;
pub use attach_scheduler::{AttachAction, AttachScheduler, DEFAULT_DEBOUNCE};
pub use capabilities::{
    AgentRuntimeCapabilities, ModelDiscovery, code_puppy_help_supports_yolo, static_capabilities,
    validate_code_puppy_launch,
};
pub use capture_ops::snapshot_from_lines;
#[cfg(feature = "psmux-smoke")]
pub use commands::configure_prefix_for_passthrough_with_plan;
pub use commands::{build_remote_attach_plan, capture_pane_lines};
pub use errors::RuntimeError;
pub use external_terminal::{
    DesktopPlatform, ExternalTerminalError, ExternalTerminalPlan, build_external_terminal_plan,
    spawn_external_terminal,
};
pub use gh_auth::{AuthRunResult, run_device_auth};
pub use liveness::{
    LivenessIdentity, SessionLiveness, alive_session_set, batch_liveness_check,
    batch_liveness_check_with_identity, check_remote_session_alive, check_session_alive,
    parse_alive_sessions, parse_pane_alive, pid_alive, reconcile_dead_agents,
    reconcile_dead_agents_with_identity, session_liveness,
};
pub use llxprt_install::{
    LlxprtInstallError, bin_dir_for, cache_root, ensure_installed, install_dir_for,
    local_managed_bin_dir,
};
pub use manager::{
    AttachInputs, HISTORY_LINE_CAP, LivenessCheck, RuntimeManager, TmuxRuntimeManager,
    drop_viewer_in_background_pub,
};
pub use multiplexer::{
    LocalPlatform, MultiplexerCapability, MultiplexerError, MultiplexerIsolation, MultiplexerPlan,
    MultiplexerVersion, ProbeObservation, classify_probe,
};
pub use non_interactive::{NON_INTERACTIVE_TIMEOUT, run_non_interactive};
/// Descendant-process observation and validated orphan-tree reaping (issue #332).
pub use orphan::{
    ObservedDescendant, OrphanClassification, PaneLiveness, ReapOutcome, capture_worker_identities,
    classify_orphan_state, descendant_liveness, descendant_still_matches_anchor,
    enumerate_descendants, reap_orphan_session, reap_orphan_tree,
};
pub use package_probe::{
    NpmPackageAvailabilityError, require_launch_package_available, require_npm_package_available,
};
pub use preflight::{
    PreflightAction, PreflightIssue, execute_preflight_action, platform_engine_diagnostic,
    sandbox_preflight, sandbox_ssh_agent_warning,
};
pub use process::{
    ProcessIdentityError, ProcessLiveness, ProcessObservation, capture_process_identity,
    classify_process_observation, process_liveness, process_liveness_indicates_alive,
};
pub use server_health::{
    ServerHealth, ServerIdentity, ServerLivenessEvidence, ServerLivenessObservation,
    classify_server_health, classify_server_liveness, parse_server_identity_output,
};
pub use server_health_io::observe_server_liveness;
pub use session::{RuntimeSession, TerminalCell, TerminalCellStyle, TerminalSnapshot};
// Issue #467 Slice 2: re-export session-host cleanup/planning items used by the
// startup path and the manager kill path.
pub use session_host::{
    SESSION_HOST_ROOT_SEGMENT, SessionCleanupOutcome, StartupCleanupReport,
    cleanup_session_directory, startup_cleanup_session_hosts,
};
pub use shell_window::{
    SHELL_WINDOW_NAME, ShellWindowInputs, capture_shell_preview, close_all_shell_windows,
    close_shell_window, hide_shell_window, observe_shell_window_sessions, open_shell_window,
    shell_window_exists,
};
pub use socket::jefe_tmux_socket_path;
pub use stub_manager::StubRuntimeManager;

/// Issue #301 Phase 2: re-export history cache types and pane capture for
/// the async capture worker in `app_shell`.
pub use manager::history_cache::{HistoryCache, strip_trailing_rows};
pub use pane_capture::{capture_pane_history, capture_pane_lines_result};

#[cfg(test)]
#[path = "agent_executable_tests.rs"]
mod agent_executable_tests;

#[cfg(test)]
#[path = "identity_tests.rs"]
mod identity_tests;

#[cfg(test)]
#[path = "process_tests.rs"]
mod process_tests;

#[cfg(test)]
#[path = "orphan_tests.rs"]
mod orphan_tests;

#[cfg(test)]
#[path = "multiplexer_tests.rs"]
mod multiplexer_tests;

#[cfg(test)]
#[path = "session_host_tests.rs"]
mod session_host_tests;

#[cfg(all(test, windows))]
#[path = "job_object_tests.rs"]
mod job_object_tests;

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
