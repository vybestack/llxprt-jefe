//! xtask command-line surface and dispatch (issue #459).
//!
//! One canonical entry point: `cargo xtask <command>`. Argument parsing is
//! hand-rolled over the standard library (no task-runner framework, per the
//! issue's "Expected paths"). Every command builds a `CommandPlan` rather than
//! a shell string.

use std::path::Path;
use std::process::ExitCode;

use crate::architecture;
use crate::clippy_policy;
use crate::process::{CommandFailed, CommandPlan, repo_path};
use crate::source_size;
use crate::toolchain;

/// The aggregate `ci` ordering — fmt, clippy-allow policy, source-size policy,
/// architecture policy, strict clippy, complexity clippy, coverage, locked
/// build, locked test (A1). Each step fails fast.
const CI_STEPS: &[&str] = &[
    "fmt",
    "check-clippy-allows",
    "check-source-size",
    "check-architecture",
    "lint",
    "complexity",
    "coverage",
    "build",
    "test",
];

/// The xtask exit code for a missing or malformed invocation.
const EXIT_USAGE: u8 = 2;

/// Run the xtask CLI. Returns a process exit code.
///
/// # Errors
/// Reported via the returned `ExitCode` and stderr text, not `Result`, because
/// this is the process entry point.
#[must_use]
pub fn run(argv: &[String]) -> ExitCode {
    let Some(command) = argv.first() else {
        usage();
        return ExitCode::from(EXIT_USAGE);
    };

    let rest = &argv[1..];
    match command.as_str() {
        "ci" => exit(run_ci()),
        "quick" => exit(run_quick()),
        "trim-cache" => exit(run_trim_cache()),
        "fmt" => exit(fmt_plan().run_inherit()),
        "lint" => exit(lint_plan().run_inherit()),
        "complexity" => exit(complexity_plan().run_inherit()),
        "coverage" => exit(run_coverage()),
        "build" => exit(build_plan().run_inherit()),
        "test" => exit(test_plan().run_inherit()),
        "check" => exit(run_check(rest)),
        "help" | "--help" | "-h" => {
            usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("error: unknown xtask command `{other}`");
            usage();
            ExitCode::from(EXIT_USAGE)
        }
    }
}

fn usage() {
    eprintln!(
        "usage: cargo xtask <command>

commands:
  ci                  run the full local CI-equivalent gate
  quick               fast iteration: fmt, check, test
  trim-cache          clean coverage artifacts and incremental build cache
  fmt                 cargo fmt --all --check
  lint                strict clippy (warnings as errors)
  complexity          complexity-threshold clippy
  coverage            llvm-cov line-coverage gate (30%)
  build               locked all-feature workspace build
  test                locked all-feature workspace test
  check clippy-allows  zero-tolerance clippy allow/expect policy
  check source-size    source file length policy
  check architecture   architecture boundary policy"
    );
}

fn exit(result: Result<(), CommandFailed>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask: {err}");
            ExitCode::FAILURE
        }
    }
}

// --- aggregate commands -----------------------------------------------------

/// `cargo xtask ci` — the full gate in CI order (A1). Fail-fast: the first
/// failing step aborts and its exit code propagates.
fn run_ci() -> Result<(), CommandFailed> {
    let root = repo_path("").map_err(|err| CommandFailed {
        program: "xtask".into(),
        args: vec!["ci".into()],
        status: None,
        stdout: Vec::new(),
        stderr: err.into_bytes(),
    })?;
    for step in CI_STEPS {
        run_ci_step(step, &root)?;
    }
    Ok(())
}

fn run_ci_step(step: &str, root: &Path) -> Result<(), CommandFailed> {
    eprintln!("xtask ci: {step}");
    match step {
        "fmt" => fmt_plan().run_inherit(),
        "check-clippy-allows" => clippy_policy::run_repo_check(root),
        "check-source-size" => source_size::run_repo_check(root),
        "check-architecture" => architecture::run_repo_check(root),
        "lint" => lint_plan().run_inherit(),
        "complexity" => complexity_plan().run_inherit(),
        "coverage" => run_coverage(),
        "build" => build_plan().run_inherit(),
        "test" => test_plan().run_inherit(),
        unknown => Err(CommandFailed {
            program: "xtask".into(),
            args: vec!["ci".into(), unknown.into()],
            status: None,
            stdout: Vec::new(),
            stderr: format!("unknown ci step `{unknown}`").into_bytes(),
        }),
    }
}

/// `cargo xtask quick` — replaces `make quick-check` (A2).
fn run_quick() -> Result<(), CommandFailed> {
    let fmt = CommandPlan::new("cargo").arg("fmt").run_inherit();
    fmt?;
    let check = CommandPlan::new("cargo")
        .args(["check", "-q"])
        .run_inherit();
    check?;
    CommandPlan::new("cargo").args(["test", "-q"]).run_inherit()
}

/// `cargo xtask trim-cache` — replaces `make trim-cache` (A3). Coverage clean
/// + removal of `target/debug/incremental`, using platform-aware paths.
fn run_trim_cache() -> Result<(), CommandFailed> {
    toolchain::coverage_clean_plan().run_inherit()?;
    let incremental = repo_path("target/debug/incremental").map_err(|err| CommandFailed {
        program: "xtask".into(),
        args: vec!["trim-cache".into()],
        status: None,
        stdout: Vec::new(),
        stderr: err.into_bytes(),
    })?;
    if incremental.exists() {
        std::fs::remove_dir_all(&incremental).map_err(|err| CommandFailed {
            program: "rm".into(),
            args: vec!["-rf".into(), incremental.to_string_lossy().into_owned()],
            status: None,
            stdout: Vec::new(),
            stderr: format!("failed to remove {}: {err}", incremental.display()).into_bytes(),
        })?;
    }
    Ok(())
}

// --- narrow commands --------------------------------------------------------

fn fmt_plan() -> CommandPlan {
    CommandPlan::new("cargo").args(["fmt", "--all", "--check"])
}

fn lint_plan() -> CommandPlan {
    // CLIPPY_CONF_DIR=.github/clippy matches the old Makefile invocation so
    // the complexity thresholds in .github/clippy/clippy.toml apply.
    CommandPlan::new("rustup")
        .args(["run", "stable", "cargo", "clippy"])
        .args(["--workspace", "--all-targets", "--all-features"])
        .arg("--")
        .args(["-D", "warnings"])
        .env("CLIPPY_CONF_DIR", ".github/clippy")
}

fn complexity_plan() -> CommandPlan {
    CommandPlan::new("rustup")
        .args(["run", "stable", "cargo", "clippy"])
        .args(["--workspace", "--all-targets", "--all-features"])
        .arg("--")
        .args(["-A", "clippy::all"])
        .args(["-A", "clippy::pedantic"])
        .args(["-A", "clippy::nursery"])
        .args(["-D", "clippy::cognitive_complexity"])
        .args(["-D", "clippy::too_many_lines"])
        .args(["-D", "clippy::too_many_arguments"])
        .args(["-D", "clippy::type_complexity"])
        .args(["-D", "clippy::struct_excessive_bools"])
        .env("CLIPPY_CONF_DIR", ".github/clippy")
}

fn build_plan() -> CommandPlan {
    CommandPlan::new("cargo").args(["build", "--workspace", "--all-features", "--locked"])
}

fn test_plan() -> CommandPlan {
    CommandPlan::new("cargo").args(["test", "--workspace", "--all-features", "--locked"])
}

fn run_coverage() -> Result<(), CommandFailed> {
    toolchain::coverage_plan()?.run_inherit()
}

// --- check subcommands ------------------------------------------------------

fn run_check(rest: &[String]) -> Result<(), CommandFailed> {
    let Some(target) = rest.first() else {
        eprintln!("usage: cargo xtask check <clippy-allows|source-size|architecture>");
        return Err(usage_error("check", "missing policy name"));
    };
    let root = repo_path("").map_err(|err| CommandFailed {
        program: "xtask".into(),
        args: vec!["check".into(), target.clone()],
        status: None,
        stdout: Vec::new(),
        stderr: err.into_bytes(),
    })?;
    match target.as_str() {
        "clippy-allows" => clippy_policy::run_repo_check(&root),
        "source-size" => source_size::run_repo_check(&root),
        "architecture" => architecture::run_repo_check(&root),
        other => {
            eprintln!("error: unknown check target `{other}`");
            Err(usage_error("check", "unknown policy name"))
        }
    }
}

fn usage_error(command: &str, reason: &str) -> CommandFailed {
    CommandFailed {
        program: "xtask".into(),
        args: vec![command.into()],
        status: None,
        stdout: Vec::new(),
        stderr: format!("usage error: {reason}").into_bytes(),
    }
}

// Exposed for unit/integration tests that assert command plans without
// spawning processes.
pub mod plans {
    use super::{CI_STEPS, build_plan, complexity_plan, fmt_plan, lint_plan, test_plan};
    use crate::process::CommandPlan;

    #[must_use]
    pub fn fmt() -> CommandPlan {
        fmt_plan()
    }
    #[must_use]
    pub fn lint() -> CommandPlan {
        lint_plan()
    }
    #[must_use]
    pub fn complexity() -> CommandPlan {
        complexity_plan()
    }
    #[must_use]
    pub fn build() -> CommandPlan {
        build_plan()
    }
    #[must_use]
    pub fn test_cmd() -> CommandPlan {
        test_plan()
    }
    #[must_use]
    pub const fn ci_step_names() -> &'static [&'static str] {
        CI_STEPS
    }
}
