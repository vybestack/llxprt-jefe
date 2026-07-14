//! Provenance metadata: binary hashes, git commits, scenario hashes, and
//! tool version collection.
//!
//! Extracted from `orchestration.rs` to keep that module under the
//! per-file line limit.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use std::fs;
use std::path::Path;
use std::process::Command;

use super::orchestration::OrchestrationError;

/// Compute a SHA-256 digest of a binary file for reproducibility metadata.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
#[must_use]
pub fn compute_binary_hash(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    Some(format!("{digest:x}"))
}

/// Get the short git commit hash of the current repository.
///
/// Returns `None` if git is unavailable or the repository cannot be read.
#[must_use]
pub fn current_git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !commit.is_empty() {
            return Some(commit);
        }
    }
    None
}

/// Compute a SHA-256 digest of a scenario file for reproducibility metadata.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
///
/// # Errors
///
/// Returns [`OrchestrationError`] if the file cannot be read.
pub fn compute_scenario_hash(scenario_path: &Path) -> Result<String, OrchestrationError> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(scenario_path).map_err(|e| OrchestrationError::Io {
        path: scenario_path.to_path_buf(),
        reason: e.to_string(),
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    Ok(format!("{digest:x}"))
}

/// Collect tool version information for reproducibility metadata.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
#[must_use]
pub fn collect_tool_versions() -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("tmux".to_string(), probe_version("tmux", &["-V"]));
    map.insert("git".to_string(), probe_version("git", &["--version"]));
    map.insert("gh".to_string(), probe_version("gh", &["--version"]));
    serde_json::Value::Object(map)
}

/// Probe a tool's version string by running it with the given args.
fn probe_version(tool: &str, args: &[&str]) -> serde_json::Value {
    let output = Command::new(tool).args(args).output();
    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            serde_json::Value::String(version)
        }
        _ => serde_json::Value::Null,
    }
}
