//! Atomic workbench publication boundary (issue #704).
//!
//! Static composition, required-provider startup, and the deferred state import
//! all complete before this module returns a [`StartupCommit`]. A failed
//! attempt returns no aggregate or coordinator; provider rollback has completed
//! and durable user bytes remain authoritative before the error is observable.

use std::sync::Arc;

use crate::domain::plugin::HostTriple;
use crate::persistence::paths::{StateImportError, StateImportPlan, commit_state_import};
use crate::published_workbench::PublishedWorkbench;
use crate::runtime::provider::environment::{HostEnv, ProcessHostEnv};
use crate::runtime::provider::persistent::PersistentStartupResult;
use crate::runtime::provider::supervisor::SupervisorBounds;
use crate::runtime::provider::{Containment, ProviderCoordinator};
use crate::startup::StartupPersistence;
use crate::startup_candidate::{
    WorkbenchCandidateRequest, WorkbenchStaticFailure, build_workbench_candidate,
};
use crate::startup_transaction::{
    ProviderTransactionFailure, ProviderTransactionResult, run_provider_transaction,
};

/// The sole successful startup publication value.
pub struct StartupCommit {
    /// The immutable aggregate every declaration consumer must share.
    pub workbench: Arc<PublishedWorkbench>,
    /// Exclusive owner of provider process, session, request, and health state.
    pub providers: ProviderCoordinator,
}

/// A startup attempt that published nothing.
#[derive(Debug)]
pub enum StartupCommitFailure {
    /// Static composition refused one declaration set before any spawn or write.
    Static(WorkbenchStaticFailure),
    /// A required provider failed; rollback completed before this value returned.
    Provider(ProviderTransactionFailure),
    /// The final atomic state import failed after providers became ready; dropping
    /// their supervisor reaped them before this value returned.
    StateImport(StateImportError),
}

impl StartupCommitFailure {
    /// Stable process exit code for startup refusal.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Static(_) | Self::Provider(_) => 2,
            Self::StateImport(error) => error.exit_code(),
        }
    }
}

impl std::fmt::Display for StartupCommitFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static(error) => write!(
                formatter,
                "workbench static validation failed: {error}; durable data preserved"
            ),
            Self::Provider(error) => write!(
                formatter,
                "{error}; rollback complete; durable data preserved"
            ),
            Self::StateImport(error) => {
                let detail = error.diagnostic().map_or_else(
                    || "state import failed".to_owned(),
                    |diagnostic| diagnostic.redacted_detail.clone(),
                );
                write!(
                    formatter,
                    "state import commit failed: {detail}; provider rollback complete; durable data preserved"
                )
            }
        }
    }
}

impl std::error::Error for StartupCommitFailure {}

/// Build and atomically publish normal startup from its validated persistence
/// staging value.
///
/// This function consumes the deferred import only after static composition and
/// every required provider reached Configure and Ready. Returning success is the
/// publication event; callers must move the resulting commit into the
/// composition root before constructing application state, runtime services,
/// PTYs, or the TUI.
pub fn commit_startup(
    startup: &mut StartupPersistence,
) -> Result<StartupCommit, StartupCommitFailure> {
    let candidate = build_workbench_candidate(&WorkbenchCandidateRequest {
        paths: &startup.paths,
        inventory: &startup.inventory,
        settings: &startup.settings,
        host: HostTriple::current(),
        containment: provider_containment(&startup.paths),
    })
    .map_err(StartupCommitFailure::Static)?;
    let state_import = startup.state_import.take();
    commit_candidate(
        candidate,
        state_import,
        &SupervisorBounds::PRODUCTION,
        &ProcessHostEnv,
    )
}

/// Commit an already composed candidate. Exposed so acceptance tests can drive
/// the exact publication boundary with controlled provider and import inputs.
pub fn commit_candidate<E: HostEnv>(
    candidate: PublishedWorkbench,
    state_import: Option<StateImportPlan>,
    bounds: &SupervisorBounds,
    host_env: &E,
) -> Result<StartupCommit, StartupCommitFailure> {
    let ProviderTransactionResult {
        supervisor,
        publication,
    } = run_provider_transaction(&candidate, bounds, host_env)
        .map_err(StartupCommitFailure::Provider)?;

    if let Some(plan) = state_import {
        commit_state_import(plan).map_err(StartupCommitFailure::StateImport)?;
    }

    let catalog = candidate.providers().clone().into_catalog();
    let providers = ProviderCoordinator::from_startup(
        PersistentStartupResult::Started {
            supervisor,
            publication,
        },
        catalog,
    );
    Ok(StartupCommit {
        workbench: Arc::new(candidate),
        providers,
    })
}

/// Where every selected provider process is contained.
#[must_use]
pub fn provider_containment(paths: &crate::persistence::paths::ResolvedPaths) -> Containment {
    let root = paths.state.path.parent().map_or_else(
        || std::path::PathBuf::from("providers"),
        |parent| parent.join("providers"),
    );
    Containment {
        home: root.join("home"),
        tmpdir: root.join("tmp"),
        working_dir: root.join("work"),
        locale: "C".to_owned(),
        host_api: crate::VERSION.to_owned(),
    }
}
