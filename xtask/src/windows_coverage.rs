//! Per-module Windows coverage floors (issue #545, deliverable 3 / V5).
//!
//! The workspace coverage gate runs on Ubuntu and enforces a single
//! whole-workspace line percentage. Windows-only paths — the job object, the
//! server watchdog, session-host staging, launch-plan transport, and `ConPTY`
//! attach — are compiled out on Ubuntu, so that gate can never observe them
//! and a workspace average is far too coarse for Windows-only code to move.
//!
//! This module enforces a floor per Windows-only module instead. A module that
//! regresses below its floor fails the build and is named explicitly, and a
//! module that disappears from the report entirely also fails, so a rename or
//! an accidental `cfg` change cannot silently pass the gate.
//!
//! Coverage is read from LCOV, which is a line-oriented text format, so this
//! stays dependency-free in keeping with xtask having no runtime dependencies.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use crate::process::{CommandFailed, CommandPlan};
use crate::toolchain::stable_coverage_tools;

/// A Windows-only module and the line-coverage percentage it must not fall
/// below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleFloor {
    /// Repository-relative module path, using forward slashes.
    pub path: &'static str,
    /// Minimum acceptable line coverage, in whole percent.
    pub floor_percent: u32,
}

/// The Windows-only modules this epic is changing, with their floors.
///
/// Floors are set from measured coverage on native Windows, rounded down, so
/// they assert "do not regress" rather than "reach an aspirational number".
/// Raise a floor when coverage genuinely improves; never lower one to make a
/// red build green.
/// Measured on native Windows (`x86_64-pc-windows-msvc`) at the time this gate
/// was introduced:
///
/// | Module | Measured |
/// |---|---|
/// | `src/runtime/job_object.rs` | 71.15% (37/52) |
/// | `src/app_shell_liveness.rs` | 41.92% (140/334) |
/// | `src/runtime/server_health_io.rs` | 0.00% (0/82) |
/// | `src/runtime/session_host.rs` | 70.95% (210/296) |
/// | `src/runtime/agent_launcher.rs` | 80.14% (230/287) |
/// | `src/runtime/attach.rs` | 38.10% (192/504) |
///
/// `server_health_io.rs` has no coverage at all. Its floor is therefore 0 and
/// only the presence check protects it today; raising that floor requires
/// writing tests for the server watchdog IO boundary first. That gap is
/// recorded rather than hidden, which is the point of a per-module gate.
pub const WINDOWS_MODULE_FLOORS: &[ModuleFloor] = &[
    ModuleFloor {
        path: "src/runtime/job_object.rs",
        floor_percent: 65,
    },
    ModuleFloor {
        path: "src/app_shell_liveness.rs",
        floor_percent: 38,
    },
    ModuleFloor {
        path: "src/runtime/server_health_io.rs",
        floor_percent: 0,
    },
    ModuleFloor {
        path: "src/runtime/session_host.rs",
        floor_percent: 65,
    },
    ModuleFloor {
        path: "src/runtime/agent_launcher.rs",
        floor_percent: 75,
    },
    ModuleFloor {
        path: "src/runtime/attach.rs",
        floor_percent: 34,
    },
];

/// Line coverage for one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCoverage {
    /// Path exactly as reported by LCOV.
    pub path: String,
    /// Instrumented lines found.
    pub lines_found: u64,
    /// Instrumented lines executed at least once.
    pub lines_hit: u64,
}

impl FileCoverage {
    /// Line coverage in basis points (hundredths of a percent). A file with no
    /// instrumented lines is treated as fully covered, matching llvm-cov's own
    /// convention.
    #[must_use]
    pub const fn basis_points(&self) -> u64 {
        if self.lines_found == 0 {
            return 10_000;
        }
        self.lines_hit * 10_000 / self.lines_found
    }

    /// True when this module's line coverage is at or above `floor_percent`.
    ///
    /// Compared with integer arithmetic so the verdict is exact; a float
    /// percentage would make results at the boundary depend on rounding.
    #[must_use]
    pub const fn meets_floor(&self, floor_percent: u64) -> bool {
        self.lines_hit * 100 >= floor_percent * self.lines_found
    }
}

/// Why one module failed its floor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloorViolation {
    /// The module was covered but fell below its floor.
    Below {
        /// Module path.
        path: String,
        /// Measured line coverage, in hundredths of a percent.
        actual_basis_points: u64,
        /// Configured floor.
        floor_percent: u32,
    },
    /// The module was absent from the coverage report entirely.
    Missing {
        /// Module path.
        path: String,
    },
}

impl std::fmt::Display for FloorViolation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Below {
                path,
                actual_basis_points,
                floor_percent,
            } => write!(
                formatter,
                "{path}: line coverage {}.{:02}% is below its floor of {floor_percent}%",
                actual_basis_points / 100,
                actual_basis_points % 100
            ),
            Self::Missing { path } => write!(
                formatter,
                "{path}: absent from the Windows coverage report; the module was \
                 renamed, removed, or compiled out, so its floor cannot be proven"
            ),
        }
    }
}

/// Parse an LCOV tracefile into per-file line coverage.
///
/// Only `SF:` (source file), `LF:` (lines found) and `LH:` (lines hit) are
/// consulted. Records for the same file are summed, because llvm-cov may emit
/// one record per test binary.
#[must_use]
pub fn parse_lcov(tracefile: &str) -> Vec<FileCoverage> {
    let mut totals: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in tracefile.lines() {
        let line = line.trim();
        if let Some(path) = line.strip_prefix("SF:") {
            current = Some(normalize_path(path));
        } else if let Some(found) = line.strip_prefix("LF:") {
            if let (Some(path), Ok(value)) = (current.as_ref(), found.trim().parse::<u64>()) {
                totals.entry(path.clone()).or_default().0 += value;
            }
        } else if let Some(hit) = line.strip_prefix("LH:") {
            if let (Some(path), Ok(value)) = (current.as_ref(), hit.trim().parse::<u64>()) {
                totals.entry(path.clone()).or_default().1 += value;
            }
        } else if line == "end_of_record" {
            current = None;
        }
    }
    totals
        .into_iter()
        .map(|(path, (lines_found, lines_hit))| FileCoverage {
            path,
            lines_found,
            lines_hit,
        })
        .collect()
}

/// Normalize an LCOV path to forward slashes without a leading `./`.
fn normalize_path(path: &str) -> String {
    let replaced = path.trim().replace('\\', "/");
    replaced.strip_prefix("./").unwrap_or(&replaced).to_owned()
}

/// Check every configured floor against the measured coverage.
///
/// Matching is by path suffix so absolute paths from the coverage tool line up
/// with repository-relative floor paths.
#[must_use]
pub fn evaluate_floors(coverage: &[FileCoverage], floors: &[ModuleFloor]) -> Vec<FloorViolation> {
    let mut violations = Vec::new();
    for floor in floors {
        let Some(measured) = coverage
            .iter()
            .find(|file| path_matches(&file.path, floor.path))
        else {
            violations.push(FloorViolation::Missing {
                path: floor.path.to_owned(),
            });
            continue;
        };
        if !measured.meets_floor(u64::from(floor.floor_percent)) {
            violations.push(FloorViolation::Below {
                path: floor.path.to_owned(),
                actual_basis_points: measured.basis_points(),
                floor_percent: floor.floor_percent,
            });
        }
    }
    violations
}

/// True when `reported` refers to the repository-relative `expected` module.
fn path_matches(reported: &str, expected: &str) -> bool {
    reported == expected || reported.ends_with(&format!("/{expected}"))
}

/// Build the `cargo llvm-cov` plan that emits an LCOV tracefile for the
/// Windows coverage gate.
///
/// # Errors
/// Propagates coverage-tool discovery failures.
/// Scoped to `--lib --bins` deliberately. The psmux integration tests drive
/// real `ConPTY` attach and viewer processes, which do not terminate reliably
/// under llvm-cov instrumentation, so including them would make the gate hang
/// rather than report. Unit and binary tests are where these modules' own
/// coverage lives, and they run fast and deterministically.
pub fn windows_coverage_plan(output: &Path) -> Result<CommandPlan, CommandFailed> {
    let (llvm_cov, llvm_profdata) = stable_coverage_tools()?;
    Ok(CommandPlan::new("rustup")
        .args(["run", "stable", "cargo", "llvm-cov"])
        .args(["--lib", "--bins", "--all-features", "--lcov"])
        .arg("--output-path")
        .arg(output.to_string_lossy().into_owned())
        .arg("--ignore-filename-regex")
        .arg(crate::toolchain::COVERAGE_IGNORE_REGEX)
        .env("LLVM_COV", llvm_cov.to_string_lossy().into_owned())
        .env(
            "LLVM_PROFDATA",
            llvm_profdata.to_string_lossy().into_owned(),
        ))
}

/// Render a human-readable report of every configured module's coverage.
#[must_use]
pub fn render_report(coverage: &[FileCoverage], floors: &[ModuleFloor]) -> String {
    let mut report = String::from("Windows-only module coverage:\n");
    for floor in floors {
        let measured = coverage
            .iter()
            .find(|file| path_matches(&file.path, floor.path));
        match measured {
            Some(file) => {
                let basis_points = file.basis_points();
                let measured_percent = format!("{}.{:02}%", basis_points / 100, basis_points % 100);
                let _ = writeln!(
                    report,
                    "  {:<40} {:>7}  (floor {}%, {}/{} lines)",
                    floor.path,
                    measured_percent,
                    floor.floor_percent,
                    file.lines_hit,
                    file.lines_found
                );
            }
            None => {
                let _ = writeln!(
                    report,
                    "  {:<40}      -   (floor {}%, ABSENT)",
                    floor.path, floor.floor_percent
                );
            }
        }
    }
    report
}
