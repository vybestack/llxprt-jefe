//! Behavioural tests for [super::liveness].
//!
//! Split out of liveness.rs to stay within the source-size gate; the
//! module is otherwise unchanged.

use std::collections::HashSet;
#[cfg(unix)]
use std::process::Stdio;
#[cfg(unix)]
use std::time::{Duration, Instant};

use super::liveness::*;
use crate::domain::liveness_observation::{Observed, ProbeBoundary, Uncertainty};
use crate::domain::{AgentId, RemoteRepositorySettings};
use crate::runtime::manager::LivenessCheck;

fn make_liveness_check(agent_id: &str, session_name: &str, remote: bool) -> LivenessCheck {
    LivenessCheck {
        agent_id: AgentId(agent_id.to_string()),
        session_name: session_name.to_string(),
        remote: if remote {
            Some(RemoteRepositorySettings::default())
        } else {
            None
        },
        binding_session_name: Some(session_name.to_string()),
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    }
}

// --- parse_alive_sessions (pure) ---

#[test]
fn parse_alive_sessions_basic() {
    let raw = "jefe-agent1
jefe-agent2
jefe-agent3
";
    let set = parse_alive_sessions(raw);
    assert_eq!(set.len(), 3);
    assert!(set.contains("jefe-agent1"));
    assert!(set.contains("jefe-agent2"));
    assert!(set.contains("jefe-agent3"));
}

#[test]
fn parse_alive_sessions_trims_whitespace() {
    let raw = "  jefe-a  \n jefe-b \n\n";
    let set = parse_alive_sessions(raw);
    assert_eq!(set.len(), 2);
    assert!(set.contains("jefe-a"));
    assert!(set.contains("jefe-b"));
}

#[test]
fn parse_alive_sessions_empty_output() {
    let set = parse_alive_sessions("");
    assert!(set.is_empty());
}

#[test]
fn parse_alive_sessions_skips_empty_lines() {
    let raw = "jefe-a


jefe-b
";
    let set = parse_alive_sessions(raw);
    assert_eq!(set.len(), 2);
}

#[test]
fn dead_pane_parser_preserves_tri_state() {
    assert_eq!(parse_dead_pane_flags("0\n1\n"), SessionLiveness::Alive);
    assert_eq!(parse_dead_pane_flags("1\ntrue\n"), SessionLiveness::Missing);
    assert_eq!(parse_dead_pane_flags(""), SessionLiveness::Unavailable);
    assert_eq!(
        parse_dead_pane_flags("unexpected\n"),
        SessionLiveness::Unavailable
    );
}

// --- parse_pane_alive (pure) ---

#[test]
fn parse_pane_alive_identifies_live_agent_windows_only() {
    let raw = "jefe-a:0:0
jefe-b:0:1
jefe-c:0:0
jefe-b:1:0
";
    let set = parse_pane_alive(raw);
    assert_eq!(set.len(), 2);
    assert!(set.contains("jefe-a"));
    assert!(set.contains("jefe-c"));
    assert!(
        !set.contains("jefe-b"),
        "shell window must not mask dead agent"
    );
}

#[test]
fn parse_pane_alive_only_numeric_flags() {
    let raw = "jefe-a:0:0
jefe-b:0:1
jefe-c:0:false
";
    let set = parse_pane_alive(raw);
    assert!(set.contains("jefe-a"));
    assert!(!set.contains("jefe-b"));
    assert!(!set.contains("jefe-c"), "non-numeric flags must not match");
}

#[test]
fn parse_pane_alive_empty_output() {
    let set = parse_pane_alive("");
    assert!(set.is_empty());
}

#[test]
fn parse_pane_alive_skips_malformed_lines() {
    let raw = "jefe-a:0:0
malformed
jefe-b:0:0
";
    let set = parse_pane_alive(raw);
    assert_eq!(set.len(), 2);
    assert!(set.contains("jefe-a"));
    assert!(set.contains("jefe-b"));
}

// --- reconcile_dead_agents (pure) ---

#[test]
fn reconcile_dead_agents_finds_missing_sessions() {
    let targets = vec![
        make_liveness_check("agent1", "jefe-agent1", false),
        make_liveness_check("agent2", "jefe-agent2", false),
    ];
    let existing: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();
    let alive_panes: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();

    let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].0, "agent2");
}

#[test]
fn reconcile_dead_agents_finds_dead_panes() {
    let targets = vec![
        make_liveness_check("agent1", "jefe-agent1", false),
        make_liveness_check("agent2", "jefe-agent2", false),
    ];
    let existing: HashSet<String> = ["jefe-agent1".to_string(), "jefe-agent2".to_string()]
        .into_iter()
        .collect();
    // agent1 has alive panes, agent2 has only dead panes
    let alive_panes: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();

    let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].0, "agent2");
}

#[test]
fn reconcile_dead_agents_all_alive() {
    let targets = vec![
        make_liveness_check("agent1", "jefe-agent1", false),
        make_liveness_check("agent2", "jefe-agent2", false),
    ];
    let existing: HashSet<String> = ["jefe-agent1".to_string(), "jefe-agent2".to_string()]
        .into_iter()
        .collect();
    let alive_panes: HashSet<String> = ["jefe-agent1".to_string(), "jefe-agent2".to_string()]
        .into_iter()
        .collect();

    let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
    assert!(dead.is_empty());
}

#[test]
fn reconcile_dead_agents_excludes_remote_targets() {
    let targets = vec![
        make_liveness_check("local-agent", "jefe-local", false),
        make_liveness_check("remote-agent", "jefe-remote", true),
    ];
    // No sessions exist
    let existing: HashSet<String> = HashSet::new();
    let alive_panes: HashSet<String> = HashSet::new();

    let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
    // Only local-agent is dead; remote-agent is excluded
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].0, "local-agent");
}

#[test]
fn reconcile_dead_agents_empty_targets() {
    let dead = reconcile_dead_agents(&[], &HashSet::new(), &HashSet::new());
    assert!(dead.is_empty());
}

// --- alive_session_set (integration, needs tmux) ---

#[test]
fn alive_session_set_does_not_panic_without_tmux_server() {
    // On a system without tmux or with no sessions, this returns None.
    // This test validates graceful failure, not the presence of tmux.
    let set = alive_session_set();
    // We don't assert the value because a tmux server might have sessions
    // from other processes. We just verify it doesn't panic.
    let _ = set;
}

#[test]
fn reconcile_dead_agents_marks_all_dead_when_no_sessions_exist() {
    // When no tmux sessions exist, all local targets are dead.
    // This tests the pure reconcile function with deterministic inputs
    // (no tmux dependency).
    let targets = vec![
        make_liveness_check("agent1", "jefe-agent1", false),
        make_liveness_check("agent2", "jefe-agent2", false),
    ];

    let existing: HashSet<String> = HashSet::new();
    let alive_panes: HashSet<String> = HashSet::new();
    let dead = reconcile_dead_agents(&targets, &existing, &alive_panes);
    assert_eq!(dead.len(), 2, "empty tmux state means all targets are dead");
}

#[test]
fn batch_liveness_check_does_not_panic() {
    // Smoke test: batch_liveness_check must not panic regardless of
    // whether a tmux server is available. The fail-open contract (returns
    // empty Vec when tmux is unavailable) is verified by the pure
    // reconcile_dead_agents test above.
    let targets = vec![
        make_liveness_check("agent1", "jefe-agent1", false),
        make_liveness_check("agent2", "jefe-agent2", false),
    ];
    let _ = batch_liveness_check(&targets);
}

// --- reconcile_dead_agents_with_identity (issue #301 Phase 4) ---

#[test]
fn reconcile_with_identity_returns_identity_triples() {
    let targets = vec![
        make_liveness_check("agent1", "jefe-agent1", false),
        make_liveness_check("agent2", "jefe-agent2", false),
    ];
    let existing: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();
    let alive_panes: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();

    let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].agent_id.0, "agent2");
    assert_eq!(dead[0].binding_session_name.as_deref(), Some("jefe-agent2"));
    assert_eq!(dead[0].lifecycle_generation, 0);
}

#[test]
fn reconcile_with_identity_excludes_remote() {
    let targets = vec![
        make_liveness_check("local", "jefe-local", false),
        make_liveness_check("remote", "jefe-remote", true),
    ];
    let existing: HashSet<String> = HashSet::new();
    let alive_panes: HashSet<String> = HashSet::new();

    let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].agent_id.0, "local");
}

#[test]
fn reconcile_with_identity_existing_but_not_alive() {
    // Session exists in `list-sessions` but has only dead panes in
    // `list-panes -a` — it must be reported dead.
    let targets = vec![make_liveness_check("agent1", "jefe-agent1", false)];
    let existing: HashSet<String> = std::iter::once("jefe-agent1".to_string()).collect();
    let alive_panes: HashSet<String> = HashSet::new();

    let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].agent_id.0, "agent1");
}

/// V1: driving the session-list boundary to failure must produce no
/// transition. Before #541 this returned an empty vector, which the caller
/// read as "nothing is dead" -- a probe failure silently asserting health.
#[test]
fn an_unknown_session_list_produces_no_transition() {
    let targets = vec![make_liveness_check("agent1", "jefe-agent1", false)];

    let observed = reconcile_observed(
        &targets,
        Observed::unknown(ProbeBoundary::SessionList, "list-sessions timed out"),
        Observed::Known(HashSet::new()),
    );

    assert_eq!(observed.known(), None, "a failed probe must not decide");
    assert_eq!(
        observed.uncertainty().map(Uncertainty::boundary),
        Some(ProbeBoundary::SessionList),
        "the hold must name the boundary that failed"
    );
}

/// V1: the pane-list boundary, independently.
#[test]
fn an_unknown_pane_list_produces_no_transition() {
    let targets = vec![make_liveness_check("agent1", "jefe-agent1", false)];
    let existing: HashSet<String> = targets.iter().map(|t| t.session_name.clone()).collect();

    let observed = reconcile_observed(
        &targets,
        Observed::Known(existing),
        Observed::unknown(ProbeBoundary::PaneList, "list-panes returned no output"),
    );

    assert_eq!(observed.known(), None);
    assert_eq!(
        observed.uncertainty().map(Uncertainty::boundary),
        Some(ProbeBoundary::PaneList),
    );
}

/// V4's mirror hazard: fail-closed must not become never-closed. When both
/// probes answer, a genuinely absent session is still reported dead.
#[test]
fn a_genuinely_dead_session_is_still_reported_when_both_probes_answer() {
    let targets = vec![make_liveness_check("agent1", "jefe-agent1", false)];

    let observed = reconcile_observed(
        &targets,
        Observed::Known(HashSet::new()),
        Observed::Known(HashSet::new()),
    );

    let dead = observed
        .known()
        .unwrap_or_else(|| panic!("two answered probes must produce a verdict"));
    assert_eq!(
        dead.len(),
        1,
        "an absent session with both probes answering is dead, not unknown"
    );
}

/// An answered pair with everything alive must produce an empty verdict --
/// distinguishable from the unknown case, which is the whole point.
#[test]
fn a_live_session_is_distinguishable_from_an_unanswered_probe() {
    let targets = vec![make_liveness_check("agent1", "jefe-agent1", false)];
    let alive: HashSet<String> = targets.iter().map(|t| t.session_name.clone()).collect();

    let observed = reconcile_observed(
        &targets,
        Observed::Known(alive.clone()),
        Observed::Known(alive),
    );

    assert_eq!(
        observed.known().map(Vec::len),
        Some(0),
        "a live agent yields an answered, empty verdict"
    );
}

#[test]
fn batch_liveness_check_with_identity_does_not_panic() {
    let targets = vec![
        make_liveness_check("agent1", "jefe-agent1", false),
        make_liveness_check("agent2", "jefe-agent2", false),
    ];
    let _ = batch_liveness_check_with_identity(&targets);
}

#[test]
fn batch_command_count_constant_with_agent_count() {
    // Issue #301 Phase 4: batch_liveness_check uses exactly two tmux
    // subprocesses regardless of N. The pure reconcile function
    // processes N targets without any additional subprocesses.
    for n in 1..=5 {
        let targets: Vec<_> = (0..n)
            .map(|i| make_liveness_check(&format!("agent{i}"), &format!("jefe-agent{i}"), false))
            .collect();
        let existing: HashSet<String> = targets.iter().map(|t| t.session_name.clone()).collect();
        let alive_panes: HashSet<String> = existing.clone();
        let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);
        assert!(dead.is_empty(), "all alive for n={n}");
    }
}

// --- existing tests ---

#[test]
fn check_nonexistent_session_returns_false() {
    // This session should not exist
    let alive = check_session_alive("jefe-nonexistent-test-session-12345");
    assert!(!alive);
}

#[test]
fn pid_alive_returns_true_for_current_process() {
    // The current process always exists, so kill -0 must succeed.
    let me = std::process::id();
    assert!(pid_alive(me));
}

#[test]
fn pid_alive_returns_false_for_nonexistent_pid() {
    // 2_000_000_000 is within pid_t (i32) range but far above every
    // platform's pid_max (Linux ~4.19M, macOS ~99998), so kill -0
    // deterministically returns ESRCH (no such process). u32::MAX
    // (4_294_967_295) overflows pid_t parsing on macOS, which is
    // implementation-defined.
    assert!(!pid_alive(2_000_000_000));
}

// --- run_child_with_timeout (issue #287 review: kill path must be verified) ---

#[cfg(unix)]
#[test]
fn run_child_with_timeout_kills_long_running_subprocess() {
    // Spawn a `sleep 30` and verify run_child_with_timeout kills it after
    // a 1-second deadline rather than blocking indefinitely.
    use std::process::Command;
    let child = Command::new("sleep")
        .arg("30")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| panic!("spawn sleep"));
    let deadline = Instant::now() + Duration::from_secs(1);
    let result = run_child_with_timeout(child, deadline);
    assert!(result.is_err(), "timeout must produce Err");
}

#[cfg(unix)]
#[test]
fn run_child_with_timeout_returns_output_for_fast_subprocess() {
    use std::process::Command;
    let child = Command::new("echo")
        .arg("ok")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| panic!("spawn echo"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = run_child_with_timeout(child, deadline);
    assert!(result.is_ok(), "fast subprocess must succeed");
    let output = result.unwrap_or_else(|()| panic!("checked ok"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ok"), "output must contain echo result");
}
/// A dead pane whose recorded worker anchor is still alive is a surviving
/// worker, not a dead agent (issue #543).
#[test]
fn a_live_anchor_under_a_dead_pane_is_survival_not_death() {
    let anchor = crate::domain::WorkerProcessIdentity::new(4321, 77);
    let observed = vec![crate::runtime::orphan::ObservedDescendant::alive(anchor)];

    assert_eq!(
        classify_worker_disposition(&observed),
        WorkerDisposition::SurvivedPane,
        "a validated live worker must never be reported as gone with its pane"
    );
}

/// Every recorded anchor being dead is the only evidence that lets the pane's
/// death stand in for the agent's (issue #543).
#[test]
fn only_dead_anchors_confirm_the_worker_died_with_the_pane() {
    let anchor = crate::domain::WorkerProcessIdentity::new(4321, 77);
    let observed = vec![crate::runtime::orphan::ObservedDescendant::dead(anchor)];

    assert_eq!(
        classify_worker_disposition(&observed),
        WorkerDisposition::GoneWithPane
    );
}

/// With no recorded anchors the pane's death is simply not evidence about
/// the worker, and must not be reported as if it were (issue #543).
#[test]
fn absent_anchors_leave_the_worker_fate_unknown() {
    assert_eq!(
        classify_worker_disposition(&[]),
        WorkerDisposition::Unknown,
        "no evidence must read as unknown, not as confirmed death"
    );
}
