//! Clippy allow policy contract tests.
//!
//! These tests verify the zero-tolerance clippy allow gate in both
//! directions:
//!
//! - The repository's own first-party Rust code must pass the gate (no
//!   clippy allow attributes outside vendor).
//! - The gate must fail when a clippy allow attribute is present, so the
//!   policy is enforced by design and not by accident.
//!
//! Negative tests inject a fixture directory via the `CLIPPY_ALLOW_SCAN_ROOT`
//! environment variable that the script honors instead of the git-tracked
//! file set.

use std::fs;
use std::process::Command;

use crate::support::TestResultExt;
use tempfile::TempDir;

const SCRIPT: &str = "scripts/check-clippy-allows.sh";

fn run_script(scan_root: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new("bash");
    cmd.arg(SCRIPT);
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
    if let Some(root) = scan_root {
        cmd.env("CLIPPY_ALLOW_SCAN_ROOT", root);
    }
    cmd.output()
        .test_unwrap("clippy allow policy script should be runnable")
}

fn write_fixture(contents: &str) -> TempDir {
    let dir = TempDir::new().test_unwrap("temp dir should be created");
    let src = dir.path().join("src");
    fs::create_dir_all(&src).test_unwrap("src dir should be created");
    fs::write(src.join("lib.rs"), contents).test_unwrap("fixture file should be written");
    dir
}

#[test]
fn clippy_allow_policy_script_passes_on_repo() {
    let output = run_script(None);
    assert!(
        output.status.success(),
        "clippy allow policy script failed on repository code
stdout:
{}
stderr:
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn clean_fixture_passes() {
    let dir = write_fixture("//! Clean module.\nfn main() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        output.status.success(),
        "clean fixture should pass the clippy allow gate
stderr:
{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn non_clippy_allow_passes() {
    // `#[allow(dead_code)]` is not a clippy allow and must not be flagged.
    let dir = write_fixture("#[allow(dead_code)]\nfn unused() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        output.status.success(),
        "non-clippy allow should pass the clippy allow gate
stderr:
{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn outer_allow_clippy_is_rejected() {
    let dir = write_fixture("#[allow(clippy::module_inception)]\nmod inner {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[allow(clippy::...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn inner_allow_clippy_is_rejected() {
    let dir = write_fixture("#![allow(clippy::all)]\nfn main() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#![allow(clippy::...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cfg_attr_allow_clippy_is_rejected() {
    let dir = write_fixture("#[cfg_attr(test, allow(clippy::all))]\npub fn example() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[cfg_attr(..., allow(clippy::...))] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn whitespace_outer_allow_clippy_is_rejected() {
    // `#[ allow(clippy::all)]` — space between `#[` and `allow` — is valid
    // Rust and must not bypass the gate.
    let dir = write_fixture("#[ allow(clippy::all)]\nfn spaced() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[ allow(clippy::...)] (whitespace variant) must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn whitespace_inner_allow_clippy_is_rejected() {
    // `#![ allow ( clippy::all )]` — spaces around every delimiter — is valid
    // Rust and must not bypass the gate.
    let dir = write_fixture("#![ allow ( clippy::all )]\nfn main() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#![ allow ( clippy::... )] (whitespace variant) must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn whitespace_cfg_attr_allow_clippy_is_rejected() {
    // `#[ cfg_attr(test, allow ( clippy::all )) ]` — spaces around delimiters
    // inside a cfg_attr — is valid Rust and must not bypass the gate.
    let dir = write_fixture("#[ cfg_attr(test, allow ( clippy::all )) ]\npub fn example() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[ cfg_attr(..., allow ( clippy::... )) ] (whitespace variant) must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn whitespace_before_bracket_outer_allow_clippy_is_rejected() {
    // `# [allow(clippy::all)]` — space between `#` and `[` — is valid Rust
    // and must not bypass the gate.
    let dir = write_fixture(
        "# [allow(clippy::all)]
fn spaced() {}
",
    );
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "# [allow(clippy::...)] (whitespace before bracket) must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn whitespace_before_bracket_inner_allow_clippy_is_rejected() {
    // `#! [allow(clippy::all)]` — space between `#!` and `[` — is valid Rust
    // and must not bypass the gate.
    let dir = write_fixture(
        "#! [allow(clippy::all)]
fn main() {}
",
    );
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#! [allow(clippy::...)] (whitespace before bracket) must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn whitespace_before_bracket_cfg_attr_allow_clippy_is_rejected() {
    // `# [cfg_attr(test, allow(clippy::all))]` — space between `#` and `[`
    // in a cfg_attr — is valid Rust and must not bypass the gate.
    let dir = write_fixture(
        "# [cfg_attr(test, allow(clippy::all))]
pub fn example() {}
",
    );
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "# [cfg_attr(..., allow(clippy::...))] (whitespace before bracket) must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn multiline_allow_clippy_is_rejected() {
    // Multi-line attribute spanning several lines must still be caught.
    let dir = write_fixture("#[\n    allow(clippy::all)\n]\npub fn example() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "multi-line #[allow(clippy::...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn whitespace_in_clippy_path_is_rejected() {
    // Rust accepts whitespace around path separators in attributes.
    let dir = write_fixture("#[allow(clippy :: all)]\nfn spaced_path() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[allow(clippy :: ...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn inner_whitespace_in_clippy_path_is_rejected() {
    let dir = write_fixture("#![allow(clippy :: all)]\nfn main() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#![allow(clippy :: ...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cfg_attr_whitespace_in_clippy_path_is_rejected() {
    let dir = write_fixture("#[cfg_attr(test, allow(clippy :: all))]\npub fn example() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[cfg_attr(..., allow(clippy :: ...))] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn multiline_cfg_attr_with_bracket_string_is_rejected() {
    // A `]` inside a string literal must not end attribute scanning early.
    let dir = write_fixture(
        "#[cfg_attr(
    test,
    doc = \"]\",
    allow(clippy::all)
)]
pub fn example() {}
",
    );
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "multi-line cfg_attr with string bracket and clippy allow must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn raw_identifier_clippy_path_is_rejected() {
    let dir = write_fixture("#[allow(r#clippy::all)]\nfn raw_clippy() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[allow(r#clippy::...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn inner_raw_identifier_clippy_path_is_rejected() {
    let dir = write_fixture("#![allow(r#clippy::all)]\nfn main() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#![allow(r#clippy::...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cfg_attr_raw_identifier_clippy_path_is_rejected() {
    let dir = write_fixture("#[cfg_attr(test, allow(r#clippy :: all))]\npub fn example() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[cfg_attr(..., allow(r#clippy :: ...))] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn multi_lint_outer_clippy_path_is_rejected() {
    let dir = write_fixture("#[allow(dead_code, clippy::all)]\nfn multi_lint() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[allow(dead_code, clippy::...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn multi_lint_inner_clippy_path_is_rejected() {
    let dir = write_fixture("#![allow(dead_code, clippy :: all)]\nfn main() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#![allow(dead_code, clippy :: ...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn multi_lint_cfg_attr_clippy_path_is_rejected() {
    let dir = write_fixture(
        "#[cfg_attr(test, allow(unused_variables, clippy::module_inception))]
pub fn example() {}
",
    );
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[cfg_attr(..., allow(unused_variables, clippy::...))] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn multi_lint_raw_identifier_clippy_path_is_rejected() {
    let dir = write_fixture("#[allow(dead_code, r#clippy :: all)]\nfn raw_multi() {}\n");
    let output = run_script(dir.path().to_str());
    assert!(
        !output.status.success(),
        "#[allow(dead_code, r#clippy :: ...)] must be rejected, but the gate passed
stdout:
{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
