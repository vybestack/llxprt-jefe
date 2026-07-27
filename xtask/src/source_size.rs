//! Source file length policy (issue #459, A6).
//!
//! Enforces the source file length policy ported from
//! `scripts/check-source-file-size.sh`. Fails when a Rust source file under
//! the scan roots exceeds the hard line limit, and warns above a recommended
//! limit. Paths are reported relative to the scan root so diagnostics are
//! stable on Windows and Unix.

use std::path::{Path, PathBuf};

use crate::process::CommandFailed;

/// Default scan roots, matching the original shell script.
pub const DEFAULT_SCAN_ROOTS: &[&str] = &["src", "tests"];
/// Hard failure limit, in lines.
pub const DEFAULT_HARD_LIMIT: usize = 1000;
/// Recommended (warning) limit, in lines.
pub const DEFAULT_WARN_LIMIT: usize = 750;

/// One file's length result.
#[derive(Debug, Clone)]
pub struct FileLength {
    pub path: PathBuf,
    pub lines: usize,
}

/// A length policy violation: either a hard error or a warning.
#[derive(Debug, Clone)]
pub enum Violation {
    Hard {
        path: PathBuf,
        lines: usize,
        limit: usize,
    },
    Warn {
        path: PathBuf,
        lines: usize,
        limit: usize,
    },
}

impl Violation {
    /// The file path that violated the policy.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Hard { path, .. } | Self::Warn { path, .. } => path,
        }
    }
}

/// Configuration for the source-size policy.
#[derive(Debug, Clone)]
pub struct Policy {
    pub hard_limit: usize,
    pub warn_limit: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            hard_limit: DEFAULT_HARD_LIMIT,
            warn_limit: DEFAULT_WARN_LIMIT,
        }
    }
}

/// Run the source-size policy against the repository root, scanning the
/// default roots (`src`, `tests`).
///
/// # Errors
/// Returns `CommandFailed` if any file exceeds the hard limit. Warnings are
/// printed to stderr but do not fail the gate (matching the original script).
#[allow(clippy::missing_errors_doc)]
pub fn run_repo_check(root: &Path) -> Result<(), CommandFailed> {
    let roots: Vec<PathBuf> = DEFAULT_SCAN_ROOTS.iter().map(|r| root.join(r)).collect();
    run_with_roots(&roots, &Policy::default(), root)
}

/// Run the policy against explicit scan roots. `relativize_to` is used to
/// produce stable relative-path diagnostics.
///
/// # Errors
/// Returns `CommandFailed` if any file exceeds the hard limit.
#[allow(clippy::missing_errors_doc)]
pub fn run_with_roots(
    roots: &[PathBuf],
    policy: &Policy,
    relativize_to: &Path,
) -> Result<(), CommandFailed> {
    let mut files = Vec::new();
    for root in roots {
        if root.is_dir() {
            collect_rust_files(root, &mut files);
        }
    }
    files.sort();
    if files.is_empty() {
        return Ok(());
    }
    let lengths = measure_files(&files);
    let violations = classify(&lengths, policy);
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for v in &violations {
        let rel = relativize(v.path(), relativize_to);
        match v {
            Violation::Hard { lines, limit, .. } => {
                eprintln!("ERROR: {rel} has {lines} lines (max {limit})");
                errors += 1;
            }
            Violation::Warn { lines, limit, .. } => {
                eprintln!("WARNING: {rel} has {lines} lines (recommended max {limit})");
                warnings += 1;
            }
        }
    }
    if warnings > 0 {
        eprintln!("Emitted {warnings} file length warning(s).");
    }
    if errors > 0 {
        return Err(CommandFailed {
            program: "xtask".into(),
            args: vec!["check".into(), "source-size".into()],
            status: Some(1),
            stdout: Vec::new(),
            stderr: format!("Found {errors} file(s) exceeding the hard limit.").into_bytes(),
        });
    }
    Ok(())
}

/// Measure line counts for a list of files. Files that cannot be read are
/// skipped (they cannot be enforced).
#[must_use]
pub fn measure_files(files: &[PathBuf]) -> Vec<FileLength> {
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        if let Ok(content) = std::fs::read_to_string(file) {
            let lines = count_lines(&content);
            out.push(FileLength {
                path: file.clone(),
                lines,
            });
        }
    }
    out
}

/// Count lines, matching `wc -l` semantics.
///
/// A line is terminated by `\n`; a file without a trailing newline has its
/// last line uncounted. We count `\n` bytes to match exactly.
#[must_use]
pub fn count_lines(content: &str) -> usize {
    content.bytes().filter(|&b| b == b'\n').count()
}

/// Classify measured file lengths into violations.
#[must_use]
pub fn classify(lengths: &[FileLength], policy: &Policy) -> Vec<Violation> {
    let mut out = Vec::new();
    for fl in lengths {
        if fl.lines > policy.hard_limit {
            out.push(Violation::Hard {
                path: fl.path.clone(),
                lines: fl.lines,
                limit: policy.hard_limit,
            });
        } else if fl.lines > policy.warn_limit {
            out.push(Violation::Warn {
                path: fl.path.clone(),
                lines: fl.lines,
                limit: policy.warn_limit,
            });
        }
    }
    out
}

/// Recursively collect `*.rs` files under `root`.
fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            collect_rust_files(&path, out);
        } else if metadata.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Produce a stable relative path string for diagnostics.
#[allow(clippy::option_if_let_else)]
fn relativize(path: &Path, base: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(base) {
        rel.to_string_lossy().into_owned()
    } else {
        path.to_string_lossy().into_owned()
    }
}
