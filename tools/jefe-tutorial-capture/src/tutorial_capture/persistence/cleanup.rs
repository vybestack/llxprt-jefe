//! Manifest-scoped cleanup of owned paths and artifact writing.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use std::fs;
use std::path::Path;

use super::atomic_io::{atomic_write, io_error_path};
use super::containment::{validate_artifact_path, validate_owned_paths};
use super::error::PersistenceError;
use super::run_root::verify_sentinel_ownership;
use crate::manifest::{OwnedPathKind, RunManifest};

/// Outcome of cleaning up a single owned path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupOutcome {
    /// The path was successfully removed.
    Removed,
    /// The path did not exist (already gone).
    AlreadyAbsent,
    /// The path was preserved as retained evidence.
    Retained,
    /// The path could not be removed.
    Failed { reason: String },
}

/// Record of cleanup for a single owned path.
#[derive(Debug, Clone)]
pub struct CleanupRecord {
    pub kind: OwnedPathKind,
    pub path: std::path::PathBuf,
    pub outcome: CleanupOutcome,
}

/// Perform manifest-scoped cleanup of owned paths.
///
/// Removes only paths that pass containment validation and sentinel
/// ownership verification. Evidence directories (artifacts) are preserved
/// by default unless `purge_evidence` is true.
///
/// The sentinel ownership check prevents forged manifests from triggering
/// cleanup on paths created by a different run.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
///
/// # Errors
///
/// Returns [`PersistenceError`] if containment validation fails, sentinel
/// ownership verification fails, or if a path cannot be removed (fail-closed).
pub fn cleanup_with_containment(
    manifest: &mut RunManifest,
    run_root: &Path,
    purge_evidence: bool,
) -> Result<Vec<CleanupRecord>, PersistenceError> {
    verify_sentinel_ownership(run_root, manifest.run_id.as_str())?;
    validate_owned_paths(&manifest.owned_paths, run_root)?;
    let mut records = Vec::new();
    // Remove deepest-first to allow directory removal.
    for entry in manifest.owned_paths.iter().rev() {
        let path = &entry.path;
        if entry.kind == OwnedPathKind::ArtifactDir && !purge_evidence {
            records.push(CleanupRecord {
                kind: entry.kind,
                path: path.clone(),
                outcome: CleanupOutcome::Retained,
            });
            continue;
        }
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                records.push(CleanupRecord {
                    kind: entry.kind,
                    path: path.clone(),
                    outcome: CleanupOutcome::AlreadyAbsent,
                });
                continue;
            }
            Err(err) => return Err(io_error_path(path, err)),
        };
        if metadata.file_type().is_symlink() {
            return Err(PersistenceError::SymlinkFound { path: path.clone() });
        }
        let result = if metadata.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        match result {
            Ok(()) => records.push(CleanupRecord {
                kind: entry.kind,
                path: path.clone(),
                outcome: CleanupOutcome::Removed,
            }),
            Err(e) => {
                return Err(PersistenceError::Io {
                    path: path.to_string_lossy().into_owned(),
                    reason: e.to_string(),
                });
            }
        }
    }
    manifest.mark_cleanup_completed();
    Ok(records)
}

fn reject_symlink_components(
    artifact_dir: &Path,
    relative_path: &Path,
) -> Result<(), PersistenceError> {
    let mut current = artifact_dir.to_path_buf();
    if let Ok(metadata) = fs::symlink_metadata(&current)
        && metadata.file_type().is_symlink()
    {
        return Err(PersistenceError::ManifestValidation(format!(
            "artifact root '{}' is a symlink",
            current.display()
        )));
    }
    for component in relative_path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(PersistenceError::ManifestValidation(format!(
                    "artifact path component '{}' is a symlink",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => return Err(io_error_path(&current, err)),
        }
    }
    Ok(())
}

/// Write an artifact file atomically to the artifact directory, registering
/// it in the manifest. The relative path is validated for safety.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
///
/// # Errors
///
/// Returns [`PersistenceError`] if the path is unsafe or the write fails.
pub fn write_artifact_atomic(
    artifact_dir: &Path,
    relative_path: &Path,
    content: &str,
    manifest: &mut RunManifest,
    label: impl Into<String>,
    kind: crate::manifest::ArtifactKind,
) -> Result<(), PersistenceError> {
    validate_artifact_path(relative_path)?;
    reject_symlink_components(artifact_dir, relative_path)?;
    let full_path = artifact_dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_error_path(parent, e))?;
        reject_symlink_components(artifact_dir, relative_path)?;
    }
    atomic_write(&full_path, content)?;
    manifest
        .add_artifact(crate::manifest::ArtifactEntry {
            label: label.into(),
            relative_path: relative_path.to_path_buf(),
            kind,
        })
        .map_err(|e| PersistenceError::ManifestValidation(e.to_string()))?;
    Ok(())
}
