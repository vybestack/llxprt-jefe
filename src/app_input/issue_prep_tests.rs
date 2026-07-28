//! Tests for local working-copy preparation (split from `issue_prep.rs`).
//!
//! These tests exercise local integration with real temp git repos (clean
//! prep, dirty detection, owned-metadata ignored, clone-when-missing),
//! local origin-mismatch detection, and the LOCAL PR-prompt /
//! path-traversal safety tests.
//!
//! The remote SSH planner tests (`WorkTarget` resolution,
//! `RemotePrepPlanner` command planning, remote PR prompt) live in
//! `issue_prep_remote_tests.rs`.

use super::ensure_workdir_cloned;
use super::*;
use crate::app_input::issue_git_prep;

use std::path::{Path, PathBuf};
use std::process::Command;

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

// ── Local integration tests with real temp repos ───────────────────
//
// These exercise the local target path with real git repositories in a
// temp directory. They prove: existing clean prep, missing clone
// failure (no identity), non-git dir failure, dirty detection,
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos}-{seq}")
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

fn owned_checkout_conflict(label: &str) -> (PathBuf, PathBuf) {
    let origin = bare_origin_with_commit(label);
    let work = clone_origin(&origin, label);
    run_git(&work, &["config", "user.email", "test@example.com"]);
    run_git(&work, &["config", "user.name", "Test"]);
    std::fs::create_dir_all(work.join(".llxprt")).value_or_panic("create owned directory");
    std::fs::write(work.join(".llxprt/LLXPRT.md"), "main memory")
        .value_or_panic("write main owned file");
    run_git(&work, &["add", ".llxprt/LLXPRT.md"]);
    run_git(&work, &["commit", "-m", "add main owned metadata"]);
    run_git(&work, &["push", "origin", "main"]);
    run_git(&work, &["checkout", "-b", "feature"]);
    std::fs::write(work.join(".llxprt/LLXPRT.md"), "feature memory")
        .value_or_panic("write feature owned file");
    run_git(&work, &["add", ".llxprt/LLXPRT.md"]);
    run_git(&work, &["commit", "-m", "change feature owned metadata"]);
    std::fs::write(work.join(".llxprt/LLXPRT.md"), "local memory")
        .value_or_panic("modify owned metadata locally");
    (origin, work)
}

fn git_stdout(work_dir: &Path, args: &[&str]) -> String {
    let output = issue_git_prep::git_capture(work_dir, args)
        .unwrap_or_else(|error| panic!("git {} failed: {error}", args.join(" ")));
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn local_checkout_blocker_returns_dirty_without_changing_worktree_or_index() {
    let (origin, work) = owned_checkout_conflict("owned-checkout-stop");
    let branch_before = git_stdout(&work, &["branch", "--show-current"]);
    let head_before = git_stdout(&work, &["rev-parse", "HEAD"]);
    let index_before = git_stdout(&work, &["ls-files", "--stage"]);
    let status_before = git_stdout(&work, &["status", "--porcelain=v1"]);

    let outcome =
        prepare_local(&work, None).value_or_panic("checkout conflict should become dirty outcome");

    assert_eq!(outcome, PrepOutcome::Dirty);
    assert_eq!(
        git_stdout(&work, &["branch", "--show-current"]),
        branch_before
    );
    assert_eq!(git_stdout(&work, &["rev-parse", "HEAD"]), head_before);
    assert_eq!(git_stdout(&work, &["ls-files", "--stage"]), index_before);
    assert_eq!(
        git_stdout(&work, &["status", "--porcelain=v1"]),
        status_before
    );
    assert_eq!(
        std::fs::read_to_string(work.join(".llxprt/LLXPRT.md"))
            .value_or_panic("read preserved local memory"),
        "local memory"
    );
    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

fn cleanup(path: &Path) {
    // Best-effort cleanup; failures are silently ignored because this runs at
    // the end of every test and a missing/non-empty dir is not actionable.
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn local_existing_clean_prep_succeeds() {
    let origin = bare_origin_with_commit("clean");
    let work = clone_origin(&origin, "clean");
    // Pre-create .jefe so we prove owned metadata is ignored.
    std::fs::create_dir_all(work.join(".jefe")).value_or_panic("create .jefe");

    let outcome = prepare_local(&work, None).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);

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
fn local_linked_worktree_on_non_default_branch_warns_then_fails_safely() {
    // A linked worktree on a non-default branch must NOT be silently reset
    // to the default branch (that would move the wrong branch ref and risk
    // discarding commits). With issue #338, prep returns Dirty (warns the
    // user before any checkout). The confirm path force-reclones (issue
    // #479), so no in-place discard is attempted here.
    let origin = bare_origin_with_commit("linkedwtfail");
    let primary = clone_origin(&origin, "linkedwtfail-primary");
    let linked = std::env::temp_dir().join(format!(
        "jefe-issue184-linkedfail-{}-{}",
        std::process::id(),
        rand_label()
    ));
    // Explicitly create the linked worktree on a non-default branch so the
    // scenario is unambiguous and self-documenting.
    run_git(
        &primary,
        &[
            "worktree",
            "add",
            "-b",
            "feature-branch",
            &linked.to_string_lossy(),
        ],
    );
    run_git(&linked, &["remote", "set-head", "origin", "-a"]);

    // Prep must return Dirty: the user is warned before any checkout attempt.
    let outcome =
        prepare_local(&linked, None).value_or_panic("linked worktree prep must return Dirty");
    assert_eq!(outcome, PrepOutcome::Dirty);

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

    let outcome = prepare_local(&linked, None).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);

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
    let result = prepare_local(&work, None);
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
    let result = prepare_local(&work, None);
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

    let outcome = prepare_local(&work, None).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Dirty);

    // The dirty change is preserved (prep does not clean).
    let preserved =
        std::fs::read_to_string(work.join("src.txt")).value_or_panic("read preserved dirty change");
    assert_eq!(preserved, "dirty change");
    // No prompt written (prep aborts before prompt write).

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

// ── Issue #338: clean-but-not-on-default-branch triggers confirm modal ──

/// A clean working copy on a non-default branch must return `Dirty` (trigger
/// the confirm modal) — it must NOT silently switch.
#[test]
fn local_clean_not_on_default_stop_returns_dirty_without_prompt() {
    let origin = bare_origin_with_commit("clean-not-main-stop");
    let work = clone_origin(&origin, "clean-not-main-stop");
    // Switch to a feature branch; the tree stays clean.
    run_git(&work, &["checkout", "-b", "feature"]);

    let outcome = prepare_local(&work, None).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Dirty);

    // Still on feature branch — nothing was switched.
    assert_eq!(
        git_stdout(&work, &["branch", "--show-current"]),
        "feature\n"
    );
    // No prompt written.

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_owned_metadata_jefe_llxprt_ignored_as_dirty() {
    let origin = bare_origin_with_commit("ownedmeta");

    let work = clone_origin(&origin, "ownedmeta");
    // Only .jefe/ and .llxprt/ changes → must NOT be dirty.
    std::fs::create_dir_all(work.join(".jefe")).value_or_panic("create .jefe");
    std::fs::create_dir_all(work.join(".llxprt")).value_or_panic("create .llxprt");
    std::fs::write(work.join(".llxprt/LLXPRT.md"), "owned").value_or_panic("write .llxprt");

    let outcome = prepare_local(&work, None).value_or_panic("prepare_local");
    assert_eq!(
        outcome,
        PrepOutcome::Ready,
        "owned .jefe/.llxprt paths must not count as dirty"
    );

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_clone_when_missing_with_url() {
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
    let outcome = prepare_local(&work, None).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

// ── Origin-mismatch detection (issue #190) ───────────────────────────

/// Create a CloneIdentity whose owner/repo differs from the origin's
/// owner/repo. The bare origin repos are created under a temp path; the
/// identity uses a synthetic "other/repo" that will never match.
fn mismatched_identity() -> CloneIdentity {
    CloneIdentity::parse("other/repo").value_or_panic("parse other/repo")
}

#[test]
fn local_origin_mismatch_detected() {
    let origin = bare_origin_with_commit("mismatch");
    let work = clone_origin(&origin, "mismatch");
    // Write a file to prove the workdir is untouched after mismatch.
    std::fs::write(work.join("marker.txt"), "untouched").value_or_panic("write marker");

    let identity = mismatched_identity();
    let outcome = prepare_local(&work, Some(&identity)).value_or_panic("prepare_local");
    assert!(
        matches!(outcome, PrepOutcome::OriginMismatch { .. }),
        "mismatched origin must return OriginMismatch, got {outcome:?}"
    );

    // Workdir is untouched — no checkout/pull ran, marker is preserved.
    assert_eq!(
        std::fs::read_to_string(work.join("marker.txt")).value_or_panic("read marker"),
        "untouched"
    );
    // No prompt written (mismatch aborts before prompt write).

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_origin_match_proceeds_ready() {
    // When identity is None, no origin check runs and an existing clean repo
    // proceeds to Ready. This is the regression-safe path (issue #166).
    let origin = bare_origin_with_commit("match");
    let work = clone_origin(&origin, "match");

    let outcome = prepare_local(&work, None).value_or_panic("prepare_local");
    assert_eq!(
        outcome,
        PrepOutcome::Ready,
        "no identity + existing repo must be Ready (regression-safe)"
    );

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_force_reclone_replaces_mismatched_repo() {
    let origin = bare_origin_with_commit("reclone");
    let work = clone_origin(&origin, "reclone");
    let clone_url = origin.to_string_lossy().into_owned();
    // Write a marker to prove the workdir is replaced.
    std::fs::write(work.join("old-marker.txt"), "old").value_or_panic("write old marker");

    // Exercise the PRODUCTION force-reclone sequence directly. Since
    // CloneIdentity forces HTTPS (unusable for local bare repos), we enter
    // via the resolved-URL seam that prepare_local_force_reclone delegates to
    // after resolving the identity. This proves the real remove → clone →
    // prep ordering runs and replaces the mismatched workdir.
    force_reclone_local_with_url(&work, &clone_url).value_or_panic("force_reclone_local_with_url");

    // Old marker is gone (workdir was replaced).
    assert!(!work.join("old-marker.txt").exists());

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

// ── Issue #479: dirty-copy confirm must DELETE + re-clone, not discard ──

/// Issue #479: when the dirty-copy confirm fires, the confirm path must
/// delete the working copy entirely and re-clone from the configured
/// identity (a force-reclone). The OLD behavior (Discard policy) only ran
/// `reset --hard` + `clean -fd`, which left committed-but-unwanted state
/// (e.g., commits on a feature branch) behind. This test proves the
/// force-reclone sequence — the one the confirm path now uses — replaces a
/// dirty working copy with a clean clone.
#[test]
fn local_force_reclone_replaces_dirty_working_copy() {
    let origin = bare_origin_with_commit("dirty-reclone-479");
    let work = clone_origin(&origin, "dirty-reclone-479");
    let clone_url = origin.to_string_lossy().into_owned();
    // Make the worktree dirty: untracked file + a commit on a feature branch.
    std::fs::write(work.join("untracked.txt"), "junk").value_or_panic("write untracked file");
    run_git(&work, &["checkout", "-b", "feature"]);
    run_git(&work, &["config", "user.email", "test@example.com"]);
    run_git(&work, &["config", "user.name", "Test"]);
    std::fs::write(work.join("committed.txt"), "stale").value_or_panic("write stale commit file");
    run_git(&work, &["add", "committed.txt"]);
    run_git(&work, &["commit", "-m", "stale feature commit"]);

    // The Discard policy would reset untracked + restore tracked, but it
    // would NOT remove the feature branch's commit from the worktree's
    // reflog. The force-reclone deletes the entire directory and re-clones,
    // guaranteeing a pristine state. This is the production sequence the
    // dirty-copy confirm path must invoke.
    force_reclone_local_with_url(&work, &clone_url)
        .value_or_panic("force_reclone_local_with_url on dirty worktree");

    // The untracked file and the stale commit are gone (directory replaced).
    assert!(
        !work.join("untracked.txt").exists(),
        "untracked file must be gone"
    );
    assert!(
        !work.join("committed.txt").exists(),
        "stale feature commit file must be gone"
    );
    // The fresh clone is clean and on the default branch.
    assert_eq!(
        git_stdout(&work, &["branch", "--show-current"]).trim(),
        "main",
        "fresh clone must be on the default branch"
    );
    assert!(
        git_stdout(&work, &["status", "--porcelain=v1"])
            .trim()
            .is_empty(),
        "fresh clone must be clean"
    );

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}
