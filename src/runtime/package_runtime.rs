//! Generic package-backed invocation and preparation boundary.
//!
//! Candidate metadata is the sole authority. Runner kind determines the closed
//! structural prefix without product-specific branches.

use std::collections::BTreeMap;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
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

/// Budget for the metadata-only query that resolves a moving dist-tag to a
/// concrete version (issue #584).
///
/// This reads registry metadata and downloads no package content, so it is far
/// cheaper than an install and is given a correspondingly small budget. When it
/// is exceeded the cached install is used, so a slow registry delays a launch
/// by at most this much rather than failing it.
const VERSION_RESOLVE_TIMEOUT: Duration = Duration::from_secs(20);

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

/// Collision-resistant install location for one concrete installed version.
///
/// `resolved` is the version a moving dist-tag currently points at. Keying on
/// it rather than on the tag is what makes a tag advance create a *new*
/// directory instead of rewriting the one live agents are executing from
/// (issue #588). A pinned selector is already a concrete version, so it passes
/// `None` and keys on itself.
#[must_use]
pub fn managed_install_dir(
    cache_root: &Path,
    selection: &PackageSelection,
    resolved: Option<&str>,
) -> PathBuf {
    cache_root.join(selection_digest(selection, resolved).to_hex())
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
    // This predicts a path without installing, so it cannot resolve a moving tag
    // itself. It uses the version the tag last resolved to when one is
    // recorded, which is the directory a prepared install would occupy.
    let remembered = remembered_version(cache_root, selection);
    let invocation = match (target, selection.runner()) {
        (PackageExecutionTarget::Local, PackageRunnerKind::Npm) => PackageInvocation {
            executable: managed_bin_dir(cache_root, selection, remembered.as_deref())
                .join(selection.binary()),
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
    finalize_local_invocation_inner(candidate, cache_root)
}

/// Core of [`finalize_local_invocation`].
///
/// Freshness of a volatile selector is decided by the version its dist-tag
/// currently resolves to, not by a clock, so no time is injected (issue #584).
pub fn finalize_local_invocation_inner(
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

/// Digest identifying one immutable installed tree.
///
/// The third part is the concrete version when one is known, and the declared
/// selector otherwise. For a pinned selector those are the same string, so its
/// cache entry is unchanged. For a moving dist-tag they differ, which is the
/// whole point: two nightlies get two directories, and installing the newer one
/// cannot touch the tree an already-running agent is executing (issue #588).
fn selection_digest(selection: &PackageSelection, resolved: Option<&str>) -> DefinitionSha256 {
    let declared = selection
        .selector()
        .effective(selection.runner())
        .unwrap_or_default();
    let mut bytes = Vec::new();
    append_digest_part(&mut bytes, selection.package().as_bytes());
    append_digest_part(&mut bytes, selection.binary().as_bytes());
    append_digest_part(&mut bytes, resolved.unwrap_or(declared).as_bytes());
    DefinitionSha256::digest(&bytes)
}

/// File recording the version a moving dist-tag last resolved to.
///
/// Keyed on the tag, so it survives the version changing. It exists for the
/// offline path: without a registry answer the version-keyed directory cannot
/// be derived, and a user with no network should still launch the build they
/// already have (issue #588 change 4, issue #584 offline behavior).
fn tag_pointer_path(cache_root: &Path, selection: &PackageSelection) -> PathBuf {
    cache_root.join(format!(
        "{}.version",
        selection_digest(selection, None).to_hex()
    ))
}

/// Version this tag last resolved to, if it has ever been installed here.
fn remembered_version(cache_root: &Path, selection: &PackageSelection) -> Option<String> {
    let stored = std::fs::read_to_string(tag_pointer_path(cache_root, selection)).ok()?;
    let version = stored.trim().to_owned();
    is_plausible_version(&version).then_some(version)
}

/// Record the version this tag now resolves to, so the offline path can find it.
fn remember_version(cache_root: &Path, selection: &PackageSelection, version: &str) {
    // Advisory only: losing this costs an offline launch, never a live agent.
    let _ = std::fs::write(
        tag_pointer_path(cache_root, selection),
        format!("{version}\n"),
    );
}

fn append_digest_part(bytes: &mut Vec<u8>, part: &[u8]) {
    bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
    bytes.extend_from_slice(part);
}

fn managed_bin_dir(
    cache_root: &Path,
    selection: &PackageSelection,
    resolved: Option<&str>,
) -> PathBuf {
    bin_dir_of(&managed_install_dir(cache_root, selection, resolved))
}

/// Executable directory inside a managed install tree, published or staged.
fn bin_dir_of(install_dir: &Path) -> PathBuf {
    install_dir.join("node_modules").join(".bin")
}

/// Contents of the install marker for a completed install.
///
/// Always three lines — package, binary, and the identity version from
/// [`marker_identity_version`] — whatever kind of selector asked for it. The
/// selector no longer appears, because two selectors naming one version are the
/// same install and must recognise each other's marker (issue #610).
///
/// Whether the tag has moved is no longer a question the marker answers: a
/// different version is a different directory, so a moved tag simply looks
/// elsewhere (issue #588). What used to be a fourth line recording the resolved
/// version is now the third.
///
/// `resolved` is `None` only when the registry could not be reached during an
/// install. The third line then falls back to the declared selector, so the
/// entry is not mistaken for a resolved one and the next preparation re-resolves
/// rather than trusting an unverified tree (issue #584).
fn marker_contents(selection: &PackageSelection, resolved: Option<&str>) -> String {
    format!(
        "{}\n{}\n{}\n",
        selection.package(),
        selection.binary(),
        marker_identity_version(selection, resolved)
    )
}

/// The third marker line: what this directory *holds*, not what was typed.
///
/// It must be the same value [`selection_digest`] keyed the directory on, or
/// identity and location disagree. They did: the digest used the resolved
/// version while the marker used the declared selector, so a tag resolving to
/// 1.2.3 and an exact 1.2.3 shared one directory and each rejected the other's
/// marker. The loser reinstalled and republished over a tree the winner could
/// be executing from, which is precisely what keying on the version was meant
/// to stop (issues #588, #571).
fn marker_identity_version<'selection>(
    selection: &'selection PackageSelection,
    resolved: Option<&'selection str>,
) -> &'selection str {
    resolved.unwrap_or_else(|| {
        selection
            .selector()
            .effective(selection.runner())
            .unwrap_or_default()
    })
}

/// Whether the cached install satisfies this selection.
///
/// Identity must always match. For a volatile selector the cached install must
/// additionally be the version the tag currently points at: `resolved` carries
/// that version when the registry answered, and is `None` when it did not. A
/// `None` therefore keeps the cached install rather than failing the launch —
/// offline is a condition to ride out, not an error to raise (issue #584).
fn cache_hit(
    install_dir: &Path,
    bin_dir: &Path,
    selection: &PackageSelection,
    resolved: Option<&str>,
) -> bool {
    let Ok(stored) = std::fs::read_to_string(install_dir.join(INSTALL_MARKER)) else {
        return false;
    };
    if !marker_identity_matches(&stored, selection, resolved) {
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

/// Resolve a moving dist-tag to the concrete version it currently points at.
///
/// Returns `None` when the registry cannot be reached or answers unusably. That
/// is deliberate and is the offline path: the caller keeps the cached install
/// instead of failing, because a user without a network should still be able to
/// launch the build they already have (issue #584).
///
/// Only the resolve is performed here; no package content is downloaded.
fn resolve_volatile_version(
    candidate: &ResolvedCandidate,
    selection: &PackageSelection,
) -> Option<String> {
    let spec = selection
        .selector()
        .package_spec(selection.runner(), selection.package())?;
    let arguments = [
        OsString::from("view"),
        OsString::from(spec),
        OsString::from("version"),
    ];
    let mut command =
        command_for_path(candidate.executable(), candidate.wrapper_kind(), &arguments);
    command.stdin(Stdio::null());
    let output =
        run_command_capture_with_timeout(command, VERSION_RESOLVE_TIMEOUT, "jefe package resolve")
            .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    is_plausible_version(&version).then_some(version)
}

/// Whether a registry answer is shaped like a version this code may act on.
///
/// The answer is the one input here that jefe does not control, so it is
/// validated rather than trusted. It is written verbatim into a newline
/// delimited marker and compared against a later answer, so anything carrying
/// whitespace, a control character, or unbounded length is rejected outright
/// instead of being allowed to corrupt the marker.
///
/// `is_ascii_graphic` admits exactly printable non-space ASCII, which excludes
/// every control character, NUL, and the newline that delimits the marker.
///
/// This deliberately does not enforce semver: npm dist-tags legitimately point
/// at prerelease and build-metadata forms, and rejecting an unfamiliar but
/// harmless shape would break a launch for no safety gain.
fn is_plausible_version(version: &str) -> bool {
    /// Generous next to any real version, small enough to bound a marker line.
    const MAX_VERSION_LEN: usize = 256;

    !version.is_empty()
        && version.len() <= MAX_VERSION_LEN
        && version
            .chars()
            .all(|character| character.is_ascii_graphic())
}

/// Whether the stored marker's package/binary/effective lines match `selection`.
fn marker_identity_matches(
    stored: &str,
    selection: &PackageSelection,
    resolved: Option<&str>,
) -> bool {
    let identity = marker_identity_version(selection, resolved);
    let mut lines = stored.split('\n');
    lines.next() == Some(selection.package())
        && lines.next() == Some(selection.binary())
        && lines.next() == Some(identity)
}
fn prepare_managed_npm(
    candidate: &ResolvedCandidate,
    selection: &PackageSelection,
    cache_root: &Path,
) -> Result<PackageInvocation, PackageRuntimeError> {
    prepare_managed_npm_with_lock_policy(candidate, selection, cache_root, LockPolicy::production())
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
    policy: LockPolicy,
) -> Result<PackageInvocation, PackageRuntimeError> {
    std::fs::create_dir_all(cache_root)
        .map_err(|error| PackageRuntimeError::InstallDirectory(error.to_string()))?;

    // Resolve before anything is keyed, because for a moving dist-tag the
    // resolved version *is* the cache key. A pinned selector is already a
    // concrete version and never reaches the registry.
    //
    // Resolving outside the lock is deliberate: it is a read-only metadata
    // query, and the lock is keyed on the result.
    let resolved = if selection.selector().is_volatile() {
        resolve_volatile_version(candidate, selection).or_else(|| {
            // Offline: fall back to the version this tag last resolved to, so
            // the build the user already has still launches (issue #584).
            let remembered = remembered_version(cache_root, selection);
            tracing::warn!(
                package = selection.package(),
                recovered = remembered.is_some(),
                "could not resolve dist-tag against the registry; using the last known version if present"
            );
            remembered
        })
    } else {
        None
    };
    let resolved_ref = resolved.as_deref();

    let digest = selection_digest(selection, resolved_ref).to_hex();
    // Ordering is always the intra-process digest guard, then the cross-process
    // lock. `digest_guard` outlives `_guard`, and `_guard` outlives `_lock`,
    // so both are released in the reverse order.
    let digest_guard = install_guard_for(&digest);
    let _guard = match digest_guard.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let install_dir = managed_install_dir(cache_root, selection, resolved_ref);
    let bin_dir = managed_bin_dir(cache_root, selection, resolved_ref);
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

    // The directory is now the version, so a hit needs only identity and a
    // usable binary; there is no separate freshness question left to ask.
    if !cache_hit(&install_dir, &bin_dir, selection, resolved_ref) {
        let staging = cache_root.join(format!("{STAGING_PREFIX}{digest}"));
        build_staged_install(candidate, selection, &staging, resolved_ref)?;
        promote_staged_install(&staging, &install_dir, &retired, &digest)?;
    }
    if let Some(version) = resolved_ref {
        remember_version(cache_root, selection, version);
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
        .map_err(|error| PackageRuntimeError::InstallDirectory(error.to_string()))?;
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
    resolved: Option<&str>,
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
        marker_contents(selection, resolved),
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
