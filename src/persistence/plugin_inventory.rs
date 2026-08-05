//! Provider-free physical package inventory
//! (issue #389 CW-09, acceptance rows R4–R9).
//!
//! The scan walks the ordered roots and reports every package it can identify
//! physically. It starts no process, and it reads no file content: identifying
//! a package requires only its directory names and the presence of its
//! manifest, so discovery is provider-free by construction. Interpreting the
//! manifest is a later, separate step.
//!
//! Three rules define the result:
//!
//! * **Aliases collapse.** One physical package reached through several root
//!   paths — a Homebrew Cellar directory and its prefix symlink, for example —
//!   is one row. The first occurrence in root order wins and every later path
//!   is retained as alias provenance. Physical identity is decided by the
//!   CW-01 path authority, which compares `(device, inode)` where the platform
//!   provides it and canonical paths otherwise.
//! * **Ambiguity selects nothing.** Two *physically distinct* packages that
//!   claim one `(id, version)` coordinate are [`PluginCode::Ambiguous`].
//!   Root precedence never resolves that collision, so neither is selected and
//!   neither publishes — even when their bytes are identical, because the
//!   inventory cannot know which one an operator meant.
//! * **Containment is physical.** A package must resolve beneath the root it
//!   was found under. A symlink that escapes, at the final component or an
//!   intermediate one, is reported rather than selected.
//!
//! A directory that does not match `<root>/<plugin-id>/<canonical-semver>/` is
//! not a package at all and is passed over silently. A directory that *does*
//! match but cannot be used is reported as [`UnavailablePackage`], so one bad
//! package never prevents its valid neighbours from publishing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::paths::{PhysicalIdentity, physical_identity};
use super::plugin_roots::{PluginRoot, PluginRootKind};
use crate::domain::plugin::{PackageCoordinate, PluginCode};

/// File naming a package's manifest inside its version directory.
pub const MANIFEST_FILE_NAME: &str = "plugin.json";

/// One physically distinct installed package version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    coordinate: PackageCoordinate,
    root: PathBuf,
    root_kind: PluginRootKind,
    directory: PathBuf,
    aliases: Vec<PathBuf>,
}

impl InstalledPackage {
    /// The package's exact identity.
    #[must_use]
    pub const fn coordinate(&self) -> &PackageCoordinate {
        &self.coordinate
    }

    /// The root this package was first found under.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The provenance of the winning root.
    #[must_use]
    pub const fn root_kind(&self) -> PluginRootKind {
        self.root_kind
    }

    /// The canonical package directory.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Later paths that resolved to this same physical package.
    #[must_use]
    pub fn aliases(&self) -> &[PathBuf] {
        &self.aliases
    }
}

/// Why a well-named package directory cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// The version directory carries no manifest.
    MissingManifest,
    /// The package does not resolve beneath the root it was found under.
    EscapesRoot,
    /// The package directory could not be inspected.
    Unreadable,
}

impl UnavailableReason {
    /// Operator-facing explanation.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::MissingManifest => "no plugin.json in the version directory",
            Self::EscapesRoot => "the package resolves outside its package root",
            Self::Unreadable => "the package directory could not be inspected",
        }
    }
}

/// A well-named package that cannot be used, listed with its reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailablePackage {
    coordinate: PackageCoordinate,
    path: PathBuf,
    reason: UnavailableReason,
}

impl UnavailablePackage {
    /// The package's declared identity.
    #[must_use]
    pub const fn coordinate(&self) -> &PackageCoordinate {
        &self.coordinate
    }

    /// The directory as discovered.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Why it cannot be used.
    #[must_use]
    pub const fn reason(&self) -> UnavailableReason {
        self.reason
    }
}

/// A coordinate claimed by two or more physically distinct packages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguousPackage {
    coordinate: PackageCoordinate,
    paths: Vec<PathBuf>,
}

impl AmbiguousPackage {
    /// The contested identity.
    #[must_use]
    pub const fn coordinate(&self) -> &PackageCoordinate {
        &self.coordinate
    }

    /// Every physical path that claims it.
    #[must_use]
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// The stable operator-visible code for this condition.
    #[must_use]
    pub const fn code(&self) -> PluginCode {
        PluginCode::Ambiguous
    }
}

/// The immutable result of one physical inventory scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginInventory {
    packages: Vec<InstalledPackage>,
    ambiguities: Vec<AmbiguousPackage>,
    unavailable: Vec<UnavailablePackage>,
}

impl PluginInventory {
    /// Selected packages, in `jefe plugin list` order.
    #[must_use]
    pub fn packages(&self) -> &[InstalledPackage] {
        &self.packages
    }

    /// Coordinates that no package may claim, and why.
    #[must_use]
    pub fn ambiguities(&self) -> &[AmbiguousPackage] {
        &self.ambiguities
    }

    /// Well-named packages that cannot be used.
    #[must_use]
    pub fn unavailable(&self) -> &[UnavailablePackage] {
        &self.unavailable
    }
}

/// One package directory found during the walk, before collapsing.
struct Discovered {
    coordinate: PackageCoordinate,
    root: PathBuf,
    root_kind: PluginRootKind,
    path: PathBuf,
    identity: PhysicalIdentity,
}

/// Scan the ordered roots and build the physical inventory.
///
/// Roots that do not exist are skipped. The scan never starts a process and
/// never reads manifest content.
#[must_use]
pub fn scan(roots: &[PluginRoot]) -> PluginInventory {
    let mut inventory = PluginInventory::default();
    let mut discovered: Vec<Discovered> = Vec::new();
    for root in roots {
        collect_root(root, &mut discovered, &mut inventory.unavailable);
    }
    let collapsed = collapse_aliases(discovered);
    (inventory.packages, inventory.ambiguities) = split_ambiguities(collapsed);
    inventory
        .packages
        .sort_by(|left, right| PackageCoordinate::listing_cmp(&left.coordinate, &right.coordinate));
    inventory
}

/// Walk one root's `<id>/<version>/` layout.
fn collect_root(
    root: &PluginRoot,
    discovered: &mut Vec<Discovered>,
    unavailable: &mut Vec<UnavailablePackage>,
) {
    let Ok(root_identity) = physical_identity(root.path()) else {
        return;
    };
    for owner in read_directories(root.path()) {
        for version in read_directories(&owner) {
            let Some(coordinate) = coordinate_of(&owner, &version) else {
                continue;
            };
            classify(
                root,
                &root_identity,
                coordinate,
                version,
                discovered,
                unavailable,
            );
        }
    }
}

/// Decide whether one well-named package directory is usable.
fn classify(
    root: &PluginRoot,
    root_identity: &PhysicalIdentity,
    coordinate: PackageCoordinate,
    path: PathBuf,
    discovered: &mut Vec<Discovered>,
    unavailable: &mut Vec<UnavailablePackage>,
) {
    let mut reject = |reason| {
        unavailable.push(UnavailablePackage {
            coordinate: coordinate.clone(),
            path: path.clone(),
            reason,
        });
    };
    let Ok(identity) = physical_identity(&path) else {
        reject(UnavailableReason::Unreadable);
        return;
    };
    if !identity
        .canonical_path()
        .starts_with(root_identity.canonical_path())
    {
        reject(UnavailableReason::EscapesRoot);
        return;
    }
    if !path.join(MANIFEST_FILE_NAME).is_file() {
        reject(UnavailableReason::MissingManifest);
        return;
    }
    discovered.push(Discovered {
        coordinate,
        root: root.path().to_path_buf(),
        root_kind: root.kind(),
        path,
        identity,
    });
}

/// Collapse discoveries that name one physical package into one entry.
fn collapse_aliases(discovered: Vec<Discovered>) -> Vec<InstalledPackage> {
    let mut collapsed: Vec<(PhysicalIdentity, InstalledPackage)> = Vec::new();
    for entry in discovered {
        if let Some((_, package)) = collapsed
            .iter_mut()
            .find(|(identity, _)| identity.equivalent(&entry.identity))
        {
            package.aliases.push(entry.path);
            continue;
        }
        collapsed.push((
            entry.identity.clone(),
            InstalledPackage {
                coordinate: entry.coordinate,
                root: entry.root,
                root_kind: entry.root_kind,
                directory: entry.identity.canonical_path().to_path_buf(),
                aliases: Vec::new(),
            },
        ));
    }
    collapsed.into_iter().map(|(_, package)| package).collect()
}

/// Split packages whose coordinate is claimed by more than one physical
/// package away from those that are uniquely claimed.
///
/// Aliases have already collapsed, so a coordinate appearing twice here means
/// two genuinely distinct packages. Both partitions keep discovery order, so
/// the result is deterministic.
fn split_ambiguities(
    packages: Vec<InstalledPackage>,
) -> (Vec<InstalledPackage>, Vec<AmbiguousPackage>) {
    let mut claims: HashMap<&PackageCoordinate, Vec<&Path>> = HashMap::new();
    for package in &packages {
        claims
            .entry(&package.coordinate)
            .or_default()
            .push(&package.directory);
    }
    let contested = |package: &InstalledPackage| {
        claims
            .get(&package.coordinate)
            .is_some_and(|paths| paths.len() > 1)
    };
    let mut ambiguous: Vec<AmbiguousPackage> = Vec::new();
    for package in packages.iter().filter(|package| contested(package)) {
        if ambiguous
            .iter()
            .any(|entry| entry.coordinate == package.coordinate)
        {
            continue;
        }
        ambiguous.push(AmbiguousPackage {
            coordinate: package.coordinate.clone(),
            paths: claims
                .get(&package.coordinate)
                .map(|paths| paths.iter().map(|path| path.to_path_buf()).collect())
                .unwrap_or_default(),
        });
    }
    let selected = packages
        .iter()
        .filter(|package| !contested(package))
        .cloned()
        .collect();
    (selected, ambiguous)
}

/// Parse `<owner>/<version>` directory names into a coordinate.
fn coordinate_of(owner: &Path, version: &Path) -> Option<PackageCoordinate> {
    let owner_name = owner.file_name()?.to_str()?;
    let version_name = version.file_name()?.to_str()?;
    PackageCoordinate::parse(owner_name, version_name).ok()
}

/// Immediate subdirectories of `path`, in a deterministic order.
fn read_directories(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut directories: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    directories.sort();
    directories
}

#[cfg(test)]
#[path = "plugin_inventory_tests.rs"]
mod tests;
