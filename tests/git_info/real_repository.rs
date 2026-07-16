//! Git-info integration tests backed by real and remote repository boundaries.

use crate::support::TestResultExt;
use jefe::git_info::GitRepoInfo;
use std::path::Path;

#[test]
fn resolve_uses_github_repo_when_set() {
    let info = GitRepoInfo::resolve("acme/widgets", false, Path::new("/nonexistent"));
    assert_eq!(info.origin_shortform.as_deref(), Some("acme/widgets"));
}

#[test]
fn resolve_trims_github_repo() {
    let info = GitRepoInfo::resolve("  acme/widgets  ", false, Path::new("/nonexistent"));
    assert_eq!(info.origin_shortform.as_deref(), Some("acme/widgets"));
}

#[test]
fn resolve_skips_branch_for_remote() {
    let info = GitRepoInfo::resolve("acme/widgets", true, Path::new("/nonexistent"));
    assert_eq!(info.origin_shortform.as_deref(), Some("acme/widgets"));
    assert!(info.branch.is_none());
}

#[test]
fn resolve_empty_github_repo_falls_back_to_git_detection() {
    // /nonexistent won't be a git repo → origin_shortform should be None.
    let info = GitRepoInfo::resolve("", false, Path::new("/nonexistent"));
    assert!(info.origin_shortform.is_none());
}

// ── dirty status: resolve with real temp git repos (issue #230) ─────────────
//
// These use real temporary git repositories (not mocks) to prove tracked and
// untracked changes produce dirty=true while a clean worktree produces
// dirty=false. Jefe-owned .jefe/ and .llxprt/ paths must NOT count as dirty.

/// Helper: create a temp git repo on a deterministically-named branch with an
/// initial commit, returning its path. Uses a named branch (`test-main`) so
/// tests can assert a concrete branch rather than guessing `master`/`main`.
fn temp_git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().test_unwrap("create git test tempdir");
    let path = dir.path();
    // `-b` is supported since git 2.28 (2020). Rename via symbolic-ref as a
    // fallback for any older git that ignores -b.
    run_git(path, &["init", "--quiet", "-b", "test-main"]);
    // Ensure the branch is test-main regardless of git version behavior.
    run_git(path, &["symbolic-ref", "HEAD", "refs/heads/test-main"]);
    run_git(path, &["config", "user.email", "test@test.test"]);
    run_git(path, &["config", "user.name", "Test"]);
    run_git(path, &["config", "commit.gpgsign", "false"]);
    std::fs::write(path.join("README.md"), "hello\n").test_unwrap("write README");
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "--quiet", "-m", "init"]);
    dir
}

fn run_git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .test_unwrap(&format!("spawn git {args:?}"));
    assert!(
        output.status.success(),
        "git {args:?} failed in {}
{}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .test_unwrap(&format!("mkdir parent for {}", path.display()));
    }
    std::fs::write(path, content).test_unwrap(&format!("write {}", path.display()));
}

fn create_dir(path: &Path) {
    std::fs::create_dir_all(path).test_unwrap(&format!("mkdir {}", path.display()));
}

#[test]
fn resolve_clean_worktree_is_not_dirty() {
    let repo = temp_git_repo();
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.branch.as_deref(), Some("test-main"));
    assert_eq!(info.dirty, Some(false));
}

#[test]
fn resolve_tracked_change_is_dirty() {
    let repo = temp_git_repo();
    write_file(&repo.path().join("README.md"), "changed\n");
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.dirty, Some(true));
}

#[test]
fn resolve_untracked_file_is_dirty() {
    let repo = temp_git_repo();
    write_file(&repo.path().join("new_file.rs"), "new\n");
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.dirty, Some(true));
}

#[test]
fn resolve_only_jefe_paths_not_dirty() {
    let repo = temp_git_repo();
    create_dir(&repo.path().join(".jefe"));
    write_file(&repo.path().join(".jefe/issue-prompt.md"), "prompt\n");
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.dirty, Some(false));
}

#[test]
fn resolve_only_llxprt_paths_not_dirty() {
    let repo = temp_git_repo();
    create_dir(&repo.path().join(".llxprt"));
    write_file(&repo.path().join(".llxprt/LLXPRT.md"), "memory\n");
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.dirty, Some(false));
}

#[test]
fn resolve_jefe_plus_real_change_is_dirty() {
    let repo = temp_git_repo();
    create_dir(&repo.path().join(".jefe"));
    write_file(&repo.path().join(".jefe/x.md"), "x\n");
    write_file(&repo.path().join("src/lib.rs"), "changed\n");
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.dirty, Some(true));
}

// ── porcelain_is_dirty: real temp git repos with arrow filenames (-z prod) ─
//
// These create REAL untracked files named `foo -> bar` under .jefe/, .llxprt/,
// and src/ to prove the production `git status --porcelain=v1 -z` command
// (exercised via GitRepoInfo::resolve) correctly ignores owned arrow-named
// files while flagging real arrow-named files as dirty. This is the exact
// regression the review flagged: a naive ` -> ` split would misclassify these.
//
// Gated to non-Windows platforms: the filename `foo -> bar` contains `>`,
// which is a reserved character on Windows (CreateFile error 123). The
// production parser logic is still covered cross-platform by the synthetic
// raw porcelain -z tests above. These filesystem tests run only where the
// OS permits `>` in filenames.

#[cfg(not(windows))]
#[test]
fn resolve_real_untracked_jefe_arrow_filename_ignored() {
    let repo = temp_git_repo();
    create_dir(&repo.path().join(".jefe"));
    write_file(&repo.path().join(".jefe/foo -> bar"), "owned\n");
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.dirty, Some(false), ".jefe/foo -> bar must be ignored");
}

#[cfg(not(windows))]
#[test]
fn resolve_real_untracked_llxprt_arrow_filename_ignored() {
    let repo = temp_git_repo();
    create_dir(&repo.path().join(".llxprt"));
    write_file(&repo.path().join(".llxprt/foo -> bar"), "owned\n");
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(
        info.dirty,
        Some(false),
        ".llxprt/foo -> bar must be ignored"
    );
}

#[cfg(not(windows))]
#[test]
fn resolve_real_untracked_src_arrow_filename_dirty() {
    let repo = temp_git_repo();
    create_dir(&repo.path().join("src"));
    write_file(&repo.path().join("src/foo -> bar"), "real\n");
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.dirty, Some(true), "src/foo -> bar must be dirty");
}

#[test]
fn resolve_real_rename_both_owned_ignored() {
    // Commit two .jefe files, then git mv one onto the other. The resulting
    // R record has both paths under .jefe/ → must be ignored (not dirty).
    let repo = temp_git_repo();
    let jefe = repo.path().join(".jefe");
    create_dir(&jefe);
    write_file(&jefe.join("old.md"), "a\n");
    write_file(&jefe.join("new.md"), "b\n");
    run_git(repo.path(), &["add", ".jefe/old.md", ".jefe/new.md"]);
    run_git(repo.path(), &["commit", "--quiet", "-m", "add jefe files"]);
    run_git(repo.path(), &["mv", ".jefe/old.md", ".jefe/renamed.md"]);
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(
        info.dirty,
        Some(false),
        "rename entirely within .jefe/ must be ignored"
    );
}

#[test]
fn resolve_real_rename_owned_to_real_dirty() {
    // Commit a .jefe file, then git mv it into src/ → owned→real rename is
    // dirty (one affected path is real).
    let repo = temp_git_repo();
    create_dir(&repo.path().join(".jefe"));
    create_dir(&repo.path().join("src"));
    write_file(&repo.path().join(".jefe/old.md"), "a\n");
    run_git(repo.path(), &["add", ".jefe/old.md"]);
    run_git(repo.path(), &["commit", "--quiet", "-m", "add jefe file"]);
    run_git(repo.path(), &["mv", ".jefe/old.md", "src/moved.md"]);
    let info = GitRepoInfo::resolve("", false, repo.path());
    assert_eq!(info.dirty, Some(true), "owned→real rename must be dirty");
}

#[test]
fn resolve_remote_repo_dirty_is_none() {
    // Remote repos must not incur SSH worktree probes; dirty must be None.
    let info = GitRepoInfo::resolve("acme/widgets", true, Path::new("/nonexistent"));
    assert_eq!(info.dirty, None);
}
