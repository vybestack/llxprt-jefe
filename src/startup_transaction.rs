//! Required-provider startup transaction (issue #704, slice S2:
//! CWR1-00, CWR1-03, CWR1-04, CWR1-05).
//!
//! This is the process-owning phase of normal startup: given the process-free
//! [`PublishedWorkbench`] candidate composed by S1, it prepares every required
//! provider — the exact owners of active declarations, [`SelectedOwner`] with
//! [`ProviderRequirement::Required`] — and only then starts them in
//! deterministic plugin-id order, handshakes each to `ready` under the
//! existing strict JSONL protocol and bounds, and returns either a typed
//! failure after complete candidate cleanup or the supervisor and publication
//! for S3 to commit.
//!
//! Preparation is fail-fast and total before any spawn: every required
//! candidate's selected binary is checked for presence, file type, and (on
//! Unix) an executable permission bit, then its environment and `Configure`
//! secrets are resolved exactly once against the host environment. Only after
//! those read-only checks succeed for every candidate are all containment
//! directories created. A defect in the first, middle, or last
//! candidate therefore prevents every spawn — no earlier provider can be left
//! running — and is reported as a typed [`ProviderTransactionFailure`]
//! carrying the exact owning plugin id and cause. There is no publication, no
//! fallback, and no degraded startup on any failure path; the only durable
//! effect of a failed transaction is the containment directories themselves.
//!
//! One-shot providers, declaration-empty persistent providers, and disabled
//! owners start zero processes here: they are not
//! [`ProviderRequirement::Required`] and no candidate is built for them. A
//! required provider that cannot be prepared or cannot reach `ready` is
//! fatal.
//!
//! The supervisor is retained in the result rather than consumed into a
//! coordinator; S3 owns the composition root that decides what happens next.

use crate::domain::Id;
use crate::published_workbench::PublishedWorkbench;
use crate::runtime::provider::environment::{EnvironmentError, HostEnv};
use crate::runtime::provider::persistent::{
    PersistentCandidate, PersistentPublication, PersistentStartupFailure, PersistentStartupResult,
    PersistentSupervisor, PreparedEnvironment, prepare_candidate_environment, run_prepared_startup,
};
use crate::runtime::provider::supervisor::SupervisorBounds;
use crate::startup_selection::ProviderRequirement;

/// The result of a successful required-provider startup transaction.
///
/// The supervisor is retained for S3 to consume; it is not moved into a
/// `ProviderCoordinator` or session owner here.
#[derive(Debug)]
pub struct ProviderTransactionResult {
    /// The supervisor owning every ready required-provider process.
    pub supervisor: PersistentSupervisor,
    /// The data-only publication snapshot of every ready candidate.
    pub publication: PersistentPublication,
}

/// Why the required-provider startup transaction failed.
///
/// Every variant is a complete stop. [`Self::Preparation`] occurs before any
/// spawn, so nothing was started and there is nothing to reap.
/// [`Self::Startup`] means every started candidate was reaped before the
/// error is returned, so no provider process survives.
#[derive(Debug)]
pub enum ProviderTransactionFailure {
    /// A required candidate failed preparation before any provider spawned.
    /// The exact owning plugin id and cause are carried.
    Preparation {
        /// The plugin id of the candidate whose preparation failed.
        owner: Id,
        /// The exact preparation defect.
        cause: PreparationCause,
    },
    /// At least one required provider failed to reach `ready`; every started
    /// candidate was reaped. The rollback evidence is carried unchanged.
    Startup(PersistentStartupFailure),
}

/// One exact preparation defect, always reported with its owning candidate.
#[derive(Debug)]
pub enum PreparationCause {
    /// The selected provider binary does not exist.
    BinaryMissing {
        /// The binary path that was absent.
        path: std::path::PathBuf,
        /// The I/O error observed while resolving it.
        error: std::io::Error,
    },
    /// The provider binary path could not be inspected for a reason other than
    /// absence (for example, a symlink loop or permission failure).
    BinaryMetadata {
        /// The binary path whose metadata was unreadable.
        path: std::path::PathBuf,
        /// The exact metadata I/O failure.
        error: std::io::Error,
    },
    /// The selected provider binary is not a regular file.
    BinaryNotAFile {
        /// The binary path that is not a file.
        path: std::path::PathBuf,
    },
    /// The binary exists but carries no executable permission bit.
    #[cfg(unix)]
    BinaryNotExecutable {
        /// The binary path without an executable bit.
        path: std::path::PathBuf,
    },
    /// A containment directory could not be created before any spawn.
    ContainmentDirectory {
        /// The directory that could not be created.
        directory: std::path::PathBuf,
        /// The I/O error.
        error: std::io::Error,
    },
    /// The contained environment or a `Configure` secret could not be
    /// resolved. No secret value is ever carried.
    Environment(EnvironmentError),
}

impl std::fmt::Display for PreparationCause {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BinaryMissing { path, error } => write!(
                formatter,
                "provider binary {} is missing: {error}",
                path.display()
            ),
            Self::BinaryMetadata { path, error } => write!(
                formatter,
                "provider binary {} could not be inspected: {error}",
                path.display()
            ),
            Self::BinaryNotAFile { path } => write!(
                formatter,
                "provider binary {} is not a regular file",
                path.display()
            ),
            #[cfg(unix)]
            Self::BinaryNotExecutable { path } => write!(
                formatter,
                "provider binary {} carries no executable permission",
                path.display()
            ),
            Self::ContainmentDirectory { directory, error } => write!(
                formatter,
                "containment directory {} could not be created: {error}",
                directory.display()
            ),
            Self::Environment(error) => {
                write!(formatter, "environment could not be prepared: {error}")
            }
        }
    }
}

impl std::error::Error for PreparationCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BinaryMissing { error, .. }
            | Self::BinaryMetadata { error, .. }
            | Self::ContainmentDirectory { error, .. } => Some(error),
            Self::Environment(error) => Some(error),
            Self::BinaryNotAFile { .. } => None,
            #[cfg(unix)]
            Self::BinaryNotExecutable { .. } => None,
        }
    }
}

impl std::fmt::Display for ProviderTransactionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparation { owner, cause } => write!(
                formatter,
                "required provider {owner} failed startup preparation: {cause}"
            ),
            Self::Startup(failure) => write!(
                formatter,
                "required provider startup failed ({}): {failure:?}",
                failure.failure.code()
            ),
        }
    }
}

impl std::error::Error for ProviderTransactionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Preparation { cause, .. } => cause.source(),
            Self::Startup(_) => None,
        }
    }
}

/// Run the required-provider startup transaction.
///
/// Filters the workbench candidate's persistent providers to exactly those
/// that own active declarations ([`ProviderRequirement::Required`]) and sorts
/// them by plugin id. In that order, and before any spawn, every candidate's
/// binary is verified (presence, file type, and on Unix an executable
/// permission bit) and its environment and `Configure` secrets are resolved
/// exactly once. All containment directories are then created. Only after
/// every candidate is prepared does the first provider spawn, under the
/// existing strict JSONL protocol and bounds. One-shot and declaration-empty
/// providers start zero processes.
///
/// On failure, every started candidate is reaped before this function returns
/// [`Err`]; the rollback evidence is carried in
/// [`ProviderTransactionFailure::Startup`]. A preparation failure occurs
/// before any spawn, so there is nothing to reap and no provider ever runs.
///
/// # Errors
///
/// Returns [`ProviderTransactionFailure::Preparation`] when a required
/// candidate's binary, containment, or environment fails preparation; the
/// variant names the exact owner and cause. Returns
/// [`ProviderTransactionFailure::Startup`] when any prepared provider fails
/// to reach `ready`; all started candidates are reaped before the error is
/// returned.
pub fn run_provider_transaction<E: HostEnv>(
    workbench: &PublishedWorkbench,
    bounds: &SupervisorBounds,
    host_env: &E,
) -> Result<ProviderTransactionResult, ProviderTransactionFailure> {
    let candidates = required_candidates(workbench);
    let prepared = prepare_candidates(&candidates, host_env)?;
    ensure_containment(&candidates)?;
    // `prepare_candidates` resolves exactly one environment per candidate, so
    // the zip pairs every prepared value with its own candidate and startup
    // consumes the resolved values without re-reading the host environment.
    let pairs = candidates.into_iter().zip(prepared).collect();
    match run_prepared_startup(pairs, bounds) {
        PersistentStartupResult::Started {
            supervisor,
            publication,
        } => Ok(ProviderTransactionResult {
            supervisor,
            publication,
        }),
        PersistentStartupResult::Failed(failure) => {
            Err(ProviderTransactionFailure::Startup(failure))
        }
    }
}

/// Collect the persistent candidates for exactly the required providers, in
/// deterministic plugin-id order.
///
/// The composition builds candidates for every persistent provider; this
/// filters to only those whose owner is [`ProviderRequirement::Required`] and
/// sorts by plugin id, so preflight and startup observe the same order
/// regardless of selection input order. One-shot and declaration-empty
/// providers produce no candidate here.
fn required_candidates(workbench: &PublishedWorkbench) -> Vec<PersistentCandidate> {
    let required_ids = required_owner_ids(workbench);
    let mut candidates: Vec<PersistentCandidate> = workbench
        .providers()
        .persistent_candidates()
        .iter()
        .filter(|candidate| required_ids.contains(&candidate.plugin_id))
        .cloned()
        .collect();
    candidates.sort_by(|left, right| left.plugin_id.as_str().cmp(right.plugin_id.as_str()));
    candidates
}

/// The owner IDs of every required provider, in selected-owner order.
fn required_owner_ids(workbench: &PublishedWorkbench) -> Vec<Id> {
    workbench
        .selected_owners()
        .iter()
        .filter(|owner| matches!(owner.requirement(), ProviderRequirement::Required { .. }))
        .map(|owner| owner.owner().clone())
        .collect()
}

/// Verify every candidate's binary and resolve every candidate's environment,
/// in plugin-id order, before any spawn.
///
/// Both checks are read-only against the filesystem and the host
/// environment, so a defect anywhere in the batch is found before any
/// durable effect exists. The resolved [`PreparedEnvironment`] values are
/// returned for startup to consume, preserving the exactly-once host-input
/// resolution.
fn prepare_candidates<E: HostEnv>(
    candidates: &[PersistentCandidate],
    host_env: &E,
) -> Result<Vec<PreparedEnvironment>, ProviderTransactionFailure> {
    let mut prepared = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        verify_executable(candidate)?;
        let environment = prepare_candidate_environment(candidate, host_env)
            .map_err(|error| preparation(candidate, PreparationCause::Environment(error)))?;
        prepared.push(environment);
    }
    Ok(prepared)
}

/// Verify the selected provider binary's presence, file type, and (on Unix)
/// executable permission.
fn verify_executable(candidate: &PersistentCandidate) -> Result<(), ProviderTransactionFailure> {
    let metadata = std::fs::metadata(&candidate.binary).map_err(|error| {
        let cause = if error.kind() == std::io::ErrorKind::NotFound {
            PreparationCause::BinaryMissing {
                path: candidate.binary.clone(),
                error,
            }
        } else {
            PreparationCause::BinaryMetadata {
                path: candidate.binary.clone(),
                error,
            }
        };
        preparation(candidate, cause)
    })?;
    if !metadata.is_file() {
        return Err(preparation(
            candidate,
            PreparationCause::BinaryNotAFile {
                path: candidate.binary.clone(),
            },
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(preparation(
                candidate,
                PreparationCause::BinaryNotExecutable {
                    path: candidate.binary.clone(),
                },
            ));
        }
    }
    Ok(())
}

/// Create every required candidate's containment directory before any spawn.
///
/// A creation failure is fatal: the spawn would fail with an I/O error that
/// says nothing about the real cause, so the directory is created here rather
/// than at spawn time. Every candidate's binary and environment were already
/// prepared before the first directory is touched, so a preparation defect
/// leaves no containment mutation behind.
fn ensure_containment(
    candidates: &[PersistentCandidate],
) -> Result<(), ProviderTransactionFailure> {
    for candidate in candidates {
        ensure_directory(&candidate.working_dir, candidate)?;
        ensure_directory(&candidate.home, candidate)?;
        ensure_directory(&candidate.tmpdir, candidate)?;
    }
    Ok(())
}

/// Create one directory, mapping an I/O failure to a typed failure carrying
/// the exact owning candidate.
fn ensure_directory(
    path: &std::path::Path,
    candidate: &PersistentCandidate,
) -> Result<(), ProviderTransactionFailure> {
    std::fs::create_dir_all(path).map_err(|error| {
        preparation(
            candidate,
            PreparationCause::ContainmentDirectory {
                directory: path.to_path_buf(),
                error,
            },
        )
    })
}

/// Build the typed failure naming the exact owning candidate and cause.
fn preparation(
    candidate: &PersistentCandidate,
    cause: PreparationCause,
) -> ProviderTransactionFailure {
    ProviderTransactionFailure::Preparation {
        owner: candidate.plugin_id.clone(),
        cause,
    }
}
