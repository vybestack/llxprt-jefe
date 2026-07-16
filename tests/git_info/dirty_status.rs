//! Dirty-status display and porcelain parsing integration tests.

use jefe::git_info::{GitRepoInfo, porcelain_is_dirty};

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
    // Every path is owned, so the stream remains clean only when Y-column
    // rename detection consumes both rename paths before the ordinary record.
    // Missing the Y-column status misreads the source path as a malformed
    // standalone record and fails safe as dirty.
    let porcelain = " R .jefe/new.md\u{0000}.jefe/old.md\u{0000}?? .llxprt/session.json\u{0000}";
    assert!(!porcelain_is_dirty(porcelain));
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
