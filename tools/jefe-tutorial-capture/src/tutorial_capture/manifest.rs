//! Run manifest: typed ownership tracking for tutorial-capture runs.
//!
//! The manifest records every resource a run creates — local paths, GitHub
//! resources, artifacts — so cleanup can be manifest-scoped and never touch
//! unrelated state. It is serialized to JSON alongside the run artifacts and
//! updated incrementally as resources are created.
//!
//! ## Boundary
//!
//! This module owns manifest data types, validation, and serialization. It
//! does not perform I/O; the orchestration layer calls `add_*` methods and
//! the persistence layer writes the serialized form.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A unique run identifier used in paths, issue titles, branch names, and
/// artifact prefixes. Constrained to ASCII alphanumeric and hyphens so it is
/// safe in all those contexts.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct RunId(String);

impl RunId {
    /// Create a `RunId` from a string, validating the character set.
    ///
    /// Returns `None` if the value is empty, longer than 64 characters, or
    /// contains characters outside `[a-zA-Z0-9-]`.
    ///
    /// @requirement REQ-TUTORIAL-CAPTURE-001
    #[must_use]
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if is_valid_run_id(&value) {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Borrow the run ID string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the inner string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl<'de> Deserialize<'de> for RunId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(&value)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid run ID: {value}")))
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether a string is safe to use as a `RunId`.
fn is_valid_run_id(value: &str) -> bool {
    if value.is_empty() || value.len() > 64 {
        return false;
    }
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Which agent runtime(s) the run makes available via PATH shims.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProfile {
    /// Deterministic shim that exposes stable terminal text. No real runtime.
    Shim,
    /// Use the real `llxprt` binary if available on the host PATH.
    RealLlxprt,
    /// Use the real `code-puppy` binary if available on the host PATH.
    RealCodePuppy,
}

impl RuntimeProfile {
    /// Whether this profile uses a deterministic shim rather than a real
    /// runtime.
    #[must_use]
    pub const fn is_shim(self) -> bool {
        matches!(self, Self::Shim)
    }
}

/// A GitHub resource created by a Tier-B run.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubResource {
    /// Resource kind: `issue`, `branch`, `pull_request`.
    pub kind: GitHubResourceKind,
    /// Full `owner/repo` the resource belongs to.
    pub repository: String,
    /// Numeric or URL identifier for cleanup (issue number, branch name, PR number).
    pub identifier: String,
    /// Human-readable URL if available.
    pub url: Option<String>,
    /// Exact title from the mutation plan for issue/PR resources.
    /// Empty for branch resources.
    ///
    /// **Finding #2**: Persisted so scenario generation can match on the
    /// exact title for filtering and assertion.
    #[serde(default)]
    pub title: String,
}

/// Kind of GitHub resource recorded in the manifest.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubResourceKind {
    Issue,
    Branch,
    PullRequest,
}

/// An artifact file produced by the run.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactEntry {
    /// Semantic checkpoint label (e.g. `dashboard-oriented`).
    pub label: String,
    /// Relative path within the artifact directory.
    pub relative_path: PathBuf,
    /// Artifact kind: text capture, scrollback, or report.
    pub kind: ArtifactKind,
}

/// Kind of artifact produced by the run.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    ScreenCapture,
    Scrollback,
    Report,
    Manifest,
    /// Visual artifact: monochrome SVG rendering of a screen capture.
    Visual,
    /// ANSI escape-sequence capture (raw color data for color SVG).
    AnsiCapture,
    /// Color-preserving SVG rendering of an ANSI capture.
    ColorSvg,
    /// Generated Tier B scenario JSON (issue #241 Finding #4).
    Scenario,
}

/// A local path created or owned by the run.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedPath {
    /// What the path represents.
    pub kind: OwnedPathKind,
    /// Absolute filesystem path.
    pub path: PathBuf,
}

/// Kind of local path recorded in the manifest.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnedPathKind {
    /// Isolated Jefe config directory.
    ConfigDir,
    /// Disposable git repository/worktree root (provisioned locally by `prepare`).
    FixtureRepo,
    /// Cloned fixture repository from `gh repo clone` during Tier-B execution.
    ///
    /// **Finding #1**: Distinct from `FixtureRepo` so containment validation
    /// can enforce the correct expected sub-directory (`fixture-clone` vs
    /// `fixture-repo`) and prevent kind/path mismatches.
    FixtureClone,
    /// Artifact output directory.
    ArtifactDir,
    /// Run-scoped PATH shim directory.
    ShimDir,
}

/// The complete run manifest.
///
/// Records all metadata and owned resources for a single tutorial-capture run.
/// Cleanup consumes the `owned_paths` and `github_resources` lists.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: RunId,
    pub jefe_version: String,
    pub git_commit: Option<String>,
    pub scenario_name: String,
    /// Optional hash of the scenario file for reproducibility verification.
    #[serde(default)]
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
    /// SHA-256 hash of the Jefe binary under test.
    #[serde(default)]
    pub binary_hash: Option<String>,
    /// Theme name used for the capture.
    #[serde(default)]
    pub theme: Option<String>,
    /// Parsed tool versions (agent runtime, tmux, etc.).
    #[serde(default)]
    pub tool_versions: Option<serde_json::Value>,
    /// Observed actions performed during the run (for the report).
    #[serde(default)]
    pub observed_actions: Vec<ObservedAction>,
    /// Discrepancies between expected and observed behavior.
    #[serde(default)]
    pub discrepancies: Vec<String>,
    /// Creation-time allowlist provenance: the repos that were allowlisted
    /// when GitHub resources were created. Cleanup revalidates every resource
    /// against this set so resources created under one allowlist cannot be
    /// cleaned under a different one.
    ///
    /// **Finding #3**: Preserve creation-time allowlist provenance.
    #[serde(default)]
    pub creation_allowlist: Vec<String>,
    /// Whether merge was explicitly authorized for this run via
    /// `plan-github --allow-merge`. The capture-github merge scenario
    /// requires this to be true.
    #[serde(default)]
    pub merge_authorized: bool,
    /// Which agent runtime shims were installed for this run.
    ///
    /// **Finding #4**: Records the shim availability so Tier B state seeding
    /// can derive the correct agent kind from the manifest alone.
    #[serde(default)]
    pub shim_availability: super::path_shim::ShimAvailability,
}

/// An action observed during the tutorial-capture run.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedAction {
    /// The keybinding or label that triggered the action.
    pub keybinding: String,
    /// Human-readable description of what happened.
    pub description: String,
    /// Semantic checkpoint label if a capture was taken.
    pub checkpoint: Option<String>,
}

/// Outcome of a run.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Pending,
    Success,
    Failed,
    Partial,
}

impl RunManifest {
    /// Create a new manifest with the given run metadata and no owned resources.
    ///
    /// @requirement REQ-TUTORIAL-CAPTURE-001
    #[must_use]
    pub fn new(
        run_id: RunId,
        jefe_version: impl Into<String>,
        scenario_name: impl Into<String>,
        cols: u16,
        rows: u16,
        runtime_profile: RuntimeProfile,
    ) -> Self {
        Self {
            run_id,
            jefe_version: jefe_version.into(),
            git_commit: None,
            scenario_name: scenario_name.into(),
            scenario_hash: None,
            cols,
            rows,
            runtime_profile,
            fixture_repo_path: None,
            fixture_github_repo: None,
            owned_paths: Vec::new(),
            github_resources: Vec::new(),
            artifacts: Vec::new(),
            outcome: RunOutcome::Pending,
            cleanup_completed: false,
            binary_hash: None,
            theme: None,
            tool_versions: None,
            observed_actions: Vec::new(),
            discrepancies: Vec::new(),
            creation_allowlist: Vec::new(),
            merge_authorized: false,
            shim_availability: super::path_shim::ShimAvailability::default(),
        }
    }

    /// Record an owned local path.
    pub fn add_owned_path(&mut self, kind: OwnedPathKind, path: PathBuf) {
        if !self.owned_paths.iter().any(|p| p.path == path) {
            self.owned_paths.push(OwnedPath { kind, path });
        }
    }

    /// Record a created GitHub resource.
    pub fn add_github_resource(&mut self, resource: GitHubResource) {
        let exists = self.github_resources.iter().any(|existing| {
            existing.kind == resource.kind
                && existing
                    .repository
                    .eq_ignore_ascii_case(&resource.repository)
                && existing.identifier == resource.identifier
        });
        if !exists {
            self.github_resources.push(resource);
        }
    }

    /// Record a produced artifact.
    ///
    /// **Finding #6**: The relative_path is validated to be truly relative
    /// (not absolute, no parent-dir traversal) before recording. If the path
    /// is invalid, a typed [`ManifestArtifactError`] is returned so the caller
    /// can decide whether to warn or fail.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestArtifactError::UnsafePath`] if the artifact's
    /// relative path is absolute or contains parent-directory traversal.
    pub fn add_artifact(&mut self, entry: ArtifactEntry) -> Result<(), ManifestArtifactError> {
        // Finding #6: reject absolute paths and path traversal in artifact entries.
        if entry.relative_path.is_absolute()
            || entry
                .relative_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(ManifestArtifactError::UnsafePath {
                path: entry.relative_path,
            });
        }
        if !self
            .artifacts
            .iter()
            .any(|a| a.relative_path == entry.relative_path)
        {
            self.artifacts.push(entry);
        }
        Ok(())
    }

    /// Set the fixture repository path.
    pub fn set_fixture_repo(&mut self, path: PathBuf) {
        self.fixture_repo_path = Some(path);
    }

    /// Set the fixture GitHub repository (`owner/repo`).
    pub fn set_fixture_github_repo(&mut self, repo: impl Into<String>) {
        self.fixture_github_repo = Some(repo.into());
    }

    /// Mark the run outcome.
    pub fn set_outcome(&mut self, outcome: RunOutcome) {
        self.outcome = outcome;
    }

    /// Mark cleanup as completed.
    pub fn mark_cleanup_completed(&mut self) {
        self.cleanup_completed = true;
    }

    /// Record an observed action (keybinding + description + optional checkpoint).
    ///
    /// @requirement REQ-TUTORIAL-CAPTURE-005
    pub fn add_observed_action(
        &mut self,
        keybinding: impl Into<String>,
        description: impl Into<String>,
        checkpoint: Option<String>,
    ) {
        self.observed_actions.push(ObservedAction {
            keybinding: keybinding.into(),
            description: description.into(),
            checkpoint,
        });
    }

    /// Record a discrepancy between expected and observed behavior.
    ///
    /// @requirement REQ-TUTORIAL-CAPTURE-005
    pub fn add_discrepancy(&mut self, description: impl Into<String>) {
        self.discrepancies.push(description.into());
    }

    /// Set the creation-time allowlist provenance for cleanup revalidation.
    ///
    /// **Finding #3**: Records which repos were allowlisted when GitHub
    /// resources were created, so cleanup can revalidate every resource.
    pub fn set_creation_allowlist(&mut self, repos: Vec<String>) {
        self.creation_allowlist = repos;
    }

    /// Set whether merge is authorized for this run.
    ///
    /// Persisted from `plan-github --allow-merge` so the capture-github
    /// merge scenario can verify authorization before driving the merge
    /// confirmation through the Jefe TUI.
    pub fn set_merge_authorized(&mut self, authorized: bool) {
        self.merge_authorized = authorized;
    }

    /// Whether a repository was in the creation-time allowlist.
    ///
    /// **Finding #3**: Used by cleanup to revalidate resources.
    #[must_use]
    pub fn was_creation_allowed(&self, repo: &str) -> bool {
        let normalized = repo.trim().to_ascii_lowercase();
        self.creation_allowlist
            .iter()
            .any(|r| r.trim().eq_ignore_ascii_case(&normalized))
    }

    /// Serialize to pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Whether a path is owned by this manifest.
    #[must_use]
    pub fn owns_path(&self, path: &std::path::Path) -> bool {
        self.owned_paths.iter().any(|p| p.path == path)
    }

    /// Find the first owned path matching a kind.
    ///
    /// Returns `None` if no path of that kind has been recorded.
    #[must_use]
    pub fn find_path_by_kind(&self, kind: OwnedPathKind) -> Option<&std::path::Path> {
        self.owned_paths
            .iter()
            .find(|p| p.kind == kind)
            .map(|p| p.path.as_path())
    }

    /// Whether a GitHub repository is the fixture repository for this run.
    #[must_use]
    pub fn is_fixture_github_repo(&self, repo: &str) -> bool {
        self.fixture_github_repo
            .as_deref()
            .is_some_and(|r| r.eq_ignore_ascii_case(repo))
    }
}

/// Error returned when a manifest operation fails validation.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// A run ID failed validation.
    InvalidRunId { value: String },
    /// A manifest file could not be read or written.
    Io { path: String, reason: String },
    /// Manifest JSON was malformed.
    Json { reason: String },
}

/// Error returned when adding an artifact to the manifest fails validation.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestArtifactError {
    /// The artifact's relative path is absolute or contains parent-directory
    /// traversal, making it unsafe to record.
    UnsafePath { path: PathBuf },
}

impl std::fmt::Display for ManifestArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafePath { path } => write!(
                f,
                "unsafe artifact path '{}': must be relative with no parent-dir traversal",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ManifestArtifactError {}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRunId { value } => {
                write!(
                    f,
                    "invalid run ID '{value}': must be 1-64 ASCII alphanumeric or hyphen chars"
                )
            }
            Self::Io { path, reason } => {
                write!(f, "manifest I/O error at '{path}': {reason}")
            }
            Self::Json { reason } => write!(f, "manifest JSON error: {reason}"),
        }
    }
}

impl std::error::Error for ManifestError {}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
