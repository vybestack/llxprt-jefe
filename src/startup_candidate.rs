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
    /// Compiled and provider actions cannot compose into one registry.
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
    let builtins = builtin_resource_schemas()
        .map_err(ResourcePublicationError::Builtin)
        .map_err(WorkbenchStaticFailure::Resources)?;
    let mut schema_declarations = builtins.schemas();
    schema_declarations.extend(screens.resource_schemas.iter().cloned());
    let resource_schemas = ResourceSchemaRegistry::publish(schema_declarations)
        .map_err(ResourcePublicationError::Composition)
        .map_err(WorkbenchStaticFailure::Resources)?;
    validate_port_resource_references(&screens.registry, &resource_schemas)?;
    validate_selected_providers(request, &selected)?;
    let providers = compose_providers(request, &packages);
    let actions = compose_actions(request, &providers)?;
    Ok(PublishedWorkbench::from_parts(WorkbenchParts {
        settings: request.settings.clone(),
        inventory: request.inventory.clone(),
        selected,
        agents,
        screens,
        resource_schemas,
        providers,
        actions,
    }))
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

/// Prove every selected provider can statically join the candidate.
///
/// Requiredness decides whether a missing host binary is fatal. Configuration
/// is different: every selected provider kind publishes its configuration as
/// part of this aggregate, so an invalid one-shot or configuration-only owner
/// must refuse the candidate just as an invalid persistent owner does.
fn validate_selected_providers(
    request: &WorkbenchCandidateRequest<'_>,
    selected: &[SelectedOwner],
) -> Result<(), WorkbenchStaticFailure> {
    for owner in selected {
        let manifest = owner.package().manifest();
        if matches!(owner.requirement(), ProviderRequirement::Required { .. })
            && !matches!(
                manifest.provider().select(&request.host),
                ProviderSelection::Ready(_)
            )
        {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        ResourcePublicationError, WorkbenchCandidateRequest, WorkbenchStaticFailure,
        build_workbench_candidate,
    };
    use crate::domain::Id;
    use crate::domain::plugin::HostTriple;
    use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
    use crate::persistence::plugin_inventory::scan;
    use crate::persistence::settings_document::PublishedSettings;
    use crate::runtime::provider::Containment;
    use crate::workbench::{
        BuiltinResourceSchemaError, CustomScreenId, ResourceSchemaError, ScreenId, ScreenIdentity,
    };

    fn custom_screen(raw: &'static str) -> CustomScreenId {
        CustomScreenId::parse(raw)
            .unwrap_or_else(|error| unreachable!("valid custom screen fixture: {error}"))
    }

    struct CandidateFixture {
        root: PathBuf,
    }

    impl CandidateFixture {
        fn new(label: &str, definition: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "jefe-startup-candidate-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("definitions"))
                .unwrap_or_else(|error| unreachable!("fixture directory must exist: {error}"));
            std::fs::write(root.join("definitions/review.screen.toml"), definition).unwrap_or_else(
                |error| unreachable!("fixture definition must be written: {error}"),
            );
            Self { root }
        }

        fn paths(&self) -> ResolvedPaths {
            let resolved = |name: &str| ResolvedFile {
                path: self.root.join(name),
                provenance: PathProvenance::ConfigArgument,
                sources: Vec::new(),
            };
            ResolvedPaths {
                settings: resolved("settings.toml"),
                state: resolved("state.json"),
                definitions: self.root.join("definitions"),
                plugins: self.root.join("plugins"),
                themes: self.root.join("themes"),
            }
        }

        fn build(
            &self,
            settings: &PublishedSettings,
        ) -> Result<crate::published_workbench::PublishedWorkbench, WorkbenchStaticFailure>
        {
            let paths = self.paths();
            let inventory = scan(&[]);
            build_workbench_candidate(&WorkbenchCandidateRequest {
                paths: &paths,
                inventory: &inventory,
                settings,
                host: HostTriple::current(),
                containment: Containment {
                    home: self.root.join("home"),
                    tmpdir: self.root.join("tmp"),
                    working_dir: self.root.join("work"),
                    locale: "C".to_owned(),
                    host_api: crate::VERSION.to_owned(),
                },
            })
        }
    }

    impl Drop for CandidateFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn enabled_settings() -> PublishedSettings {
        let mut settings = PublishedSettings::default();
        settings.workbench.enabled_screens = vec![
            Id::parse("local.review")
                .unwrap_or_else(|error| unreachable!("fixture screen ID must parse: {error}")),
        ];
        settings
    }

    fn review_definition() -> String {
        include_str!("workbench/testdata/local-review.screen.toml").replace("\r\n", "\n")
    }
    #[test]
    fn definition_resource_faults_are_configuration_failures() {
        let failure = WorkbenchStaticFailure::Resources(ResourcePublicationError::Composition(
            ResourceSchemaError::InvalidVersion { version: 0 },
        ));

        assert!(failure.is_configuration_failure());
        assert_eq!(failure.exit_code(), 2);
    }

    #[test]
    fn compiled_resource_faults_remain_internal_failures() {
        let builtin = WorkbenchStaticFailure::Resources(ResourcePublicationError::Builtin(
            BuiltinResourceSchemaError::Resource(ResourceSchemaError::InvalidVersion {
                version: 0,
            }),
        ));
        let port = WorkbenchStaticFailure::Resources(ResourcePublicationError::InvalidPortType {
            screen: ScreenIdentity::Compiled(ScreenId::Issues),
            panel: "issues.list".to_owned(),
            port: "selection".to_owned(),
            type_id: "invalid".to_owned(),
        });

        for failure in [builtin, port] {
            assert!(!failure.is_configuration_failure());
            assert_eq!(failure.exit_code(), 78);
        }
    }

    #[test]
    fn definition_port_resource_faults_are_configuration_failures() {
        let failure =
            WorkbenchStaticFailure::Resources(ResourcePublicationError::InvalidPortType {
                screen: ScreenIdentity::Custom(custom_screen("local.review")),
                panel: "review.list".to_owned(),
                port: "selection".to_owned(),
                type_id: "invalid".to_owned(),
            });

        assert!(failure.is_configuration_failure());
        assert_eq!(failure.exit_code(), 2);
    }

    #[test]
    fn candidate_publishes_an_enabled_definition_resource_schema() {
        let fixture = CandidateFixture::new("resource-valid", &review_definition());
        let candidate = fixture
            .build(&enabled_settings())
            .unwrap_or_else(|error| unreachable!("valid definition must publish: {error}"));
        let owner = Id::parse("local.review")
            .unwrap_or_else(|error| unreachable!("fixture owner must parse: {error}"));
        let type_id = Id::parse("local.review.note")
            .unwrap_or_else(|error| unreachable!("fixture type must parse: {error}"));

        assert_eq!(
            candidate
                .resource_schemas()
                .validate_reference(&owner, &type_id, 1),
            Ok(())
        );
    }

    #[test]
    fn candidate_refuses_unknown_wrong_version_and_wrong_owner_port_references() {
        let base = review_definition();
        let cases = [
            (
                "resource-unknown",
                base.replace("github.pull-request@1", "github.unknown@1"),
                "resource type github.unknown is not published",
            ),
            (
                "resource-version",
                base.replace("github.pull-request@1", "github.pull-request@2"),
                "resource type github.pull-request version 2 is not published",
            ),
            (
                "resource-owner",
                base.replace("github.pull-requests", "github.issues"),
                "resource schema owner github.issues does not match github.pull-requests",
            ),
        ];

        for (label, definition, expected) in cases {
            let fixture = CandidateFixture::new(label, &definition);
            let Err(error) = fixture.build(&enabled_settings()) else {
                panic!("invalid port reference must refuse the whole candidate");
            };
            assert!(error.is_configuration_failure());
            assert_eq!(error.exit_code(), 2);
            let diagnostic = error.to_string();
            assert!(diagnostic.contains(expected), "{diagnostic}");
            assert!(!diagnostic.contains("hidden"));
        }
    }

    #[test]
    fn candidate_refuses_a_definition_schema_colliding_with_a_builtin() {
        let definition = review_definition().replace("local.review.note", "github.issue");
        let fixture = CandidateFixture::new("resource-duplicate", &definition);

        let Err(error) = fixture.build(&enabled_settings()) else {
            panic!("duplicate schema identity must refuse the whole candidate");
        };

        assert!(matches!(
            error,
            WorkbenchStaticFailure::Resources(ResourcePublicationError::Composition(
                ResourceSchemaError::DuplicateSchema { .. }
            ))
        ));
        assert!(error.is_configuration_failure());
        assert_eq!(error.exit_code(), 2);
    }
}
