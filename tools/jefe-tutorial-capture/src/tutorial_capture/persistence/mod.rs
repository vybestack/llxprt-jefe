//! Persistence boundary: versioned manifest DTO, atomic writes, and domain
//! reconstruction with canonical run-root containment.
//!
//! This module is the **only** place that serializes/deserializes the run
//! manifest to/from disk. It enforces:
//!
//! - **Versioned schema**: a `schema_version` field is written with every
//!   manifest. On load, unknown or incompatible versions are rejected so a
//!   stale manifest from a previous format can never be misinterpreted.
//! - **Atomic writes**: manifests are written to a temp file and renamed,
//!   so a crash never leaves a partially-written manifest that cleanup
//!   could mis-trust.
//! - **Canonical run-root containment**: every owned path recorded in the
//!   manifest must be canonicalized and contained within the run root.
//!   Symlinks, traversal, and duplicates are rejected so cleanup cannot
//!   follow an attacker-controlled path outside the run root.
//! - **Exact expected resource-kind paths**: only paths matching the known
//!   sub-directory layout (`config`, `artifacts`, `shims`, `fixture-repo`)
//!   are accepted.
//! - **Production/current-checkout refusal**: cleanup refuses to operate if
//!   the run root is detected to be inside a production repository or the
//!   developer's current working checkout.
//!
//! ## Sub-module layout
//!
//! - [`error`] — `PersistenceError` enum and Display impl.
//! - [`dto`] — versioned `ManifestDto` and domain reconstruction.
//! - [`atomic_io`] — atomic temp-file + rename writes.
//! - [`containment`] — path containment and NUL-byte validation.
//! - [`run_root`] — exclusive run-root creation and sentinel verification.
//! - [`cleanup`] — manifest-scoped cleanup and artifact writing.
//!
//! ## Boundary
//!
//! This module owns manifest serialization, file I/O, path containment
//! validation, and run-root collision detection. It does not call tmux, git
//! (beyond the version probe), or `gh`.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

mod atomic_io;
mod cleanup;
mod containment;
mod dto;
mod error;
mod run_root;

use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::RunManifest;

pub use cleanup::{CleanupOutcome, CleanupRecord, cleanup_with_containment, write_artifact_atomic};
pub use containment::{validate_artifact_path, validate_owned_paths};
pub use dto::{MANIFEST_FILENAME, MANIFEST_SCHEMA_VERSION, ManifestDto};
pub use error::PersistenceError;
pub use run_root::{
    EXCLUSIVE_SENTINEL, create_run_root_exclusive, create_run_root_with_run_id,
    verify_sentinel_ownership,
};

// Re-export internal helpers needed by non-test code and the tests module.
pub use atomic_io::atomic_write;
#[cfg(test)]
pub use containment::lexical_canonical;
pub use dto::{dto_to_manifest, manifest_to_dto};

// Manifest types re-exported for the tests module (which uses `super::*`).
#[cfg(test)]
use crate::manifest::{OwnedPath, OwnedPathKind, RunId, RunOutcome, RuntimeProfile};

/// Write a manifest atomically: serialize to JSON, write to a temp file in
/// the same directory, then rename. This prevents partial writes from
/// corrupting the manifest that cleanup trusts.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
///
/// # Errors
///
/// Returns [`PersistenceError`] if serialization, temp-file creation, or the
/// rename fails.
pub fn save_manifest_atomic(
    manifest: &RunManifest,
    run_root: &Path,
) -> Result<(), PersistenceError> {
    let manifest_path = manifest_path(run_root);
    let dto = manifest_to_dto(manifest);
    let json = serde_json::to_string_pretty(&dto).map_err(|e| PersistenceError::Json {
        reason: e.to_string(),
    })?;
    atomic_write(&manifest_path, &json)
}

/// The manifest path within a run root.
#[must_use]
pub fn manifest_path(run_root: &Path) -> PathBuf {
    run_root.join(MANIFEST_FILENAME)
}

/// Load a manifest from disk, validate its schema version, and reconstruct
/// the domain type with full containment validation against the run root.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
///
/// # Errors
///
/// Returns [`PersistenceError`] on any I/O, schema, validation, or
/// containment error.
pub fn load_and_validate(run_root: &Path) -> Result<RunManifest, PersistenceError> {
    let path = manifest_path(run_root);
    let json = fs::read_to_string(&path).map_err(|e| atomic_io::io_error_path(&path, e))?;
    let dto: ManifestDto = serde_json::from_str(&json).map_err(|e| PersistenceError::Json {
        reason: e.to_string(),
    })?;
    dto_to_manifest(&dto, run_root)
}

#[cfg(test)]
#[path = "../persistence_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../persistence_sentinel_tests.rs"]
mod sentinel_tests;
