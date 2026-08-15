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

use crate::domain::action_registry::{
    Action as RegistryAction, ActionAvailability, ActionError, ActionId, ActionMetadata,
    Availability, HandlerKey,
};
use crate::domain::input_context::ContextId;
use crate::domain::plugin::action::Action as DeclaredAction;
use crate::domain::plugin::provider::{ProviderMode, ProviderSelection};
use crate::domain::plugin::surface::ConfigSchema;
use crate::domain::plugin::{FieldKind, HostTriple, Manifest};
use crate::domain::{CanonicalSemver, Id, TypedMap, TypedValue};
use crate::persistence::plugin_inventory::{InstalledPackage, selected_packages};
use crate::persistence::settings_document::PublishedSettings;
use crate::runtime::provider::coordinator::{ProviderActionDescriptor, ProviderCatalog};
use crate::runtime::provider::dto::{Capability, ConfigurePayload};
use crate::runtime::provider::environment::ProviderEnvironment;
use crate::runtime::provider::identifiers::{
    EnvName, INITIAL_PROCESS_GENERATION, RequestId, RequestIdError,
};
use crate::runtime::provider::migration::MigrationRequest;
use crate::runtime::provider::panel_model::MigrateConfigPayload;
use crate::runtime::provider::persistent::PersistentCandidate;
use crate::state::provider_requests::ActionPolicy;

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
    /// The selected, typed package configuration published for this startup.
    pub settings: &'a PublishedSettings,
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
}

/// Compose the static provider contribution from the package inventory.
///
/// Starts nothing. Only the exact Settings-selected installed package version
/// contributes; a package is skipped when it declares no provider. An action
/// is skipped when its declaration cannot be expressed as a registry action,
/// because a half-published action is worse than an absent one.
#[must_use]
pub fn compose(request: &CompositionRequest<'_>) -> ProviderComposition {
    let mut composition = ProviderComposition::default();
    for package in selected_packages(request.packages, request.settings) {
        compose_package(&mut composition, package, request);
    }
    composition
        .persistent_candidates
        .sort_by(|left, right| left.plugin_id.as_str().cmp(right.plugin_id.as_str()));
    composition
}
/// Wire identity and payload for one migration request.
///
/// These three fields flow into [`MigrationRequest`] unchanged; bundling them
/// keeps the composer argument list within the lint limit.
#[derive(Debug, Clone)]
pub struct MigrationInputs {
    /// Fixed positive generation for this invocation (the process generation).
    pub generation: u64,
    /// Host-originated request id for this invocation.
    pub request_id: RequestId,
    /// The `migrate-config` payload.
    pub migrate: MigrateConfigPayload,
}

/// Resolve one exact installed package provider for a provisional migration.
///
/// This performs no Settings selection and resolves no Configure secrets: the
/// caller supplies the already-authoritative owner/version and a reference-only
/// migration payload.
pub fn compose_migration_request(
    packages: &[InstalledPackage],
    owner: &Id,
    version: &CanonicalSemver,
    host: HostTriple,
    containment: &Containment,
    inputs: MigrationInputs,
) -> Result<MigrationRequest, String> {
    let Some(package) = packages.iter().find(|package| {
        package.coordinate().id().owner_id() == owner && package.coordinate().version() == version
    }) else {
        return Err("the exact migration target package is not installed".to_owned());
    };
    let binary = match package.manifest().provider().select(&host) {
        ProviderSelection::Ready(relative) => resolve_binary(package.directory(), relative),
        ProviderSelection::NotDeclared => {
            return Err("the migration target does not declare a provider".to_owned());
        }
        ProviderSelection::UnsupportedPlatform => {
            return Err("the migration target has no provider for this host".to_owned());
        }
    };
    Ok(MigrationRequest {
        environment: ProviderEnvironment {
            provider_dir: binary
                .parent()
                .map_or_else(|| binary.clone(), Path::to_path_buf),
            nonsecret: BTreeMap::new(),
            secret_env: BTreeMap::new(),
            configure_secret_sources: BTreeMap::new(),
        },
        binary,
        arguments: Vec::new(),
        working_dir: containment.working_dir.clone(),
        home: containment.home.clone(),
        tmpdir: containment.tmpdir.clone(),
        locale: containment.locale.clone(),
        host_api: containment.host_api.clone(),
        plugin_id: owner.clone(),
        plugin_version: version.clone(),
        generation: inputs.generation,
        request_id: inputs.request_id,
        migrate: inputs.migrate,
    })
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

    let (environment, configure) = match provider_configuration(request.settings, manifest, &binary)
    {
        Ok(configuration) => configuration,
        Err(reason) => {
            publish_unavailable_actions(composition, manifest, &reason);
            return;
        }
    };
    publish_available_actions(
        composition,
        package,
        request,
        AvailableProvider {
            binary: &binary,
            mode,
            environment: &environment,
            configure: &configure,
        },
    );

    if mode == ProviderMode::Persistent {
        match persistent_candidate(package, request, &binary, environment, configure) {
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

struct AvailableProvider<'a> {
    binary: &'a Path,
    mode: ProviderMode,
    environment: &'a ProviderEnvironment,
    configure: &'a ConfigurePayload,
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
    provider: AvailableProvider<'_>,
) {
    let manifest = package.manifest();
    let plugin_id = manifest.id().owner_id().clone();
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
        composition.catalog.insert(
            action_id.clone(),
            ProviderActionDescriptor {
                plugin_id: plugin_id.clone(),
                plugin_version: manifest.version().clone(),
                action_id,
                mode: provider.mode,
                binary: provider.binary.to_path_buf(),
                provider_args: Vec::new(),
                working_dir: request.containment.working_dir.clone(),
                home: request.containment.home.clone(),
                tmpdir: request.containment.tmpdir.clone(),
                locale: request.containment.locale.clone(),
                host_api: request.containment.host_api.clone(),
                environment: provider.environment.clone(),
                configure: provider.configure.clone(),
                policy: action_policy(declared, manifest),
                timeout_seconds: declared.timeout_seconds(),
            },
        );
    }
}

fn validate_provider_configuration(manifest: &Manifest, config: &TypedMap) -> Result<(), String> {
    let Some(schema) = manifest.config() else {
        return Ok(());
    };
    let errors = crate::domain::plugin_config::validate_config(schema, config);
    if errors.is_empty() {
        return Ok(());
    }
    let details = errors
        .iter()
        .map(|error| format!("{} ({})", error.field, error.reason))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "provider {} configuration is invalid: {details}",
        manifest.id()
    ))
}

/// Resolve one selected package's effective configuration values.
///
/// This is the single owner of "what Settings selected for this package,
/// defaults applied, schema-validated": [`provider_configuration`] and the
/// workbench candidate's strict validation both read here, so a config that
/// would compose can never be reported invalid, and vice versa.
fn resolve_selected_configuration(
    settings: &PublishedSettings,
    manifest: &Manifest,
) -> Result<TypedMap, String> {
    let user_config = settings
        .plugins
        .get(manifest.id().owner_id())
        .map_or_else(TypedMap::new, |owner| owner.values.clone());
    let config = match manifest.config() {
        Some(schema) => crate::domain::plugin_config::effective_values(schema, &user_config),
        None => user_config,
    };
    validate_provider_configuration(manifest, &config)?;
    Ok(config)
}

/// Validate a selected package's configuration without composing payloads.
///
/// The workbench candidate calls this before any provider process may exist:
/// an active selected configuration that does not validate against the
/// package's schema is a static failure of the whole candidate, not an
/// unavailable action (issue #704, CWR1-02). The reason string is the same
/// one runtime composition would produce, so the operator sees one diagnosis.
///
/// # Errors
///
/// Returns the operator-facing reason when the effective values violate the
/// package's declared schema.
pub fn validate_selected_configuration(
    settings: &PublishedSettings,
    manifest: &Manifest,
) -> Result<(), String> {
    resolve_selected_configuration(settings, manifest).map(|_| ())
}

/// Build one package's selected configuration and bounded environment.
///
/// Secret-reference fields name host environment variables. Their references
/// are removed from ordinary configuration and resolved only by the supervisor
/// into `Configure.secrets`. Effective non-secret defaults are applied so a
/// Configure carries every declared value a provider expects, while
/// secret-reference defaults remain references resolved only at the Configure
/// boundary. The manifest has no environment-export declaration, so selected
/// nonsecret configuration is never implicitly exported.
fn provider_configuration(
    settings: &PublishedSettings,
    manifest: &Manifest,
    binary: &Path,
) -> Result<(ProviderEnvironment, ConfigurePayload), String> {
    let mut config = resolve_selected_configuration(settings, manifest)?;
    let mut configure_secret_sources = BTreeMap::new();
    if let Some(schema) = manifest.config() {
        for field in schema.fields() {
            if field.kind() != FieldKind::SecretReference {
                continue;
            }
            let Some(value) = config.remove(field.id()) else {
                continue;
            };
            let TypedValue::SecretRef(reference) = value else {
                return Err(format!(
                    "provider {} secret reference {} must name a host environment variable",
                    manifest.id(),
                    field.id()
                ));
            };
            let source = EnvName::parse(reference.env.env()).map_err(|_| {
                format!(
                    "provider {} secret reference {} is not a valid environment name",
                    manifest.id(),
                    field.id()
                )
            })?;
            configure_secret_sources.insert(source.clone(), source);
        }
    }
    let environment = ProviderEnvironment {
        provider_dir: binary
            .parent()
            .map_or_else(|| binary.to_path_buf(), Path::to_path_buf),
        nonsecret: BTreeMap::new(),
        secret_env: BTreeMap::new(),
        configure_secret_sources,
    };
    let configure = ConfigurePayload {
        config_version: manifest.config().map_or(1, ConfigSchema::schema_version),
        config,
        secrets: BTreeMap::new(),
        environment: BTreeMap::new(),
    };
    Ok((environment, configure))
}

/// Derive the immutable invocation policy from a declared action and its owner package.
fn action_policy(declared: &DeclaredAction, manifest: &Manifest) -> ActionPolicy {
    ActionPolicy::new(
        declared.confirmation(),
        declared.allowed_outcomes().to_vec(),
        declared.destructive(),
    )
    .with_declared_routes(
        manifest
            .routes()
            .iter()
            .map(|route| route.id().clone())
            .collect(),
    )
}

/// Build the persistent startup candidate for one package.
fn persistent_candidate(
    package: &InstalledPackage,
    request: &CompositionRequest<'_>,
    binary: &Path,
    environment: ProviderEnvironment,
    configure: ConfigurePayload,
) -> Result<PersistentCandidate, RequestIdError> {
    let manifest = package.manifest();
    Ok(PersistentCandidate {
        plugin_id: manifest.id().owner_id().clone(),
        plugin_version: manifest.version().clone(),
        binary: binary.to_path_buf(),
        arguments: Vec::new(),
        working_dir: request.containment.working_dir.clone(),
        environment,
        home: request.containment.home.clone(),
        tmpdir: request.containment.tmpdir.clone(),
        locale: request.containment.locale.clone(),
        host_api: request.containment.host_api.clone(),
        generation: INITIAL_PROCESS_GENERATION,
        request_id: RequestId::new_host(INITIAL_PROCESS_GENERATION)?,
        configure,
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
