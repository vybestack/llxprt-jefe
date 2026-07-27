//! Architecture boundary policy fixture tests (issue #459, A7).
//!
//! Tests the ported architecture policy: crate-wide clippy allow detection,
//! required symbol checks, and handler-module line limits. Pure-function
//! tests use temp fixtures; the repo-level integration is covered by the
//! contract test in `tests/core/`.

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use xtask::architecture::{
    CrateWideAllow, find_crate_wide_clippy_allows, is_crate_wide_clippy_allow_line,
};

// --- crate-wide clippy allow line detection ---------------------------------

#[test]
fn inner_allow_clippy_detected() {
    assert!(is_crate_wide_clippy_allow_line(
        "#![allow(clippy::expect_used)]"
    ));
}

#[test]
fn inner_allow_clippy_multi_lint_detected() {
    assert!(is_crate_wide_clippy_allow_line(
        "#![allow(clippy::unwrap_used, clippy::expect_used)]"
    ));
}

#[test]
fn inner_cfg_attr_clippy_detected() {
    assert!(is_crate_wide_clippy_allow_line(
        "#![cfg_attr(test, allow(clippy::all))]"
    ));
}

#[test]
fn outer_allow_clippy_not_detected() {
    // `#[allow(...)]` (outer attribute) is NOT a crate-wide allow.
    assert!(!is_crate_wide_clippy_allow_line("#[allow(clippy::all)]"));
}

#[test]
fn inner_allow_non_clippy_not_detected() {
    assert!(!is_crate_wide_clippy_allow_line("#![allow(unused)]"));
}

#[test]
fn random_code_not_detected() {
    assert!(!is_crate_wide_clippy_allow_line("fn main() {}"));
}

#[test]
fn indented_inner_allow_detected() {
    // The original grep anchors on `^#!\[`, so leading whitespace would NOT
    // match. Our port trims first to be slightly more robust. Verify both the
    // canonical (unindented) form and an indented form are detected.
    assert!(is_crate_wide_clippy_allow_line("#![allow(clippy::all)]"));
    assert!(is_crate_wide_clippy_allow_line("  #![allow(clippy::all)]"));
    assert!(is_crate_wide_clippy_allow_line("\t#![allow(clippy::all)]"));
}

// --- crate-wide allow file scanning -----------------------------------------

fn write_src(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).expect("dirs");
    fs::write(path, content).expect("write");
}

#[test]
fn scan_finds_inner_clippy_allow() {
    let dir = TempDir::new().expect("temp");
    write_src(
        dir.path(),
        "src/lib.rs",
        "#![allow(clippy::expect_used)]\nfn main() {}\n",
    );
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].relative_path, "src/lib.rs");
    assert_eq!(found[0].line, 1);
    assert_eq!(found[0].attribute, "#![allow(clippy::expect_used)]");
}

#[test]
fn scan_reports_correct_line_number() {
    let dir = TempDir::new().expect("temp");
    write_src(
        dir.path(),
        "src/lib.rs",
        "//! doc comment\n//! more doc\n#![allow(clippy::all)]\nfn main() {}\n",
    );
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 3);
}

#[test]
fn scan_finds_multiple_in_one_file() {
    let dir = TempDir::new().expect("temp");
    write_src(
        dir.path(),
        "src/lib.rs",
        "#![allow(clippy::expect_used)]\n#![allow(clippy::unwrap_used)]\n",
    );
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    assert_eq!(found.len(), 2);
}

#[test]
fn scan_finds_in_tests_directory() {
    let dir = TempDir::new().expect("temp");
    write_src(
        dir.path(),
        "tests/integration.rs",
        "#![allow(clippy::unwrap_used, clippy::expect_used)]\n#[test]\nfn t() {}\n",
    );
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].relative_path, "tests/integration.rs");
}

#[test]
fn scan_ignores_outer_attributes() {
    let dir = TempDir::new().expect("temp");
    write_src(
        dir.path(),
        "src/lib.rs",
        "#[allow(clippy::all)]\nfn f() {}\n",
    );
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    assert!(
        found.is_empty(),
        "outer attributes must not be flagged: {found:?}"
    );
}

#[test]
fn scan_ignores_non_clippy_inner_allows() {
    let dir = TempDir::new().expect("temp");
    write_src(
        dir.path(),
        "src/lib.rs",
        "#![allow(unused_imports)]\nfn f() {}\n",
    );
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    assert!(found.is_empty());
}

#[test]
fn scan_skips_missing_directories() {
    let dir = TempDir::new().expect("temp");
    // No src/ or tests/ dirs at all.
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    assert!(found.is_empty());
}

#[test]
fn scan_normalizes_windows_path_separators() {
    let dir = TempDir::new().expect("temp");
    write_src(
        dir.path(),
        "src/deep/mod.rs",
        "#![allow(clippy::all)]\nfn f() {}\n",
    );
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    assert_eq!(found.len(), 1);
    // The relative path uses forward slashes on every platform.
    assert!(
        found[0].relative_path.contains("src/deep/mod.rs"),
        "expected forward-slash path, got: {}",
        found[0].relative_path
    );
}

#[test]
fn scan_crate_wide_allow_struct_shape() {
    let dir = TempDir::new().expect("temp");
    write_src(
        dir.path(),
        "src/lib.rs",
        "#![allow(clippy::all)]\nfn f() {}\n",
    );
    let (found, _infra) = find_crate_wide_clippy_allows(dir.path());
    let expected = CrateWideAllow {
        relative_path: "src/lib.rs".into(),
        line: 1,
        attribute: "#![allow(clippy::all)]".into(),
    };
    assert_eq!(found, vec![expected]);
}
