//! Generic package-backed invocation and preparation boundary.
//!
//! Candidate metadata is the sole authority. Runner kind determines the closed
//! structural prefix without product-specific branches.

use std::collections::BTreeMap;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
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
use super::package_install_lock::{self, LockPolicy};

const INSTALL_TIMEOUT: Duration = Duration::from_millis(
    crate::domain::agent_definition::limits::PACKAGE_MATERIALIZATION_TIMEOUT_MS,
);
const INSTALL_MARKER: &str = ".jefe-installed";

/// Directory-name prefixes for the two transient siblings of an install
/// directory. Both begin with `.`, which no selector digest (lowercase hex) can
/// produce, so neither can collide with a published cache entry.
const STAGING_PREFIX: &str = ".staging-";
const RETIRED_PREFIX: &str = ".retired-";

/// How long a volatile-selector install (a moving dist-tag such as `nightly`)
/// is trusted before jefe re-resolves it against the registry (issue #554).
///
/// Nightlies publish roughly daily; trusting an install for ~half that cadence
/// bounds staleness to about twelve hours without hitting the registry on every
/// launch. Explicit (pinned) selectors are immutable and never expire.
const VOLATILE_SELECTOR_TTL: Duration = Duration::from_secs(12 * 60 * 60);

/// Per-digest intra-process install guards (issue #382, narrowed by #556).
///
/// The runtime, the non-interactive rewrite path, and the capture worker share
/// this process, so the single-preparer-per-digest invariant is guarded
/// explicitly rather than assumed. The guard is keyed by digest because
/// preparation now waits on another process's install: a process-global guard
/// would make one contended digest block every unrelated one.
static INSTALL_LOCKS: Mutex<BTreeMap<String, Arc<Mutex<()>>>> = Mutex::new(BTreeMap::new());

/// The intra-process guard for one selector digest, created on first use.
///
/// Unreferenced guards are dropped first so a long-lived process that sees many
/// distinct selectors does not accumulate them. A guard is only unreferenced
/// once no preparer holds it: a preparer keeps its `Arc` alive for the whole
/// critical section, so two preparers of one digest can never be handed
/// different mutexes.
fn install_guard_for(digest: &str) -> Arc<Mutex<()>> {
    let mut guards = match INSTALL_LOCKS.lock() {
        Ok(guards) => guards,
        Err(poisoned) => poisoned.into_inner(),
    };
    guards.retain(|_, guard| Arc::strong_count(guard) > 1);
    Arc::clone(guards.entry(digest.to_owned()).or_default())
}

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
    /// Another jefe process holds the managed-install lock for this digest and
    /// still held it at the wait ceiling (issue #556).
    InstallLockUnavailable(String),
    /// A complete staged install could not be published into the cache.
    InstallPromotionFailed(String),
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
            Self::InstallLockUnavailable(detail) => write!(
                formatter,
                "could not acquire the managed package install lock: {detail}"
            ),
            Self::InstallPromotionFailed(detail) => write!(
                formatter,
                "could not publish the staged managed package install: {detail}"
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
    bin_dir_of(&managed_install_dir(cache_root, selection))
}

/// Executable directory inside a managed install tree, published or staged.
fn bin_dir_of(install_dir: &Path) -> PathBuf {
    install_dir.join("node_modules").join(".bin")
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
    prepare_managed_npm_with_lock_policy(
        candidate,
        selection,
        cache_root,
        now,
        LockPolicy::production(),
    )
}

/// Lock-policy-injected core of [`prepare_managed_npm`].
///
/// `policy` is [`LockPolicy::production`] on every production path; the
/// parameter exists so tests can exercise waiting and stale-lock recovery
/// without sleeping for the production ceiling.
fn prepare_managed_npm_with_lock_policy(
    candidate: &ResolvedCandidate,
    selection: &PackageSelection,
    cache_root: &Path,
    now: SystemTime,
    policy: LockPolicy,
) -> Result<PackageInvocation, PackageRuntimeError> {
    let digest = selection_digest(selection).to_hex();
    // Ordering is always the intra-process digest guard, then the cross-process
    // lock. `digest_guard` outlives `_guard`, and `_guard` outlives `_lock`,
    // so both are released in the reverse order.
    let digest_guard = install_guard_for(&digest);
    let _guard = match digest_guard.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    std::fs::create_dir_all(cache_root)
        .map_err(|error| PackageRuntimeError::InstallDirectory(error.to_string()))?;
    let install_dir = managed_install_dir(cache_root, selection);
    let bin_dir = managed_bin_dir(cache_root, selection);
    let retired = cache_root.join(format!("{RETIRED_PREFIX}{digest}"));
    // Held from before the cache-hit check through the fingerprint capture, so
    // no other process can rewrite the tree under a reader that has already
    // decided the cache is a hit. Spanning `npm install` is the point of the
    // lock, not an oversight: this boundary is synchronous by contract (it is
    // called from launch composition and from the probe adapter, never from an
    // async executor), and both guards are keyed by digest, so a slow install
    // blocks only other preparers of the same selector.
    let _lock =
        package_install_lock::acquire(&install_lock_path(cache_root, &digest), &digest, policy)?;
    reconcile_interrupted_promotion(&install_dir, &retired, selection, &digest)?;

    if !cache_hit(&install_dir, &bin_dir, selection, now) {
        let staging = cache_root.join(format!("{STAGING_PREFIX}{digest}"));
        build_staged_install(candidate, selection, &staging, now)?;
        promote_staged_install(&staging, &install_dir, &retired, &digest)?;
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
    let fingerprint = capture_candidate_fingerprint(&executable)
        .map_err(PackageRuntimeError::InstallDirectory)?;
    Ok(PackageInvocation {
        executable,
        wrapper_kind,
        prefix: Vec::new(),
        fingerprint: Some(fingerprint),
    })
}

/// Cross-process lock file for one selector digest.
///
/// A sibling of the install directory rather than a file inside it: the install
/// directory is replaced wholesale by `rename`, which would carry away a lock
/// held inside it.
fn install_lock_path(cache_root: &Path, digest: &str) -> PathBuf {
    cache_root.join(format!("{digest}.lock"))
}

/// Build a complete, marked install in a staging directory.
///
/// Staging always starts empty, so nothing from a previous entry — including a
/// `package-lock.json` that would pin an already-resolved dist-tag (issue #554)
/// — can leak into a re-resolve. The marker is written last but still *inside*
/// staging, so the directory published by [`promote_staged_install`] is
/// complete the instant it appears: there is no window in which `node_modules`
/// exists without its marker.
fn build_staged_install(
    candidate: &ResolvedCandidate,
    selection: &PackageSelection,
    staging: &Path,
    now: SystemTime,
) -> Result<(), PackageRuntimeError> {
    remove_tree(staging).map_err(|error| {
        PackageRuntimeError::InstallDirectory(format!("failed to reset staging: {error}"))
    })?;
    write_package_json(staging, selection)?;
    run_npm_install(candidate, staging)?;
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::current(),
        vec![bin_dir_of(staging)],
        std::env::var_os("PATHEXT"),
    );
    if snapshot.resolve_binary(selection.binary()).is_none() {
        // Fail before publication: a tree without its selected binary is never
        // promoted into the cache.
        return Err(PackageRuntimeError::InstalledBinaryMissing {
            binary: selection.binary().to_owned(),
        });
    }
    std::fs::write(
        staging.join(INSTALL_MARKER),
        marker_contents(selection, now),
    )
    .map_err(|error| PackageRuntimeError::InstallDirectory(error.to_string()))
}

/// Reconcile a cache entry left retired by a promotion that never completed.
///
/// Publication retires the previous entry before publishing the new one, so a
/// process that dies between those two renames leaves the install path absent
/// while a complete previous tree sits at the retired path. Without this, the
/// next preparation would have to reach the registry to become usable again;
/// with it, the previous entry is simply published again.
///
/// The retired tree is only discarded once the published entry is known to be
/// structurally complete. Discarding it against an unusable published entry
/// would throw away the one tree that could still run offline.
fn reconcile_interrupted_promotion(
    install_dir: &Path,
    retired: &Path,
    selection: &PackageSelection,
    digest: &str,
) -> Result<(), PackageRuntimeError> {
    if !exists(retired)
        .map_err(|error| promotion_failure(digest, "inspect retired install", &error))?
    {
        return Ok(());
    }
    if install_is_complete(install_dir, selection) {
        // Publication completed; the retired tree is only leftover cleanup.
        return remove_tree(retired)
            .map_err(|error| promotion_failure(digest, "discard retired install", &error));
    }
    remove_tree(install_dir)
        .map_err(|error| promotion_failure(digest, "discard unusable install", &error))?;
    std::fs::rename(retired, install_dir)
        .map_err(|error| promotion_failure(digest, "restore retired install", &error))
}

/// Whether a published directory holds both its marker and its selected binary.
///
/// Structural only: freshness is [`cache_hit`]'s concern, because an entry that
/// is merely past its TTL is still a better fallback than no entry at all.
fn install_is_complete(install_dir: &Path, selection: &PackageSelection) -> bool {
    if !install_dir.join(INSTALL_MARKER).exists() {
        return false;
    }
    PathSnapshot::for_platform(
        AgentExecutablePlatform::current(),
        vec![bin_dir_of(install_dir)],
        std::env::var_os("PATHEXT"),
    )
    .resolve_binary(selection.binary())
    .is_some()
}

/// Publish a completed staging directory at the install path by rename.
///
/// Neither rename ever targets an existing directory, which keeps the sequence
/// valid on Unix and Windows alike. A reader that goes through preparation
/// therefore observes either the complete previous tree or the complete new
/// one, because it holds the same lock; the published path is briefly absent
/// between the two renames, which is why [`restore_interrupted_promotion`]
/// reconciles that state under the lock before the cache is consulted.
///
/// If publication fails after the previous entry was retired, that entry is
/// restored, and a restore that itself fails is reported rather than hidden.
fn promote_staged_install(
    staging: &Path,
    install_dir: &Path,
    retired: &Path,
    digest: &str,
) -> Result<(), PackageRuntimeError> {
    let retired_existing = exists(install_dir)
        .map_err(|error| promotion_failure(digest, "inspect published install", &error))?;
    if retired_existing {
        std::fs::rename(install_dir, retired)
            .map_err(|error| promotion_failure(digest, "retire published install", &error))?;
    }
    if let Err(error) = std::fs::rename(staging, install_dir) {
        if retired_existing && std::fs::rename(retired, install_dir).is_err() {
            return Err(promotion_failure(
                digest,
                "publish staged install and restore the previous install",
                &error,
            ));
        }
        return Err(promotion_failure(digest, "publish staged install", &error));
    }
    if let Err(error) = remove_tree(retired) {
        // The published entry is complete and usable; the leftover tree is
        // reclaimed by the next preparation.
        tracing::warn!(kind = ?error.kind(), "could not discard the retired managed install");
    }
    Ok(())
}

/// Bounded, redacted promotion diagnostic. Absolute cache paths are omitted:
/// they embed the user's home directory and therefore the account name.
fn promotion_failure(digest: &str, stage: &str, error: &std::io::Error) -> PackageRuntimeError {
    PackageRuntimeError::InstallPromotionFailed(package_install_lock::bounded_detail(
        digest,
        &format!("{stage} failed ({:?})", error.kind()),
    ))
}

/// Whether a path exists, distinguishing absence from an unreadable path.
fn exists(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Remove a path whether it is a directory or a file.
///
/// Only a genuinely absent path counts as already removed: an unreadable path
/// must not be mistaken for a clean slate, or a staging directory that was
/// never actually reset would be reused.
fn remove_tree(path: &Path) -> std::io::Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
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
    include!("package_runtime_lock_tests.rs");
}
