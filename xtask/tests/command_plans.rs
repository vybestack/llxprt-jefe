//! Command-plan contract tests for xtask (issue #459).
//!
//! These tests assert the shape of the command plans (program + args + env)
//! that xtask builds, without spawning child processes. They prove the
//! aggregate `ci` ordering (A1), the standalone command surface (A8), and the
//! platform-agnostic argument construction.

use xtask::cli;

#[test]
fn ci_runs_steps_in_the_documented_order() {
    let steps = cli::plans::ci_step_names();
    assert_eq!(
        steps,
        &[
            "fmt",
            "check-clippy-allows",
            "check-source-size",
            "check-architecture",
            "check-multiplexer-surface",
            "lint",
            "complexity",
            "coverage",
            "build",
            "test",
        ],
        "ci must run fmt -> policies -> strict clippy -> complexity -> coverage -> build -> test"
    );
}

#[test]
fn fmt_plan_invokes_cargo_fmt_check() {
    let plan = cli::plans::fmt();
    assert_eq!(plan.program, "cargo");
    assert_eq!(plan.args, vec!["fmt", "--all", "--check"]);
}

#[test]
fn lint_plan_uses_stable_clippy_with_ci_conf_dir() {
    let plan = cli::plans::lint();
    assert_eq!(plan.program, "rustup");
    assert_eq!(
        plan.args,
        vec![
            "run",
            "stable",
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ]
    );
    assert!(
        plan.env
            .iter()
            .any(|(k, v)| k == "CLIPPY_CONF_DIR" && v == ".github/clippy"),
        "lint must set CLIPPY_CONF_DIR so the CI thresholds apply"
    );
}

#[test]
fn complexity_plan_only_denies_the_complexity_lints() {
    let plan = cli::plans::complexity();
    assert_eq!(plan.program, "rustup");
    assert_eq!(
        plan.args,
        vec![
            "run",
            "stable",
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-A",
            "clippy::all",
            "-A",
            "clippy::pedantic",
            "-A",
            "clippy::nursery",
            "-D",
            "clippy::cognitive_complexity",
            "-D",
            "clippy::too_many_lines",
            "-D",
            "clippy::too_many_arguments",
            "-D",
            "clippy::type_complexity",
            "-D",
            "clippy::struct_excessive_bools",
        ]
    );
}

#[test]
fn build_plan_is_locked_all_feature_workspace() {
    let plan = cli::plans::build();
    assert_eq!(plan.program, "cargo");
    assert_eq!(
        plan.args,
        vec!["build", "--workspace", "--all-features", "--locked"]
    );
}

#[test]
fn test_plan_is_locked_all_feature_workspace() {
    let plan = cli::plans::test_cmd();
    assert_eq!(plan.program, "cargo");
    assert_eq!(
        plan.args,
        vec!["test", "--workspace", "--all-features", "--locked"]
    );
}
