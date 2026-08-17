//! The unpublished workbench aggregate (issue #704).
//!
//! [`PublishedWorkbench`] is the data-only result of the process-free static
//! phase: effective Settings, the one retained package inventory with its
//! exact selection, the shipped agents, the composed screen registry, the
//! static provider composition, and the one action registry snapshot — all
//! composed and validated together before any provider process, runtime
//! service, or durable write may exist.
//!
//! The fields are private and there is deliberately no `Default`, no global
//! accessor, and no compatibility constructor: the aggregate exists so every
//! declaration consumer can hold one identity for the process lifetime, and
//! each of those escape hatches would re-create the split authorities this
//! type replaces. Construction belongs to the startup candidate boundary
//! ([`crate::startup_candidate`]); later slices commit it atomically.
//!
//! This type owns no handle, spawns nothing, and performs no I/O.

use crate::agent_registry::AgentTypeRegistry;
use crate::domain::action_registry::ActionRegistrySnapshot;
use crate::persistence::diagnostic::Diagnostic;
use crate::persistence::plugin_inventory::PluginInventory;
use crate::persistence::settings_document::PublishedSettings;
use crate::runtime::provider::persistent::PersistentPublication;
use crate::runtime::provider::{ProviderCatalog, ProviderComposition};
use crate::startup_selection::SelectedOwner;
use crate::workbench::compose::ScreenComposition;
use crate::workbench::screens::ScreenRegistry;

/// One atomically composed workbench candidate: every static declaration the
/// session will run, validated before anything is started or published.
#[derive(Debug)]
pub struct PublishedWorkbench {
    settings: PublishedSettings,
    inventory: PluginInventory,
    selected: Vec<SelectedOwner>,
    agents: AgentTypeRegistry,
    screens: ScreenComposition,
    providers: ProviderComposition,
    provider_ready: Option<PersistentPublication>,
    actions: ActionRegistrySnapshot,
}

/// The already-validated parts one candidate was composed from.
///
/// A single parameter rather than seven: construction is crate-private and
/// this keeps the argument list inside the project's lint bounds while naming
/// what must arrive together.
pub(crate) struct WorkbenchParts {
    /// The effective Settings snapshot everything was composed against.
    pub(crate) settings: PublishedSettings,
    /// The one retained package inventory scan.
    pub(crate) inventory: PluginInventory,
    /// Every active selection, resolved to its exact installed package.
    pub(crate) selected: Vec<SelectedOwner>,
    /// The shipped agent registry, validated at construction.
    pub(crate) agents: AgentTypeRegistry,
    /// The composed screen registry and its non-fatal warnings.
    pub(crate) screens: ScreenComposition,
    /// The static provider composition.
    pub(crate) providers: ProviderComposition,
    /// The one static action registry snapshot.
    pub(crate) actions: ActionRegistrySnapshot,
}

impl PublishedWorkbench {
    /// Assemble the aggregate from its already-validated parts.
    ///
    /// Crate-private by design: only the candidate boundary may build this,
    /// because only it can prove the parts were composed from one inventory
    /// and one Settings snapshot in the right order.
    #[must_use]
    pub(crate) fn from_parts(parts: WorkbenchParts) -> Self {
        Self {
            settings: parts.settings,
            inventory: parts.inventory,
            selected: parts.selected,
            agents: parts.agents,
            screens: parts.screens,
            providers: parts.providers,
            provider_ready: None,
            actions: parts.actions,
        }
    }

    /// The effective, typed Settings the candidate was composed from.
    #[must_use]
    pub const fn settings(&self) -> &PublishedSettings {
        &self.settings
    }

    /// The one package inventory scan the candidate retains.
    #[must_use]
    pub const fn inventory(&self) -> &PluginInventory {
        &self.inventory
    }

    /// Every active selection, resolved to its exact installed package.
    #[must_use]
    pub fn selected_owners(&self) -> &[SelectedOwner] {
        &self.selected
    }

    /// The shipped agent registry, validated at candidate construction.
    #[must_use]
    pub const fn agent_registry(&self) -> &AgentTypeRegistry {
        &self.agents
    }

    /// The composed, validated screen registry.
    #[must_use]
    pub const fn screen_registry(&self) -> &ScreenRegistry {
        &self.screens.registry
    }

    /// Non-fatal composition warnings, one per preserved omitted definition.
    #[must_use]
    pub fn screen_warnings(&self) -> &[Diagnostic] {
        &self.screens.warnings
    }

    /// The static provider composition: descriptors, availability, and the
    /// persistent candidates a later slice may start.
    #[must_use]
    pub const fn providers(&self) -> &ProviderComposition {
        &self.providers
    }

    /// The immutable provider action descriptors composed into this workbench.
    #[must_use]
    pub fn provider_catalog(&self) -> &ProviderCatalog {
        self.providers.catalog()
    }

    /// Ready metadata committed after every required persistent provider has
    /// completed Configure and Ready.
    #[must_use]
    pub const fn provider_ready(&self) -> Option<&PersistentPublication> {
        self.provider_ready.as_ref()
    }

    /// Attach the successful transaction's Ready metadata before publication.
    #[must_use]
    pub(crate) fn with_provider_ready(mut self, ready: PersistentPublication) -> Self {
        self.provider_ready = Some(ready);
        self
    }

    /// The one static action registry snapshot, compiled actions and provider
    /// actions composed together.
    #[must_use]
    pub const fn actions(&self) -> &ActionRegistrySnapshot {
        &self.actions
    }

    /// Config schemas for the package versions selected by this workbench.
    #[must_use]
    pub fn selected_plugin_configs(
        &self,
    ) -> std::collections::BTreeMap<
        crate::domain::Id,
        crate::messages::settings::SelectedPluginConfig,
    > {
        crate::persistence::plugin_inventory::configured_packages(
            self.inventory.packages(),
            &self.settings,
        )
        .into_iter()
        .filter_map(|package| {
            package.manifest().config().map(|schema| {
                (
                    package.coordinate().id().owner_id().clone(),
                    crate::messages::settings::SelectedPluginConfig {
                        version: package.coordinate().version().clone(),
                        schema: schema.clone(),
                        can_migrate: package.manifest().provider().mode()
                            != crate::domain::plugin::ProviderMode::None,
                    },
                )
            })
        })
        .collect()
    }

    /// Config schemas for every installed package version in this workbench.
    #[must_use]
    pub fn installed_plugin_configs(
        &self,
    ) -> std::collections::BTreeMap<
        crate::domain::Id,
        Vec<crate::messages::settings::SelectedPluginConfig>,
    > {
        let mut installed = std::collections::BTreeMap::new();
        for package in self.inventory.packages() {
            let Some(schema) = package.manifest().config() else {
                continue;
            };
            installed
                .entry(package.coordinate().id().owner_id().clone())
                .or_insert_with(Vec::new)
                .push(crate::messages::settings::SelectedPluginConfig {
                    version: package.coordinate().version().clone(),
                    schema: schema.clone(),
                    can_migrate: package.manifest().provider().mode()
                        != crate::domain::plugin::ProviderMode::None,
                });
        }
        installed
    }
}
