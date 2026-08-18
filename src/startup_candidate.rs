//! Process-free workbench candidate construction (issue #704, slices S1:
//! CWR1-00 and CWR1-02).
//!
//! This is the static phase of normal startup: given resolved paths, the one
//! package inventory, published Settings, the host triple, and containment,
//! it composes every active static declaration into one
//! [`PublishedWorkbench`] — or refuses with one typed static failure. It scans
//! packages once ([`scan_inventory`]), never writes a durable byte, creates no
//! containment directory, and starts no process. The caller keeps provider
//! ownership local until a later slice commits the candidate atomically.
//!
//! The order is fixed and each step is fatal before the next: exact selection,
//! shipped agents, screen composition, strict required-provider validation,
//! provider composition, and the one action snapshot. A failure at any step
//! publishes nothing, which is what makes the candidate atomic rather than a
//! set of independent publications that can disagree.

use crate::agent_registry::{AgentTypeRegistry, RegistryPublishError};
use crate::domain::Id;
use crate::domain::action_registry::ActionRegistrySnapshot;
use crate::domain::plugin::{HostTriple, ProviderSelection};
use crate::persistence::keymap_edit::compose_published_with_providers;
use crate::persistence::paths::ResolvedPaths;
use crate::persistence::plugin_inventory::PluginInventory;
use crate::persistence::settings_document::PublishedSettings;
use crate::published_workbench::{PublishedWorkbench, WorkbenchParts};
use crate::runtime::provider::{
    CompositionRequest, Containment, ProviderComposition, compose, validate_selected_configuration,
};
use crate::startup_screens::{self as screens, ScreenStartupError};
use crate::startup_selection::{ProviderRequirement, SelectedOwner, SelectionRefused};
use crate::workbench::{
    BuiltinResourceSchemaError, ResourceSchemaError, ResourceSchemaRegistry, ScreenIdentity,
    builtin_resource_schemas,
};

/// Why the workbench candidate could not be composed statically.
///
/// Every variant is a complete stop: nothing was started, nothing was
/// published, and durable bytes are untouched. The variants carry their own
/// evidence because the operator's recovery differs — an unresolvable
/// selection, a broken compiled table, a refused definition, a provider that
/// cannot serve its declarations, or actions that cannot share one registry.
#[derive(Debug)]
pub enum WorkbenchStaticFailure {
    /// An active selection did not resolve to exactly one installed package.
    Selection(SelectionRefused),
    /// The shipped agent registry failed to publish, which is this program's
    /// own defect rather than anything on disk.
    Agents(RegistryPublishError),
    /// An enabled screen definition was refused by composition.
    Screens(ScreenStartupError),
    /// A typed-resource schema failed publication.
    Resources(ResourcePublicationError),
    /// A required provider cannot statically serve its active declarations.
    Provider(ProviderStaticRefused),
    /// Published actions and screen declarations cannot compose into one registry.
    Actions(crate::persistence::keymap_edit::KeymapDiagnostic),
}
/// Why immutable resource schemas could not join one candidate registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourcePublicationError {
    /// A shipped resource declaration is internally inconsistent.
    Builtin(BuiltinResourceSchemaError),
    /// Definition-owned and shipped schemas conflict.
    Composition(ResourceSchemaError),
    /// One active port does not name an exact schema in the candidate catalog.
    PortReference {
        /// Identity of the screen containing the rejected port.
        screen: ScreenIdentity,
        /// Panel containing the rejected port.
        panel: String,
        /// Rejected port identifier.
        port: String,
        /// Exact catalog mismatch.
        source: ResourceSchemaError,
    },
    /// A validated workbench type could not be represented by the domain ID contract.
    InvalidPortType {
        /// Identity of the screen containing the rejected port.
        screen: ScreenIdentity,
        /// Panel containing the rejected port.
        panel: String,
        /// Rejected port identifier.
        port: String,
        /// Full versioned type spelling.
        type_id: String,
    },
}

impl ResourcePublicationError {
    /// Whether the refusal was caused by an active screen definition.
    #[must_use]
    pub const fn is_definition_fault(&self) -> bool {
        match self {
            Self::Builtin(_) => false,
            Self::Composition(_) => true,
            Self::PortReference { screen, .. } | Self::InvalidPortType { screen, .. } => {
                !matches!(screen, ScreenIdentity::Compiled(_))
            }
        }
    }
}

impl std::fmt::Display for ResourcePublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin(error) => write!(formatter, "{error}"),
            Self::Composition(error) => write!(formatter, "{error}"),
            Self::PortReference {
                screen,
                panel,
                port,
                source,
            } => write!(
                formatter,
                "screen {screen} port {panel}.{port} has invalid resource schema reference: {source}"
            ),
            Self::InvalidPortType {
                screen,
                panel,
                port,
                type_id,
            } => write!(
                formatter,
                "screen {screen} port {panel}.{port} has invalid resource type {type_id}"
            ),
        }
    }
}

impl std::error::Error for ResourcePublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Builtin(error) => Some(error),
            Self::Composition(error) => Some(error),
            Self::PortReference { source, .. } => Some(source),
            Self::InvalidPortType { .. } => None,
        }
    }
}

impl WorkbenchStaticFailure {
    /// Stable process exit code for this static refusal.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Resources(error) if error.is_definition_fault() => 2,
            Self::Agents(_) | Self::Resources(_) => 78,
            Self::Screens(error) => error.exit_code(),
            Self::Selection(_) | Self::Provider(_) | Self::Actions(_) => 2,
        }
    }

    /// Whether provider-free configuration validation is an applicable recovery.
    #[must_use]
    pub const fn is_configuration_failure(&self) -> bool {
        match self {
            Self::Selection(_)
            | Self::Provider(_)
            | Self::Actions(_)
            | Self::Screens(ScreenStartupError::Definitions(_) | ScreenStartupError::Refused(_)) => {
                true
            }
            Self::Resources(error) => error.is_definition_fault(),
            Self::Agents(_) | Self::Screens(ScreenStartupError::Compiled(_)) => false,
        }
    }
}

impl std::fmt::Display for WorkbenchStaticFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Selection(refusal) => write!(formatter, "selection refused: {refusal}"),
            Self::Agents(error) => write!(formatter, "shipped agents failed to publish: {error}"),
            Self::Screens(error) => write!(formatter, "screen composition refused: {error}"),
            Self::Resources(error) => {
                write!(formatter, "resource schemas failed to publish: {error}")
            }
            Self::Provider(refusal) => write!(formatter, "required provider refused: {refusal}"),
            Self::Actions(diagnostic) => {
                write!(formatter, "action registry refused candidate: {diagnostic}")
            }
        }
    }
}

impl std::error::Error for WorkbenchStaticFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Selection(refusal) => Some(refusal),
            Self::Agents(error) => Some(error),
            Self::Screens(error) => Some(error),
            Self::Resources(error) => Some(error),
            Self::Provider(refusal) => Some(refusal),
            Self::Actions(diagnostic) => Some(diagnostic),
        }
    }
}

/// Why a required provider cannot statically serve its active declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStaticRefused {
    /// The provider owns active declarations but declares no Ready binary for
    /// this host, so nothing could ever serve them.
    RequiredUnavailable {
        /// The published owner whose provider is required.
        owner: crate::domain::Id,
        /// The exact selected package version. Boxed to keep the refusal
        /// inside the `Result`-size lint bound.
        version: Box<crate::domain::CanonicalSemver>,
        /// The host triple that lacks a binary.
        host: HostTriple,
    },
    /// The selected configuration for the package violates its own schema.
    InvalidConfiguration {
        /// The published owner whose configuration is invalid.
        owner: crate::domain::Id,
        /// The operator-facing reason, the same one runtime composition
        /// would produce.
        detail: String,
    },
}

impl std::fmt::Display for ProviderStaticRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiredUnavailable {
                owner,
                version,
                host,
            } => write!(
                formatter,
                "provider {owner} {version} owns active declarations but has no binary for host {host}"
            ),
            Self::InvalidConfiguration { owner, detail } => {
                write!(
                    formatter,
                    "provider {owner} configuration is invalid: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for ProviderStaticRefused {}

/// Everything one candidate construction reads.
pub struct WorkbenchCandidateRequest<'a> {
    /// The resolved startup paths, which own the definitions directory.
    pub paths: &'a ResolvedPaths,
    /// The one package inventory scan; see [`scan_inventory`].
    pub inventory: &'a PluginInventory,
    /// The published Settings snapshot the inventory was resolved against.
    pub settings: &'a PublishedSettings,
    /// The exact build host triple provider binaries are selected against.
    pub host: HostTriple,
    /// The contained process locations a later startup slice will use.
    pub containment: Containment,
}

struct CandidateInputs<'a> {
    inventory: &'a PluginInventory,
    settings: &'a PublishedSettings,
    host: &'a HostTriple,
    containment: &'a Containment,
}

/// Scan the ordered package roots exactly once for candidate construction.
///
/// The scan is the same one normal startup performs, kept here so the
/// candidate and the inventory its consumers see are one moment rather than
/// two that can disagree. The scan starts no process and writes nothing.
#[must_use]
pub fn scan_inventory(paths: &ResolvedPaths) -> PluginInventory {
    crate::startup::scan_plugin_inventory(paths)
}

/// Compose every active static declaration into one workbench candidate.
///
/// Captures definition and package-screen bytes once, then performs deterministic,
/// process-free composition from that immutable snapshot. No provider process is
/// started, no containment directory is created, and no durable byte is written —
/// including the state import, which a later slice performs as the final fallible
/// step before commit.
///
/// # Errors
///
/// Returns [`WorkbenchStaticFailure`] for the first static input that cannot
/// be part of a valid workbench: an unresolvable active selection, a broken
/// shipped agent table, a refused enabled screen definition, a required
/// provider that cannot serve its declarations, or actions that cannot share
/// one registry. Nothing is published in any of those cases.
pub fn build_workbench_candidate(
    request: &WorkbenchCandidateRequest<'_>,
) -> Result<PublishedWorkbench, WorkbenchStaticFailure> {
    let screen_sources = screens::ScreenSources::capture(request.paths, request.inventory)
        .map_err(WorkbenchStaticFailure::Screens)?;
    compose_workbench_candidate(
        CandidateInputs {
            inventory: request.inventory,
            settings: request.settings,
            host: &request.host,
            containment: &request.containment,
        },
        screen_sources,
    )
}

/// Recompose the candidate screen stage once, then ask every independent later
/// registry owner whether it can publish from that same immutable stage.
pub(crate) fn recompose_workbench_candidate_failures_with_screens(
    current: &PublishedWorkbench,
    settings: &PublishedSettings,
) -> (
    Vec<WorkbenchStaticFailure>,
    Option<crate::workbench::ScreenRegistry>,
) {
    let inputs = CandidateInputs {
        inventory: current.inventory(),
        settings,
        host: current.host(),
        containment: current.containment(),
    };
    let sources = current.screen_sources().clone();
    let (selected, packages, screens) = match compose_screen_stage(&inputs, &sources) {
        Ok(stage) => stage,
        Err(error) => return (vec![error], None),
    };
    let registry = screens.registry.clone();
    let mut failures = Vec::new();
    if let Err(error) = AgentTypeRegistry::shipped().map_err(WorkbenchStaticFailure::Agents) {
        failures.push(error);
    }
    if let Err(error) = compose_resource_schemas(&screens) {
        failures.push(error);
    }
    if let Err(error) = validate_selected_providers(&inputs, &selected) {
        failures.push(error);
    }
    let providers = compose_providers(&inputs, &packages);
    match compose_actions(&inputs, &providers) {
        Ok(actions) => {
            if let Err(error) = validate_screen_bindings(&registry, &actions)
                .map_err(WorkbenchStaticFailure::Actions)
            {
                failures.push(error);
            }
        }
        Err(error) => failures.push(error),
    }
    (failures, Some(registry))
}

fn compose_workbench_candidate(
    inputs: CandidateInputs<'_>,
    screen_sources: screens::ScreenSources,
) -> Result<PublishedWorkbench, WorkbenchStaticFailure> {
    let (selected, packages, screens) = compose_screen_stage(&inputs, &screen_sources)?;
    compose_after_screens(inputs, screen_sources, selected, &packages, screens)
}

fn compose_screen_stage(
    inputs: &CandidateInputs<'_>,
    sources: &screens::ScreenSources,
) -> Result<
    (
        Vec<SelectedOwner>,
        Vec<crate::persistence::plugin_inventory::InstalledPackage>,
        crate::workbench::compose::ScreenComposition,
    ),
    WorkbenchStaticFailure,
> {
    let selected = select_owners(inputs).map_err(WorkbenchStaticFailure::Selection)?;
    let packages = selected
        .iter()
        .map(|owner| owner.package().clone())
        .collect::<Vec<_>>();
    let screens = compose_screens(inputs, sources, &packages)?;
    Ok((selected, packages, screens))
}

fn compose_after_screens(
    inputs: CandidateInputs<'_>,
    screen_sources: screens::ScreenSources,
    selected: Vec<SelectedOwner>,
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
    screens: crate::workbench::compose::ScreenComposition,
) -> Result<PublishedWorkbench, WorkbenchStaticFailure> {
    let agents = AgentTypeRegistry::shipped().map_err(WorkbenchStaticFailure::Agents)?;
    let resource_schemas = compose_resource_schemas(&screens)?;
    validate_selected_providers(&inputs, &selected)?;
    let providers = compose_providers(&inputs, packages);
    let actions = compose_actions(&inputs, &providers)?;
    validate_screen_bindings(&screens.registry, &actions)
        .map_err(WorkbenchStaticFailure::Actions)?;
    Ok(PublishedWorkbench::from_parts(WorkbenchParts {
        screen_sources,
        host: inputs.host.clone(),
        containment: inputs.containment.clone(),
        settings: inputs.settings.clone(),
        inventory: inputs.inventory.clone(),
        selected,
        agents,
        screens,
        resource_schemas,
        providers,
        actions,
    }))
}
pub(crate) fn validate_screen_bindings(
    screens: &crate::workbench::ScreenRegistry,
    actions: &ActionRegistrySnapshot,
) -> Result<(), crate::persistence::keymap_edit::KeymapDiagnostic> {
    let fallback =
        crate::domain::input_context::ContextStack::from_ordered(["workbench", "global"], false)
            .map_err(|error| {
                crate::persistence::keymap_edit::KeymapDiagnostic::from_detail(error.to_string())
            })?;
    for screen in screens.screens() {
        if screen.bindings.is_empty() {
            continue;
        }
        let declared = screen
            .bindings
            .iter()
            .map(|binding| (binding.context.clone(), binding.action.clone()))
            .collect::<Vec<_>>();
        actions
            .validate_declared_bindings(&declared, &fallback)
            .map_err(|error| {
                crate::persistence::keymap_edit::KeymapDiagnostic::from_detail(format!(
                    "screen '{}' declared invalid bindings: {error}",
                    screen.id
                ))
            })?;
    }

    Ok(())
}

fn compose_resource_schemas(
    screens: &crate::workbench::compose::ScreenComposition,
) -> Result<ResourceSchemaRegistry, WorkbenchStaticFailure> {
    let builtins = builtin_resource_schemas()
        .map_err(ResourcePublicationError::Builtin)
        .map_err(WorkbenchStaticFailure::Resources)?;
    let mut declarations = builtins.schemas();
    declarations.extend(screens.resource_schemas.iter().cloned());
    let schemas = ResourceSchemaRegistry::publish(declarations)
        .map_err(ResourcePublicationError::Composition)
        .map_err(WorkbenchStaticFailure::Resources)?;
    validate_port_resource_references(&screens.registry, &schemas)?;
    Ok(schemas)
}

fn validate_port_resource_references(
    screens: &crate::workbench::ScreenRegistry,
    schemas: &ResourceSchemaRegistry,
) -> Result<(), WorkbenchStaticFailure> {
    for screen in screens.screens() {
        for panel in &screen.panels {
            for port in &panel.ports {
                let type_id = Id::parse(port.type_id.name()).map_err(|_| {
                    WorkbenchStaticFailure::Resources(ResourcePublicationError::InvalidPortType {
                        screen: screen.id,
                        panel: panel.id.as_str().to_owned(),
                        port: port.id.as_str().to_owned(),
                        type_id: port.type_id.as_str().to_owned(),
                    })
                })?;
                schemas
                    .validate_reference(&port.owner_id, &type_id, port.type_id.version())
                    .map_err(|source| {
                        WorkbenchStaticFailure::Resources(ResourcePublicationError::PortReference {
                            screen: screen.id,
                            panel: panel.id.as_str().to_owned(),
                            port: port.id.as_str().to_owned(),
                            source,
                        })
                    })?;
            }
        }
    }
    Ok(())
}

/// Resolve every active selection against the retained inventory.
fn select_owners(inputs: &CandidateInputs<'_>) -> Result<Vec<SelectedOwner>, SelectionRefused> {
    crate::startup_selection::select_exactly(inputs.inventory, inputs.settings)
}

/// Compose the screen registry from compiled descriptors, captured definitions,
/// and exactly the selected packages.
fn compose_screens(
    inputs: &CandidateInputs<'_>,
    sources: &screens::ScreenSources,
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
) -> Result<crate::workbench::compose::ScreenComposition, WorkbenchStaticFailure> {
    screens::compose_captured(sources, packages, inputs.settings)
        .map_err(WorkbenchStaticFailure::Screens)
}

/// Prove every selected provider can statically join the candidate.
///
/// Requiredness decides whether a missing host binary is fatal. Configuration
/// is different: every selected provider kind publishes its configuration as
/// part of this aggregate, so an invalid one-shot or configuration-only owner
/// must refuse the candidate just as an invalid persistent owner does.
fn validate_selected_providers(
    inputs: &CandidateInputs<'_>,
    selected: &[SelectedOwner],
) -> Result<(), WorkbenchStaticFailure> {
    for owner in selected {
        let manifest = owner.package().manifest();
        if matches!(owner.requirement(), ProviderRequirement::Required { .. })
            && !matches!(
                manifest.provider().select(inputs.host),
                ProviderSelection::Ready(_)
            )
        {
            return Err(WorkbenchStaticFailure::Provider(
                ProviderStaticRefused::RequiredUnavailable {
                    owner: owner.owner().clone(),
                    version: Box::new(owner.package().coordinate().version().clone()),
                    host: inputs.host.clone(),
                },
            ));
        }
        if let Err(detail) = validate_selected_configuration(inputs.settings, manifest) {
            return Err(WorkbenchStaticFailure::Provider(
                ProviderStaticRefused::InvalidConfiguration {
                    owner: owner.owner().clone(),
                    detail,
                },
            ));
        }
    }
    Ok(())
}

/// Compose the static provider contribution from the selected packages.
fn compose_providers(
    inputs: &CandidateInputs<'_>,
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
) -> ProviderComposition {
    compose(&CompositionRequest {
        packages,
        settings: inputs.settings,
        host: inputs.host.clone(),
        containment: inputs.containment.clone(),
    })
}

/// Compose compiled and provider actions into the one static snapshot.
fn compose_actions(
    inputs: &CandidateInputs<'_>,
    providers: &ProviderComposition,
) -> Result<ActionRegistrySnapshot, WorkbenchStaticFailure> {
    compose_published_with_providers(
        inputs.settings,
        "startup candidate",
        providers.actions().to_vec(),
        providers.availability().to_vec(),
    )
    .map_err(WorkbenchStaticFailure::Actions)
    .map(|composed| composed.snapshot().clone())
}

#[cfg(test)]
#[path = "startup_candidate_tests.rs"]
mod tests;
