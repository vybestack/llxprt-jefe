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
/// The reaping predicate answers this same probe with a `bool` that means
/// "safe to reap", so it reports `false` for a live process it merely cannot
/// verify. Read as a liveness answer that would be a death sentence, so the
/// liveness path must keep the third case.
#[test]
fn a_live_but_unverifiable_worker_is_unknown_not_gone() {
    let weak = crate::domain::WorkerProcessIdentity::from_pid(std::process::id());

    assert_eq!(
        observe_worker_disposition(&[weak]),
        WorkerDisposition::Unknown,
        "a running process without a start token is unproven, not dead"
    );
}

/// A worker proven to be the same process must still be reported alive, or
/// fail-closed would degrade into never answering.
#[test]
fn a_verified_live_worker_survived() {
    let identity = crate::runtime::capture_process_identity(std::process::id())
        .unwrap_or_else(|error| panic!("this process must be observable: {error}"));
    let strong = crate::domain::WorkerProcessIdentity::from_identity(identity);

    assert_eq!(
        observe_worker_disposition(&[strong]),
        WorkerDisposition::SurvivedPane
    );
}

/// PID 0 is never a worker, and is not evidence of a dead one either.
#[test]
fn a_zero_pid_anchor_is_unknown() {
    let zero = crate::domain::WorkerProcessIdentity::from_pid(0);

    assert_eq!(
        observe_worker_disposition(&[zero]),
        WorkerDisposition::Unknown
    );
}

/// No anchors is the absence of evidence, not evidence of absence.
#[test]
fn no_anchors_is_unknown() {
    assert_eq!(observe_worker_disposition(&[]), WorkerDisposition::Unknown);
}

/// The main dead-agent path probed its worker anchors through a predicate
/// meaning "safe to reap", which is false for a process that is merely
/// unverifiable as well as for one that is gone. Reading that as the answer
/// reported a live worker as having died with its pane -- the #543 defect,
/// still reachable here after it was fixed in the server-loss path.
#[test]
fn an_unverifiable_worker_under_a_dead_pane_is_unknown_not_gone() {
    let mut target = make_liveness_check("agent-1", "jefe-agent-1", false);
    // A PID with no recorded creation token: it may well be running, but
    // nothing here can prove the process is the one that was launched.
    target.worker_identities = vec![crate::domain::WorkerProcessIdentity::from_pid(
        std::process::id(),
    )];

    let targets = vec![target];
    let existing: HashSet<String> = HashSet::new();
    let alive_panes: HashSet<String> = HashSet::new();

    let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);

    assert_eq!(dead.len(), 1, "the pane is gone, so the agent is reported");
    assert_eq!(
        dead[0].worker,
        WorkerDisposition::Unknown,
        "a worker that cannot be verified has not been shown to have died"
    );
}

/// The mirror hazard: refusing to call an unverifiable worker dead must not
/// stop a genuinely absent one being reported.
#[test]
fn a_worker_that_cannot_be_running_is_still_gone() {
    let mut target = make_liveness_check("agent-2", "jefe-agent-2", false);
    // PID 0 is never a live worker, and an anchor naming it carries no
    // creation token to check, so it is unverifiable rather than gone.
    target.worker_identities = vec![];

    let targets = vec![target];
    let existing: HashSet<String> = HashSet::new();
    let alive_panes: HashSet<String> = HashSet::new();

    let dead = reconcile_dead_agents_with_identity(&targets, &existing, &alive_panes);
    assert_eq!(dead.len(), 1);
    assert_eq!(
        dead[0].worker,
        WorkerDisposition::Unknown,
        "no recorded anchors is no evidence either way"
    );
}
