//! Clippy allow/expect suppression policy fixture tests (issue #459, A4).
//!
//! Migrated from `tests/core/clippy_allow_policy.rs`. The old tests spawned
//! `bash scripts/check-clippy-allows.sh` and required Python/Git/Unix
//! utilities; these tests call the Rust scanner directly, so they run natively
//! on Windows and Unix. Every positive/negative case from the original suite
//! is preserved.

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use xtask::clippy_policy::{Suppression, scan_directory, scan_source};

/// Write a fixture tree containing a single `src/lib.rs` and return its root.
fn fixture(contents: &str) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    let src = dir.path().join("src");
    fs::create_dir_all(&src).expect("src dir");
    fs::write(src.join("lib.rs"), contents).expect("fixture");
    dir
}

/// Assert the fixture is clean (no suppressions found).
fn assert_clean(contents: &str) {
    let dir = fixture(contents);
    let found = scan_directory(dir.path()).expect("scan should succeed");
    assert!(
        found.is_empty(),
        "expected no suppressions, but found {found:?}\nin source:\n{contents}"
    );
}

/// Assert the fixture triggers exactly one suppression.
fn assert_flagged(contents: &str) {
    let dir = fixture(contents);
    let found = scan_directory(dir.path()).expect("scan should succeed");
    assert_eq!(
        found.len(),
        1,
        "expected exactly one suppression, found {found:?}\nin source:\n{contents}"
    );
}

// --- clean fixtures (must NOT be flagged) -----------------------------------

#[test]
fn clean_module_passes() {
    assert_clean("//! Clean module.\nfn main() {}\n");
}

#[test]
fn non_clippy_allow_passes() {
    // `#[allow(dead_code)]` is not a clippy allow.
    assert_clean("#[allow(dead_code)]\nfn unused() {}\n");
}

#[test]
fn allow_dead_code_with_doc_string_passes() {
    // A doc string containing the word "allow" must not be mistaken for an
    // attribute.
    assert_clean("/// allow this to be documented\nfn documented() {}\n");
}

#[test]
fn block_comment_with_allow_passes() {
    assert_clean("/* allow(clippy::all) in a comment */\nfn commented() {}\n");
}

#[test]
fn string_with_bracket_passes() {
    // A `]` inside a string must not end attribute scanning early.
    assert_clean("let s = \"]\";\nfn main() {}\n");
}

#[test]
fn raw_string_with_allow_passes() {
    // Use r## so the inner r#"..."# is valid. The content is a raw string
    // literal containing what *looks* like an attribute, but it is data, not
    // an attribute, so it must not be flagged.
    assert_clean(
        r##"let s = r#"#[allow(clippy::all)]"#;
fn main() {}
"##,
    );
}

#[test]
fn lifetime_not_treated_as_char_literal() {
    // `'a` is a lifetime, not a char literal; the scanner must not skip to the
    // end of the file and miss later content.
    assert_clean("fn foo<'a>(x: &'a str) {}\n");
}

#[test]
fn char_literal_passes() {
    assert_clean("let c = '\'';\nfn main() {}\n");
}

#[test]
fn expect_dead_code_passes() {
    // `#[expect(dead_code)]` is not a clippy expect.
    assert_clean("#[expect(dead_code)]\nfn unused() {}\n");
}

// --- flagged fixtures (must be rejected) ------------------------------------

#[test]
fn outer_allow_clippy_is_rejected() {
    assert_flagged("#[allow(clippy::module_inception)]\nmod inner {}\n");
}

#[test]
fn inner_allow_clippy_is_rejected() {
    assert_flagged("#![allow(clippy::all)]\nfn main() {}\n");
}

#[test]
fn cfg_attr_allow_clippy_is_rejected() {
    assert_flagged("#[cfg_attr(test, allow(clippy::all))]\npub fn example() {}\n");
}

#[test]
fn whitespace_outer_allow_clippy_is_rejected() {
    // `#[ allow(clippy::all)]` — space between `#[` and `allow`.
    assert_flagged("#[ allow(clippy::all)]\nfn spaced() {}\n");
}

#[test]
fn whitespace_inner_allow_clippy_is_rejected() {
    // `#![ allow ( clippy::all )]` — spaces around every delimiter.
    assert_flagged("#![ allow ( clippy::all )]\nfn main() {}\n");
}

#[test]
fn whitespace_cfg_attr_allow_clippy_is_rejected() {
    assert_flagged("#[ cfg_attr(test, allow ( clippy::all )) ]\npub fn example() {}\n");
}

#[test]
fn whitespace_before_bracket_outer_allow_clippy_is_rejected() {
    // `# [allow(clippy::all)]` — space between `#` and `[`.
    assert_flagged("# [allow(clippy::all)]\nfn spaced() {}\n");
}

#[test]
fn whitespace_before_bracket_inner_allow_clippy_is_rejected() {
    // `#! [allow(clippy::all)]` — space between `#!` and `[`.
    assert_flagged("#! [allow(clippy::all)]\nfn main() {}\n");
}

#[test]
fn whitespace_before_bracket_cfg_attr_allow_clippy_is_rejected() {
    assert_flagged("# [cfg_attr(test, allow(clippy::all))]\npub fn example() {}\n");
}

#[test]
fn multiline_allow_clippy_is_rejected() {
    assert_flagged("#[\n    allow(clippy::all)\n]\npub fn example() {}\n");
}

#[test]
fn whitespace_in_clippy_path_is_rejected() {
    assert_flagged("#[allow(clippy :: all)]\nfn spaced_path() {}\n");
}

#[test]
fn inner_whitespace_in_clippy_path_is_rejected() {
    assert_flagged("#![allow(clippy :: all)]\nfn main() {}\n");
}

#[test]
fn cfg_attr_whitespace_in_clippy_path_is_rejected() {
    assert_flagged("#[cfg_attr(test, allow(clippy :: all))]\npub fn example() {}\n");
}

#[test]
fn multiline_cfg_attr_with_bracket_string_is_rejected() {
    // A `]` inside a string literal must not end attribute scanning early.
    assert_flagged(
        "#[cfg_attr(\n    test,\n    doc = \"]\",\n    allow(clippy::all)\n)]\npub fn example() {}\n",
    );
}

#[test]
fn raw_identifier_clippy_path_is_rejected() {
    assert_flagged("#[allow(r#clippy::all)]\nfn raw_clippy() {}\n");
}

#[test]
fn inner_raw_identifier_clippy_path_is_rejected() {
    assert_flagged("#![allow(r#clippy::all)]\nfn main() {}\n");
}

#[test]
fn cfg_attr_raw_identifier_clippy_path_is_rejected() {
    assert_flagged("#[cfg_attr(test, allow(r#clippy :: all))]\npub fn example() {}\n");
}

#[test]
fn multi_lint_outer_clippy_path_is_rejected() {
    assert_flagged("#[allow(dead_code, clippy::all)]\nfn multi_lint() {}\n");
}

#[test]
fn multi_lint_inner_clippy_path_is_rejected() {
    assert_flagged("#![allow(dead_code, clippy :: all)]\nfn main() {}\n");
}

#[test]
fn multi_lint_cfg_attr_clippy_path_is_rejected() {
    assert_flagged(
        "#[cfg_attr(test, allow(unused_variables, clippy::module_inception))]\npub fn example() {}\n",
    );
}

#[test]
fn multi_lint_raw_identifier_clippy_path_is_rejected() {
    assert_flagged("#[allow(dead_code, r#clippy :: all)]\nfn raw_multi() {}\n");
}

#[test]
fn expect_clippy_path_is_rejected() {
    assert_flagged("#[expect(clippy::all)]\nfn expect_clippy() {}\n");
}

#[test]
fn cfg_attr_expect_clippy_path_is_rejected() {
    assert_flagged(
        "#[cfg_attr(test, expect(dead_code, r#clippy :: all))]\nfn expect_cfg_attr() {}\n",
    );
}

#[test]
fn inner_expect_clippy_is_rejected() {
    assert_flagged("#![expect(clippy::all)]\nfn main() {}\n");
}

#[test]
fn nested_brackets_do_not_end_attribute_early() {
    // An attribute with a nested `[...]` inside must be scanned in full.
    assert_flagged("#[derive(Debug)]\n#[allow(clippy::all)]\nfn nested() {}\n");
}

#[test]
fn multiple_suppressions_in_one_file() {
    // Two distinct attributes → two suppressions.
    let dir = fixture("#[allow(clippy::all)]\nfn a() {}\n#[allow(clippy::pedantic)]\nfn b() {}\n");
    let found = scan_directory(dir.path()).expect("scan");
    assert_eq!(found.len(), 2, "expected two suppressions, found {found:?}");
}

// --- scan_source direct tests (no filesystem) -------------------------------

#[test]
fn scan_source_empty() {
    assert!(scan_source(Path::new("x.rs"), "").is_empty());
}

#[test]
fn scan_source_records_file_path() {
    let found = scan_source(
        Path::new("nested/deep.rs"),
        "#[allow(clippy::all)]\nfn x() {}\n",
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file, Path::new("nested/deep.rs"));
}

#[test]
fn scan_source_returns_attribute_text() {
    let found = scan_source(Path::new("x.rs"), "#[allow(clippy::all)]\nfn x() {}\n");
    assert_eq!(found.len(), 1);
    let Suppression { attribute, .. } = &found[0];
    assert!(attribute.contains("allow"), "attribute was: {attribute}");
    assert!(attribute.contains("clippy"), "attribute was: {attribute}");
}

#[test]
fn scan_source_does_not_match_clippy_substring_in_path() {
    // A lint path like `my_clippy::all` must NOT be classified as a clippy
    // suppression — `clippy` must be a complete path segment, not a substring.
    assert_clean("#[allow(my_clippy::all)]\nfn x() {}\n");
    assert_clean("#[allow(not_clippy::warn)]\nfn x() {}\n");
}

#[test]
fn scan_source_matches_rust_clippy_raw_identifier() {
    // `r#clippy` (raw identifier) is still a real clippy path and must match.
    assert_flagged("#[allow(r#clippy::all)]\nfn x() {}\n");
}
