//! Capture shim registration, record model, and expectation checks
//! (issue #380, CW00-03).
//!
//! A capture materializes the `jefe-capture-shim` fixture executable at a
//! workspace-relative path. The shim locates its behavior file beside its
//! own executable (no env, no PATH lookup), claims a start ordinal, records
//! exact process-boundary fields, emits the configured streams, and exits
//! with the configured code. This module owns the shared DTOs, the runner
//! side of registration, and `assert-capture` evaluation.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::contract::CaptureExpectation;
#[cfg(unix)]
use super::contract::{CaptureBehavior, RelPath};
use super::error::HarnessError;
use super::limits::MAX_PROCESSES_PER_CAPTURE;
#[cfg(unix)]
use super::workspace::Workspace;

/// Suffix appended to the shim executable path to locate its behavior file.
pub const BEHAVIOR_SUFFIX: &str = ".capture.json";
/// Directory under the workspace root holding per-capture records.
pub const RECORDS_DIR: &str = ".captures";
/// Role marker instructing a spawned shim child to hang without recording.
pub const HANG_CHILD_ENV: &str = "JEFE_CAPTURE_SHIM_ROLE";
/// Role value for the hanging child.
pub const HANG_CHILD_ROLE: &str = "hang-child";
/// Shim exit code when every ordinal slot is already claimed.
pub const EXHAUSTED_EXIT: u8 = 96;

/// Behavior file written beside the shim executable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShimBehaviorFile {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
    pub stdin_limit: u64,
    pub hang: bool,
    pub spawn_child_hang: bool,
    /// Absolute records directory for this capture.
    pub records_dir: String,
}

/// One recorded shim invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureRecord {
    pub ordinal: u64,
    pub pid: u32,
    pub child_pid: Option<u32>,
    pub argv: Vec<String>,
    /// Env pairs sorted by name.
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub stdin: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
    /// False when only the start record exists (the shim hung or was killed).
    pub completed: bool,
}

/// Register a capture: materialize the shim at `path`, write its behavior
/// file, and create the records directory.
///
/// # Errors
///
/// `HAR-E004`/`HAR-E005` per the workspace contract.
#[cfg(unix)]
pub fn register(
    workspace: &mut Workspace,
    shim_binary: &Path,
    name: &str,
    path: &RelPath,
    behavior: &CaptureBehavior,
) -> Result<(), HarnessError> {
    let records_rel = RelPath::derived(format!("{RECORDS_DIR}/{name}"));
    ensure_records_dirs(workspace, name)?;
    let records_dir = workspace.root().join(records_rel.as_str());
    let target = workspace.resolve(path)?;
    std::fs::copy(shim_binary, &target)
        .map_err(|err| HarnessError::process(format!("materialize capture '{name}': {err}")))?;
    std::fs::set_permissions(&target, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .map_err(|err| HarnessError::process(format!("chmod capture '{name}': {err}")))?;
    let file = ShimBehaviorFile {
        stdout: behavior.stdout.clone(),
        stderr: behavior.stderr.clone(),
        exit_code: behavior.exit_code,
        stdin_limit: behavior.stdin_limit,
        hang: behavior.hang,
        spawn_child_hang: behavior.spawn_child_hang,
        records_dir: records_dir.to_string_lossy().into_owned(),
    };
    let serialized = serde_json::to_string(&file)
        .map_err(|err| HarnessError::process(format!("encode behavior '{name}': {err}")))?;
    let behavior_path = behavior_path_for(&target);
    std::fs::write(&behavior_path, serialized)
        .map_err(|err| HarnessError::process(format!("write behavior '{name}': {err}")))?;
    Ok(())
}

#[cfg(unix)]
fn ensure_records_dirs(workspace: &mut Workspace, name: &str) -> Result<(), HarnessError> {
    use super::contract::DirSpec;
    let base = RelPath::derived(RECORDS_DIR.to_string());
    if !workspace.exists(&base)? {
        workspace.mkdir(&DirSpec {
            path: base,
            mode: 0o700,
        })?;
    }
    let capture_dir = RelPath::derived(format!("{RECORDS_DIR}/{name}"));
    if !workspace.exists(&capture_dir)? {
        workspace.mkdir(&DirSpec {
            path: capture_dir,
            mode: 0o700,
        })?;
    }
    Ok(())
}

/// The behavior file path for a shim executable path.
#[must_use]
pub fn behavior_path_for(shim_path: &Path) -> std::path::PathBuf {
    let mut name = shim_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.push_str(BEHAVIOR_SUFFIX);
    shim_path.with_file_name(name)
}

/// Load all records for a capture, ordered by ordinal. A start-only record
/// (shim hung or killed) is included with `completed:false`.
///
/// # Errors
///
/// `HAR-E005` for unreadable or undecodable records.
pub fn load_records(workspace: &Path, name: &str) -> Result<Vec<CaptureRecord>, HarnessError> {
    let dir = workspace.join(RECORDS_DIR).join(name);
    let mut records = Vec::new();
    for ordinal in 1..=(MAX_PROCESSES_PER_CAPTURE as u64) {
        let done = dir.join(format!("{ordinal}.json"));
        let start = dir.join(format!("{ordinal}.start.json"));
        let path = if done.exists() {
            done
        } else if start.exists() {
            start
        } else {
            break;
        };
        let bytes = std::fs::read(&path).map_err(|err| {
            HarnessError::process(format!("read capture record '{name}' #{ordinal}: {err}"))
        })?;
        let record: CaptureRecord = serde_json::from_slice(&bytes).map_err(|err| {
            HarnessError::process(format!("decode capture record '{name}' #{ordinal}: {err}"))
        })?;
        records.push(record);
    }
    Ok(records)
}

/// Evaluate an `assert-capture` expectation against loaded records.
///
/// Recorded argv, cwd, and env values are workspace-normalized before
/// comparison: a `workspace_root` prefix is rewritten to the literal
/// `${workspace}` so expectations can state exact bytes despite the unique
/// per-run workspace path. Env pairs listed in the expectation must match
/// exactly by name; deterministic base variables not listed are permitted.
///
/// # Errors
///
/// `HAR-E006` describing the first mismatching field.
pub fn check_expectation(
    records: &[CaptureRecord],
    expectation: &CaptureExpectation,
    workspace_root: &str,
) -> Result<(), HarnessError> {
    let index = usize::try_from(expectation.invocation - 1)
        .map_err(|_| mismatch(expectation, "invocation", "out of range"))?;
    let record = records.get(index).ok_or_else(|| {
        mismatch(
            expectation,
            "invocation",
            &format!("only {} invocation(s) recorded", records.len()),
        )
    })?;
    check_argv(record, expectation, workspace_root)?;
    check_env(record, expectation, workspace_root)?;
    let recorded_cwd = normalize(&record.cwd, workspace_root);
    if recorded_cwd != expectation.cwd {
        return Err(mismatch(
            expectation,
            "cwd",
            &format!("expected '{}', recorded '{recorded_cwd}'", expectation.cwd),
        ));
    }
    check_optional(
        expectation,
        "stdin",
        expectation.stdin.as_ref(),
        &record.stdin,
    )?;
    check_optional(
        expectation,
        "stdout",
        expectation.stdout.as_ref(),
        &record.stdout,
    )?;
    check_optional(
        expectation,
        "stderr",
        expectation.stderr.as_ref(),
        &record.stderr,
    )?;
    check_exit(record, expectation)
}

/// Rewrite a `workspace_root` prefix to the literal `${workspace}` token.
fn normalize(value: &str, workspace_root: &str) -> String {
    value
        .strip_prefix(workspace_root)
        .map_or_else(|| value.to_string(), |rest| format!("${{workspace}}{rest}"))
}

fn check_argv(
    record: &CaptureRecord,
    expectation: &CaptureExpectation,
    workspace_root: &str,
) -> Result<(), HarnessError> {
    let recorded: Vec<String> = record
        .argv
        .iter()
        .map(|arg| normalize(arg, workspace_root))
        .collect();
    if recorded != expectation.argv {
        return Err(mismatch(
            expectation,
            "argv",
            &format!("expected {:?}, recorded {recorded:?}", expectation.argv),
        ));
    }
    Ok(())
}

fn check_env(
    record: &CaptureRecord,
    expectation: &CaptureExpectation,
    workspace_root: &str,
) -> Result<(), HarnessError> {
    for entry in &expectation.env {
        let recorded = record
            .env
            .iter()
            .find(|(name, _)| *name == entry.name)
            .map(|(_, value)| normalize(value, workspace_root));
        match recorded {
            Some(value) if value == entry.value => {}
            Some(value) => {
                return Err(mismatch(
                    expectation,
                    "env",
                    &format!(
                        "'{}' expected '{}', recorded '{value}'",
                        entry.name, entry.value
                    ),
                ));
            }
            None => {
                return Err(mismatch(
                    expectation,
                    "env",
                    &format!("'{}' was not recorded", entry.name),
                ));
            }
        }
    }
    Ok(())
}

fn check_optional(
    expectation: &CaptureExpectation,
    field: &str,
    expected: Option<&String>,
    recorded: &str,
) -> Result<(), HarnessError> {
    if let Some(value) = expected
        && value != recorded
    {
        return Err(mismatch(
            expectation,
            field,
            &format!("expected '{value}', recorded '{recorded}'"),
        ));
    }
    Ok(())
}

fn check_exit(
    record: &CaptureRecord,
    expectation: &CaptureExpectation,
) -> Result<(), HarnessError> {
    if let Some(code) = expectation.exit_code {
        if !record.completed {
            return Err(mismatch(
                expectation,
                "exit_code",
                "invocation did not complete",
            ));
        }
        if record.exit_code != code {
            return Err(mismatch(
                expectation,
                "exit_code",
                &format!("expected {code}, recorded {}", record.exit_code),
            ));
        }
    }
    if expectation.signal.is_some() && record.completed {
        return Err(mismatch(
            expectation,
            "signal",
            "invocation completed normally",
        ));
    }
    Ok(())
}

fn mismatch(expectation: &CaptureExpectation, field: &str, detail: &str) -> HarnessError {
    HarnessError::assertion(format!(
        "capture '{}' invocation {} {field}: {detail}",
        expectation.name, expectation.invocation
    ))
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod capture_tests;
