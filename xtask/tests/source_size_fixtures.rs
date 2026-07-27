//! Source-size policy fixture tests (issue #459, A6).
//!
//! Ported from `scripts/check-source-file-size.sh` semantics: 1000-line hard
//! limit, 750-line warning, scan roots `src` + `tests`, stable relative-path
//! diagnostics on Windows and Unix.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use xtask::source_size::{
    DEFAULT_HARD_LIMIT, DEFAULT_WARN_LIMIT, FileLength, Policy, Violation, classify, count_lines,
    measure_files, run_with_roots,
};

/// Write a Rust file with the given line count under `root/src/`.
fn write_file(root: &Path, rel: &str, line_count: usize) -> PathBuf {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("dirs");
    }
    let content = "fn line() {}\n".repeat(line_count);
    fs::write(&path, content).expect("write");
    path
}

fn default_policy() -> Policy {
    Policy::default()
}

// --- count_lines semantics (matches wc -l) ---------------------------------

#[test]
fn count_lines_empty() {
    assert_eq!(count_lines(""), 0);
}

#[test]
fn count_lines_no_trailing_newline() {
    // "abc" has no newline → 0 lines (wc -l semantics).
    assert_eq!(count_lines("abc"), 0);
}

#[test]
fn count_lines_one_newline() {
    assert_eq!(count_lines("abc\n"), 1);
}

#[test]
fn count_lines_trailing_newline() {
    assert_eq!(count_lines("a\nb\n"), 2);
}

#[test]
fn count_lines_no_trailing_newline_two_logical_lines() {
    // "a\nb" → one newline → 1 by wc -l, even though there are two logical
    // lines. This matches the original shell script (wc -l).
    assert_eq!(count_lines("a\nb"), 1);
}

// --- classify ---------------------------------------------------------------

#[test]
fn classify_clean() {
    let policy = Policy {
        hard_limit: 100,
        warn_limit: 50,
    };
    let lengths = vec![FileLength {
        path: PathBuf::from("src/a.rs"),
        lines: 10,
    }];
    assert!(classify(&lengths, &policy).is_empty());
}

#[test]
fn classify_warning() {
    let policy = Policy {
        hard_limit: 100,
        warn_limit: 50,
    };
    let lengths = vec![FileLength {
        path: PathBuf::from("src/a.rs"),
        lines: 75,
    }];
    let violations = classify(&lengths, &policy);
    assert_eq!(violations.len(), 1);
    assert!(matches!(
        violations[0],
        Violation::Warn {
            lines: 75,
            limit: 50,
            ..
        }
    ));
}

#[test]
fn classify_hard_failure() {
    let policy = Policy {
        hard_limit: 100,
        warn_limit: 50,
    };
    let lengths = vec![FileLength {
        path: PathBuf::from("src/a.rs"),
        lines: 150,
    }];
    let violations = classify(&lengths, &policy);
    assert_eq!(violations.len(), 1);
    assert!(matches!(
        violations[0],
        Violation::Hard {
            lines: 150,
            limit: 100,
            ..
        }
    ));
}

#[test]
fn classify_boundary_warn_limit_not_flagged() {
    // Exactly at the warn limit is not a warning (uses `>`).
    let policy = Policy {
        hard_limit: 100,
        warn_limit: 50,
    };
    let lengths = vec![FileLength {
        path: PathBuf::from("src/a.rs"),
        lines: 50,
    }];
    assert!(classify(&lengths, &policy).is_empty());
}

#[test]
fn classify_boundary_hard_limit_not_flagged_as_hard() {
    // Exactly at the hard limit is not a hard violation (uses `>`), but it may
    // still be a warning if above the warn limit. Here warn == hard so it is
    // clean at the boundary.
    let policy = Policy {
        hard_limit: 100,
        warn_limit: 100,
    };
    let lengths = vec![FileLength {
        path: PathBuf::from("src/a.rs"),
        lines: 100,
    }];
    assert!(classify(&lengths, &policy).is_empty());
}

// --- measure_files ---------------------------------------------------------

#[test]
fn measure_files_counts_lines() {
    let dir = TempDir::new().expect("temp");
    let path = write_file(dir.path(), "src/lib.rs", 5);
    let lengths = measure_files(&[path]);
    assert_eq!(lengths.len(), 1);
    assert_eq!(lengths[0].lines, 5);
}

#[test]
fn measure_files_skips_unreadable() {
    let lengths = measure_files(&[PathBuf::from("/nonexistent/file.rs")]);
    assert!(lengths.is_empty());
}

// --- run_with_roots (end-to-end) -------------------------------------------

#[test]
fn clean_tree_passes() {
    let dir = TempDir::new().expect("temp");
    write_file(dir.path(), "src/lib.rs", 10);
    let roots = vec![dir.path().join("src")];
    assert!(run_with_roots(&roots, &default_policy(), dir.path()).is_ok());
}

#[test]
fn warning_does_not_fail() {
    let dir = TempDir::new().expect("temp");
    // 800 lines: above warn (750) but below hard (1000).
    write_file(dir.path(), "src/big.rs", DEFAULT_WARN_LIMIT + 50);
    let roots = vec![dir.path().join("src")];
    assert!(run_with_roots(&roots, &default_policy(), dir.path()).is_ok());
}

#[test]
fn hard_failure_fails() {
    let dir = TempDir::new().expect("temp");
    write_file(dir.path(), "src/huge.rs", DEFAULT_HARD_LIMIT + 100);
    let roots = vec![dir.path().join("src")];
    let result = run_with_roots(&roots, &default_policy(), dir.path());
    assert!(result.is_err(), "files over hard limit must fail");
}

#[test]
fn no_trailing_newline_file_handled() {
    // A file without a trailing newline must not crash the policy.
    let dir = TempDir::new().expect("temp");
    let path = dir.path().join("src").join("no_newline.rs");
    fs::create_dir_all(path.parent().unwrap()).expect("dirs");
    fs::write(&path, "fn main() {}").expect("write"); // no trailing \n
    let roots = vec![dir.path().join("src")];
    assert!(run_with_roots(&roots, &default_policy(), dir.path()).is_ok());
}

#[test]
fn nested_path_scanned() {
    let dir = TempDir::new().expect("temp");
    write_file(dir.path(), "src/deep/nested/mod.rs", 5);
    let roots = vec![dir.path().join("src")];
    assert!(run_with_roots(&roots, &default_policy(), dir.path()).is_ok());
}

#[test]
fn override_limits_via_policy() {
    let dir = TempDir::new().expect("temp");
    write_file(dir.path(), "src/lib.rs", 20);
    let roots = vec![dir.path().join("src")];
    let strict = Policy {
        hard_limit: 10,
        warn_limit: 5,
    };
    assert!(run_with_roots(&roots, &strict, dir.path()).is_err());
}

#[test]
fn missing_root_is_ok() {
    // A scan root that does not exist is silently skipped (no files to check).
    let dir = TempDir::new().expect("temp");
    let roots = vec![dir.path().join("nonexistent")];
    assert!(run_with_roots(&roots, &default_policy(), dir.path()).is_ok());
}

#[test]
fn multiple_roots_scanned() {
    let dir = TempDir::new().expect("temp");
    write_file(dir.path(), "src/lib.rs", 5);
    write_file(dir.path(), "tests/integration.rs", 5);
    let roots = vec![dir.path().join("src"), dir.path().join("tests")];
    assert!(run_with_roots(&roots, &default_policy(), dir.path()).is_ok());
}
