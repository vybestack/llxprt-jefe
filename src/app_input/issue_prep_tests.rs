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

fn cleanup(path: &Path) {
    // Best-effort cleanup; failures are silently ignored because this runs at
    // the end of every test and a missing/non-empty dir is not actionable.
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

    let outcome = prepare_local(&work, None, DirtyPolicy::Discard, "prompt")
        .value_or_panic("native local preparation");

    assert_eq!(outcome, PrepOutcome::Ready);
    assert!(!work.join(adversarial).exists());
    assert!(!work.join("nested Ω").exists());
    assert!(work.join(".jefe/issue-prompt.md").exists());
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
    let outcome =
        prepare_local(&work, None, DirtyPolicy::Stop, "prompt").value_or_panic("prepare_local");
    assert_eq!(outcome, PrepOutcome::Ready);
    assert!(work.join(".jefe/issue-prompt.md").exists());

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
    let outcome = prepare_local(&work, Some(&identity), DirtyPolicy::Stop, "prompt")
        .value_or_panic("prepare_local");
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
    assert!(!work.join(".jefe/issue-prompt.md").exists());

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

#[test]
fn local_origin_match_proceeds_ready() {
    // When identity is None, no origin check runs and an existing clean repo
    // proceeds to Ready. This is the regression-safe path (issue #166).
    let origin = bare_origin_with_commit("match");
    let work = clone_origin(&origin, "match");

    let outcome =
        prepare_local(&work, None, DirtyPolicy::Stop, "prompt").value_or_panic("prepare_local");
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
    force_reclone_local_with_url(&work, &clone_url, "prompt")
        .value_or_panic("force_reclone_local_with_url");

    // Old marker is gone (workdir was replaced).
    assert!(!work.join("old-marker.txt").exists());
    // Prompt is written.
    assert!(work.join(".jefe/issue-prompt.md").exists());

    cleanup(origin.parent().unwrap_or(&origin));
    cleanup(&work);
}

// ── Local PR prompt writing (reuses write_prompt_to_target) ────────────
//
// The PR send path (`prs_orchestration::dispatch_pr_agent_chooser_confirm`)
// reuses the issue-prep safe target prompt writer for writing
// `.jefe/pr-prompt.md` to the local filesystem. These tests exercise the
// local path, including adversarial content that must never appear in any
// shell argv. The remote-host path is covered in `issue_prep_remote_tests.rs`.

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
