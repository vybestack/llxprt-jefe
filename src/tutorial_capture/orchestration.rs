//! Orchestration: setup, teardown, and manifest-scoped cleanup.
//!
//! The orchestration layer composes the pure manifest, path-shim, allowlist,
//! and redaction layers with filesystem I/O. It creates the isolated run
//! directory tree, writes shim scripts, provisions a disposable git repo,
//! and performs manifest-scoped cleanup.
//!
//! ## Boundary
//!
//! This module owns filesystem setup and teardown. It delegates pure decisions
//! to the manifest, path_shim, allowlist, and redaction modules. It does not
//! call tmux or the Jefe binary — that is the harness runner's job.
//! Manifest persistence is delegated to the [`persistence`] module.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-002

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::allowlist::{AllowlistDecision, FixtureAllowlist};
use super::manifest::{ManifestError, OwnedPathKind, RunId, RunManifest, RuntimeProfile};
use super::path_shim;
use super::persistence::{self, CleanupRecord, PersistenceError, save_manifest_atomic};
use super::redaction::{RedactionError, RedactionSet, build_redaction_set};

/// Error returned by orchestration operations.
///
/// @requirement REQ-TUTORIAL-CAPTURE-002
#[derive(Debug)]
pub enum OrchestrationError {
    /// A filesystem operation failed.
    Io { path: PathBuf, reason: String },
    /// A manifest operation failed.
    Manifest(ManifestError),
    /// A persistence operation failed.
    Persistence(PersistenceError),
    /// A git operation failed.
    Git { reason: String },
    /// A fixture repository was refused by the allowlist.
    FixtureRefused { repo: String, reason: String },
    /// A path was not owned by the manifest.
    NotOwned { path: PathBuf },
    /// A redaction operation failed.
    Redaction(RedactionError),
    /// Required system tools are missing for Tier A (sh, git, tmux).
    ///
    /// **Finding #1**: `prepare_run` fails if sh/git/tmux are unavailable.
    MissingRequiredTools { tools: Vec<String> },
}

impl std::fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, reason } => {
                write!(f, "I/O error at '{}': {reason}", path.display())
            }
            Self::Manifest(err) => write!(f, "{err}"),
            Self::Persistence(err) => write!(f, "{err}"),
            Self::Git { reason } => write!(f, "git error: {reason}"),
            Self::FixtureRefused { repo, reason } => {
                write!(f, "fixture refused for '{repo}': {reason}")
            }
            Self::NotOwned { path } => {
                write!(f, "path '{}' is not owned by the manifest", path.display())
            }
            Self::Redaction(err) => write!(f, "{err}"),
            Self::MissingRequiredTools { tools } => {
                write!(
                    f,
                    "required system tools not found on PATH: {}",
                    tools.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for OrchestrationError {}

impl From<ManifestError> for OrchestrationError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

impl From<PersistenceError> for OrchestrationError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}

impl From<std::io::Error> for OrchestrationError {
    fn from(value: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            reason: value.to_string(),
        }
    }
}

impl From<RedactionError> for OrchestrationError {
    fn from(value: RedactionError) -> Self {
        Self::Redaction(value)
    }
}

/// Configuration for a tutorial-capture run's local setup.
///
/// @requirement REQ-TUTORIAL-CAPTURE-002
#[derive(Debug, Clone)]
pub struct RunSetup {
    pub run_id: RunId,
    pub base_dir: PathBuf,
    pub jefe_version: String,
    pub scenario_name: String,
    pub cols: u16,
    pub rows: u16,
    pub runtime_profile: RuntimeProfile,
    pub fixture_github_repo: Option<String>,
    /// Optional path to the jefe binary for hashing.
    pub jefe_bin: Option<PathBuf>,
    /// Theme name for the capture run.
    pub theme: Option<String>,
    /// Optional scenario hash for reproducibility verification.
    pub scenario_hash: Option<String>,
    /// Which agent runtime shims to install for the `Shim` profile.
    /// Ignored for real-runtime profiles.
    ///
    /// **Finding**: Persisted from `prepare --shim-availability`.
    pub shim_availability: path_shim::ShimAvailability,
}

/// The directories created during setup.
///
/// @requirement REQ-TUTORIAL-CAPTURE-002
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDirectories {
    /// Root directory for the run (`base_dir/run_id`).
    pub root: PathBuf,
    /// Isolated Jefe config directory.
    pub config_dir: PathBuf,
    /// Artifact output directory.
    pub artifact_dir: PathBuf,
    /// Run-scoped PATH shim directory.
    pub shim_dir: PathBuf,
    /// Disposable git repository root.
    pub fixture_repo: PathBuf,
}

impl RunDirectories {
    /// The manifest JSON path.
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        persistence::manifest_path(&self.root)
    }

    /// The report Markdown path.
    #[must_use]
    pub fn report_path(&self) -> PathBuf {
        self.artifact_dir.join("run-report.md")
    }
}

/// The inherited PATH environment variable, or a safe default.
fn inherited_path() -> String {
    std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string())
}

/// Get the current hostname for redaction purposes.
/// Returns None if the hostname cannot be determined.
fn hostname_str() -> Option<String> {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

/// Compute the controlled PATH for a run, prepending the shim directory.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn controlled_path_for(shim_dir: &Path) -> String {
    path_shim::controlled_path(shim_dir, &inherited_path())
}

/// Compute a detection-isolated PATH using **curated PATH projection**.
///
/// Returns ONLY the curated bin directory. The orchestration layer writes
/// selected runtime shims and system-tool symlinks into this directory so
/// the launched process sees only what the harness projected — no inherited
/// PATH entries are used.
///
/// **Finding #2**: Curated PATH projection replaces the unsafe
/// directory-dropping filter.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn detection_path_for(shim_dir: &Path, profile: RuntimeProfile) -> String {
    path_shim::detection_path(shim_dir, profile, &inherited_path())
}

/// Plan the system-tool symlinks to write into the curated bin directory.
///
/// **Finding #2**: System tools (git, tmux, sh, gh) are projected as symlinks
/// into the curated bin, not inherited from PATH.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn plan_system_tool_links_for() -> Vec<path_shim::SystemToolLink> {
    path_shim::plan_system_tool_links(&inherited_path())
}

/// Plan the real-runtime symlink for a real-runtime profile.
///
/// **Finding #2**: The real runtime is projected as a symlink into the
/// curated bin.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn plan_real_runtime_link_for(profile: RuntimeProfile) -> Option<path_shim::SystemToolLink> {
    path_shim::plan_real_runtime_link(profile)
}

/// Prepare the local run directory tree for a tutorial-capture run.
///
/// Writes shims, provisions a git repo, creates the run root exclusively
/// with a run-ID-bound sentinel, and persists an atomic initial versioned
/// manifest immediately after sentinel creation.
///
/// This is the main setup entry point. It creates:
/// - `root/config` — isolated Jefe config
/// - `root/artifacts` — artifact output
/// - `root/shims` — PATH shim directory with scripts
/// - `root/fixture-repo` — disposable git repo with one commit
/// - `root/run-manifest.json` — atomic initial versioned manifest
///
/// The run root is created exclusively: if it already exists, an error is
/// returned to prevent collision with a prior run. The sentinel file binds
/// the run ID so cleanup can verify ownership.
///
/// **Non-Unix platforms fail fast**: the tutorial-capture workflow requires
/// tmux, git, and POSIX shell, so it is Unix-only.
///
/// @requirement REQ-TUTORIAL-CAPTURE-002
///
/// # Errors
///
/// Returns [`OrchestrationError`] if directory creation, shim writing, git
/// initialization, or manifest persistence fails.
pub fn prepare_run(setup: &RunSetup) -> Result<(RunDirectories, RunManifest), OrchestrationError> {
    // Non-Unix fail fast: the workflow requires tmux, git, and POSIX shell.
    #[cfg(not(unix))]
    {
        return Err(OrchestrationError::Io {
            path: PathBuf::new(),
            reason: "tutorial-capture is Unix-only: requires tmux, git, and POSIX shell"
                .to_string(),
        });
    }
    // Finding #1: fail prepare if sh/git/tmux required tools are unavailable.
    let missing = path_shim::check_tier_a_required_tools(&inherited_path());
    if !missing.is_empty() {
        return Err(OrchestrationError::MissingRequiredTools { tools: missing });
    }
    let dirs = compute_directories(&setup.base_dir, &setup.run_id);
    // Exclusive run-root creation with run-ID-bound sentinel.
    persistence::create_run_root_with_run_id(&dirs.root, Some(setup.run_id.as_str()))?;
    // Save the initial manifest immediately after the sentinel, so the run
    // root is always in a consistent state even if later steps fail.
    let initial_manifest = build_initial_manifest(setup, &dirs);
    persistence::save_manifest_atomic(&initial_manifest, &dirs.root)?;
    create_subdirectories(&dirs)?;
    write_shim_scripts(
        &dirs.shim_dir,
        setup.runtime_profile,
        setup.shim_availability,
    )?;
    // Finding #2: project system-tool symlinks into the curated bin so the
    // launched process can find git, tmux, sh without inheriting PATH.
    write_system_tool_links(&dirs.shim_dir)?;
    // Finding #2: for real-runtime profiles, symlink the selected runtime
    // into the curated bin.
    if let Some(link) = path_shim::plan_real_runtime_link(setup.runtime_profile) {
        write_runtime_link(&dirs.shim_dir, &link)?;
    }
    provision_fixture_repo(&dirs.fixture_repo)?;
    // Re-save manifest with all subdirectories provisioned.
    let manifest = build_initial_manifest(setup, &dirs);
    persistence::save_manifest_atomic(&manifest, &dirs.root)?;
    Ok((dirs, manifest))
}

/// Compute the directory layout for a run.
pub(super) fn compute_directories(base_dir: &Path, run_id: &RunId) -> RunDirectories {
    let root = base_dir.join(run_id.as_str());
    RunDirectories {
        config_dir: root.join("config"),
        artifact_dir: root.join("artifacts"),
        shim_dir: root.join("shims"),
        fixture_repo: root.join("fixture-repo"),
        root,
    }
}

/// Create all sub-directories in the run tree (the root is already created
/// exclusively by `create_run_root_exclusive`).
fn create_subdirectories(dirs: &RunDirectories) -> Result<(), OrchestrationError> {
    for dir in [&dirs.config_dir, &dirs.artifact_dir, &dirs.shim_dir] {
        fs::create_dir_all(dir).map_err(|e| io_error(dir, e))?;
    }
    Ok(())
}

/// Write shim scripts for the given runtime profile into the shim directory.
///
/// Each shim is written as an executable shell script with the binary name
/// Jefe detects.
///
/// **Finding**: `availability` controls which agent shims are installed
/// (llxprt-only, code-puppy-only, or both). Persisted from
/// `prepare --shim-availability`.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
pub(super) fn write_shim_scripts(
    shim_dir: &Path,
    profile: RuntimeProfile,
    availability: path_shim::ShimAvailability,
) -> Result<(), OrchestrationError> {
    let shims = path_shim::plan_shims(profile, availability);
    for shim in &shims {
        let shim_path = shim_dir.join(&shim.binary_name);
        write_executable_script(&shim_path, &shim.script)?;
    }
    Ok(())
}

/// Write a shell script and make it executable.
fn write_executable_script(path: &Path, content: &str) -> Result<(), OrchestrationError> {
    let mut file = fs::File::create(path).map_err(|e| io_error(path, e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| io_error(path, e))?;
    drop(file);
    make_executable(path)?;
    Ok(())
}

/// Write system-tool symlinks into the curated bin directory.
///
/// **Finding #2**: System tools (git, tmux, sh, gh) are projected as symlinks
/// into the curated bin directory so the launched process can find them
/// without inheriting the host PATH.
fn write_system_tool_links(shim_dir: &Path) -> Result<(), OrchestrationError> {
    let links = path_shim::plan_system_tool_links(&inherited_path());
    for link in &links {
        let link_path = shim_dir.join(&link.name);
        // Remove stale link if it exists, then create a fresh symlink.
        let _ = fs::remove_file(&link_path);
        symlink(&link.target, &link_path)?;
    }
    Ok(())
}

/// Write a real-runtime symlink into the curated bin directory.
///
/// **Finding #2**: For real-runtime profiles, the selected runtime binary
/// is symlinked into the curated bin so Jefe detects only it.
fn write_runtime_link(
    shim_dir: &Path,
    link: &path_shim::SystemToolLink,
) -> Result<(), OrchestrationError> {
    let link_path = shim_dir.join(&link.name);
    let _ = fs::remove_file(&link_path);
    symlink(&link.target, &link_path)?;
    Ok(())
}

/// Create a symlink (Unix). On non-Unix, copies the file.
#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> Result<(), OrchestrationError> {
    std::os::unix::fs::symlink(target, link).map_err(|e| io_error(link, e))
}

/// Create a symlink (non-Unix fallback: copy the file).
#[cfg(not(unix))]
fn symlink(target: &Path, link: &Path) -> Result<(), OrchestrationError> {
    fs::copy(target, link).map_err(|e| io_error(link, e))?;
    Ok(())
}

/// Set the executable bit on a file (Unix only; documented no-op on other
/// platforms).
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), OrchestrationError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o755);
    fs::set_permissions(path, perms).map_err(|e| io_error(path, e))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(path: &Path) -> Result<(), OrchestrationError> {
    let _ = path;
    Ok(())
}

/// Provision a disposable git repository with an initial commit.
///
/// @requirement REQ-TUTORIAL-CAPTURE-002
pub(super) fn provision_fixture_repo(repo_path: &Path) -> Result<(), OrchestrationError> {
    fs::create_dir_all(repo_path).map_err(|e| io_error(repo_path, e))?;
    git_run(repo_path, &["init"])?;
    git_run(
        repo_path,
        &["config", "user.email", "tutorial-capture@jefe.local"],
    )?;
    git_run(repo_path, &["config", "user.name", "Jefe Tutorial Capture"])?;
    let readme = repo_path.join("README.md");
    fs::write(
        &readme,
        "# Fixture Repository\n\nCreated by jefe tutorial-capture.\n",
    )
    .map_err(|e| io_error(&readme, e))?;
    git_run(repo_path, &["add", "README.md"])?;
    git_run(repo_path, &["commit", "-m", "Initial commit"])?;
    Ok(())
}

/// Run a git command in the given directory.
fn git_run(dir: &Path, args: &[&str]) -> Result<(), OrchestrationError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| OrchestrationError::Git {
            reason: format!("failed to spawn git {}: {e}", args.join(" ")),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrchestrationError::Git {
            reason: format!("git {} failed: {stderr}", args.join(" ")),
        });
    }
    Ok(())
}

/// Build the initial manifest from the setup and directory layout.
fn build_initial_manifest(setup: &RunSetup, dirs: &RunDirectories) -> RunManifest {
    let mut manifest = RunManifest::new(
        setup.run_id.clone(),
        setup.jefe_version.clone(),
        setup.scenario_name.clone(),
        setup.cols,
        setup.rows,
        setup.runtime_profile,
    );
    manifest.git_commit = current_git_commit();
    manifest.scenario_hash.clone_from(&setup.scenario_hash);
    manifest.theme.clone_from(&setup.theme);
    manifest.binary_hash = setup.jefe_bin.as_deref().and_then(compute_binary_hash);
    manifest.tool_versions = Some(collect_tool_versions());
    manifest.shim_availability = setup.shim_availability;
    manifest.add_owned_path(OwnedPathKind::ConfigDir, dirs.config_dir.clone());
    manifest.add_owned_path(OwnedPathKind::ArtifactDir, dirs.artifact_dir.clone());
    manifest.add_owned_path(OwnedPathKind::ShimDir, dirs.shim_dir.clone());
    manifest.add_owned_path(OwnedPathKind::FixtureRepo, dirs.fixture_repo.clone());
    manifest.set_fixture_repo(dirs.fixture_repo.clone());
    if let Some(repo) = &setup.fixture_github_repo {
        manifest.set_fixture_github_repo(repo);
    }
    manifest
}

/// Compute a SHA-256 hash of the jefe binary for reproducibility metadata.
// Provenance functions extracted to `provenance.rs` to keep this module
// under the per-file line limit (Finding #7).
pub use super::provenance::{
    collect_tool_versions, compute_binary_hash, compute_scenario_hash, current_git_commit,
};

/// Returns [`OrchestrationError`] if writing or serialization fails.
pub fn save_manifest(manifest: &RunManifest, path: &Path) -> Result<(), OrchestrationError> {
    let run_root = path.parent().unwrap_or_else(|| Path::new("."));
    save_manifest_atomic(manifest, run_root).map_err(OrchestrationError::from)
}

/// Load a manifest from a JSON file with full validation.
///
/// @requirement REQ-TUTORIAL-CAPTURE-002
///
/// # Errors
///
/// Returns [`OrchestrationError`] if reading or deserialization fails.
pub fn load_manifest(path: &Path) -> Result<RunManifest, OrchestrationError> {
    let run_root = path.parent().unwrap_or_else(|| Path::new("."));
    persistence::load_and_validate(run_root).map_err(OrchestrationError::from)
}

/// Validate a fixture repository against the allowlist before any mutation.
///
/// Returns `Ok(())` if allowed, or `Err(FixtureRefused)` if refused.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`OrchestrationError::FixtureRefused`] if the repository is not
/// allowlisted or is the production repository.
pub fn check_fixture_repo(
    allowlist: &FixtureAllowlist,
    repo: &str,
) -> Result<(), OrchestrationError> {
    match allowlist.evaluate(repo) {
        AllowlistDecision::Allowed => Ok(()),
        decision => Err(OrchestrationError::FixtureRefused {
            repo: repo.to_string(),
            reason: decision.reason(),
        }),
    }
}

/// Perform manifest-scoped cleanup: remove only paths listed in the manifest
/// that pass containment validation. Evidence is preserved by default.
///
/// @requirement REQ-TUTORIAL-CAPTURE-002
///
/// # Errors
///
/// Returns [`OrchestrationError`] if a path cannot be removed or containment
/// validation fails.
pub fn cleanup_manifest(
    manifest: &mut RunManifest,
    purge_evidence: bool,
) -> Result<Vec<CleanupRecord>, OrchestrationError> {
    let run_root = manifest
        .owned_paths
        .iter()
        .map(|p| p.path.clone())
        .find_map(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    cleanup_manifest_with_root(manifest, &run_root, purge_evidence)
}

/// Perform manifest-scoped cleanup with an explicit run root and purge option.
///
/// @requirement REQ-TUTORIAL-CAPTURE-002
///
/// # Errors
///
/// Returns [`OrchestrationError`] if containment validation fails or a path
/// cannot be removed.
pub fn cleanup_manifest_with_root(
    manifest: &mut RunManifest,
    run_root: &Path,
    purge_evidence: bool,
) -> Result<Vec<CleanupRecord>, OrchestrationError> {
    let records = persistence::cleanup_with_containment(manifest, run_root, purge_evidence)
        .map_err(OrchestrationError::from)?;
    Ok(records)
}

/// Write the Markdown evidence report to the artifact directory, redacted
/// of credentials and personal data.
///
/// **Finding #5**: The report is redacted after generation and before
/// publication. Fixture repo/URLs/paths are redacted from the report text.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
///
/// # Errors
///
/// Returns [`OrchestrationError`] if the report cannot be written.
pub fn save_report(manifest: &RunManifest, path: &Path) -> Result<(), OrchestrationError> {
    let report = super::report::render_report(manifest);
    // Finding #5: redact the report before writing.
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/user"));
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok();
    let mut repos: Vec<&str> = manifest
        .fixture_github_repo
        .as_deref()
        .into_iter()
        .collect();
    for resource in &manifest.github_resources {
        // Redact all resource URLs and repo names.
        repos.push(&resource.repository);
    }
    let mut set =
        super::redaction::build_redaction_set_with_repos(&home, username.as_deref(), &repos);
    let hostname = hostname_str();
    super::redaction::add_privacy_rules(&mut set, hostname.as_deref());
    let redacted = set.apply(&report);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    }
    fs::write(path, redacted).map_err(|e| io_error(path, e))?;
    Ok(())
}

/// Build an `io_error` helper.
fn io_error(path: &Path, e: std::io::Error) -> OrchestrationError {
    OrchestrationError::Io {
        path: path.to_path_buf(),
        reason: e.to_string(),
    }
}

/// Scrub all text artifacts in the artifact directory with full token
/// redaction. Recursively scans sub-directories. Fails typed on I/O errors
/// rather than silently skipping files.
///
/// **Finding #6**: Redaction fails closed — any I/O error during redaction
/// is returned as an error, not silently swallowed. This prevents publishing
/// unredacted artifacts containing credentials or private repo names.
///
/// **Finding #6**: Redacts `.txt`, `.md`, and `.svg` files (the latter being
/// generated SVG reports that may contain captured screen text).
///
/// **Finding**: Also applies hostname/timestamp privacy redaction rules as
/// defense-in-depth against leaking the host machine identity.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
///
/// # Errors
///
/// Returns [`OrchestrationError`] if a file cannot be read or written.
pub fn redact_artifacts(artifact_dir: &Path) -> Result<usize, OrchestrationError> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/user"));
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok();
    let mut set = build_redaction_set(&home, username.as_deref());
    let hostname = hostname_str();
    super::redaction::add_privacy_rules(&mut set, hostname.as_deref());
    redact_directory_recursive(artifact_dir, &set)
}

/// Scrub all text artifacts in the artifact directory with full token
/// redaction, including fixture/private repo names. Recursively scans
/// sub-directories. Fails typed on I/O errors.
///
/// **Finding #6**: Includes fixture/private repo names in the redaction set
/// so they are not leaked in published artifacts.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
///
/// # Errors
///
/// Returns [`OrchestrationError`] if a file cannot be read or written.
pub fn redact_artifacts_with_repos(
    artifact_dir: &Path,
    repos: &[&str],
) -> Result<usize, OrchestrationError> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/user"));
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok();
    let mut set =
        super::redaction::build_redaction_set_with_repos(&home, username.as_deref(), repos);
    let hostname = hostname_str();
    super::redaction::add_privacy_rules(&mut set, hostname.as_deref());
    redact_directory_recursive(artifact_dir, &set)
}

/// Recursively redact all text files in a directory tree.
///
/// **Finding #5**: Uses `symlink_metadata` instead of `metadata` to reject
/// symlinks recursively. A symlinked file or directory could point outside
/// the artifact directory, so symlinks are explicitly rejected with an error.
fn redact_directory_recursive(dir: &Path, set: &RedactionSet) -> Result<usize, OrchestrationError> {
    let entries = fs::read_dir(dir).map_err(|e| io_error(dir, e))?;
    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|e| io_error(dir, e))?;
        let path = entry.path();
        // Finding #5: use symlink_metadata to detect symlinks without following them.
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
        if metadata.file_type().is_symlink() {
            // Reject symlinks: they could point outside the artifact directory.
            return Err(OrchestrationError::Redaction(
                RedactionError::EnumerateFailed {
                    path: path.to_string_lossy().into_owned(),
                    reason: "symlink detected in artifact directory — refusing to follow"
                        .to_string(),
                },
            ));
        }
        if metadata.is_dir() {
            count += redact_directory_recursive(&path, set)?;
        } else if metadata.is_file() && is_text_file(&path) {
            count += redact_single_file(&path, set)?;
        }
    }
    Ok(count)
}

/// Whether a file is a text file suitable for redaction.
///
/// **Finding #6**: Includes `.svg` files because generated SVG reports may
/// contain captured screen text with credentials or private repo names.
/// Includes `.ansi` files because raw ANSI captures contain the same
/// terminal text as `.txt` captures.
fn is_text_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        ext == "txt" || ext == "md" || ext == "svg" || ext == "json" || ext == "ansi"
    })
}

/// Redact a single file in-place. Returns 1 if the file was modified, 0 if
/// unchanged.
fn redact_single_file(path: &Path, set: &RedactionSet) -> Result<usize, OrchestrationError> {
    let content = fs::read_to_string(path).map_err(|e| {
        OrchestrationError::Redaction(RedactionError::ReadFailed {
            path: path.to_string_lossy().into_owned(),
            reason: e.to_string(),
        })
    })?;
    let redacted = set.apply(&content);
    if redacted == content {
        return Ok(0);
    }
    fs::write(path, redacted).map_err(|e| {
        OrchestrationError::Redaction(RedactionError::WriteFailed {
            path: path.to_string_lossy().into_owned(),
            reason: e.to_string(),
        })
    })?;
    Ok(1)
}

#[cfg(test)]
#[path = "orchestration_tests.rs"]
mod tests;
