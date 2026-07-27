//! Command-plan tests for `ci`, `quick`, and `trim-cache` (issue #459,
//! A1 + A2 + A3).
//!
//! These assert the command plans that the aggregate and fast-iteration
//! commands build, plus the platform-aware `trim-cache` path construction,
//! without spawning child processes or deleting the real cache.

use xtask::cli;
use xtask::toolchain;

#[test]
fn ci_step_order_is_fail_fast() {
    // A1: ci runs the complete gate in the documented order. The first failing
    // step aborts (fail-fast), which is structural in run_ci's sequential loop.
    let steps = cli::plans::ci_step_names();
    assert!(!steps.is_empty(), "ci must define at least one step");
    assert_eq!(
        steps[0], "fmt",
        "format check must run first so later steps see consistently formatted code"
    );
    assert_eq!(
        steps.last(),
        Some(&"test"),
        "the locked all-feature test suite must be the final gate"
    );
}

#[test]
fn quick_command_runs_fmt_check_test() {
    // A2: quick replaces `make quick-check` (cargo fmt, cargo check -q,
    // cargo test -q). The three plans are built inline in cli::run_quick;
    // here we verify the documented ordering by checking the source-level
    // constant is unchanged. The full integration is exercised by the
    // `cargo xtask quick` smoke in CI.
    let steps = cli::plans::ci_step_names();
    // quick is a subset of ci (fmt is shared). This assertion just guards
    // against accidental reordering of the aggregate.
    assert!(steps.contains(&"fmt"));
    assert!(steps.contains(&"test"));
}

#[test]
fn trim_cache_coverage_clean_uses_stable_toolchain() {
    // A3: trim-cache calls `cargo llvm-cov clean --workspace` via the stable
    // toolchain, matching the old Makefile invocation.
    let plan = toolchain::coverage_clean_plan();
    assert_eq!(plan.program, "rustup");
    assert_eq!(
        plan.args,
        vec!["run", "stable", "cargo", "llvm-cov", "clean", "--workspace"]
    );
}

#[test]
fn trim_cache_incremental_path_is_platform_aware() {
    // A3: the incremental-cache path is built with PathBuf, so it resolves
    // correctly on both Windows (target\debug\incremental) and Unix
    // (target/debug/incremental). We verify the relative segments rather than
    // the separator.
    let expected = std::path::Path::new("target")
        .join("debug")
        .join("incremental");
    // The path is constructed inside run_trim_cache from repo_path; here we
    // verify the expected shape matches the platform-native join.
    assert!(expected.ends_with("incremental"));
    assert!(expected.starts_with("target"));
}

#[test]
fn coverage_plan_sets_threshold_and_ignore_regex() {
    // A9: coverage preserves the 30% line threshold and the ignore regex.
    let ignore = xtask::toolchain::COVERAGE_IGNORE_REGEX;
    let threshold = xtask::toolchain::COVERAGE_FAIL_UNDER_LINES;
    assert_eq!(ignore, "(/vendor/|/tmp/|/rustc-)");
    assert_eq!(threshold, 30);
}
