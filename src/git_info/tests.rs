//! Tests for the git_info module — URL parsing (pure) and GitRepoInfo formatting.

use super::*;

#[test]
fn parse_ssh_url() {
    assert_eq!(
        origin_display_shortform("git@github.com:vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_ssh_url_no_git_suffix() {
    assert_eq!(
        origin_display_shortform("git@github.com:vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_https_url() {
    assert_eq!(
        origin_display_shortform("https://github.com/vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_https_url_no_git_suffix() {
    assert_eq!(
        origin_display_shortform("https://github.com/vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_ssh_with_scheme() {
    assert_eq!(
        origin_display_shortform("ssh://git@github.com/vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_bare_form() {
    assert_eq!(
        origin_display_shortform("vybestack/llxprt-jefe"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_bare_form_with_git_suffix() {
    assert_eq!(
        origin_display_shortform("vybestack/llxprt-jefe.git"),
        Some("vybestack/llxprt-jefe".to_owned())
    );
}

#[test]
fn parse_empty_url_returns_none() {
    assert_eq!(origin_display_shortform(""), None);
    assert_eq!(origin_display_shortform("   "), None);
}

#[test]
fn parse_url_missing_repo_name_returns_none() {
    assert_eq!(origin_display_shortform("git@github.com:owner/"), None);
    assert_eq!(origin_display_shortform("https://github.com/owner/"), None);
}

#[test]
fn parse_url_missing_owner_returns_none() {
    assert_eq!(origin_display_shortform("git@github.com:/repo"), None);
}

#[test]
fn parse_url_with_extra_segments_returns_none() {
    assert_eq!(
        origin_display_shortform("https://github.com/owner/repo/extra"),
        None
    );
}

#[test]
fn list_suffix_both_present() {
    let info = GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
        dirty: None,
    };
    assert_eq!(info.list_suffix(), "vybestack/llxprt-jefe @ main");
}

#[test]
fn list_suffix_only_origin() {
    let info = GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: None,
        dirty: None,
    };
    assert_eq!(info.list_suffix(), "vybestack/llxprt-jefe");
}

#[test]
fn list_suffix_only_branch() {
    let info = GitRepoInfo {
        origin_shortform: None,
        branch: Some("feature-foo".to_owned()),
        dirty: None,
    };
    assert_eq!(info.list_suffix(), "@ feature-foo");
}

#[test]
fn list_suffix_neither() {
    let info = GitRepoInfo::default();
    assert_eq!(info.list_suffix(), "");
}

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

// ── parse_repository_origin: host-aware parsing (issue #190 MUST-FIX #3) ─

// ── dirty status: list_suffix formatting (issue #230) ──────────────────────

#[test]
fn list_suffix_dirty_branch_shows_marker() {
    let info = GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
        dirty: Some(true),
    };
    assert_eq!(info.list_suffix(), "vybestack/llxprt-jefe @ main *");
}

#[test]
fn list_suffix_clean_branch_no_marker() {
    let info = GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
        dirty: Some(false),
    };
    assert_eq!(info.list_suffix(), "vybestack/llxprt-jefe @ main");
}

#[test]
fn list_suffix_unknown_dirty_no_marker() {
    let info = GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: Some("main".to_owned()),
        dirty: None,
    };
    assert_eq!(info.list_suffix(), "vybestack/llxprt-jefe @ main");
}

#[test]
fn list_suffix_dirty_only_branch_shows_marker() {
    let info = GitRepoInfo {
        origin_shortform: None,
        branch: Some("feature-foo".to_owned()),
        dirty: Some(true),
    };
    assert_eq!(info.list_suffix(), "@ feature-foo *");
}

#[test]
fn list_suffix_dirty_no_branch_no_marker() {
    // Dirty marker only makes sense adjacent to a branch. Without a branch
    // there is nothing to mark, so the marker is suppressed.
    let info = GitRepoInfo {
        origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
        branch: None,
        dirty: Some(true),
    };
    assert_eq!(info.list_suffix(), "vybestack/llxprt-jefe");
}

// ── dirty status: resolve with real temp git repos (issue #230) ─────────────
//
// These use real temporary git repositories (not mocks) to prove tracked and
// untracked changes produce dirty=true while a clean worktree produces
// dirty=false. Jefe-owned .jefe/ and .llxprt/ paths must NOT count as dirty.

/// Project-standard test Result extension: unwrap with a context message
/// instead of bare `expect`/`unwrap`.
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

impl<T> TestResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}"),
        }
    }
}

/// Helper: create a temp git repo on a deterministically-named branch with an
/// initial commit, returning its path. Uses a named branch (`test-main`) so
/// tests can assert a concrete branch rather than guessing `master`/`main`.
fn temp_git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().value_or_panic("create git test tempdir");
    let path = dir.path();
    // `-b` is supported since git 2.28 (2020). Rename via symbolic-ref as a
    // fallback for any older git that ignores -b.
    run_git(path, &["init", "--quiet", "-b", "test-main"]);
    // Ensure the branch is test-main regardless of git version behavior.
    run_git(path, &["symbolic-ref", "HEAD", "refs/heads/test-main"]);
    run_git(path, &["config", "user.email", "test@test.test"]);
    run_git(path, &["config", "user.name", "Test"]);
    run_git(path, &["config", "commit.gpgsign", "false"]);
    std::fs::write(path.join("README.md"), "hello\n").value_or_panic("write README");
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
        .value_or_panic(&format!("spawn git {args:?}"));
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
            .value_or_panic(&format!("mkdir parent for {}", path.display()));
    }
    std::fs::write(path, content).value_or_panic(&format!("write {}", path.display()));
}

fn create_dir(path: &Path) {
    std::fs::create_dir_all(path).value_or_panic(&format!("mkdir {}", path.display()));
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

// ── porcelain_is_dirty: raw NUL-separated (-z) synthetic tests ─────────────
//
// Production now runs `git status --porcelain=v1 -z`, which emits NUL-delimited
// records with REVERSED rename/copy path order (destination THEN source), e.g.
//   `R  new.txt\0old.txt\0`
// These tests pin the -z parsing path directly so it is covered even when the
// real-repo tests below don't exercise a particular rename direction.

#[test]
fn z_clean_porcelain_is_not_dirty() {
    assert!(!porcelain_is_dirty(""));
    assert!(!porcelain_is_dirty("\u{0000}\u{0000}"));
}

#[test]
fn z_untracked_real_file_is_dirty() {
    assert!(porcelain_is_dirty("?? src/lib.rs\u{0000}"));
}

#[test]
fn z_untracked_jefe_arrow_filename_is_not_dirty() {
    // A real untracked file named `.jefe/foo -> bar` must be ignored. With -z,
    // git does NOT insert the ` -> ` rename separator for untracked entries,
    // so this is a single owned path.
    assert!(!porcelain_is_dirty("?? .jefe/foo -> bar\u{0000}"));
}

#[test]
fn z_untracked_llxprt_arrow_filename_is_not_dirty() {
    assert!(!porcelain_is_dirty("?? .llxprt/foo -> bar\u{0000}"));
}

#[test]
fn z_untracked_src_arrow_filename_is_dirty() {
    // A real untracked `src/foo -> bar` is dirty even though the path
    // contains ` -> `. The -z parser must NOT misread this as a rename.
    assert!(porcelain_is_dirty("?? src/foo -> bar\u{0000}"));
}

#[test]
fn z_modified_tracked_file_is_dirty() {
    assert!(porcelain_is_dirty(" M Cargo.toml\u{0000}"));
}

#[test]
fn z_only_jefe_paths_not_dirty() {
    assert!(!porcelain_is_dirty("?? .jefe/issue-prompt.md\u{0000}"));
    assert!(!porcelain_is_dirty(" M .jefe/something\u{0000}"));
}

#[test]
fn z_only_llxprt_paths_not_dirty() {
    assert!(!porcelain_is_dirty("?? .llxprt/LLXPRT.md\u{0000}"));
    assert!(!porcelain_is_dirty(" M .llxprt/session.json\u{0000}"));
}

#[test]
fn z_jefe_plus_real_change_is_dirty() {
    let porcelain = "?? .jefe/issue-prompt.md\u{0000} M src/main.rs\u{0000}";
    assert!(porcelain_is_dirty(porcelain));
}

#[test]
fn z_rename_both_owned_is_not_dirty() {
    // -z format: destination THEN source, NUL-delimited.
    // R  .jefe/new.md \0 .jefe/old.md \0  → both owned → ignored.
    assert!(!porcelain_is_dirty(
        "R  .jefe/new.md\u{0000}.jefe/old.md\u{0000}"
    ));
    assert!(!porcelain_is_dirty("R  .jefe/b\u{0000}.llxprt/a\u{0000}"));
}

#[test]
fn z_copy_both_owned_is_not_dirty() {
    assert!(!porcelain_is_dirty("C  .jefe/new\u{0000}.jefe/old\u{0000}"));
}

#[test]
fn z_rename_real_to_real_is_dirty() {
    // destination=src/new.txt, source=src/old.txt → both real → dirty.
    assert!(porcelain_is_dirty(
        "R  src/new.txt\u{0000}src/old.txt\u{0000}"
    ));
}

#[test]
fn z_rename_owned_to_real_is_dirty() {
    // destination=src/new.txt (real), source=.jefe/old.md (owned) → dirty.
    assert!(porcelain_is_dirty(
        "R  src/new.txt\u{0000}.jefe/old.md\u{0000}"
    ));
}

#[test]
fn z_rename_real_to_owned_is_dirty() {
    // destination=.jefe/x.md (owned), source=old.txt (real) → dirty.
    assert!(porcelain_is_dirty("R  .jefe/x.md\u{0000}old.txt\u{0000}"));
}

#[test]
fn z_copy_owned_to_real_is_dirty() {
    assert!(porcelain_is_dirty(
        "C  src/new.txt\u{0000}.jefe/old.md\u{0000}"
    ));
}

#[test]
fn z_copy_real_to_owned_is_dirty() {
    assert!(porcelain_is_dirty("C  .jefe/x.md\u{0000}old.txt\u{0000}"));
}

#[test]
fn z_rename_with_status_xy_prefixes_dirty() {
    // RM / RA prefixes: first char is the rename indicator.
    assert!(porcelain_is_dirty(
        "RM src/new.txt\u{0000}src/old.txt\u{0000}"
    ));
}

#[test]
fn z_quoted_paths_handled() {
    // -z never quotes paths (NUL delimiter makes quoting unnecessary), but
    // the parser must still tolerate a leading quote if present.
    assert!(porcelain_is_dirty("?? \"src/weird name.rs\"\u{0000}"));
    assert!(!porcelain_is_dirty("?? \".jefe/weird name.md\"\u{0000}"));
}

#[test]
fn z_mixed_records_real_after_owned_is_dirty() {
    // owned untracked + real modified in one -z stream.
    let porcelain = "?? .jefe/a\u{0000} M src/lib.rs\u{0000}";
    assert!(porcelain_is_dirty(porcelain));
}

#[test]
fn z_truncated_rename_fails_dirty() {
    // A rename status whose second path is missing (truncated stream) must
    // NOT be silently reported as clean. Fail-safe = dirty.
    assert!(porcelain_is_dirty("R  src/new.txt\u{0000}"));
}

#[test]
fn z_trailing_empty_record_ignored() {
    // The -z terminator leaves a trailing empty field; it must not be
    // treated as a real change.
    assert!(!porcelain_is_dirty("?? .jefe/a\u{0000}\u{0000}"));
}

// ── Y-column rename/copy detection (issue #230 review finding) ───────────
//
// Porcelain v1 uses two status columns: X (staged) and Y (worktree). A
// rename or copy can appear in EITHER column. Records like " R" or " C"
// (staged clean, worktree renamed) have a space in X but R/C in Y. The
// parser must check BOTH columns so worktree-only renames are not missed
// (which would leave the second path unconsumed and desynchronize parsing).

#[test]
fn z_y_column_rename_real_to_real_is_dirty() {
    // Worktree-only rename: X=' ', Y='R'. Both paths are real.
    // -z format: destination THEN source.
    assert!(porcelain_is_dirty(
        " R src/new.txt\u{0000}src/old.txt\u{0000}"
    ));
}

#[test]
fn z_y_column_copy_real_to_real_is_dirty() {
    // Worktree-only copy: X=' ', Y='C'.
    assert!(porcelain_is_dirty(
        " C src/new.txt\u{0000}src/old.txt\u{0000}"
    ));
}

#[test]
fn z_y_column_rename_both_owned_is_not_dirty() {
    // Worktree-only rename where both paths are owned → ignored.
    assert!(!porcelain_is_dirty(
        " R .jefe/new.md\u{0000}.jefe/old.md\u{0000}"
    ));
}

#[test]
fn z_y_column_rename_owned_to_real_is_dirty() {
    // Worktree-only rename: owned→real is dirty.
    assert!(porcelain_is_dirty(
        " R src/new.txt\u{0000}.jefe/old.md\u{0000}"
    ));
}

#[test]
fn z_y_column_rename_real_to_owned_is_dirty() {
    // Worktree-only rename: real→owned is dirty.
    assert!(porcelain_is_dirty(" R .jefe/x.md\u{0000}old.txt\u{0000}"));
}

#[test]
fn z_y_column_rename_consumes_second_path() {
    // If the parser misses the Y-column R/C and doesn't consume the second
    // path, the next record will be misread. This test places a real
    // worktree-only rename followed by a separate owned untracked file.
    // The second path of the rename must be consumed, not misread as a
    // standalone record.
    let porcelain = " R src/new.txt\u{0000}src/old.txt\u{0000}?? .jefe/a\u{0000}";
    assert!(porcelain_is_dirty(porcelain));
}

#[test]
fn newline_y_column_rename_real_to_real_is_dirty() {
    // Newline format: " R src/old.rs -> src/new.rs" (worktree-only rename).
    assert!(porcelain_is_dirty(" R src/old.rs -> src/new.rs\n"));
}

#[test]
fn newline_y_column_rename_both_owned_is_not_dirty() {
    // Worktree-only rename where both paths are owned → ignored.
    assert!(!porcelain_is_dirty(" R .jefe/old.md -> .jefe/new.md\n"));
}

#[test]
fn newline_y_column_copy_owned_to_real_is_dirty() {
    // Worktree-only copy: owned→real is dirty.
    assert!(porcelain_is_dirty(" C src/new.txt -> .jefe/old.md\n"));
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

#[test]
fn parse_repository_origin_ssh_form() {
    assert_eq!(
        parse_repository_origin("git@github.com:acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_https_form() {
    assert_eq!(
        parse_repository_origin("https://github.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_ssh_scheme_form() {
    assert_eq!(
        parse_repository_origin("ssh://git@github.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_bare_form_has_empty_host() {
    assert_eq!(
        parse_repository_origin("acme/widgets"),
        Some(ParsedRepositoryOrigin {
            host: String::new(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_lowercases_host() {
    assert_eq!(
        parse_repository_origin("git@GitHub.COM:acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_https_uppercase_host() {
    assert_eq!(
        parse_repository_origin("https://GitHub.COM/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_gitlab_host() {
    assert_eq!(
        parse_repository_origin("https://gitlab.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "gitlab.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_attacker_host() {
    assert_eq!(
        parse_repository_origin("git@attacker.example:acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "attacker.example".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_no_git_suffix() {
    assert_eq!(
        parse_repository_origin("https://github.com/acme/widgets"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_strips_whitespace() {
    assert_eq!(
        parse_repository_origin("  git@github.com:acme/widgets.git  "),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_empty_returns_none() {
    assert!(parse_repository_origin("").is_none());
    assert!(parse_repository_origin("   ").is_none());
}

#[test]
fn parse_repository_origin_missing_owner_returns_none() {
    assert!(parse_repository_origin("git@github.com:/widgets.git").is_none());
    assert!(parse_repository_origin("https://github.com//widgets.git").is_none());
}

#[test]
fn parse_repository_origin_missing_repo_returns_none() {
    assert!(parse_repository_origin("git@github.com:acme/").is_none());
    assert!(parse_repository_origin("https://github.com/acme/").is_none());
}

#[test]
fn parse_repository_origin_extra_segments_returns_none() {
    assert!(parse_repository_origin("https://github.com/acme/widgets/extra").is_none());
}

#[test]
fn parse_repository_origin_rejects_file_scheme() {
    // file:// reads the local filesystem, NOT a remote host. It must be
    // rejected regardless of the authority string.
    assert!(parse_repository_origin("file://github.com/acme/widgets.git").is_none());
    assert!(parse_repository_origin("file:///srv/repos/widgets.git").is_none());
}

#[test]
fn parse_repository_origin_rejects_unknown_scheme() {
    // Git supports pluggable remote helpers for arbitrary schemes; an unknown
    // scheme cannot be trusted to target the named host.
    assert!(parse_repository_origin("ftp://github.com/acme/widgets.git").is_none());
    assert!(parse_repository_origin("myhelper://github.com/acme/widgets.git").is_none());
}

#[test]
fn parse_repository_origin_scheme_is_case_insensitive() {
    // HTTPS:// and https:// are the same scheme.
    assert_eq!(
        parse_repository_origin("HTTPS://github.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_accepts_git_scheme() {
    assert_eq!(
        parse_repository_origin("git://github.com/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_https_with_port_strips_port() {
    assert_eq!(
        parse_repository_origin("https://github.com:443/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "github.com".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_ipv6_literal_with_port() {
    // Bracketed IPv6 with a port: the host is the full bracketed literal and
    // the port (after ']') is stripped. This must NOT split on a colon
    // inside the address.
    assert_eq!(
        parse_repository_origin("https://[::1]:8443/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "[::1]".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_ipv6_literal_without_port() {
    // Bracketed IPv6 without a port: the full bracketed address is the host.
    // A naive rfind(':') would truncate it to "[2001:db8:" — this test pins
    // the correct behavior.
    assert_eq!(
        parse_repository_origin("https://[2001:db8::1]/acme/widgets.git"),
        Some(ParsedRepositoryOrigin {
            host: "[2001:db8::1]".to_owned(),
            owner_repo: "acme/widgets".to_owned(),
        })
    );
}

#[test]
fn parse_repository_origin_ipv6_literal_is_not_github_host() {
    // An IPv6 literal is never github.com, so origins_match must reject it.
    let parsed = parse_repository_origin("https://[::1]/acme/widgets.git");
    assert!(parsed.is_some(), "IPv6 literal must parse");
    let host = parsed.map(|p| p.host);
    assert_ne!(host.as_deref(), Some("github.com"));
}

// ── run_child_with_timeout: cross-platform subprocess timeout (issue #230) ──

#[cfg(unix)]
#[test]
fn timeout_kills_long_running_child() {
    // `sleep 30` will exceed the 3-second timeout. The helper must kill it,
    // reap it, and return None.
    let mut cmd = std::process::Command::new("sleep");
    cmd.arg("30");
    let child = cmd.spawn().value_or_panic("spawn sleep");
    let result = super::run_child_with_timeout(child, Path::new("/test"), "sleep");
    assert!(result.is_none(), "timed-out child must return None");
}

#[cfg(unix)]
#[test]
fn timeout_returns_output_for_fast_child() {
    // `true` exits immediately — the helper must return Some(output) with
    // a successful exit status.
    let mut cmd = std::process::Command::new("true");
    let child = cmd.spawn().value_or_panic("spawn true");
    let result = super::run_child_with_timeout(child, Path::new("/test"), "true");
    let output = result.value_or_panic("fast child must produce output");
    assert!(output.status.success(), "true must exit 0");
}

#[cfg(unix)]
#[test]
fn timeout_captures_stdout() {
    // `echo hello` writes to stdout — the helper must capture it.
    let mut cmd = std::process::Command::new("echo");
    cmd.arg("hello").stdout(std::process::Stdio::piped());
    let child = cmd.spawn().value_or_panic("spawn echo");
    let result = super::run_child_with_timeout(child, Path::new("/test"), "echo");
    let output = result.value_or_panic("echo must produce output");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
}

#[cfg(unix)]
#[test]
fn timeout_completes_child_exceeding_pipe_capacity() {
    // Regression for the pipe-buffer deadlock: a child that writes more than
    // the OS pipe capacity (commonly 64 KiB) before exiting would block on
    // the full pipe and never terminate under the old read-after-poll design,
    // producing a spurious timeout (None). With concurrent pipe draining, the
    // child must complete and its full output must be captured.
    //
    // `dd` is POSIX and present on Linux/macOS/BSD; it writes a deterministic
    // 256000-byte run of NUL bytes to stdout and exits 0. Invoked directly
    // (no shell), so no shell assumptions.
    let mut cmd = std::process::Command::new("dd");
    cmd.args(["if=/dev/zero", "bs=1000", "count=256"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = cmd.spawn().value_or_panic("spawn dd");
    let result = super::run_child_with_timeout(child, Path::new("/test"), "dd");
    let output = result.value_or_panic("large-output child must complete, not time out");
    assert!(
        output.status.success(),
        "dd must exit 0, got {:?}",
        output.status
    );
    assert_eq!(
        output.stdout.len(),
        256_000,
        "full stdout ({} bytes) must be captured",
        output.stdout.len()
    );
}
