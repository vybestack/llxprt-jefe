//! Architecture boundary policy (issue #459, A7).
//!
//! Ported from `scripts/check-architecture.sh`. Enforces three invariants:
//!
//! 1. No crate-wide (`#![...]`) clippy allow attributes outside an explicit,
//!    reviewed exception ledger.
//! 2. Required message/state/input symbols are present in the canonical
//!    source files.
//! 3. Handler modules (`*ops.rs`, `*handlers.rs`, `*dispatch.rs` under
//!    `src/app_input` and `src/state`) stay under the 850-line default
//!    (955 for `src/state/form_ops.rs`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::process::{CommandFailed, CommandPlan};

/// Default handler-module line limit.
pub const DEFAULT_HANDLER_LIMIT: usize = 850;
/// Override limit for `src/state/form_ops.rs`.
pub const FORM_OPS_LIMIT: usize = 955;

/// One allowed crate-wide clippy suppression, keyed by
/// `(relative_path, line_number, raw_attribute_text)`.
type ExceptionKey = (&'static str, usize, &'static str);

/// The reviewed exception ledger for crate-wide clippy allows. Each entry is
/// an exact `(file, 1-based line, attribute text)` triple that the policy
/// permits. Anything else is a violation.
///
/// This mirrors the `grep -Ev '...'` filter in the original shell script. If
/// a new exception is genuinely required, it must be raised as a design
/// discussion and added here, not committed as debt.
const ALLOWED_CRATE_WIDE_ALLOWS: &[ExceptionKey] = &[
    ("src/main.rs", 6, "#![allow(clippy::print_stderr)]"),
    ("src/main.rs", 7, "#![allow(clippy::collapsible_if)]"),
    ("src/main.rs", 8, "#![allow(clippy::clone_on_copy)]"),
    (
        "src/main.rs",
        9,
        "#![allow(clippy::significant_drop_tightening)]",
    ),
    ("src/runtime/mod.rs", 29, "#![allow(clippy::expect_used)]"),
    (
        "tests/e2e/end_to_end.rs",
        13,
        "#![allow(clippy::unwrap_used, clippy::expect_used)]",
    ),
    (
        "tests/e2e/recovery_paths.rs",
        11,
        "#![allow(clippy::unwrap_used, clippy::expect_used)]",
    ),
    (
        "tests/core/persistence_theme_contracts.rs",
        10,
        "#![allow(clippy::expect_used)]",
    ),
    (
        "tests/core/domain_state_contracts.rs",
        9,
        "#![allow(clippy::expect_used)]",
    ),
    (
        "tests/core/domain_state_contracts.rs",
        10,
        "#![allow(clippy::unwrap_used)]",
    ),
    (
        "tests/runtime/terminal_focus_routing.rs",
        9,
        "#![allow(clippy::expect_used)]",
    ),
    (
        "tests/runtime/terminal_focus_routing.rs",
        10,
        "#![allow(clippy::unwrap_used)]",
    ),
    (
        "tests/core/visibility_filter_contracts.rs",
        3,
        "#![allow(clippy::expect_used)]",
    ),
    (
        "tests/core/visibility_filter_contracts.rs",
        4,
        "#![allow(clippy::unwrap_used)]",
    ),
    (
        "tests/runtime/runtime_lifecycle.rs",
        9,
        "#![allow(clippy::expect_used)]",
    ),
    (
        "tests/runtime/runtime_lifecycle.rs",
        10,
        "#![allow(clippy::unwrap_used)]",
    ),
];

/// Required `(symbol, file, description)` triples for the message/state/input
/// architecture.
const REQUIRED_SYMBOLS: &[(&str, &str, &str)] = &[
    (
        "pub enum AppMessage",
        "src/messages.rs",
        "the typed AppMessage bus",
    ),
    (
        "pub enum UiNavigationMessage",
        "src/messages.rs",
        "the ui_navigation channel",
    ),
    (
        "pub enum ModalMessage",
        "src/messages.rs",
        "the modal channel",
    ),
    (
        "pub enum RepositoryAgentMessage",
        "src/messages.rs",
        "the repository_agent channel",
    ),
    (
        "pub enum RuntimeMessage",
        "src/messages.rs",
        "the runtime channel",
    ),
    (
        "pub enum PersistenceMessage",
        "src/messages.rs",
        "the persistence channel",
    ),
    (
        "pub enum ThemeMessage",
        "src/messages.rs",
        "the theme channel",
    ),
    (
        "pub enum IssuesMessage",
        "src/messages.rs",
        "the issues channel",
    ),
    (
        "pub enum SystemMessage",
        "src/messages.rs",
        "the system channel",
    ),
    (
        "pub fn apply_message",
        "src/state/mod.rs",
        "apply_message for routed state transitions",
    ),
    (
        "pub fn dispatch_app_message",
        "src/app_input/mod.rs",
        "dispatch routed AppMessage values",
    ),
];

/// Run the architecture policy against the repository root.
///
/// # Errors
/// Returns `CommandFailed` if any crate-wide clippy allow is found outside the
/// ledger, any required symbol is missing, or any handler module exceeds its
/// line limit.
pub fn run_repo_check(root: &Path) -> Result<(), CommandFailed> {
    let mut errors: Vec<String> = Vec::new();

    // 1. Crate-wide clippy allow check.
    let allowed: BTreeSet<ExceptionKey> = ALLOWED_CRATE_WIDE_ALLOWS.iter().copied().collect();
    let (crate_wide_findings, crate_wide_infra_errors) = find_crate_wide_clippy_allows(root);
    for finding in &crate_wide_findings {
        let key = (
            finding.relative_path.as_str(),
            finding.line,
            finding.attribute.as_str(),
        );
        if !allowed.contains(&key) {
            errors.push(format!(
                "global clippy allow attribute is not permitted: {}:{}: {}",
                finding.relative_path, finding.line, finding.attribute
            ));
        }
    }
    for infra in &crate_wide_infra_errors {
        errors.push(infra.clone());
    }

    // 2. Required symbols.
    for (symbol, file, desc) in REQUIRED_SYMBOLS {
        let path = root.join(file);
        if !file_contains(&path, symbol) {
            errors.push(format!("{file} must define {desc}"));
        }
    }

    // 3. Handler module line limits.
    let (handler_violations, handler_infra_errors) = handler_line_violations(root);
    for finding in &handler_violations {
        errors.push(format!(
            "handler module {} has {} lines (max {})",
            finding.relative_path, finding.lines, finding.limit
        ));
    }
    for infra in &handler_infra_errors {
        errors.push(infra.clone());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let stderr = format!(
            "Architecture boundary checks failed with {} error(s).\n{}",
            errors.len(),
            errors.join("\n")
        );
        Err(CommandFailed {
            program: "xtask".into(),
            args: vec!["check".into(), "architecture".into()],
            status: Some(1),
            stdout: Vec::new(),
            stderr: stderr.into_bytes(),
        })
    }
}

/// A found crate-wide clippy allow/expect attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateWideAllow {
    pub relative_path: String,
    pub line: usize,
    pub attribute: String,
}

/// A handler-module line-limit violation.
#[derive(Debug, Clone)]
pub struct HandlerViolation {
    pub relative_path: String,
    pub lines: usize,
    pub limit: usize,
}

/// Scan `src` and `tests` for crate-wide clippy allow attributes.
///
/// Matches the original `grep -nE '^#!\[(cfg_attr\([^]]*clippy|allow\([^]]*clippy)'`:
/// a line starting with `#![` followed by either `cfg_attr(...clippy` or
/// `allow(...clippy`.
///
/// Returns `(findings, infra_errors)`. Files that cannot be read produce an
/// infra error so the caller can fail loudly.
#[must_use]
pub fn find_crate_wide_clippy_allows(root: &Path) -> (Vec<CrateWideAllow>, Vec<String>) {
    let mut found = Vec::new();
    let mut infra_errors = Vec::new();
    for scan_dir in ["src", "tests"] {
        let dir = root.join(scan_dir);
        if !dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_rust_files(&dir, &mut files);
        files.sort();
        for file in files {
            let relative = file
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            match std::fs::read_to_string(&file) {
                Ok(content) => {
                    for (idx, line) in content.lines().enumerate() {
                        let lineno = idx + 1;
                        if is_crate_wide_clippy_allow_line(line) {
                            found.push(CrateWideAllow {
                                relative_path: relative.clone(),
                                line: lineno,
                                attribute: line.trim().to_string(),
                            });
                        }
                    }
                }
                Err(err) => {
                    infra_errors.push(format!(
                        "could not read {relative} for crate-wide clippy allow scan: {err}"
                    ));
                }
            }
        }
    }
    (found, infra_errors)
}

/// Does a single source line match the crate-wide clippy allow pattern?
#[must_use]
pub fn is_crate_wide_clippy_allow_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    // Match `#![(cfg_attr(...clippy` or `#![allow(...clippy` (the original
    // regex anchors on `^#!\[`).
    if !trimmed.starts_with("#![") {
        return false;
    }
    let inner = &trimmed[3..];
    // cfg_attr(...clippy
    if inner.starts_with("cfg_attr(") && inner.contains("clippy") {
        return true;
    }
    // allow(...clippy
    if inner.starts_with("allow(") && inner.contains("clippy") {
        return true;
    }
    false
}

/// Count lines in a handler module, counting the final line even when the
/// file has no trailing newline. Unlike `source_size::count_lines` (which
/// intentionally matches `wc -l`), the handler limit counts true lines so a
/// file with 851 lines and no trailing newline is still flagged at 851.
fn count_handler_lines(content: &str) -> usize {
    if content.is_empty() {
        return 0;
    }
    let newline_count = content.bytes().filter(|&b| b == b'\n').count();
    if content.ends_with('\n') {
        newline_count
    } else {
        newline_count + 1
    }
}

/// Check handler modules for line-limit violations.
///
/// Returns the violations plus a list of infrastructure-error messages
/// (e.g. `git ls-files` failed, a file could not be read). Infrastructure
/// errors are surfaced so a broken check fails loudly instead of silently
/// passing.
#[must_use]
pub fn handler_line_violations(root: &Path) -> (Vec<HandlerViolation>, Vec<String>) {
    let (files, mut infra_errors) = git_handler_files(root);
    let mut violations = Vec::new();
    for file in files {
        match std::fs::read_to_string(&file) {
            Ok(content) => {
                let lines = count_handler_lines(&content);
                let relative = file
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                let limit = if relative == "src/state/form_ops.rs" {
                    FORM_OPS_LIMIT
                } else {
                    DEFAULT_HANDLER_LIMIT
                };
                if lines > limit {
                    violations.push(HandlerViolation {
                        relative_path: relative,
                        lines,
                        limit,
                    });
                }
            }
            Err(err) => {
                let relative = relative_path(&file, root);
                infra_errors.push(format!("could not read handler module {relative}: {err}"));
            }
        }
    }
    (violations, infra_errors)
}

/// Enumerate handler-module files via `git ls-files`, matching the globs
/// `src/{app_input,state}/{*ops,*handlers,*dispatch}.rs`.
///
/// Returns `(files, infra_errors)`. If `git ls-files` fails, the error is
/// captured in `infra_errors` (and `files` is empty) so the caller can fail
/// loudly rather than silently passing.
fn git_handler_files(root: &Path) -> (Vec<PathBuf>, Vec<String>) {
    let patterns = [
        "src/app_input/*ops.rs",
        "src/app_input/*handlers.rs",
        "src/app_input/*dispatch.rs",
        "src/state/*ops.rs",
        "src/state/*handlers.rs",
        "src/state/*dispatch.rs",
    ];
    let output = CommandPlan::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .args(patterns)
        .current_dir(root)
        .run_captured();
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut files: Vec<PathBuf> = stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| root.join(l))
                .filter(|p| p.is_file())
                .collect();
            files.sort();
            (files, Vec::new())
        }
        Err(err) => (
            Vec::new(),
            vec![format!(
                "git ls-files failed; handler module line-limit check could not enumerate files: {err}"
            )],
        ),
    }
}

/// Recursively collect `*.rs` files under `dir`.
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

/// Does `path` contain `needle` as a substring?
fn file_contains(path: &Path, needle: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|content| content.contains(needle))
}

/// Produce a stable forward-slash relative path for diagnostics. Falls back to
/// the file's display form if it cannot be relativized against `root`.
fn relative_path(file: &Path, root: &Path) -> String {
    file.strip_prefix(root).map_or_else(
        |_| file.to_string_lossy().into_owned(),
        |p| p.to_string_lossy().replace('\\', "/"),
    )
}
