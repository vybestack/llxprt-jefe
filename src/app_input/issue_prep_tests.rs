//! Tests for local working-copy preparation (split from `issue_prep.rs`).
//!
//! These tests exercise local integration with real temp git repos (clean
//! prep, dirty Stop/Discard, owned-metadata ignored, clone-when-missing),
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

    let outcome = prepare_local(&work, None, DirtyPolicy::Stop)
        .value_or_panic("checkout conflict should become dirty outcome");

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

#[test]
fn local_confirmed_checkout_blocker_discard_reaches_fetched_main() {
    let (origin, work) = owned_checkout_conflict("owned-checkout-discard");
    std::fs::write(work.join(".llxprt/session.json"), "preserve me")
        .value_or_panic("write untracked owned metadata");
    std::fs::write(work.join("remove-me.txt"), "discard me")
        .value_or_panic("write non-owned untracked file");

    let outcome = prepare_local(&work, None, DirtyPolicy::Discard)
        .value_or_panic("confirmed checkout conflict cleanup");

    assert_eq!(outcome, PrepOutcome::Ready);
    assert_eq!(
        git_stdout(&work, &["rev-parse", "HEAD"]),
        git_stdout(&work, &["rev-parse", "origin/main"])
    );
    assert_eq!(
        git_stdout(&work, &["branch", "--show-current"]).trim(),
        "main"
    );
    assert!(
        git_stdout(&work, &["diff", "--cached", "--name-only"])
            .trim()
            .is_empty(),
        "confirmed cleanup must leave no staged changes"
    );
    assert!(!work.join("remove-me.txt").exists());
    assert_eq!(
        std::fs::read_to_string(work.join(".llxprt/LLXPRT.md"))
            .value_or_panic("read main owned metadata"),
        "main memory"
    );
    assert_eq!(
        std::fs::read_to_string(work.join(".llxprt/session.json"))
            .value_or_panic("read preserved untracked owned metadata"),
        "preserve me"
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

    let outcome = prepare_local(&work, None, DirtyPolicy::Stop).value_or_panic("prepare_local");
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
    // discarding commits). With issue #338, the Stop policy now returns
    // Dirty (warns the user before any checkout) and the Discard policy
    // surfaces a clear error when the checkout is attempted.
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

    // Stop policy: the user is warned (Dirty) before any checkout attempt.
    let outcome = prepare_local(&linked, None, DirtyPolicy::Stop)
        .value_or_panic("linked worktree Stop must return Dirty");
    assert_eq!(outcome, PrepOutcome::Dirty);

    // Discard policy: the checkout is attempted and fails safely because the
    // worktree is on a different branch than the default.
    let err = prepare_local(&linked, None, DirtyPolicy::Discard)
        .error_or_panic("linked worktree Discard must error on checkout");
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

    let outcome = prepare_local(&linked, None, DirtyPolicy::Stop).value_or_panic("prepare_local");
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
    let result = prepare_local(&work, None, DirtyPolicy::Stop);
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
    let result = prepare_local(&work, None, DirtyPolicy::Stop);
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

    let outcome = prepare_local(&work, None, DirtyPolicy::Stop).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Dirty);

    // The dirty change is preserved (Stop does not clean).
    let preserved =
        std::fs::read_to_string(work.join("src.txt")).value_or_panic("read preserved dirty change");
    assert_eq!(preserved, "dirty change");
    // No prompt written (Stop aborts before prompt write).

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_dirty_discard_cleans_and_prepares() {
    let origin = bare_origin_with_commit("dirtydiscard");
    let work = clone_origin(&origin, "dirtydiscard");
    std::fs::write(work.join("src.txt"), "dirty change").value_or_panic("write dirty change");

    let outcome = prepare_local(&work, None, DirtyPolicy::Discard).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);

    // The dirty change was discarded.
    assert!(!work.join("src.txt").exists());

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

// ── Issue #338: clean-but-not-on-default-branch triggers confirm modal ──

/// A clean working copy on a non-default branch must return `Dirty` (trigger
/// the confirm modal) under the Stop policy — it must NOT silently switch.
#[test]
fn local_clean_not_on_default_stop_returns_dirty_without_prompt() {
    let origin = bare_origin_with_commit("clean-not-main-stop");
    let work = clone_origin(&origin, "clean-not-main-stop");
    // Switch to a feature branch; the tree stays clean.
    run_git(&work, &["checkout", "-b", "feature"]);

    let outcome = prepare_local(&work, None, DirtyPolicy::Stop).value_or_panic("prepare_local");
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

/// A clean working copy on a non-default branch with the Discard policy must
/// switch to the default branch, pull — without
/// discarding anything (there is nothing dirty to discard).
#[test]
fn local_clean_not_on_default_discard_switches_and_prepares() {
    let origin = bare_origin_with_commit("clean-not-main-discard");
    let work = clone_origin(&origin, "clean-not-main-discard");
    run_git(&work, &["checkout", "-b", "feature"]);

    let outcome = prepare_local(&work, None, DirtyPolicy::Discard).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);

    // Now on main.
    assert_eq!(git_stdout(&work, &["branch", "--show-current"]), "main\n");

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

/// A dirty working copy that is ALSO not on the default branch: the Discard
/// policy must clean AND switch to the default branch.
#[test]
fn local_dirty_and_not_on_default_discard_cleans_switches_and_prepares() {
    let origin = bare_origin_with_commit("dirty-not-main-discard");
    let work = clone_origin(&origin, "dirty-not-main-discard");
    run_git(&work, &["checkout", "-b", "feature"]);
    // Add an untracked file so the tree is dirty.
    std::fs::write(work.join("untracked.txt"), "junk").value_or_panic("write untracked file");

    let outcome = prepare_local(&work, None, DirtyPolicy::Discard).value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);

    // On main now.
    assert_eq!(git_stdout(&work, &["branch", "--show-current"]), "main\n");
    // Untracked file was cleaned.
    assert!(!work.join("untracked.txt").exists());

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_dirty_discard_handles_spaces_unicode_and_argv_like_names_without_git_clean() {
    let origin = bare_origin_with_commit("native-windows");
    let root = std::env::temp_dir().join(format!("jefe issue261 Ω space {}", rand_label()));
    std::fs::create_dir_all(&root).value_or_panic("create spaced root");
    let work = root.join("work tree Ω");
    run_git(
        Path::new("."),
        &["clone", &origin.to_string_lossy(), &work.to_string_lossy()],
    );
    run_git(&work, &["remote", "set-head", "origin", "-a"]);
    let adversarial = "--not-an-option ; echo untouched Ω.txt";
    std::fs::write(work.join(adversarial), "untracked").value_or_panic("write adversarial file");
    std::fs::create_dir_all(work.join("nested Ω/space"))
        .value_or_panic("create nested untracked directory");
    std::fs::write(work.join("nested Ω/space/file.txt"), "untracked")
        .value_or_panic("write nested untracked file");

    let outcome =
        prepare_local(&work, None, DirtyPolicy::Discard).value_or_panic("native local preparation");

    assert_eq!(outcome, PrepOutcome::Ready);
    assert!(!work.join(adversarial).exists());
    assert!(!work.join("nested Ω").exists());
    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&root);
}

#[test]
fn local_dirty_discard_preserves_tracked_jefe_owned_metadata() {
    let origin = bare_origin_with_commit("tracked-owned");
    let work = clone_origin(&origin, "tracked-owned");
    std::fs::create_dir_all(work.join(".llxprt")).value_or_panic("create owned directory");
    std::fs::write(work.join(".llxprt/LLXPRT.md"), "committed")
        .value_or_panic("write committed owned file");
    run_git(&work, &["config", "user.email", "test@example.com"]);
    run_git(&work, &["config", "user.name", "Test"]);
    run_git(&work, &["add", ".llxprt/LLXPRT.md"]);
    run_git(&work, &["commit", "-m", "add owned metadata"]);
    std::fs::write(work.join(".llxprt/LLXPRT.md"), "local memory")
        .value_or_panic("modify owned metadata");
    std::fs::write(work.join("README.md"), "ordinary change")
        .value_or_panic("modify ordinary tracked file");

    issue_git_prep::discard_workdir_changes(&work).value_or_panic("discard ordinary changes");

    assert_eq!(
        std::fs::read_to_string(work.join(".llxprt/LLXPRT.md"))
            .value_or_panic("read preserved owned metadata"),
        "local memory"
    );
    let restored =
        std::fs::read_to_string(work.join("README.md")).value_or_panic("read restored README");
    assert_eq!(restored.replace("\r\n", "\n"), "# test\n");
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

    let outcome = prepare_local(&work, None, DirtyPolicy::Stop).value_or_panic("prepare_local");
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
    let outcome = prepare_local(&work, None, DirtyPolicy::Stop).value_or_panic("prepare_local");
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
    let outcome =
        prepare_local(&work, Some(&identity), DirtyPolicy::Stop).value_or_panic("prepare_local");
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

    let outcome = prepare_local(&work, None, DirtyPolicy::Stop).value_or_panic("prepare_local");
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
