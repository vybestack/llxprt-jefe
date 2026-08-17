//! Exact selected-package resolution for the workbench candidate
//! (issue #704, CWR1-00).
//!
//! This is the static seam that decides, from the one retained inventory and
//! published Settings, exactly which installed package each *active*
//! selection owns, and whether that package's provider must reach `ready`
//! before anything may spawn. An owner that Settings did not publish, or that
//! Settings published disabled, is not active and selects nothing: its
//! declarations stay dormant for a later restart.
//!
//! Resolution is exact and total. A pinned version that no installed package
//! provides, a coordinate claimed by two physical packages, and a package that
//! exists but cannot be classified are all typed refusals — the candidate is
//! refused rather than started with a different program than the one the
//! operator named. An unpinned owner resolves to its highest discovered
//! coordinate — across installed, ambiguous, and unusable claims alike — and
//! that coordinate must be uniquely valid, so a broken or contested higher
//! version is fatal rather than silently skipped for a lower usable one.
//!
//! The module is pure: it reads typed snapshots, spawns nothing, writes
//! nothing, and resolves deterministically from its inputs.

use std::path::PathBuf;

use crate::domain::plugin::PackageCoordinate;
use crate::domain::plugin::ProviderMode;
use crate::domain::{CanonicalSemver, Id};
use crate::persistence::plugin_inventory::{
    AmbiguousPackage, InstalledPackage, PluginInventory, UnavailablePackage, UnavailableReason,
};
use crate::persistence::settings_document::PublishedSettings;

/// One kind of active declaration a persistent provider can own.
///
/// Requiredness is decided by what the selected package's manifest declares,
/// never by metadata or defaults alone (issue #704 decision 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    /// A configuration schema Settings can edit.
    Config,
    /// Actions executed by the provider.
    Actions,
    /// Panels contributed to screens.
    Panels,
    /// Routes contributed to navigation.
    Routes,
    /// Screen descriptor files.
    Screens,
}

impl DeclarationKind {
    /// The wire-visible name, matching the manifest's own vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Actions => "actions",
            Self::Panels => "panels",
            Self::Routes => "routes",
            Self::Screens => "screens",
        }
    }
}

/// Why a selected package does not require a startup process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRequiredReason {
    /// The provider runs per invocation and exits; nothing starts at startup.
    OneShot,
    /// A persistent provider that owns no active declaration.
    DeclarationEmpty,
    /// The package declares no provider at all.
    NoProvider,
}

impl NotRequiredReason {
    /// The operator-visible name of this reason.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OneShot => "one-shot provider",
            Self::DeclarationEmpty => "persistent provider owns no declaration",
            Self::NoProvider => "no provider declared",
        }
    }
}

/// Whether the selected package's provider must complete Configure and Ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRequirement {
    /// The provider owns at least one active declaration, so startup cannot
    /// proceed until it is ready.
    Required {
        /// Every active declaration kind the selected package owns.
        declarations: Vec<DeclarationKind>,
    },
    /// No startup process is required for this package.
    NotRequired {
        /// Why no process is required.
        reason: NotRequiredReason,
    },
}

/// One exactly-resolved active selection: the installed package a published,
/// enabled Settings entry owns, plus that package's provider requirement.
#[derive(Debug, Clone)]
pub struct SelectedOwner {
    owner: Id,
    package: InstalledPackage,
    requirement: ProviderRequirement,
}

impl SelectedOwner {
    /// The published Settings owner identifier.
    #[must_use]
    pub const fn owner(&self) -> &Id {
        &self.owner
    }

    /// The one installed package this selection resolved to.
    #[must_use]
    pub const fn package(&self) -> &InstalledPackage {
        &self.package
    }

    /// Whether the package's provider must reach `ready` before spawn phases.
    #[must_use]
    pub const fn requirement(&self) -> &ProviderRequirement {
        &self.requirement
    }
}

/// Why an active selection could not resolve to exactly one package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionRefused {
    /// The pinned version names a package that is not installed.
    Missing {
        /// The published owner whose selection is missing.
        owner: Id,
        /// The pinned version, when Settings recorded one. Boxed because the
        /// parsed semver is three times the variant's other payload and this
        /// refusal travels through `Result`.
        version: Option<Box<CanonicalSemver>>,
    },
    /// Two physically distinct packages claim the selected coordinate.
    Ambiguous {
        /// The published owner whose coordinate is contested.
        owner: Id,
        /// Every physical directory claiming the coordinate.
        paths: Vec<PathBuf>,
    },
    /// The selected package exists but cannot be classified as installed.
    Unavailable {
        /// The published owner whose package is unusable.
        owner: Id,
        /// Why the scan could not use the package.
        reason: UnavailableReason,
    },
}

impl std::fmt::Display for SelectionRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { owner, version } => match version {
                Some(version) => write!(
                    formatter,
                    "enabled plugin {owner} pins version {version}, which no installed package provides"
                ),
                None => write!(
                    formatter,
                    "enabled plugin {owner} has no usable installed package"
                ),
            },
            Self::Ambiguous { owner, paths } => write!(
                formatter,
                "enabled plugin {owner} is claimed by {} distinct installed packages",
                paths.len()
            ),
            Self::Unavailable { owner, reason } => write!(
                formatter,
                "enabled plugin {owner} selects an unavailable package ({})",
                reason.message()
            ),
        }
    }
}

impl std::error::Error for SelectionRefused {}

/// Resolve every active selection to exactly one installed package.
///
/// Active means Settings published the owner under `plugins` with
/// `enabled = true`. Selection is exact: a pinned version must match an
/// installed package's version precisely. An unpinned owner resolves to its
/// highest discovered coordinate — across installed, ambiguous, and
/// unavailable claims, ordered by the inventory's SemVer precedence — and that
/// winning coordinate must be uniquely valid; an ambiguous or unusable higher
/// coordinate refuses the selection rather than falling back to a lower usable
/// version. The result is ordered by owner id because published Settings is.
///
/// # Errors
///
/// Returns [`SelectionRefused`] when an active selection pins a version that
/// is missing, ambiguous, or unavailable, or when an unpinned owner's highest
/// discovered coordinate is ambiguous or unavailable. Disabled and dormant
/// owners are not active and never refuse.
pub fn select_exactly(
    inventory: &PluginInventory,
    settings: &PublishedSettings,
) -> Result<Vec<SelectedOwner>, SelectionRefused> {
    let mut selected = Vec::new();
    for (owner, published) in &settings.plugins {
        if published.enabled != Some(true) {
            continue;
        }
        let package = resolve_owner(inventory, owner, published.version.as_ref())?;
        let requirement = classify_requirement(package);
        selected.push(SelectedOwner {
            owner: owner.clone(),
            package: package.clone(),
            requirement,
        });
    }
    Ok(selected)
}

/// Resolve one active owner to its exactly-selected package.
fn resolve_owner<'a>(
    inventory: &'a PluginInventory,
    owner: &Id,
    pinned: Option<&CanonicalSemver>,
) -> Result<&'a InstalledPackage, SelectionRefused> {
    match pinned {
        Some(version) => resolve_pinned(inventory, owner, version),
        None => resolve_unpinned(inventory, owner),
    }
}

/// Resolve a pinned owner: the pin names one coordinate, and only an
/// installed package providing exactly that coordinate satisfies it.
fn resolve_pinned<'a>(
    inventory: &'a PluginInventory,
    owner: &Id,
    version: &CanonicalSemver,
) -> Result<&'a InstalledPackage, SelectionRefused> {
    if let Some(package) = inventory.packages().iter().find(|package| {
        package.coordinate().id().owner_id() == owner
            && package.coordinate().version().as_str() == version.as_str()
    }) {
        return Ok(package);
    }
    Err(refuse_pinned(inventory, owner, version))
}

/// Resolve an unpinned owner to the highest coordinate discovery found for
/// it, then require that coordinate to be uniquely valid.
///
/// Every claim on the owner's coordinate space competes — installed,
/// ambiguous, and unavailable alike — in the inventory's own listing order,
/// which is SemVer precedence descending. Falling back past a higher
/// coordinate that turned out to be contested or unusable would silently
/// start a different program than the one discovery ranked first, so the
/// winner is refused, not skipped.
fn resolve_unpinned<'a>(
    inventory: &'a PluginInventory,
    owner: &Id,
) -> Result<&'a InstalledPackage, SelectionRefused> {
    let claims = inventory
        .packages()
        .iter()
        .map(Claim::Installed)
        .chain(inventory.ambiguities().iter().map(Claim::Ambiguous))
        .chain(inventory.unavailable().iter().map(Claim::Unavailable))
        .filter(|claim| claim.coordinate().id().owner_id() == owner);
    let Some(winner) = claims.min_by(|left, right| {
        PackageCoordinate::listing_cmp(left.coordinate(), right.coordinate())
    }) else {
        return Err(SelectionRefused::Missing {
            owner: owner.clone(),
            version: None,
        });
    };
    match winner {
        Claim::Installed(package) => Ok(package),
        Claim::Ambiguous(contested) => Err(SelectionRefused::Ambiguous {
            owner: owner.clone(),
            paths: contested.paths().to_vec(),
        }),
        Claim::Unavailable(unusable) => Err(SelectionRefused::Unavailable {
            owner: owner.clone(),
            reason: unusable.reason().clone(),
        }),
    }
}

/// One discovered claim on a coordinate, whatever its usability turned out
/// to be.
#[derive(Clone, Copy)]
enum Claim<'a> {
    /// A uniquely claimed, usable package.
    Installed(&'a InstalledPackage),
    /// A coordinate contested by two or more physical packages.
    Ambiguous(&'a AmbiguousPackage),
    /// A well-named package that cannot be used.
    Unavailable(&'a UnavailablePackage),
}

impl Claim<'_> {
    /// The coordinate this claim was discovered on.
    const fn coordinate(&self) -> &PackageCoordinate {
        match self {
            Self::Installed(package) => package.coordinate(),
            Self::Ambiguous(claim) => claim.coordinate(),
            Self::Unavailable(claim) => claim.coordinate(),
        }
    }
}

/// Classify the unresolved pinned case: ambiguous and unavailable coordinates
/// name their evidence; anything else is missing.
fn refuse_pinned(
    inventory: &PluginInventory,
    owner: &Id,
    version: &CanonicalSemver,
) -> SelectionRefused {
    if let Some(contested) = inventory.ambiguities().iter().find(|claim| {
        claim.coordinate().id().owner_id() == owner
            && claim.coordinate().version().as_str() == version.as_str()
    }) {
        return SelectionRefused::Ambiguous {
            owner: owner.clone(),
            paths: contested.paths().to_vec(),
        };
    }
    if let Some(unusable) = unavailable_of(inventory, owner, version) {
        return SelectionRefused::Unavailable {
            owner: owner.clone(),
            reason: unusable.reason().clone(),
        };
    }
    SelectionRefused::Missing {
        owner: owner.clone(),
        version: Some(Box::new(version.clone())),
    }
}

/// The first unusable package claim for an owner, preferring the pinned one.
fn unavailable_of<'a>(
    inventory: &'a PluginInventory,
    owner: &Id,
    version: &CanonicalSemver,
) -> Option<&'a UnavailablePackage> {
    inventory
        .unavailable()
        .iter()
        .find(|claim| claim.coordinate().id().owner_id() == owner)
        .filter(|claim| claim.coordinate().version().as_str() == version.as_str())
        .or_else(|| {
            inventory
                .unavailable()
                .iter()
                .find(|claim| claim.coordinate().id().owner_id() == owner)
        })
}

/// Classify whether one selected package's provider must reach `ready`.
fn classify_requirement(package: &InstalledPackage) -> ProviderRequirement {
    let manifest = package.manifest();
    match manifest.provider().mode() {
        ProviderMode::None => ProviderRequirement::NotRequired {
            reason: NotRequiredReason::NoProvider,
        },
        ProviderMode::OneShot => ProviderRequirement::NotRequired {
            reason: NotRequiredReason::OneShot,
        },
        ProviderMode::Persistent => classify_persistent(manifest),
    }
}

/// A persistent provider is required exactly when it owns at least one active
/// declaration; metadata and defaults alone never make it required.
fn classify_persistent(manifest: &crate::domain::plugin::Manifest) -> ProviderRequirement {
    let mut declarations = Vec::new();
    if manifest.config().is_some() {
        declarations.push(DeclarationKind::Config);
    }
    if !manifest.actions().is_empty() {
        declarations.push(DeclarationKind::Actions);
    }
    if !manifest.panels().is_empty() {
        declarations.push(DeclarationKind::Panels);
    }
    if !manifest.routes().is_empty() {
        declarations.push(DeclarationKind::Routes);
    }
    if !manifest.screens().is_empty() {
        declarations.push(DeclarationKind::Screens);
    }
    if declarations.is_empty() {
        return ProviderRequirement::NotRequired {
            reason: NotRequiredReason::DeclarationEmpty,
        };
    }
    ProviderRequirement::Required { declarations }
}
