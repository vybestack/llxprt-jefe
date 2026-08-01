//! Generic package-backed invocation and preparation boundary.
//!
//! Candidate metadata is the sole authority. Runner kind determines the closed
//! structural prefix without product-specific branches.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::agent_candidate::{
    PackageRunnerKind, PackageSelection, ResolvedCandidate, capture_candidate_fingerprint,
};
use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::agent_candidate_path::{AgentExecutablePlatform, AgentWrapperKind, PathSnapshot};
use crate::domain::agent_definition::DefinitionSha256;

use super::agent_probe::command_for_path;
use super::command_capture::run_command_capture_with_timeout;

const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);
const INSTALL_MARKER: &str = ".jefe-installed";

/// How long a volatile-selector install (a moving dist-tag such as `nightly`)
/// is trusted before jefe re-resolves it against the registry (issue #554).
///
/// Nightlies publish roughly daily; trusting an install for ~half that cadence
/// bounds staleness to about twelve hours without hitting the registry on every
/// launch. Explicit (pinned) selectors are immutable and never expire.
const VOLATILE_SELECTOR_TTL: Duration = Duration::from_secs(12 * 60 * 60);

static INSTALL_LOCK: Mutex<()> = Mutex::new(());

/// Local managed execution or remote structural execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageExecutionTarget {
    /// Local process boundary.
    Local,
    /// Remote POSIX serialization boundary.
    Remote,
}

/// Exact package invocation prefix before definition-emitted agent argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInvocation {
    executable: PathBuf,
    wrapper_kind: AgentWrapperKind,
    prefix: Vec<OsString>,
    fingerprint: Option<CandidateFingerprint>,
}

impl PackageInvocation {
    /// Runner or managed package executable.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    /// Wrapper required by the local runner.
    #[must_use]
    pub const fn wrapper_kind(&self) -> AgentWrapperKind {
        self.wrapper_kind
    }
    /// Closed structural prefix.
    #[must_use]
    pub fn prefix(&self) -> &[OsString] {
        &self.prefix
    }
    /// Physical executable fingerprint for stale-evidence checks.
    #[must_use]
    pub const fn fingerprint(&self) -> Option<&CandidateFingerprint> {
        self.fingerprint.as_ref()
    }
}

/// Package preparation or plan-validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageRuntimeError {
    /// Candidate is package-backed but selector metadata is invalid.
    InvalidSelection,
    /// The immutable plan is for a different definition.
    DefinitionChanged,
    /// The immutable plan's generation is stale.
    ProbeGenerationChanged { plan: u64, current: u64 },
    /// The plan did not originate from this resolved candidate.
    CandidateChanged,
    /// Package target does not match the plan target.
    TargetChanged,
    /// The managed cache could not be prepared.
    InstallDirectory(String),
    /// npm install failed or timed out.
    InstallFailed(String),
    /// The selected installed package binary is absent.
    InstalledBinaryMissing { binary: String },
}

impl std::fmt::Display for PackageRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSelection => formatter.write_str("package selection is invalid"),
            Self::DefinitionChanged => {
                formatter.write_str("agent definition changed; replan required")
            }
            Self::ProbeGenerationChanged { plan, current } => write!(
                formatter,
                "probe generation changed: plan={plan}, current={current}"
            ),
            Self::CandidateChanged => {
                formatter.write_str("resolved candidate changed; reprobe required")
            }
            Self::TargetChanged => {
                formatter.write_str("package execution target changed; replan required")
            }
            Self::InstallDirectory(detail) => write!(
                formatter,
                "could not prepare managed package install: {detail}"
            ),
            Self::InstallFailed(detail) => {
                write!(formatter, "managed npm install failed: {detail}")
            }
            Self::InstalledBinaryMissing { binary } => write!(
                formatter,
                "managed package binary `{binary}` is missing after install"
            ),
        }
    }
}

impl std::error::Error for PackageRuntimeError {}

/// Generalized Jefe package-cache root. Existing compatibility caches remain
/// separately owned until sole-route convergence.
#[must_use]
pub fn managed_package_cache_root() -> PathBuf {
    if let Some(root) = std::env::var_os("JEFE_PACKAGE_CACHE_DIR") {
        let root = PathBuf::from(root);
        if root.is_absolute() {
            return root;
        }
    }
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".jefe-cache"))
        .join("jefe")
        .join("package-versions")
}

/// Collision-resistant install location based on declared package, binary, and
/// effective selector.
#[must_use]
pub fn managed_install_dir(cache_root: &Path, selection: &PackageSelection) -> PathBuf {
    cache_root.join(selection_digest(selection).to_hex())
}

/// Build the exact no-effect package invocation for a resolved candidate.
///
/// Remote invocations retain a closed structural package-runner prefix. Local
/// npm callers use [`finalize_local_invocation`] so installation, executable
/// resolution, and fingerprinting finish before immutable planning.
pub fn package_invocation(
    candidate: &ResolvedCandidate,
    target: PackageExecutionTarget,
    cache_root: &Path,
) -> Result<Option<PackageInvocation>, PackageRuntimeError> {
    let Some(selection) = candidate.package() else {
        return Ok(None);
    };
    let invocation = match (target, selection.runner()) {
        (PackageExecutionTarget::Local, PackageRunnerKind::Npm) => PackageInvocation {
            executable: managed_bin_dir(cache_root, selection).join(selection.binary()),
            wrapper_kind: AgentWrapperKind::Direct,
            prefix: Vec::new(),
            fingerprint: None,
        },
        (PackageExecutionTarget::Local, PackageRunnerKind::Uvx) => PackageInvocation {
            executable: candidate.executable().to_path_buf(),
            wrapper_kind: candidate.wrapper_kind(),
            prefix: uvx_prefix(selection)?,
            fingerprint: Some(candidate.fingerprint().clone()),
        },
        (PackageExecutionTarget::Remote, PackageRunnerKind::Npm) => PackageInvocation {
            executable: PathBuf::from(selection.runner().executable_name()),
            wrapper_kind: AgentWrapperKind::Direct,
            prefix: npm_prefix(selection)?,
            fingerprint: None,
        },
        (PackageExecutionTarget::Remote, PackageRunnerKind::Uvx) => PackageInvocation {
            executable: PathBuf::from(selection.runner().executable_name()),
            wrapper_kind: AgentWrapperKind::Direct,
            prefix: uvx_prefix(selection)?,
            fingerprint: None,
        },
    };
    Ok(Some(invocation))
}

/// Finalize the selected local executable and structural prefix before planning.
///
/// Managed npm installation, platform wrapper resolution, and physical
/// fingerprint capture all complete here. The returned invocation can be
/// copied directly into [`super::agent_plan::PlanRequest`]; runtime execution
/// never mutates or rediscovers it.
pub fn finalize_local_invocation(
    candidate: &ResolvedCandidate,
    cache_root: &Path,
) -> Result<PackageInvocation, PackageRuntimeError> {
    finalize_local_invocation_at(candidate, cache_root, SystemTime::now())
}

/// Time-injected core of [`finalize_local_invocation`] for deterministic tests.
///
/// `now` stamps the install marker and gates the volatile-selector freshness
/// check (issue #554); production callers pass [`SystemTime::now`].
pub fn finalize_local_invocation_at(
    candidate: &ResolvedCandidate,
    cache_root: &Path,
    now: SystemTime,
) -> Result<PackageInvocation, PackageRuntimeError> {
    let Some(selection) = candidate.package() else {
        return Ok(PackageInvocation {
            executable: candidate.executable().to_path_buf(),
            wrapper_kind: candidate.wrapper_kind(),
            prefix: Vec::new(),
            fingerprint: Some(candidate.fingerprint().clone()),
        });
    };
    if selection.runner() == PackageRunnerKind::Npm {
        prepare_managed_npm(candidate, selection, cache_root, now)
    } else {
        package_invocation(candidate, PackageExecutionTarget::Local, cache_root)?
            .ok_or(PackageRuntimeError::InvalidSelection)
    }
}

/// Prepare the selected local invocation used by the generic probe adapter.
pub(crate) fn prepare_local_probe(
    candidate: &ResolvedCandidate,
    cache_root: &Path,
) -> Result<Option<PackageInvocation>, PackageRuntimeError> {
    candidate
        .package()
        .map(|_| finalize_local_invocation(candidate, cache_root))
        .transpose()
}

fn npm_prefix(selection: &PackageSelection) -> Result<Vec<OsString>, PackageRuntimeError> {
    let spec = selection
        .selector()
        .package_spec(selection.runner(), selection.package())
        .ok_or(PackageRuntimeError::InvalidSelection)?;
    Ok(vec![
        OsString::from("exec"),
        OsString::from("--yes"),
        OsString::from(format!("--package={spec}")),
        OsString::from("--"),
        OsString::from(selection.binary()),
    ])
}

fn uvx_prefix(selection: &PackageSelection) -> Result<Vec<OsString>, PackageRuntimeError> {
    let spec = selection
        .selector()
        .package_spec(selection.runner(), selection.package())
        .ok_or(PackageRuntimeError::InvalidSelection)?;
    Ok(vec![
        OsString::from("--from"),
        OsString::from(spec),
        OsString::from(selection.binary()),
    ])
}

fn selection_digest(selection: &PackageSelection) -> DefinitionSha256 {
    let mut bytes = Vec::new();
    append_digest_part(&mut bytes, selection.package().as_bytes());
    append_digest_part(&mut bytes, selection.binary().as_bytes());
    append_digest_part(
        &mut bytes,
        selection
            .selector()
            .effective(selection.runner())
            .unwrap_or_default()
            .as_bytes(),
    );
    DefinitionSha256::digest(&bytes)
}

fn append_digest_part(bytes: &mut Vec<u8>, part: &[u8]) {
    bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
    bytes.extend_from_slice(part);
}

fn managed_bin_dir(cache_root: &Path, selection: &PackageSelection) -> PathBuf {
    managed_install_dir(cache_root, selection)
        .join("node_modules")
        .join(".bin")
}

fn marker_contents(selection: &PackageSelection, now: SystemTime) -> String {
    let effective = selection
        .selector()
        .effective(selection.runner())
        .unwrap_or_default();
    let base = format!(
        "{}\n{}\n{}\n",
        selection.package(),
        selection.binary(),
        effective
    );
    // Issue #554: volatile selectors carry an install-time epoch on a 4th line so
    // the cache can expire and re-resolve the moving dist-tag. Pinned selectors
    // keep the legacy 3-line marker (a permanent hit — explicit versions never
    // change).
    if !selection.selector().is_volatile() {
        return base;
    }
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    format!("{base}{secs}\n")
}

/// Whether the stored marker identifies this selection AND, for volatile
/// selectors, was installed within [`VOLATILE_SELECTOR_TTL`] of `now`.
fn cache_hit(
    install_dir: &Path,
    bin_dir: &Path,
    selection: &PackageSelection,
    now: SystemTime,
) -> bool {
    let Ok(stored) = std::fs::read_to_string(install_dir.join(INSTALL_MARKER)) else {
        return false;
    };
    if !marker_identity_matches(&stored, selection) {
        return false;
    }
    if selection.selector().is_volatile() && !marker_install_is_fresh(&stored, now) {
        return false;
    }
    PathSnapshot::for_platform(
        AgentExecutablePlatform::current(),
        vec![bin_dir.to_path_buf()],
        std::env::var_os("PATHEXT"),
    )
    .resolve_binary(selection.binary())
    .is_some()
}

/// Whether the stored marker's package/binary/effective lines match `selection`.
fn marker_identity_matches(stored: &str, selection: &PackageSelection) -> bool {
    let effective = selection
        .selector()
        .effective(selection.runner())
        .unwrap_or_default();
    let mut lines = stored.split('\n');
    lines.next() == Some(selection.package())
        && lines.next() == Some(selection.binary())
        && lines.next() == Some(effective)
}

/// Whether a volatile selector's marker install-time line is still within TTL.
///
/// A missing or unparseable 4th line (a legacy/stuck 3-line marker) is treated
/// as expired so the install is rebuilt and auto-healed (issue #554).
fn marker_install_is_fresh(stored: &str, now: SystemTime) -> bool {
    let Some(secs_str) = stored.split('\n').nth(3) else {
        return false;
    };
    let Ok(secs) = secs_str.parse::<u64>() else {
        return false;
    };
    let Some(installed) = SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs)) else {
        return false;
    };
    // A future-dated marker (clock skew) is treated as expired: re-resolve
    // rather than trust an untrusted timestamp.
    now.duration_since(installed)
        .is_ok_and(|age| age < VOLATILE_SELECTOR_TTL)
}

fn prepare_managed_npm(
    candidate: &ResolvedCandidate,
    selection: &PackageSelection,
    cache_root: &Path,
    now: SystemTime,
) -> Result<PackageInvocation, PackageRuntimeError> {
    let _guard = match INSTALL_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let install_dir = managed_install_dir(cache_root, selection);
    let bin_dir = managed_bin_dir(cache_root, selection);
    let cache_hit = cache_hit(&install_dir, &bin_dir, selection, now);
    if !cache_hit {
        write_package_json(&install_dir, selection)?;
        // Issue #554: a stale lockfile makes `npm install` reuse the previously
        // resolved dist-tag target. Drop it for volatile selectors so npm
        // re-resolves `nightly`/`latest` against the registry. A real failure to
        // remove an existing lockfile must surface — otherwise npm would silently
        // reinstall the old target and the freshly stamped marker would freeze
        // the stale build for another TTL window.
        if selection.selector().is_volatile() {
            match std::fs::remove_file(install_dir.join("package-lock.json")) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(PackageRuntimeError::InstallDirectory(format!(
                        "failed to remove stale lockfile for re-resolve: {error}"
                    )));
                }
            }
        }
        run_npm_install(candidate, &install_dir)?;
    }
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::current(),
        vec![bin_dir],
        std::env::var_os("PATHEXT"),
    );
    let Some((executable, wrapper_kind)) = snapshot.resolve_binary(selection.binary()) else {
        return Err(PackageRuntimeError::InstalledBinaryMissing {
            binary: selection.binary().to_owned(),
        });
    };
    if !cache_hit {
        std::fs::write(
            install_dir.join(INSTALL_MARKER),
            marker_contents(selection, now),
        )
        .map_err(|error| PackageRuntimeError::InstallDirectory(error.to_string()))?;
    }
    let fingerprint = capture_candidate_fingerprint(&executable)
        .map_err(PackageRuntimeError::InstallDirectory)?;
    Ok(PackageInvocation {
        executable,
        wrapper_kind,
        prefix: Vec::new(),
        fingerprint: Some(fingerprint),
    })
}

#[derive(Serialize)]
struct ManagedPackageManifest<'a> {
    name: &'a str,
    version: &'a str,
    private: bool,
    dependencies: BTreeMap<&'a str, &'a str>,
}

fn write_package_json(
    install_dir: &Path,
    selection: &PackageSelection,
) -> Result<(), PackageRuntimeError> {
    std::fs::create_dir_all(install_dir)
        .map_err(|error| PackageRuntimeError::InstallDirectory(error.to_string()))?;
    let selector = selection
        .selector()
        .effective(selection.runner())
        .ok_or(PackageRuntimeError::InvalidSelection)?;
    let manifest = ManagedPackageManifest {
        name: "jefe-package-cache",
        version: "0.0.0",
        private: true,
        dependencies: BTreeMap::from([(selection.package(), selector)]),
    };
    let mut contents = serde_json::to_string_pretty(&manifest)
        .map_err(|error| PackageRuntimeError::InstallDirectory(error.to_string()))?;
    contents.push('\n');
    std::fs::write(install_dir.join("package.json"), contents)
        .map_err(|error| PackageRuntimeError::InstallDirectory(error.to_string()))
}

fn run_npm_install(
    candidate: &ResolvedCandidate,
    install_dir: &Path,
) -> Result<(), PackageRuntimeError> {
    let arguments = [OsString::from("install")];
    let mut command =
        command_for_path(candidate.executable(), candidate.wrapper_kind(), &arguments);
    command.current_dir(install_dir).stdin(Stdio::null());
    let output = run_command_capture_with_timeout(command, INSTALL_TIMEOUT, "jefe package install")
        .map_err(|error| PackageRuntimeError::InstallFailed(error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        let diagnostic: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(512)
            .collect();
        Err(PackageRuntimeError::InstallFailed(diagnostic))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::domain::agent_definition::{AgentDefinition, AgentLaunchPlan, Target};

    include!("package_runtime_tests.rs");
}
