//! StatusSuppressHook behavioral tests.
//!
//! These tests exercise the lifecycle-aware StatusSuppressHook: pure argv
//! planning (exact private socket + exact owned session + ordered argv),
//! pre-agent no-op behavior, dynamic actual ID discovery, idempotence,
//! and failure surfacing. All use an injected recording command runner — no
//! real tmux.
//!
//! Extracted from `tests.rs` to keep file sizes under the project limit.

use crate::cli_cmd::tmux_helpers::{
    CommandRunner, StatusSuppressHook, nested_agent_session_name, status_suppress_argv,
};
use jefe::harness::CaptureHook;

/// A recording command runner that captures every invocation without
/// spawning a real process. Returns a configurable result. Uses `Rc` so that
/// clones share the same invocation log.
type RecordedInvocations = std::rc::Rc<std::cell::RefCell<Vec<(String, Vec<String>)>>>;

#[derive(Default, Debug, Clone)]
struct RecordingRunner {
    invocations: RecordedInvocations,
    fail: bool,
}

impl RecordingRunner {
    fn new(fail: bool) -> Self {
        Self {
            invocations: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            fail,
        }
    }
}

impl CommandRunner for RecordingRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<(), String> {
        self.invocations
            .borrow_mut()
            .push((program.to_string(), args.to_vec()));
        if self.fail {
            Err("socket not found".to_string())
        } else {
            Ok(())
        }
    }
}

/// Write a state.json with the given agent IDs into a temp config dir.
fn write_config_with_agents(config_dir: &std::path::Path, agent_ids: &[&str]) {
    use jefe::domain::{Agent, AgentId, AgentStatus, SandboxEngine};
    use jefe::persistence::State;
    let agents: Vec<Agent> = agent_ids
        .iter()
        .map(|id| Agent {
            id: AgentId((*id).to_string()),
            display_id: (*id).to_string(),
            repository_id: jefe::domain::RepositoryId("repo".to_string()),
            shortcut_slot: None,
            name: (*id).to_string(),
            description: String::new(),
            work_dir: std::path::PathBuf::from("/tmp"),
            profile: "default".to_string(),
            code_puppy_model: String::new(),
            code_puppy_yolo: None,
            code_puppy_quick_resume: false,
            mode_flags: Vec::new(),
            llxprt_debug: String::new(),
            pass_continue: false,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::default(),
            sandbox_flags: String::new(),
            agent_kind: jefe::domain::AgentKind::Llxprt,
            status: AgentStatus::Running,
            runtime_binding: None,
        })
        .collect();
    let state = State {
        agents,
        ..State::default_with_version()
    };
    let json = serde_json::to_string(&state).unwrap_or_else(|e| panic!("serialize: {e:?}"));
    std::fs::create_dir_all(config_dir).unwrap_or_else(|e| panic!("mkdir: {e:?}"));
    std::fs::write(config_dir.join("state.json"), json).unwrap_or_else(|e| panic!("write: {e:?}"));
}

/// Pure argv planning: the exact ordered argv targets the private socket
/// and the owned session with `status off`.
#[test]
fn status_suppress_argv_targets_exact_socket_and_session() {
    let socket = std::path::Path::new("/tmp/jefe-private.sock");
    let argv = status_suppress_argv(socket, "jefe-tutorial-agent");

    let expected = vec![
        "-S".to_string(),
        "/tmp/jefe-private.sock".to_string(),
        "set-option".to_string(),
        "-t".to_string(),
        "jefe-tutorial-agent".to_string(),
        "status".to_string(),
        "off".to_string(),
    ];
    assert_eq!(
        argv, expected,
        "argv must target exact socket and session in tmux CLI contract order"
    );
}

/// Pure argv planning: a different socket path and session name produce a
/// different argv — no hardcoding.
#[test]
fn status_suppress_argv_reflects_different_inputs() {
    let socket = std::path::Path::new("/run/user/1000/jefe.sock");
    let argv = status_suppress_argv(socket, "jefe-myagent");

    assert_eq!(argv[1], "/run/user/1000/jefe.sock");
    assert_eq!(argv[4], "jefe-myagent");
    assert_eq!(argv[5], "status");
    assert_eq!(argv[6], "off");
}

/// BLOCKER 1: Pre-agent capture is a no-op. When no agent exists in state.json,
/// the hook does NOT invoke the runner at all.
#[test]
fn pre_agent_capture_is_noop() {
    let runner = RecordingRunner::default();
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let config_dir = dir.path().join("config");
    // No state.json written → no agents.
    let mut hook = StatusSuppressHook::with_runner(
        std::path::PathBuf::from("/tmp/test.sock"),
        config_dir,
        runner.clone(),
    );

    hook.before_capture("dashboard")
        .unwrap_or_else(|e| panic!("pre-agent capture should succeed (no-op): {e}"));

    assert!(
        runner.invocations.borrow().is_empty(),
        "pre-agent capture must NOT invoke tmux"
    );
}

/// BLOCKER 1: Once an agent exists, the hook dynamically discovers the actual
/// agent ID and targets exactly that session.
#[test]
fn dynamic_actual_id_targets_exact_session() {
    let runner = RecordingRunner::default();
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let config_dir = dir.path().join("config");
    write_config_with_agents(&config_dir, &["my-dynamic-agent"]);

    let mut hook = StatusSuppressHook::with_runner(
        std::path::PathBuf::from("/tmp/test.sock"),
        config_dir,
        runner.clone(),
    );

    hook.before_capture("post-agent")
        .unwrap_or_else(|e| panic!("capture should succeed: {e}"));

    let invocations = runner.invocations.borrow();
    assert_eq!(invocations.len(), 1, "runner must be invoked once");
    let (program, args) = &invocations[0];
    assert_eq!(program, "tmux");
    // The session name must be derived from the ACTUAL agent ID, not hardcoded.
    assert_eq!(args[4], "jefe-my-dynamic-agent");
}

/// BLOCKER 1: Exact argv is passed to the runner, matching the pure planning
/// function output for the discovered agent ID.
#[test]
fn hook_passes_exact_argv_to_runner() {
    let runner = RecordingRunner::default();
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let config_dir = dir.path().join("config");
    write_config_with_agents(&config_dir, &["tutorial-agent"]);

    let mut hook = StatusSuppressHook::with_runner(
        std::path::PathBuf::from("/tmp/test.sock"),
        config_dir,
        runner.clone(),
    );

    hook.before_capture("step-1")
        .unwrap_or_else(|e| panic!("capture should succeed: {e}"));

    let invocations = runner.invocations.borrow();
    assert_eq!(invocations.len(), 1);
    let (_, args) = &invocations[0];
    assert_eq!(
        args,
        &status_suppress_argv(
            std::path::Path::new("/tmp/test.sock"),
            "jefe-tutorial-agent"
        ),
        "runner must receive the exact planned argv"
    );
}

/// BLOCKER 1: Idempotence — the runner is invoked once per agent, and
/// subsequent captures skip already-suppressed agents.
#[test]
fn hook_is_idempotent_across_captures() {
    let runner = RecordingRunner::default();
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let config_dir = dir.path().join("config");
    write_config_with_agents(&config_dir, &["tutorial-agent"]);

    let mut hook = StatusSuppressHook::with_runner(
        std::path::PathBuf::from("/tmp/test.sock"),
        config_dir,
        runner.clone(),
    );

    hook.before_capture("first")
        .unwrap_or_else(|e| panic!("first: {e}"));
    hook.before_capture("second")
        .unwrap_or_else(|e| panic!("second: {e}"));

    let invocations = runner.invocations.borrow();
    assert_eq!(
        invocations.len(),
        1,
        "runner must be invoked exactly once (idempotent)"
    );
}

/// BLOCKER 1: Failure for a known session is fatal — the hook surfaces the
/// error before the capture.
#[test]
fn hook_failure_for_known_session_is_fatal() {
    let runner = RecordingRunner::new(true);
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let config_dir = dir.path().join("config");
    write_config_with_agents(&config_dir, &["agent-99"]);

    let mut hook = StatusSuppressHook::with_runner(
        std::path::PathBuf::from("/tmp/missing.sock"),
        config_dir,
        runner,
    );

    let err = hook
        .before_capture("any-label")
        .err()
        .unwrap_or_else(|| panic!("hook should fail when runner fails"));

    assert!(
        err.contains("tmux set-option status off failed"),
        "err: {err}"
    );
    assert!(err.contains("/tmp/missing.sock"), "err: {err}");
    assert!(err.contains("jefe-agent-99"), "err: {err}");
    assert!(err.contains("socket not found"), "err: {err}");
}

/// A freshly seeded queued Tier-B agent has no nested session yet, so the
/// first dashboard capture must not invoke tmux suppression.
#[test]
fn tier_b_seeded_queued_agent_is_noop() {
    use jefe_tutorial_capture::state_seed;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("mkdir: {e:?}"));

    // Seed Tier B state (writes state.json with the known tutorial-agent ID).
    let seed = state_seed::TierBStateSeed {
        config_dir: config_dir.clone(),
        fixture_clone_path: config_dir.join("fixture-clone"),
        fixture_github_repo: "fixture/test-repo".to_string(),
        theme: "green-screen".to_string(),
        agent_name: "TutorialAgent".to_string(),
        agent_kind: jefe::domain::AgentKind::Llxprt,
    };
    state_seed::seed_tier_b_state(&seed).unwrap_or_else(|e| panic!("seed: {e:?}"));

    let runner = RecordingRunner::default();
    let mut hook = StatusSuppressHook::with_runner(
        std::path::PathBuf::from("/tmp/test.sock"),
        config_dir,
        runner.clone(),
    );

    hook.before_capture("tier-b")
        .unwrap_or_else(|e| panic!("tier-b capture should succeed: {e}"));

    let invocations = runner.invocations.borrow();
    assert!(invocations.is_empty());
}

/// Nested session naming follows the canonical runtime convention via
/// `RuntimeSession::session_name_for`.
#[test]
fn nested_agent_session_name_uses_jefe_prefix() {
    assert_eq!(
        nested_agent_session_name("tutorial-agent"),
        "jefe-tutorial-agent"
    );
    assert_eq!(nested_agent_session_name("agent-42"), "jefe-agent-42");
    assert_eq!(nested_agent_session_name(""), "jefe-");
}

/// Cross-boundary test: an AgentId containing characters that require
/// sanitization (slashes, spaces, shell metacharacters) must be sanitized
/// by delegating to the canonical `RuntimeSession::session_name_for`.
/// This proves the tool does not duplicate the naming policy and matches
/// the root runtime exactly.
#[test]
fn nested_agent_session_name_sanitizes_unsafe_characters() {
    // Slashes, spaces, dots → underscores (tmux-safe).
    assert_eq!(
        nested_agent_session_name("repo-a/branch b/agent.c"),
        "jefe-repo-a_branch_b_agent_c"
    );
    // Shell metacharacters → underscores.
    assert_eq!(nested_agent_session_name("agent$x;y"), "jefe-agent_x_y");
    // Non-ASCII → underscore.
    assert_eq!(nested_agent_session_name("café"), "jefe-caf_");
}

/// Cross-boundary consistency: the tool's `nested_agent_session_name` must
/// produce exactly the same value as the root runtime's
/// `RuntimeSession::session_name_for` for any agent id.
#[test]
fn nested_agent_session_name_matches_root_runtime() {
    let test_ids = [
        "tutorial-agent",
        "agent-42",
        "repo/branch/agent",
        "a b.c",
        "café",
        "",
    ];
    for id in test_ids {
        let tool_name = nested_agent_session_name(id);
        let root_name =
            jefe::runtime::RuntimeSession::session_name_for(&jefe::domain::AgentId(id.to_string()));
        assert_eq!(
            tool_name, root_name,
            "tool session name must match root runtime for id '{id}'"
        );
    }
}

/// The production hook (with ProcessCommandRunner) can be constructed via
/// the default `new` constructor.
#[test]
fn production_hook_constructable_via_new() {
    let hook = StatusSuppressHook::new(
        std::path::PathBuf::from("/tmp/prod.sock"),
        std::path::PathBuf::from("/tmp/config"),
    );
    let mut hook = hook;
    let _: &mut dyn CaptureHook = &mut hook;
}
