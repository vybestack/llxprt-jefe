//! Canonical run-root containment validation.
//!
//! Every owned path recorded in the manifest must be canonicalized and
//! contained within the run root. Symlinks, traversal, and duplicates are
//! rejected so cleanup cannot follow an attacker-controlled path outside the
//! run root.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use std::fs;
use std::path::{Path, PathBuf};

use super::error::PersistenceError;
use crate::tutorial_capture::manifest::{OwnedPath, OwnedPathKind};

/// The sub-directory names that are valid owned-path kinds within a run root.
///
/// Cleanup only removes paths that match one of these exact canonical names
/// under the run root. Any other path — even if syntactically contained — is
/// refused.
const VALID_SUBDIR_NAMES: &[(OwnedPathKind, &str)] = &[
    (OwnedPathKind::ConfigDir, "config"),
    (OwnedPathKind::ArtifactDir, "artifacts"),
    (OwnedPathKind::ShimDir, "shims"),
    (OwnedPathKind::FixtureRepo, "fixture-repo"),
    (OwnedPathKind::FixtureClone, "fixture-clone"),
];

/// Validate that every owned path in the manifest is:
///
/// 1. Free of NUL bytes.
/// 2. Canonicalized and lexically contained within `run_root`.
/// 3. Not a symlink (neither the path itself nor any parent component).
/// 4. Not a duplicate of another owned path.
/// 5. Matches an expected sub-directory name within the run root.
///
/// This is the core safety gate that prevents cleanup from following
/// attacker-controlled paths outside the run root.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
pub fn validate_owned_paths(owned: &[OwnedPath], run_root: &Path) -> Result<(), PersistenceError> {
    let canonical_root = lexical_canonical(run_root);
    let mut seen: Vec<PathBuf> = Vec::new();
    for entry in owned {
        check_nul_free(&entry.path)?;
        let canonical = lexical_canonical(&entry.path);
        if !canonical.starts_with(&canonical_root) {
            return Err(PersistenceError::PathNotContained {
                path: entry.path.clone(),
                run_root: run_root.to_path_buf(),
            });
        }
        validate_expected_subdir(&canonical, &canonical_root, entry.kind)?;
        check_no_symlink(&entry.path, run_root)?;
        if seen.contains(&canonical) {
            return Err(PersistenceError::DuplicatePath {
                path: entry.path.clone(),
            });
        }
        seen.push(canonical);
    }
    Ok(())
}

/// Check that a path contains no NUL bytes (path injection defense).
pub fn check_nul_free(path: &Path) -> Result<(), PersistenceError> {
    for component in path.components() {
        let s = component.as_os_str().to_string_lossy();
        if s.contains('\0') {
            return Err(PersistenceError::NulInPath {
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    Ok(())
}

/// Lexically canonicalize a path: resolve `.` and `..` components without
/// touching the filesystem. This avoids TOCTOU issues and handles paths that
/// do not yet exist.
///
/// Note: this does NOT resolve symlinks on disk (that is done separately by
/// `check_no_symlink`). Lexical canonicalization is sufficient for containment
/// checking because the run root is always absolute and all sub-paths are
/// constructed by joining known components.
#[must_use]
pub fn lexical_canonical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => {
                result.push(other.as_os_str());
            }
        }
    }
    result
}

/// Check that a canonical path matches the expected sub-directory for its kind.
fn validate_expected_subdir(
    canonical_path: &Path,
    canonical_root: &Path,
    kind: OwnedPathKind,
) -> Result<(), PersistenceError> {
    let expected_name = VALID_SUBDIR_NAMES
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, name)| *name)
        .ok_or_else(|| PersistenceError::UnexpectedSubdir {
            path: canonical_path.to_path_buf(),
        })?;
    let expected_path = canonical_root.join(expected_name);
    if canonical_path != expected_path {
        return Err(PersistenceError::UnexpectedSubdir {
            path: canonical_path.to_path_buf(),
        });
    }
    Ok(())
}

/// Check that neither the path itself nor any component **within the run
/// root** is a symlink. Parent directories above the run root are not checked
/// because they are system-managed (e.g. `/var` → `/private/var` on macOS).
/// Only the final path component and its parents within the run root are
/// checked to prevent symlink-based traversal attacks within the run tree.
fn check_no_symlink(path: &Path, run_root: &Path) -> Result<(), PersistenceError> {
    let canonical_path = lexical_canonical(path);
    let canonical_root = lexical_canonical(run_root);
    // Only check components that are within the run root.
    let relative = canonical_path
        .strip_prefix(&canonical_root)
        .unwrap_or(&canonical_path);
    let mut current = canonical_root.clone();
    for component in relative.components() {
        current.push(component.as_os_str());
        if is_symlink(&current) {
            return Err(PersistenceError::SymlinkFound {
                path: current.clone(),
            });
        }
    }
    Ok(())
}

/// Whether a path is a symlink (without following it).
fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Validate that an artifact's relative path is safe: no traversal, no
/// absolute paths, no NUL bytes.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
///
/// # Errors
///
/// Returns [`PersistenceError`] if the path is absolute, contains `..`,
/// or contains NUL bytes.
pub fn validate_artifact_path(relative_path: &Path) -> Result<(), PersistenceError> {
    check_nul_free(relative_path)?;
    if relative_path.is_absolute() {
        return Err(PersistenceError::PathNotContained {
            path: relative_path.to_path_buf(),
            run_root: PathBuf::new(),
        });
    }
    for component in relative_path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(PersistenceError::PathNotContained {
                path: relative_path.to_path_buf(),
                run_root: PathBuf::new(),
            });
        }
    }
    Ok(())
}
