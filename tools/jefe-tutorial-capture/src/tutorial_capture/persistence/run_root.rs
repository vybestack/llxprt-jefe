//! Exclusive run-root creation with run-ID-bound sentinel.
//!
//! The sentinel is bound to the run ID so cleanup can verify that the
//! manifest's run ID matches the sentinel's run ID, preventing forged
//! manifests from triggering cleanup on paths they don't own.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use std::fs;
use std::path::{Path, PathBuf};

use super::atomic_io::atomic_write;
use super::containment::check_nul_free;
use super::error::PersistenceError;

/// The file name written when a run root is created exclusively, to detect
/// collisions.
pub const EXCLUSIVE_SENTINEL: &str = ".jefe-tutorial-claimed";

/// Create the run root exclusively. If it already exists (or a sentinel is
/// present), return a collision error. On success, write the sentinel file
/// and return.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
///
/// # Errors
///
/// Returns [`PersistenceError::RunRootCollision`] if the run root or sentinel
/// already exists, or [`PersistenceError::Io`] on I/O failure.
pub fn create_run_root_exclusive(run_root: &Path) -> Result<(), PersistenceError> {
    create_run_root_with_run_id(run_root, None)
}

/// Create the run root exclusively, binding the sentinel to the given run ID.
/// When `run_id` is provided, the sentinel file records it so cleanup can
/// verify ownership before removing paths.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
///
/// # Errors
///
/// Returns [`PersistenceError`] on collision or I/O failure.
pub fn create_run_root_with_run_id(
    run_root: &Path,
    run_id: Option<&str>,
) -> Result<(), PersistenceError> {
    check_production_checkout(run_root)?;
    check_nul_free(run_root)?;
    if run_root.exists() {
        return Err(PersistenceError::RunRootCollision {
            path: run_root.to_path_buf(),
        });
    }
    let parent = run_root.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| super::atomic_io::io_error_path(parent, e))?;
    // Use create_dir (not create_dir_all) on the run root itself so we get
    // an error if it was created by a race between our exists() check and
    // now.
    fs::create_dir(run_root).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            PersistenceError::RunRootCollision {
                path: run_root.to_path_buf(),
            }
        } else {
            super::atomic_io::io_error_path(run_root, e)
        }
    })?;
    let sentinel = run_root.join(EXCLUSIVE_SENTINEL);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let sentinel_content = if let Some(id) = run_id {
        format!("pid={}\ntime={time}\nrun_id={id}\n", std::process::id())
    } else {
        format!("pid={}\ntime={time}\n", std::process::id())
    };
    atomic_write(&sentinel, &sentinel_content)?;
    Ok(())
}

/// Verify that the sentinel file in the run root binds to the expected run ID.
/// This prevents forged manifests (created by an attacker or a different run)
/// from triggering cleanup on paths they don't own.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
///
/// # Errors
///
/// Returns [`PersistenceError`] if the sentinel is missing, unreadable, or
/// its run ID does not match `expected_run_id`.
pub fn verify_sentinel_ownership(
    run_root: &Path,
    expected_run_id: &str,
) -> Result<(), PersistenceError> {
    let sentinel = run_root.join(EXCLUSIVE_SENTINEL);
    let content =
        fs::read_to_string(&sentinel).map_err(|e| super::atomic_io::io_error_path(&sentinel, e))?;
    // Check for the run_id binding line.
    let bound_run_id = content
        .lines()
        .find_map(|line| line.strip_prefix("run_id=").map(str::trim));
    match bound_run_id {
        Some(id) if id == expected_run_id => Ok(()),
        Some(id) => Err(PersistenceError::InvalidField {
            field: "sentinel run_id".to_string(),
            reason: format!(
                "sentinel run_id '{id}' does not match manifest run_id '{expected_run_id}'"
            ),
        }),
        None => Err(PersistenceError::InvalidField {
            field: "sentinel run_id".to_string(),
            reason: "sentinel file does not bind a run_id — possible forged manifest".to_string(),
        }),
    }
}

/// Refuse to create a run root inside a production/current git checkout.
///
/// This prevents cleanup from ever touching the developer's working repository.
/// We check whether the run root's parent chain contains a `.git` directory
/// that is the actual production repository.
///
/// **Finding #10**: Uses canonical (symlink-resolved) existing ancestor
/// validation. On macOS, `/tmp` is a symlink to `/private/tmp`, so lexical
/// path walking would miss production checkouts reachable through symlinks.
/// We canonicalize the longest existing prefix of the run root path to
/// resolve symlinks before walking parents.
fn check_production_checkout(run_root: &Path) -> Result<(), PersistenceError> {
    let abs = if run_root.is_absolute() {
        run_root.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(run_root)
    };
    // Canonicalize the longest existing ancestor of the path to resolve
    // symlinks (e.g. /tmp → /private/tmp on macOS). The run root itself
    // may not exist yet (we're about to create it), so we walk up to find
    // the first existing ancestor and canonicalize that.
    let canonical_abs = canonicalize_existing_ancestor(&abs);
    let mut current = Some(canonical_abs.as_path());
    while let Some(dir) = current {
        let git_dir = dir.join(".git");
        if git_dir.exists() && is_production_git_repo(dir) {
            return Err(PersistenceError::ProductionCheckout {
                path: run_root.to_path_buf(),
                reason: format!(
                    "run root is inside a production git repository at '{}'",
                    dir.display()
                ),
            });
        }
        current = dir.parent();
    }
    Ok(())
}

/// Canonicalize the longest existing ancestor of a path by resolving
/// symlinks. This handles macOS `/tmp` → `/private/tmp` and similar
/// symlinked parent directories.
///
/// Walks up the path until it finds a component that exists on disk, then
/// calls `std::fs::canonicalize` on it and re-appends the remaining
/// non-existent components.
pub(super) fn canonicalize_existing_ancestor(path: &Path) -> PathBuf {
    // If the path itself exists, canonicalize it directly.
    if path.exists()
        && let Ok(canon) = fs::canonicalize(path)
    {
        return canon;
    }
    // Walk up to find the first existing ancestor.
    let mut existing = path.to_path_buf();
    let mut remaining: Vec<std::path::PathBuf> = Vec::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            remaining.push(PathBuf::from(name));
        }
        if !existing.pop() {
            break;
        }
    }
    // Canonicalize the existing ancestor.
    let canonical_base = fs::canonicalize(&existing).unwrap_or(existing);
    // Re-append the non-existent components.
    let mut result = canonical_base;
    for component in remaining.iter().rev() {
        result.push(component);
    }
    result
}

/// Whether a git repository's remote URL points to a known production repo.
fn is_production_git_repo(dir: &Path) -> bool {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output();
    let Ok(out) = output else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let url = String::from_utf8_lossy(&out.stdout)
        .trim()
        .to_ascii_lowercase();
    url.contains("vybestack/jefe") || url.contains("vybestack/llxprt-jefe")
}
