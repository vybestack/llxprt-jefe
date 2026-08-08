//! Startup publication of trusted package providers (issue #390 CW-10).
//!
//! This is the one place a provider process can come into existence. It runs
//! from the TUI startup path only, never from `build_persistence`, so the
//! offline `jefe config` and recovery commands keep starting zero providers
//! even when a selected package declares a hanging one (CW10-12).
//!
//! Publication is all-or-nothing per lifecycle. One-shot packages publish
//! action metadata and a runnable catalog entry while starting nothing
//! (CW10-01). Persistent packages start in plugin-id order and publish only
//! after every required candidate reached `ready`; any failure reaps every
//! started candidate and republishes those actions as unavailable with one
//! shared reason (CW10-03/CW10-04).
//!
//! Everything that owns a handle stays inside the returned
//! [`ProviderCoordinator`]. Nothing here reaches `AppState`.

use crate::domain::action_registry::ActionRegistrySnapshot;
use crate::domain::plugin::HostTriple;
use crate::persistence::plugin_inventory::{InstalledPackage, package_trusted};
use crate::persistence::settings_document::PublishedSettings;
use crate::runtime::provider::environment::ProcessHostEnv;
use crate::runtime::provider::persistent::{PersistentStartup, PersistentStartupResult};
use crate::runtime::provider::supervisor::SupervisorBounds;
use crate::runtime::provider::{
    CompositionRequest, Containment, ProviderComposition, ProviderCoordinator, compose,
};

/// Everything one publication attempt reads.
pub struct ProviderPublicationRequest<'a> {
    /// The packages scanned once at the startup boundary.
    pub packages: &'a [InstalledPackage],
    /// Published settings, which decide which packages are trusted.
    pub settings: &'a PublishedSettings,
    /// The compiled-only snapshot providers are added to.
    pub base_snapshot: &'a ActionRegistrySnapshot,
    /// Contained process locations and identity.
    pub containment: Containment,
}

/// The result of publishing providers at startup.
pub struct ProviderPublication {
    /// The single immutable snapshot, now including every provider action.
    pub snapshot: ActionRegistrySnapshot,
    /// The runtime owner of every persistent handle and the action catalog.
    pub coordinator: ProviderCoordinator,
    /// Why some provider is unavailable, when the operator needs to be told.
    pub startup_warning: Option<String>,
}

/// Compose and publish every trusted package's provider contribution.
///
/// Never fails startup: a provider that cannot run makes its own actions
/// unavailable and says why, which is strictly more usable than refusing to
/// start the host over a package the operator can simply disable.
#[must_use]
pub fn publish_providers(request: &ProviderPublicationRequest<'_>) -> ProviderPublication {
    ensure_containment(&request.containment);
    let trusted = |id: &str| package_trusted(request.settings, id);
    let mut composition = compose(&CompositionRequest {
        packages: request.packages,
        trusted: &trusted,
        host: HostTriple::current(),
        containment: request.containment.clone(),
    });

    let (coordinator, startup_warning) = start_persistent(&mut composition);
    let snapshot = publish_snapshot(request, &composition);
    ProviderPublication {
        snapshot,
        coordinator,
        startup_warning,
    }
}

/// Create the contained directories a provider will be spawned into.
///
/// `Command::spawn` sets the child's working directory and fails outright if it
/// does not exist, so composing a descriptor that names a directory nobody
/// created would make every invocation fail at spawn with an I/O error that
/// says nothing about the real cause. They are created once, here, because this
/// is the only place that decides a provider may run at all.
///
/// A creation failure is logged rather than fatal: the spawn will fail and
/// report itself as a provider that could not start, which is the honest
/// outcome and is already a state the operator can see.
fn ensure_containment(containment: &Containment) {
    for directory in [
        &containment.working_dir,
        &containment.home,
        &containment.tmpdir,
    ] {
        if let Err(error) = std::fs::create_dir_all(directory) {
            tracing::warn!(
                directory = %directory.display(),
                %error,
                "provider containment directory could not be created"
            );
        }
    }
}

/// Start every persistent candidate, or none.
///
/// Returns the coordinator that owns whatever is running plus the operator
/// warning when publication did not happen. A failure marks the composition's
/// persistent actions unavailable *before* the snapshot is built, so the
/// snapshot never advertises an action whose process is not there.
fn start_persistent(
    composition: &mut ProviderComposition,
) -> (ProviderCoordinator, Option<String>) {
    if composition.persistent_candidates().is_empty() {
        return (
            ProviderCoordinator::from_catalog(composition.clone().into_catalog()),
            None,
        );
    }
    let startup = PersistentStartup {
        candidates: composition.persistent_candidates().to_vec(),
    };
    let result = crate::runtime::provider::persistent::run_persistent_startup(
        &startup,
        &SupervisorBounds::PRODUCTION,
        &ProcessHostEnv,
    );
    match result {
        PersistentStartupResult::Started {
            supervisor,
            publication,
        } => (
            ProviderCoordinator::from_startup(
                PersistentStartupResult::Started {
                    supervisor,
                    publication,
                },
                composition.clone().into_catalog(),
            ),
            None,
        ),
        PersistentStartupResult::Failed(failure) => {
            let reason = format!(
                "provider unavailable: persistent startup failed ({})",
                failure.failure.code()
            );
            composition.mark_persistent_unavailable(&reason);
            (
                ProviderCoordinator::from_catalog(composition.clone().into_catalog()),
                Some(reason),
            )
        }
    }
}

/// Add the composed provider actions to the compiled snapshot.
///
/// A composition the registry refuses is dropped whole: the compiled snapshot
/// is kept exactly as it was rather than published half-formed, because the
/// registry is the single authority every dispatch and every reason string
/// reads from.
fn publish_snapshot(
    request: &ProviderPublicationRequest<'_>,
    composition: &ProviderComposition,
) -> ActionRegistrySnapshot {
    if composition.actions().is_empty() {
        return request.base_snapshot.clone();
    }
    match crate::persistence::keymap_edit::compose_published_with_providers(
        request.settings,
        "compiled defaults",
        composition.actions().to_vec(),
        composition.availability().to_vec(),
    ) {
        Ok(composed) => composed.snapshot().clone(),
        Err(error) => {
            tracing::warn!(
                error = %error,
                "provider actions could not be composed; the compiled registry is unchanged"
            );
            request.base_snapshot.clone()
        }
    }
}

#[cfg(test)]
#[path = "startup_providers_tests.rs"]
mod tests;
