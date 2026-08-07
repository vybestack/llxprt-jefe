//! Static startup provider composition (issue #390 CW-10, rows CW10-01/03/13).
//!
//! Composition is the pure step between the CW-09 package inventory and the
//! CW-10 runtime: it decides, without starting anything, which trusted packages
//! contribute action metadata, which of those actions are runnable, and which
//! packages need a persistent candidate started before publication.
//!
//! Nothing here spawns a process, touches `AppState`, or holds a handle. A
//! one-shot package therefore contributes exactly action metadata and a catalog
//! entry, and startup process capture stays empty (CW10-01). A persistent
//! package additionally contributes a data-only [`PersistentCandidate`], which
//! the caller hands to the supervisor; the supervisor owns every handle.
//!
//! A package whose provider has no binary for this exact host still publishes
//! its action metadata, but as unavailable carrying the *same* reason string
//! the package inventory already shows, so a refused keybind, the Actions
//! palette, and the Settings section never disagree (CW10-13).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::domain::TypedMap;
use crate::domain::action_registry::{
    Action as RegistryAction, ActionAvailability, ActionError, ActionId, ActionMetadata,
    Availability, HandlerKey,
};
use crate::domain::input_context::ContextId;
use crate::domain::plugin::action::Action as DeclaredAction;
use crate::domain::plugin::provider::{ProviderMode, ProviderSelection};
use crate::domain::plugin::surface::ConfigSchema;
use crate::domain::plugin::{HostTriple, Manifest};
use crate::persistence::plugin_inventory::InstalledPackage;
use crate::runtime::provider::coordinator::{ProviderActionDescriptor, ProviderCatalog};
use crate::runtime::provider::dto::{Capability, ConfigurePayload};
use crate::runtime::provider::environment::ProviderEnvironment;
use crate::runtime::provider::identifiers::{RequestId, RequestIdError};
use crate::runtime::provider::persistent::PersistentCandidate;
use crate::state::provider_requests::ActionPolicy;

/// The fixed positive generation every startup-composed provider process runs
/// under. One process has exactly one generation, and startup composition
/// happens once, so a single value is correct rather than a counter.
const STARTUP_GENERATION: u64 = 1;

/// The contained process locations and identity every composed provider shares.
///
/// These are host decisions, not package decisions: the package never chooses
/// its own `HOME`, `TMPDIR`, working directory, or locale.
#[derive(Debug, Clone)]
pub struct Containment {
    /// Contained `HOME` for every provider process.
    pub home: PathBuf,
    /// Contained `TMPDIR` for every provider process.
    pub tmpdir: PathBuf,
    /// Contained working directory for every provider process.
    pub working_dir: PathBuf,
    /// Locale (`LC_ALL`/`LANG`).
    pub locale: String,
    /// Host API identifier sent in `hello`.
    pub host_api: String,
}

/// Everything one composition attempt reads.
pub struct CompositionRequest<'a> {
    /// The immutable package inventory scanned at the startup boundary.
    pub packages: &'a [InstalledPackage],
    /// Whether the operator currently trusts a package id.
    pub trusted: &'a dyn Fn(&str) -> bool,
    /// The exact build host triple provider binaries are selected against.
    pub host: HostTriple,
    /// The contained process locations and identity.
    pub containment: Containment,
}

/// The pure result of one composition attempt.
///
/// The action list and availability list are complete and consistent with each
/// other: registry composition requires an availability entry for every action,
/// so they are produced together rather than by two independent passes.
#[derive(Debug, Clone, Default)]
pub struct ProviderComposition {
    actions: Vec<RegistryAction>,
    availability: Vec<ActionAvailability>,
    catalog: ProviderCatalog,
    persistent_candidates: Vec<PersistentCandidate>,
    persistent_action_ids: Vec<ActionId>,
}

impl ProviderComposition {
    /// The provider actions to add to the single immutable registry snapshot.
    #[must_use]
    pub fn actions(&self) -> &[RegistryAction] {
        &self.actions
    }

    /// One availability entry per composed action.
    pub fn availability(&self) -> &[ActionAvailability] {
        &self.availability
    }

    /// The runtime catalog of actions that can actually be invoked.
    #[must_use]
    pub const fn catalog(&self) -> &ProviderCatalog {
        &self.catalog
    }

    /// The persistent candidates to start, in plugin-id order.
    #[must_use]
    pub fn persistent_candidates(&self) -> &[PersistentCandidate] {
        &self.persistent_candidates
    }

    /// Take ownership of the catalog for the runtime coordinator.
    #[must_use]
    pub fn into_catalog(self) -> ProviderCatalog {
        self.catalog
    }

    /// Report that persistent startup did not publish.
    ///
    /// Publication is all-or-nothing (CW10-03/CW10-04), so a startup failure
    /// makes every persistent-package action unavailable with one shared reason
    /// and removes them from the runnable catalog. One-shot actions are
    /// untouched: they never needed the failed candidates.
    pub fn mark_persistent_unavailable(&mut self, reason: &str) {
        for action_id in &self.persistent_action_ids {
            self.catalog.remove(action_id);
            if let Some(entry) = self
                .availability
                .iter_mut()
                .find(|entry| entry.action() == action_id)
            {
                *entry = ActionAvailability::new(
                    action_id.clone(),
                    Availability::Unavailable {
                        reason: reason.to_owned(),
                    },
                );
            }
        }
        self.persistent_candidates.clear();
    }
}

/// Compose the static provider contribution from the package inventory.
///
/// Starts nothing. A package is skipped entirely when it is untrusted or
/// declares no provider; an action is skipped when its declaration cannot be
/// expressed as a registry action, because a half-published action is worse
/// than an absent one.
#[must_use]
pub fn compose(request: &CompositionRequest<'_>) -> ProviderComposition {
    let mut composition = ProviderComposition::default();
    for package in request.packages {
        if !(request.trusted)(package.coordinate().id().as_str()) {
            continue;
        }
        compose_package(&mut composition, package, request);
    }
    composition
        .persistent_candidates
        .sort_by(|left, right| left.plugin_id.as_str().cmp(right.plugin_id.as_str()));
    composition
}

/// Compose one trusted package's contribution.
fn compose_package(
    composition: &mut ProviderComposition,
    package: &InstalledPackage,
    request: &CompositionRequest<'_>,
) {
    let manifest = package.manifest();
    let mode = manifest.provider().mode();
    let selection = manifest.provider().select(&request.host);
    let binary = match selection {
        ProviderSelection::NotDeclared => return,
        ProviderSelection::UnsupportedPlatform => {
            let reason = manifest
                .provider()
                .unsupported_message(&request.host)
                .unwrap_or_else(|| format!("no binary for {}", request.host.as_str()));
            publish_unavailable_actions(composition, manifest, &reason);
            return;
        }
        ProviderSelection::Ready(relative) => resolve_binary(package.directory(), relative),
    };

    publish_available_actions(composition, package, request, &binary, mode);

    if mode == ProviderMode::Persistent {
        match persistent_candidate(package, request, &binary) {
            Ok(candidate) => composition.persistent_candidates.push(candidate),
            Err(_error) => {
                // A request id this composition cannot build is a host defect,
                // not a package defect; the candidate simply does not start and
                // its actions are marked unavailable with the shared reason.
                let reason = format!(
                    "provider {} could not be prepared for startup",
                    package.coordinate().id()
                );
                let ids = manifest_action_ids(manifest);
                mark_actions_unavailable(composition, &ids, &reason);
            }
        }
    }
}

/// Resolve the package-relative provider binary under its package directory.
fn resolve_binary(directory: &Path, relative: &crate::domain::plugin::RelativePath) -> PathBuf {
    let mut path = directory.to_path_buf();
    for component in relative.components() {
        path.push(component);
    }
    path
}

/// Publish every declared action of a package that can run.
///
/// The registry action and its runtime descriptor are built together, action by
/// action, so the two lists cannot drift apart. An action whose declaration
/// cannot be expressed as a registry action is skipped on its own; it must not
/// take the rest of the package with it, because one malformed id is not a
/// reason to hide nine working actions.
fn publish_available_actions(
    composition: &mut ProviderComposition,
    package: &InstalledPackage,
    request: &CompositionRequest<'_>,
    binary: &Path,
    mode: ProviderMode,
) {
    let manifest = package.manifest();
    let plugin_id = manifest.id().owner_id().clone();
    let environment = provider_environment(binary);
    let configure = configure_payload(manifest);
    for declared in manifest.actions() {
        let Some(action) = registry_action(declared) else {
            continue;
        };
        let action_id = action.id.clone();
        composition.availability.push(ActionAvailability::new(
            action_id.clone(),
            Availability::Available,
        ));
        composition.actions.push(action);
        if mode == ProviderMode::Persistent {
            composition.persistent_action_ids.push(action_id.clone());
        }
        composition.catalog.insert(
            action_id.clone(),
            ProviderActionDescriptor {
                plugin_id: plugin_id.clone(),
                plugin_version: manifest.version().clone(),
                action_id,
                mode,
                binary: binary.to_path_buf(),
                provider_args: Vec::new(),
                working_dir: request.containment.working_dir.clone(),
                home: request.containment.home.clone(),
                tmpdir: request.containment.tmpdir.clone(),
                locale: request.containment.locale.clone(),
                host_api: request.containment.host_api.clone(),
                environment: environment.clone(),
                configure: configure.clone(),
                policy: action_policy(declared),
                timeout_seconds: declared.timeout_seconds(),
            },
        );
    }
}

/// Build the bounded provider environment for one selected binary.
///
/// Only the provider directory is derived here. Declared non-secret values and
/// secret references come from persisted package configuration, which CW-09
/// does not yet publish, so they are empty rather than guessed: the supervisor
/// resolves exactly what it is given and nothing else (CW10-14).
fn provider_environment(binary: &Path) -> ProviderEnvironment {
    ProviderEnvironment {
        provider_dir: binary
            .parent()
            .map_or_else(|| binary.to_path_buf(), Path::to_path_buf),
        nonsecret: BTreeMap::new(),
        secret_env: BTreeMap::new(),
        configure_secret_sources: BTreeMap::new(),
    }
}

/// Build the base `configure` payload for one package.
///
/// The supervisor is the sole secret resolver, so `secrets` is always empty
/// here; it refuses a caller-supplied secret outright.
fn configure_payload(manifest: &Manifest) -> ConfigurePayload {
    ConfigurePayload {
        config_version: u64::from(manifest.config().map_or(1, ConfigSchema::schema_version)),
        config: TypedMap::new(),
        secrets: BTreeMap::new(),
        environment: BTreeMap::new(),
    }
}

/// Derive the immutable invocation policy from a declared action.
fn action_policy(declared: &DeclaredAction) -> ActionPolicy {
    ActionPolicy::new(
        declared.confirmation(),
        declared.allowed_outcomes().to_vec(),
        declared.destructive(),
    )
}

/// Build the persistent startup candidate for one package.
fn persistent_candidate(
    package: &InstalledPackage,
    request: &CompositionRequest<'_>,
    binary: &Path,
) -> Result<PersistentCandidate, RequestIdError> {
    let manifest = package.manifest();
    Ok(PersistentCandidate {
        plugin_id: manifest.id().owner_id().clone(),
        plugin_version: manifest.version().clone(),
        binary: binary.to_path_buf(),
        arguments: Vec::new(),
        working_dir: request.containment.working_dir.clone(),
        environment: provider_environment(binary),
        home: request.containment.home.clone(),
        tmpdir: request.containment.tmpdir.clone(),
        locale: request.containment.locale.clone(),
        host_api: request.containment.host_api.clone(),
        generation: STARTUP_GENERATION,
        request_id: RequestId::new_host(STARTUP_GENERATION)?,
        configure: configure_payload(manifest),
        declared_capabilities: declared_capabilities(manifest),
    })
}

/// The capabilities a manifest permits its provider to report at `ready`.
///
/// Derived from what the package actually declares, so a provider that claims
/// a surface its manifest never contributed fails startup rather than being
/// quietly trusted. `config-migration` is never derived: CW-10 does not own
/// config migration, so a provider claiming it is refused.
fn declared_capabilities(manifest: &Manifest) -> Vec<Capability> {
    let mut capabilities = Vec::new();
    if !manifest.actions().is_empty() {
        capabilities.push(Capability::Actions);
    }
    if !manifest.panels().is_empty() {
        capabilities.push(Capability::Panels);
    }
    capabilities
}

/// Publish every declared action of a package that cannot run.
fn publish_unavailable_actions(
    composition: &mut ProviderComposition,
    manifest: &Manifest,
    reason: &str,
) {
    for declared in manifest.actions() {
        let Some(action) = registry_action(declared) else {
            continue;
        };
        composition.availability.push(ActionAvailability::new(
            action.id.clone(),
            Availability::Unavailable {
                reason: reason.to_owned(),
            },
        ));
        composition.actions.push(action);
    }
}

/// Every registry action id a manifest declares.
fn manifest_action_ids(manifest: &Manifest) -> Vec<ActionId> {
    manifest
        .actions()
        .iter()
        .filter_map(|declared| ActionId::parse(declared.id().as_str()).ok())
        .collect()
}

/// Mark already-published actions unavailable and drop them from the catalog.
fn mark_actions_unavailable(
    composition: &mut ProviderComposition,
    action_ids: &[ActionId],
    reason: &str,
) {
    for action_id in action_ids {
        composition.catalog.remove(action_id);
        if let Some(entry) = composition
            .availability
            .iter_mut()
            .find(|entry| entry.action() == action_id)
        {
            *entry = ActionAvailability::new(
                action_id.clone(),
                Availability::Unavailable {
                    reason: reason.to_owned(),
                },
            );
        }
    }
}

/// Express one declared action as a registry action.
///
/// Returns `None` when the declaration cannot satisfy the registry's own
/// bounds; publishing a partially valid action would break the single
/// authority the whole registry exists to be.
fn registry_action(declared: &DeclaredAction) -> Option<RegistryAction> {
    let id = ActionId::parse(declared.id().as_str()).ok()?;
    let contexts: Vec<ContextId> = declared
        .contexts()
        .iter()
        .filter_map(|context| ContextId::parse(context.as_str()).ok())
        .collect();
    let metadata = ActionMetadata {
        id,
        label: declared.label().to_owned(),
        description: declared.description().to_owned(),
        category: declared.category().as_str().to_owned(),
        contexts,
    };
    match RegistryAction::new(metadata, HandlerKey::ProviderAction, false) {
        Ok(action) => Some(action),
        Err(error) => {
            let _: ActionError = error;
            None
        }
    }
}
