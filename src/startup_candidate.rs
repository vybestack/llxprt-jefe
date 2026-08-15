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
    /// A required provider cannot statically serve its active declarations.
    Provider(ProviderStaticRefused),
    /// Compiled and provider actions cannot compose into one registry.
    Actions(crate::persistence::keymap_edit::KeymapDiagnostic),
}

impl std::fmt::Display for WorkbenchStaticFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Selection(refusal) => write!(formatter, "selection refused: {refusal}"),
            Self::Agents(error) => write!(formatter, "shipped agents failed to publish: {error}"),
            Self::Screens(error) => write!(formatter, "screen composition refused: {error}"),
            Self::Provider(refusal) => write!(formatter, "required provider refused: {refusal}"),
            Self::Actions(diagnostic) => {
                write!(
                    formatter,
                    "action registry refused provider actions: {diagnostic}"
                )
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
/// Deterministic and side-effect-free: identical inputs compose an identical
/// aggregate. No provider process is started, no containment directory is
/// created, and no durable byte is written — including the state import,
/// which a later slice performs as the final fallible step before commit.
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
    let selected = select_owners(request).map_err(WorkbenchStaticFailure::Selection)?;
    let packages = selected
        .iter()
        .map(|owner| owner.package().clone())
        .collect::<Vec<_>>();
    let agents = AgentTypeRegistry::shipped().map_err(WorkbenchStaticFailure::Agents)?;
    let screens = compose_screens(request, &packages)?;
    validate_required_providers(request, &selected)?;
    let providers = compose_providers(request, &packages);
    let actions = compose_actions(request, &providers)?;
    Ok(PublishedWorkbench::from_parts(WorkbenchParts {
        settings: request.settings.clone(),
        inventory: request.inventory.clone(),
        selected,
        agents,
        screens,
        providers,
        actions,
    }))
}

/// Resolve every active selection against the retained inventory.
fn select_owners(
    request: &WorkbenchCandidateRequest<'_>,
) -> Result<Vec<SelectedOwner>, SelectionRefused> {
    crate::startup_selection::select_exactly(request.inventory, request.settings)
}

/// Compose the screen registry from compiled descriptors, definitions, and
/// exactly the selected packages.
fn compose_screens(
    request: &WorkbenchCandidateRequest<'_>,
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
) -> Result<crate::workbench::compose::ScreenComposition, WorkbenchStaticFailure> {
    screens::compose(request.paths, packages, request.settings)
        .map_err(WorkbenchStaticFailure::Screens)
}

/// Prove every required provider can statically serve its declarations.
///
/// Requiredness was decided during selection; this enforces it. A required
/// provider without a Ready binary for this host, or with a selected
/// configuration its schema rejects, refuses the candidate before anything
/// composes runtime state around it.
fn validate_required_providers(
    request: &WorkbenchCandidateRequest<'_>,
    selected: &[SelectedOwner],
) -> Result<(), WorkbenchStaticFailure> {
    for owner in selected {
        if !matches!(owner.requirement(), ProviderRequirement::Required { .. }) {
            continue;
        }
        let manifest = owner.package().manifest();
        if !matches!(
            manifest.provider().select(&request.host),
            ProviderSelection::Ready(_)
        ) {
            return Err(WorkbenchStaticFailure::Provider(
                ProviderStaticRefused::RequiredUnavailable {
                    owner: owner.owner().clone(),
                    version: Box::new(owner.package().coordinate().version().clone()),
                    host: request.host.clone(),
                },
            ));
        }
        if let Err(detail) = validate_selected_configuration(request.settings, manifest) {
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
    request: &WorkbenchCandidateRequest<'_>,
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
) -> ProviderComposition {
    compose(&CompositionRequest {
        packages,
        settings: request.settings,
        host: request.host.clone(),
        containment: request.containment.clone(),
    })
}

/// Compose compiled and provider actions into the one static snapshot.
fn compose_actions(
    request: &WorkbenchCandidateRequest<'_>,
    providers: &ProviderComposition,
) -> Result<ActionRegistrySnapshot, WorkbenchStaticFailure> {
    compose_published_with_providers(
        request.settings,
        "startup candidate",
        providers.actions().to_vec(),
        providers.availability().to_vec(),
    )
    .map_err(WorkbenchStaticFailure::Actions)
    .map(|composed| composed.snapshot().clone())
}
