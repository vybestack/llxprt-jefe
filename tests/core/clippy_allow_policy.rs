//! Clippy allow policy contract tests.
//!
//! Issue #459: the policy is now a Rust xtask command rather than a Bash
//! script. The detailed positive/negative fixture coverage lives in
//! `xtask/tests/clippy_allow_fixtures.rs` (calling the scanner directly); this
//! file is the repo-level contract test that the repository's own first-party
//! Rust code passes the zero-tolerance gate.
//!
//! Invoking `cargo xtask check clippy-allows` runs natively on Windows and
//! Unix — no Bash, Python, or Unix-utility dependency.

use std::process::Command;

use crate::support::TestResultExt;

#[test]
fn clippy_allow_policy_passes_on_repo() {
    let output = Command::new("cargo")
        .args(["xtask", "check", "clippy-allows"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .test_unwrap("`cargo xtask check clippy-allows` should be runnable");

    assert!(
        output.status.success(),
        "clippy allow policy failed on repository code
stdout:
{}
stderr:
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
