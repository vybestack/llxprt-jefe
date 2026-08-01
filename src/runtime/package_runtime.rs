//! Generic package-backed invocation and preparation boundary.
//!
//! Candidate metadata is the sole authority. Runner kind determines the closed
//! structural prefix without product-specific branches.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::agent_candidate::{
    PackageRunnerKind, PackageSelection, ResolvedCandidate, capture_candidate_fingerprint,
};
use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::agent_candidate_path::{AgentExecutablePlatform, AgentWrapperKind, PathSnapshot};
use crate::domain::agent_definition::DefinitionSha256;

use super::agent_probe::command_for_path;
use super::command_capture::run_command_capture_with_timeout;

const INSTALL_TIMEOUT: Duration = Duration::from_millis(
    crate::domain::agent_definition::limits::PACKAGE_MATERIALIZATION_TIMEOUT_MS,
);
const INSTALL_MARKER: &str = ".jefe-installed";
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
    let Some(selection) = candidate.package() else {
        return Ok(PackageInvocation {
            executable: candidate.executable().to_path_buf(),
            wrapper_kind: candidate.wrapper_kind(),
            prefix: Vec::new(),
            fingerprint: Some(candidate.fingerprint().clone()),
        });
    };
    if selection.runner() == PackageRunnerKind::Npm {
        prepare_managed_npm(candidate, selection, cache_root)
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

fn marker_contents(selection: &PackageSelection) -> String {
    format!(
        "{}\n{}\n{}\n",
        selection.package(),
        selection.binary(),
        selection
            .selector()
            .effective(selection.runner())
            .unwrap_or_default()
    )
}

fn prepare_managed_npm(
    candidate: &ResolvedCandidate,
    selection: &PackageSelection,
    cache_root: &Path,
) -> Result<PackageInvocation, PackageRuntimeError> {
    let _guard = match INSTALL_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let install_dir = managed_install_dir(cache_root, selection);
    let bin_dir = managed_bin_dir(cache_root, selection);
    let cache_hit = cache_hit(&install_dir, &bin_dir, selection);
    if !cache_hit {
        write_package_json(&install_dir, selection)?;
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
        std::fs::write(install_dir.join(INSTALL_MARKER), marker_contents(selection))
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

fn cache_hit(install_dir: &Path, bin_dir: &Path, selection: &PackageSelection) -> bool {
    std::fs::read_to_string(install_dir.join(INSTALL_MARKER))
        .is_ok_and(|value| value == marker_contents(selection))
        && PathSnapshot::for_platform(
            AgentExecutablePlatform::current(),
            vec![bin_dir.to_path_buf()],
            std::env::var_os("PATHEXT"),
        )
        .resolve_binary(selection.binary())
        .is_some()
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
