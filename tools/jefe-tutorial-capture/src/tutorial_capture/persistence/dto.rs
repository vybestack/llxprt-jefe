//! Versioned manifest DTO and serialization.
//!
//! This module defines the on-disk wire format and the conversion to/from
//! the domain [`RunManifest`] type with full validation.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::PersistenceError;
use crate::manifest::{
    ArtifactEntry, GitHubResource, ObservedAction, OwnedPath, RunId, RunManifest, RunOutcome,
    RuntimeProfile,
};
use crate::path_shim::ShimAvailability;

/// The manifest schema version.
///
/// Increment when the DTO shape changes incompatibly. Old manifests with
/// unknown versions are rejected on load so cleanup never misinterprets a
/// stale or attacker-crafted manifest.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// The file name for the run manifest within the run root.
pub const MANIFEST_FILENAME: &str = "run-manifest.json";

/// Versioned persistence DTO for the run manifest. This is the on-disk wire
/// format. The domain [`RunManifest`] is reconstructed from it after
/// validation.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDto {
    pub schema_version: u32,
    pub run_id: String,
    pub jefe_version: String,
    pub git_commit: Option<String>,
    pub scenario_name: String,
    pub scenario_hash: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub runtime_profile: RuntimeProfile,
    pub fixture_repo_path: Option<PathBuf>,
    pub fixture_github_repo: Option<String>,
    pub owned_paths: Vec<OwnedPath>,
    pub github_resources: Vec<GitHubResource>,
    pub artifacts: Vec<ArtifactEntry>,
    pub outcome: RunOutcome,
    pub cleanup_completed: bool,
    pub binary_hash: Option<String>,
    pub theme: Option<String>,
    pub tool_versions: Option<serde_json::Value>,
    #[serde(default)]
    pub observed_actions: Vec<ObservedAction>,
    #[serde(default)]
    pub discrepancies: Vec<String>,
    #[serde(default)]
    pub creation_allowlist: Vec<String>,
    /// Whether merge was explicitly authorized via `plan-github --allow-merge`.
    ///
    /// **Finding**: Persisted through the versioned DTO so reloads preserve
    /// the authorization decision for the capture-github merge scenario.
    #[serde(default)]
    pub merge_authorized: bool,
    /// Which agent runtime shims were installed for this run.
    ///
    /// **Finding #4**: Persisted through the versioned DTO in all variants
    /// (llxprt_only, code_puppy_only, both) so reloads preserve the shim
    /// availability for Tier B state seeding.
    #[serde(default)]
    pub shim_availability: ShimAvailability,
}

/// Convert a domain manifest to its versioned DTO.
#[must_use]
pub fn manifest_to_dto(manifest: &RunManifest) -> ManifestDto {
    ManifestDto {
        schema_version: MANIFEST_SCHEMA_VERSION,
        run_id: manifest.run_id.as_str().to_string(),
        jefe_version: manifest.jefe_version.clone(),
        git_commit: manifest.git_commit.clone(),
        scenario_name: manifest.scenario_name.clone(),
        scenario_hash: manifest.scenario_hash.clone(),
        cols: manifest.cols,
        rows: manifest.rows,
        runtime_profile: manifest.runtime_profile,
        fixture_repo_path: manifest.fixture_repo_path.clone(),
        fixture_github_repo: manifest.fixture_github_repo.clone(),
        owned_paths: manifest.owned_paths.clone(),
        github_resources: manifest.github_resources.clone(),
        artifacts: manifest.artifacts.clone(),
        outcome: manifest.outcome,
        cleanup_completed: manifest.cleanup_completed,
        binary_hash: manifest.binary_hash.clone(),
        theme: manifest.theme.clone(),
        tool_versions: manifest.tool_versions.clone(),
        observed_actions: manifest.observed_actions.clone(),
        discrepancies: manifest.discrepancies.clone(),
        creation_allowlist: manifest.creation_allowlist.clone(),
        merge_authorized: manifest.merge_authorized,
        shim_availability: manifest.shim_availability,
    }
}

/// Reconstruct a domain manifest from a DTO with full validation.
///
/// Validation order:
/// 1. Schema version check.
/// 2. Run ID reconstruction.
/// 3. Field validation (cols/rows non-zero, strings non-empty).
/// 4. Owned-path containment against the run root.
fn validated_fixture_repo_path(
    dto: &ManifestDto,
    run_root: &Path,
) -> Result<Option<PathBuf>, PersistenceError> {
    let Some(path) = &dto.fixture_repo_path else {
        return Ok(None);
    };
    super::containment::check_nul_free(path)?;
    let canonical = super::containment::lexical_canonical(path);
    let canonical_root = super::containment::lexical_canonical(run_root);
    if !canonical.starts_with(&canonical_root) {
        return Err(PersistenceError::PathNotContained {
            path: path.clone(),
            run_root: run_root.to_path_buf(),
        });
    }
    Ok(Some(canonical))
}

pub fn dto_to_manifest(
    dto: &ManifestDto,
    run_root: &Path,
) -> Result<RunManifest, PersistenceError> {
    if dto.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(PersistenceError::SchemaVersion {
            found: dto.schema_version,
            expected: MANIFEST_SCHEMA_VERSION,
        });
    }
    let run_id = RunId::new(&dto.run_id).ok_or_else(|| PersistenceError::InvalidRunId {
        value: dto.run_id.clone(),
    })?;
    validate_fields(dto)?;
    super::containment::validate_owned_paths(&dto.owned_paths, run_root)?;
    let mut manifest = RunManifest::new(
        run_id,
        dto.jefe_version.clone(),
        dto.scenario_name.clone(),
        dto.cols,
        dto.rows,
        dto.runtime_profile,
    );
    manifest.git_commit.clone_from(&dto.git_commit);
    manifest.scenario_hash.clone_from(&dto.scenario_hash);
    if let Some(p) = validated_fixture_repo_path(dto, run_root)? {
        manifest.set_fixture_repo(p);
    }
    if let Some(r) = &dto.fixture_github_repo {
        manifest.set_fixture_github_repo(r);
    }
    for path in &dto.owned_paths {
        manifest.add_owned_path(path.kind, path.path.clone());
    }
    for resource in &dto.github_resources {
        manifest.add_github_resource(resource.clone());
    }
    for (index, artifact) in dto.artifacts.iter().enumerate() {
        manifest.add_artifact(artifact.clone()).map_err(|err| {
            PersistenceError::ManifestValidation(format!(
                "artifact {index} ('{}') is invalid: {err}",
                artifact.relative_path.display()
            ))
        })?;
    }
    manifest.set_outcome(dto.outcome);
    if dto.cleanup_completed {
        manifest.mark_cleanup_completed();
    }
    manifest.binary_hash.clone_from(&dto.binary_hash);
    manifest.theme.clone_from(&dto.theme);
    manifest.tool_versions.clone_from(&dto.tool_versions);
    manifest.observed_actions.clone_from(&dto.observed_actions);
    manifest.discrepancies.clone_from(&dto.discrepancies);
    manifest
        .creation_allowlist
        .clone_from(&dto.creation_allowlist);
    manifest.merge_authorized = dto.merge_authorized;
    manifest.shim_availability = dto.shim_availability;
    Ok(manifest)
}

/// Validate non-structural fields in the DTO.
fn validate_fields(dto: &ManifestDto) -> Result<(), PersistenceError> {
    if dto.jefe_version.is_empty() {
        return Err(PersistenceError::InvalidField {
            field: "jefe_version".to_string(),
            reason: "must not be empty".to_string(),
        });
    }
    if dto.scenario_name.is_empty() {
        return Err(PersistenceError::InvalidField {
            field: "scenario_name".to_string(),
            reason: "must not be empty".to_string(),
        });
    }
    if dto.cols == 0 {
        return Err(PersistenceError::InvalidField {
            field: "cols".to_string(),
            reason: "must be non-zero".to_string(),
        });
    }
    if dto.rows == 0 {
        return Err(PersistenceError::InvalidField {
            field: "rows".to_string(),
            reason: "must be non-zero".to_string(),
        });
    }
    Ok(())
}
