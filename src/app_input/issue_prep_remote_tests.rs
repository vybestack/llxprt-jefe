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

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
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
        ..RemoteRepositorySettings::default()
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
        ..RemoteRepositorySettings::default()
    };
    assert_eq!(WorkTarget::from_remote(&remote), WorkTarget::Local);
}

// ── RemotePrepPlanner: command planning ────────────────────────────

#[test]
fn plan_uses_ssh_t_not_tt() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Git,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
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
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Git,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
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
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Git,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
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
fn plan_does_not_write_prompt_to_disk() {
    // Issue #315: the prompt content is inlined into the launch instruction
    // (-i), so no op should carry stdin_prompt bytes and no SSH command
    // should reference .jefe/ or a prompt file path.
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Git,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
    assert!(
        ops.iter().all(|op| op.stdin_prompt.is_none()),
        "no op should carry a prompt via stdin (issue #315 inlines the prompt): {ops:?}"
    );
    // No SSH command should create .jefe/ or target a prompt file.
    for op in &ops {
        for arg in &op.ssh_argv {
            assert!(
                !arg.contains(".jefe/")
                    && !arg.contains("issue-prompt")
                    && !arg.contains("pr-prompt"),
                "SSH command must not reference .jefe or prompt paths: {arg}"
            );
            assert!(
                !arg.contains("cat >"),
                "SSH command must not write a prompt via cat redirect: {arg}"
            );
        }
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
    let _ops = planner
        .plan(&PlanInputs {
            work_dir: &local_marker,
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Git,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
    assert!(
        !local_marker.exists(),
        "remote planner must not create local directories"
    );
}

#[test]
fn plan_dirty_stop_emits_no_cleanup() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Git,
            is_dirty: true,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
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
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Discard,
            presence: WorkdirPresence::Git,
            is_dirty: true,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
    // Discard: reset --hard + clean -fd first, then checkout, then prompt.
    let reset_idx = ops
        .iter()
        .position(|op| op.ssh_argv.iter().any(|a| a.contains("git reset")))
        .value_or_panic("a reset op must be planned");
    let checkout_idx = ops
        .iter()
        .position(|op| op.ssh_argv.iter().any(|a| a.contains("git checkout")))
        .value_or_panic("a checkout op must be planned");
    assert!(reset_idx < checkout_idx, "reset before checkout");
}

#[test]
fn plan_clone_when_missing() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Absent,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
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
    // ops — it must not plan checkout operations against a
    // path that was never created.
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: None,
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Absent,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
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
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Absent,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
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
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Git,
            is_dirty: false,
            not_on_default: false,
            origin_mismatch: true,
        })
        .value_or_panic("plan");
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
    let ops = planner.plan_force_reclone(Path::new(PLAN_WORK_DIR), &identity());

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
}

#[test]
fn plan_not_git_is_a_hard_error_not_empty() {
    // An existing path that is NOT a git worktree is a hard error in the live
    // runner. The planner must encode that as Err (mirroring the runner),
    // NOT silently emit an empty plan, so a planner/runner divergence would
    // be caught by tests.
    let planner = RemotePrepPlanner::new(remote_settings());
    let result = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        presence: WorkdirPresence::NotGit,
        is_dirty: false,
        not_on_default: false,
        origin_mismatch: false,
    });
    assert!(result.is_err(), "NotGit must be a hard error: {result:?}");
    let err = match result {
        Err(reason) => reason,
        Ok(ops) => panic!("expected Err, got ops: {ops:?}"),
    };
    // The static reason mentions the non-git-worktree condition.
    assert!(
        err.contains("not a git worktree"),
        "error must explain non-git worktree: {err}"
    );
}

// ── Issue #338: not-on-default-branch planner behavior ─────────────

/// Clean but not on the default branch with Stop: the planner must
/// short-circuit with no ops at all, mirroring the dirty+Stop behavior.
#[test]
fn plan_not_on_default_stop_emits_no_cleanup() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Stop,
            presence: WorkdirPresence::Git,
            is_dirty: false,
            not_on_default: true,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
    assert!(
        ops.is_empty(),
        "Stop policy on a clean non-default branch must emit no ops: {ops:?}"
    );
}

/// Clean but not on the default branch with Discard: the planner must emit
/// the checkout script (to switch to the default branch),
/// but NO reset/clean op (the tree is clean, only a branch switch is needed).
#[test]
fn plan_not_on_default_discard_emits_checkout_without_cleanup() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner
        .plan(&PlanInputs {
            work_dir: Path::new(PLAN_WORK_DIR),
            identity: Some(&identity()),
            policy: DirtyPolicy::Discard,
            presence: WorkdirPresence::Git,
            is_dirty: false,
            not_on_default: true,
            origin_mismatch: false,
        })
        .value_or_panic("plan");
    assert!(!ops.is_empty(), "Discard must emit checkout + prompt ops");
    // No dedicated cleanup op — the tree is clean, so no reset+clean step.
    // (The checkout script itself contains a reset --hard fallback for the
    // linked-worktree edge case, but that is NOT the destructive cleanup.)
    for op in &ops {
        for arg in &op.ssh_argv {
            assert!(
                !arg.contains("git clean -fd"),
                "clean non-default branch must not trigger git clean -fd: {arg}"
            );
        }
    }
    // The checkout script must be present.
    assert!(
        ops.iter()
            .any(|op| { op.ssh_argv.iter().any(|a| a.contains("git fetch origin")) }),
        "Discard must emit a fetch+checkout op: {ops:?}"
    );
}
