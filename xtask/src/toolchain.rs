//! Stable-toolchain and coverage-binary discovery (issue #459, A9).
//!
//! Mirrors the shell logic from the old Makefile/CI: locate the stable
//! toolchain's `rustc`, derive the host triple and the llvm-cov/llvm-profdata
//! siblings, and build the `cargo llvm-cov` invocation that enforces the 30%
//! line threshold. All paths are `PathBuf`, so the same code produces correct
//! Windows and Unix command plans.

use std::path::PathBuf;

use crate::process::{CommandFailed, CommandPlan};

/// The ignore-filename regex used by the coverage gate. Kept as a single
/// source of truth so the command plan and its tests cannot drift.
pub const COVERAGE_IGNORE_REGEX: &str = "(/vendor/|/tmp/|/rustc-)";
/// The project's line-coverage floor.
pub const COVERAGE_FAIL_UNDER_LINES: u32 = 30;

/// Run `rustup which --toolchain stable rustc` and return the resolved path.
///
/// # Errors
/// Returns `CommandFailed` if rustup is absent or reports a nonzero exit.
pub fn stable_rustc() -> Result<PathBuf, CommandFailed> {
    let output = CommandPlan::new("rustup")
        .args(["which", "--toolchain", "stable", "rustc"])
        .run_captured()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err(CommandFailed {
            program: "rustup".into(),
            args: vec![
                "which".into(),
                "--toolchain".into(),
                "stable".into(),
                "rustc".into(),
            ],
            status: Some(0),
            stdout: output.stdout,
            stderr: b"rustup which returned an empty rustc path".to_vec(),
        });
    }
    Ok(PathBuf::from(path))
}

/// Discover the stable toolchain's host triple via `rustc -vV`.
///
/// # Errors
/// Returns `CommandFailed` if the stable rustc cannot be queried or the
/// `host:` line is missing.
pub fn stable_host_triple() -> Result<String, CommandFailed> {
    let rustc = stable_rustc()?;
    let output = CommandPlan::new(rustc.to_string_lossy().into_owned())
        .arg("-vV")
        .run_captured()?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(rest) = line.strip_prefix("host:") {
            let triple = rest.trim();
            if !triple.is_empty() {
                return Ok(triple.to_string());
            }
        }
    }
    Err(CommandFailed {
        program: "rustc -vV".into(),
        args: Vec::new(),
        status: Some(0),
        stdout: output.stdout,
        stderr: b"rustc -vV output did not contain a host: line".to_vec(),
    })
}

/// Resolve the llvm-cov / llvm-profdata binaries that ship inside the stable
/// toolchain's lib/rustlib/<host>/bin directory.
///
/// Returns `(llvm_cov, llvm_profdata)`.
///
/// # Errors
/// Propagates discovery failures from `stable_rustc` / `stable_host_triple`.
pub fn stable_coverage_tools() -> Result<(PathBuf, PathBuf), CommandFailed> {
    let rustc = stable_rustc()?;
    // rustc lives at <toolchain>/bin/rustc; the coverage tools live at
    // <toolchain>/lib/rustlib/<host>/bin.
    let toolchain_bin = rustc.parent().ok_or_else(|| CommandFailed {
        program: "rustc".into(),
        args: Vec::new(),
        status: None,
        stdout: Vec::new(),
        stderr: b"resolved rustc has no parent directory".to_vec(),
    })?;
    let toolchain_root = toolchain_bin.parent().ok_or_else(|| CommandFailed {
        program: "rustc".into(),
        args: Vec::new(),
        status: None,
        stdout: Vec::new(),
        stderr: b"resolved toolchain bin has no parent directory".to_vec(),
    })?;
    let host = stable_host_triple()?;
    let cov_bin = toolchain_root
        .join("lib")
        .join("rustlib")
        .join(&host)
        .join("bin");
    let llvm_cov = cov_bin.join("llvm-cov");
    let llvm_profdata = cov_bin.join("llvm-profdata");
    Ok((llvm_cov, llvm_profdata))
}

/// Build the `cargo llvm-cov` command plan with the project's coverage
/// configuration. This does NOT run the command; callers (the `coverage`
/// command and the aggregate `ci`) invoke `run_inherit`.
///
/// # Errors
/// Propagates `stable_coverage_tools`.
pub fn coverage_plan() -> Result<CommandPlan, CommandFailed> {
    let (llvm_cov, llvm_profdata) = stable_coverage_tools()?;
    Ok(CommandPlan::new("rustup")
        .args(["run", "stable", "cargo", "llvm-cov"])
        .args(["--workspace", "--all-features", "--summary-only"])
        .arg("--ignore-filename-regex")
        .arg(COVERAGE_IGNORE_REGEX)
        .arg("--fail-under-lines")
        .arg(COVERAGE_FAIL_UNDER_LINES.to_string())
        .env("LLVM_COV", llvm_cov.to_string_lossy().into_owned())
        .env(
            "LLVM_PROFDATA",
            llvm_profdata.to_string_lossy().into_owned(),
        ))
}

/// Build the `cargo llvm-cov clean --workspace` plan used by `trim-cache`.
#[must_use]
pub fn coverage_clean_plan() -> CommandPlan {
    CommandPlan::new("rustup").args(["run", "stable", "cargo", "llvm-cov", "clean", "--workspace"])
}
