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
/// Definition-driven post-preflight fresh-send assembly (issue #382 S11).
pub mod agent_fresh_send;
mod agent_launcher;
/// Definition-driven immutable local launch plan generation (issue #382 S7).
pub mod agent_plan;
/// Ordered execution preparation boundary (issue #382 CW02-09 / S10).
pub mod agent_preflight;
mod agent_probe;
mod agent_probe_parse;
mod agent_probe_process;
/// Definition-driven immutable remote launch plan generation (issue #382 S9).
pub mod agent_remote_plan;
mod agent_remote_probe;
mod async_attach;
mod attach;
/// Terminal-model event sink for a hosted agent PTY (issue #627).
mod attach_listener;
mod attach_mode_recovery;
mod attach_scheduler;
mod capabilities;
mod capture_ops;
mod command_capture;
mod commands;
mod commands_finalize;
mod errors;
/// External terminal launch boundary (issue #222).
mod external_terminal;
/// One-shot `gh auth login --web` device-code subprocess driver (issue #244).
mod gh_auth;
mod identity;
/// Narrow safe wrapper over Windows Job Object containment (issue #467 Slice 3).
#[cfg(windows)]
mod job_object;
mod jsp_launch;
/// Enter separation in the PTY input path (issue #627).
mod key_pacing;
pub mod launch_compose;
/// Declared registry of every gate in the agent launch pipeline (issue #544).
///
/// Contract: `dev-docs/standards/windows-launch-pipeline.md`.
pub mod launch_gates;
mod liveness;
/// Jefe-managed install cache for selector-backed LLxprt launches (issue #425).
mod manager;
/// Pane/worker/server identity accessors, split out of `manager.rs` (issue #543).
mod manager_identity;
mod manager_liveness;
mod manager_passthrough;
mod multiplexer;
mod multiplexer_conformance;
mod multiplexer_conformance_io;
mod multiplexer_contract;
/// Non-interactive (single-prompt, capture-stdout) agent execution (issue #214).
mod non_interactive;
mod orphan;
/// Owner-lifetime anchor for the Windows session process tree (issue #542).
///
/// Model: `dev-docs/standards/windows-session-ownership.md`.
///
/// The anchor describes the Windows `psmux server -> pane -> host -> worker`
/// topology, so the module is compiled only there, matching `job_object`.
#[cfg(windows)]
mod owner_anchor;
/// Cross-process advisory lock over the managed package install cache
/// (issue #556).
mod package_install_lock;
mod package_probe;
/// Generic package-backed invocation and preparation boundary (issue #382 S12).
pub mod package_runtime;
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
/// Session-host report of the agent worker's identity (issue #543).
pub mod worker_report;

pub use crate::agent_candidate_path::{AgentExecutablePlatform, AgentWrapperKind};
pub use agent_executable::{
    AgentExecutableError, AgentExecutableResolver, AgentExecutableTarget,
    CanonicalScriptLaunchPlan, ResolvedAgentExecutable,
};
pub use agent_execution_guard::{
    AuthorizationRejection, AuthorizationResult, AuthorizedExecution, ExecutionEvidence,
    StaleDimension, authorize_execution,
};
pub use agent_fresh_send::{
    FreshSendRejection, PreparedFreshSend, fresh_send_support, prepare_fresh_send,
};
pub use agent_launcher::{AgentLauncherError, INTERNAL_LAUNCH_ARGUMENT, run_launch_plan};
pub use agent_preflight::{
    AuthorizedLaunchPlan, InspectOutcome, LaunchProofError, PreflightCleared, PreparationOutcome,
    ProcessSandboxInspector, SandboxInspector, UnavailableReason, prepare_execution,
};
pub use agent_probe::{
    AgentProbeResult, AgentProbeTarget, run_local_agent_probe, run_local_agent_probe_with_cache,
};
pub use attach::AttachedViewer;
pub use attach_scheduler::{AttachAction, AttachScheduler, DEFAULT_DEBOUNCE};
pub use capabilities::validate_launch_request;
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
pub use key_pacing::{ENTER_INPUT_GAP, KeyWritePacing, PacedPtyInput, PtyInputKind};
pub use launch_gates::{
    GateFailureBehaviour, LaunchGate, LaunchGateDegradation, LaunchGateFailure,
    UNCONTAINED_WORKER_MODE,
};
pub use liveness::{
    LivenessIdentity, SessionLiveness, WorkerDisposition, alive_session_set, batch_liveness_check,
    batch_liveness_check_with_identity, list_jefe_sessions, observe_worker_disposition,
    parse_alive_sessions, parse_pane_alive, reconcile_dead_agents,
    reconcile_dead_agents_with_identity, session_liveness,
};
pub use manager::{
    AttachInputs, HISTORY_LINE_CAP, LivenessCheck, RuntimeManager, TmuxRuntimeManager,
    drop_viewer_in_background_pub,
};
pub use multiplexer::{
    AgentPaneLaunch, LocalPlatform, MultiplexerCapability, MultiplexerError, MultiplexerIsolation,
    MultiplexerPlan, MultiplexerVersion, ProbeObservation, classify_probe,
};
pub use multiplexer_conformance::{
    ConformanceFinding, ConformanceReport, ConformanceVerdict, MultiplexerQualification,
    ProbeOutcome, ProbePlan, classify_contract_probe, probe_ordered_items, probe_plan_for,
    probe_rank, qualification_from_report, summarize_conformance,
};
pub use multiplexer_conformance_io::{qualify_multiplexer, qualify_multiplexer_for_startup};
pub use multiplexer_contract::{
    BudgetSource, ContractCapability, ContractItem, ContractItemKind, PaneCommandBudget,
    ResponseShape, contract_item, contract_items, pane_command_budget,
};
pub use multiplexer_contract::{
    Divergence, declared_divergences, divergence, exit_empty_remediation, page_up_root_unbind,
    prefix_value_for_platform, psmux_session_routing_vars,
};
pub use non_interactive::{NON_INTERACTIVE_TIMEOUT, run_non_interactive};
/// Descendant-process observation and validated orphan-tree reaping (issue #332).
pub use orphan::{
    ObservedDescendant, OrphanClassification, PaneLiveness, ReapOutcome, capture_worker_identities,
    classify_orphan_state, descendant_liveness, descendant_still_matches_anchor,
    enumerate_descendants, reap_orphan_session, reap_orphan_tree,
};
pub use package_probe::{NpmPackageAvailabilityError, require_launch_package_available};
pub use preflight::{
    PreflightAction, PreflightIssue, execute_preflight_action, platform_engine_diagnostic,
    sandbox_preflight, sandbox_ssh_agent_warning,
};
pub use process::{
    ProcessIdentityError, ProcessLiveness, ProcessObservation, capture_process_identity,
    classify_process_observation, process_liveness,
};
pub use server_health::{
    ServerHealth, ServerIdentity, ServerInstanceToken, ServerLivenessEvidence,
    ServerLivenessObservation, classify_server_health, classify_server_liveness,
    parse_server_identity_output,
};
pub use server_health_io::observe_server_liveness;
pub use session::{RuntimeSession, TerminalCell, TerminalCellStyle, TerminalSnapshot};
// Issue #542: the Windows owner-lifetime anchor. Exported so the native psmux
// lifecycle regression exercises the production capture-and-watch path instead
// of a re-implementation of it.
// Model: `dev-docs/standards/windows-session-ownership.md`.
#[cfg(windows)]
pub use owner_anchor::{
    OWNER_LOST_EXIT_CODE, OwnerAnchor, OwnerAnchorError, OwnerLink, OwnerRole,
    capture_owner_anchor, spawn_owner_watchdog,
};
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

#[cfg(all(test, windows))]
#[path = "owner_anchor_tests.rs"]
mod owner_anchor_tests;

#[cfg(test)]
#[path = "liveness_tests.rs"]
mod liveness_tests;

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
#[path = "jsp_launch_tests.rs"]
mod jsp_launch_tests;

/// Shared test support for sealing a fixture [`AgentLaunchPlan`] into an
/// [`AuthorizedLaunchPlan`] through the real authorize + preflight proof chain.
///
/// Available only under `cfg(test)`. In-crate test modules use this to build
/// runtime launch proofs without forging private fields or adding backdoors.
#[cfg(test)]
pub(crate) mod test_support {
    use crate::domain::agent_definition::AgentLaunchPlan;
    use crate::runtime::agent_execution_guard::{
        AuthorizationResult, ExecutionEvidence, authorize_execution,
    };
    use crate::runtime::agent_preflight::{
        AuthorizedLaunchPlan, PreparationOutcome, ProcessSandboxInspector, prepare_execution,
    };

    /// Seal `plan` into an [`AuthorizedLaunchPlan`] using evidence derived from
    /// the plan's own generation-bearing fields so the (default) fixture
    /// authorizes and clears preflight trivially.
    #[must_use]
    pub fn authorized_launch_plan(plan: &AgentLaunchPlan) -> AuthorizedLaunchPlan {
        let evidence = ExecutionEvidence::new(
            plan.definition_sha256,
            plan.executable_fingerprint.clone(),
            plan.probe_generation,
            plan.target_generation,
            plan.activation_generation,
        );
        let authorized = match authorize_execution(plan, &evidence) {
            AuthorizationResult::Authorized(authorized) => authorized,
            AuthorizationResult::Rejected(error) => panic!("fixture must authorize: {error}"),
        };
        let cleared = match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
            PreparationOutcome::Cleared(cleared) => cleared,
            PreparationOutcome::Unavailable(reason) => {
                panic!("fixture must clear preflight: {reason}")
            }
        };
        AuthorizedLaunchPlan::from_cleared(cleared, plan.clone(), evidence)
            .unwrap_or_else(|error| panic!("fixture must seal: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::domain::AgentId;
    use crate::domain::agent_definition::AgentLaunchPlan;
    use crate::runtime::agent_preflight::AuthorizedLaunchPlan;
    use crate::runtime::test_support::authorized_launch_plan;

    fn fixture_plan(work_dir: &str) -> AgentLaunchPlan {
        AgentLaunchPlan {
            cwd: PathBuf::from(work_dir),
            target: crate::domain::agent_definition::Target::Remote(
                crate::domain::agent_definition::RemoteTarget {
                    user: "fixture".to_owned(),
                    host: "example.invalid".to_owned(),
                    port: None,
                    run_as_user: String::new(),
                    canonical_cwd: PathBuf::from(work_dir),
                },
            ),
            ..AgentLaunchPlan::default()
        }
    }

    /// Seal a fixture plan into an [`AuthorizedLaunchPlan`] through the real
    /// authorize + preflight proof chain, deriving matching evidence from the
    /// plan's own fields.
    fn authorized(plan: &AgentLaunchPlan) -> AuthorizedLaunchPlan {
        authorized_launch_plan(plan)
    }

    #[test]
    fn stub_spawn_and_attach() {
        let mut mgr = StubRuntimeManager::default();
        let agent_id = AgentId("test-1".into());
        let plan = authorized(&fixture_plan("/tmp"));

        if let Err(error) = mgr.spawn_session(&agent_id, &plan, None) {
            panic!("spawn should succeed: {error}");
        }
        assert!(mgr.has_session_record(&agent_id));

        if let Err(error) = mgr.attach(&agent_id) {
            panic!("attach should succeed: {error}");
        }
        assert_eq!(mgr.attached_agent(), Some(&agent_id));
    }

    #[test]
    fn stub_kill_removes_session() {
        let mut mgr = StubRuntimeManager::default();
        let agent_id = AgentId("test-1".into());
        let plan = authorized(&fixture_plan("/tmp"));

        if let Err(error) = mgr.spawn_session(&agent_id, &plan, None) {
            panic!("spawn should succeed: {error}");
        }
        if let Err(error) = mgr.kill(&agent_id) {
            panic!("kill should succeed: {error}");
        }
        assert!(!mgr.has_session_record(&agent_id));
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
        let plan = authorized(&fixture_plan("/tmp"));
        if let Err(error) = mgr.spawn_session(&agent_id, &plan, None) {
            panic!("first spawn should succeed: {error}");
        }
        let result = mgr.spawn_session(&agent_id, &plan, None);
        assert!(result.is_err());
    }

    #[test]
    fn stub_spawn_session_fresh_matches_spawn_semantics() {
        let mut mgr = StubRuntimeManager::default();
        let agent_id = AgentId("fresh-test".into());
        let plan = authorized(&fixture_plan("/tmp"));

        if let Err(error) = mgr.spawn_session_fresh(&agent_id, &plan, None) {
            panic!("fresh spawn should succeed: {error}");
        }
        assert!(mgr.has_session_record(&agent_id));

        let duplicate = mgr.spawn_session_fresh(&agent_id, &plan, None);
        assert!(duplicate.is_err());
    }

    #[test]
    fn session_name_for_agent() {
        let agent_id = AgentId("my-agent".into());
        let name = RuntimeSession::session_name_for(&agent_id);
        assert_eq!(name, "jefe-my-agent");
    }
}
