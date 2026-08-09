//! Static, I/O-free catalog of configuration owners built into this executable.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    CanonicalSemver, ConfigContractError, Id, OwnerCatalog, OwnerDescriptor, OwnerKind,
};

/// Build the deterministic owner catalog used by normal startup and recovery.
///
/// The catalog describes only owners compiled into this executable. It never
/// performs provider, plugin, process, or filesystem discovery.
pub fn builtin_owner_catalog() -> Result<OwnerCatalog, ConfigContractError> {
    let version = CanonicalSemver::parse(env!("CARGO_PKG_VERSION"))?;
    let mut catalog = OwnerCatalog::default();
    for (owner_id, kind) in BUILTIN_OWNERS {
        catalog.insert(OwnerDescriptor {
            owner_id: Id::parse(owner_id)?,
            version: version.clone(),
            kind,
            defaults: BTreeMap::new(),
            secret_paths: BTreeSet::new(),
        })?;
    }
    for definition in crate::domain::agent_definition::AgentDefinition::shipped() {
        catalog.insert(OwnerDescriptor {
            owner_id: Id::parse(definition.id.as_str())?,
            version: version.clone(),
            kind: OwnerKind::Agent,
            defaults: BTreeMap::new(),
            secret_paths: BTreeSet::new(),
        })?;
    }
    Ok(catalog)
}

/// Extend the compiled catalog with the packages physically installed
/// (issue #390 CW-10).
///
/// A package's trust lives at `plugins.<id>`, and an owner the catalog does not
/// know is published as dormant rather than as settings. Without this, every
/// installed package reads as untrusted no matter what the operator chose, so
/// the boundary that reads trust has to know which packages exist.
///
/// A duplicate id is skipped rather than failing the catalog: the inventory
/// already collapses versions to one selected package per id, so a duplicate
/// here would mean a compiled owner and a package share an id, and the compiled
/// owner must win.
///
/// Discovery is the caller's: this only turns already-scanned packages into
/// descriptors.
pub fn owner_catalog_with_packages(
    packages: &[crate::persistence::plugin_inventory::InstalledPackage],
) -> Result<OwnerCatalog, ConfigContractError> {
    let mut catalog = builtin_owner_catalog()?;
    for package in packages {
        let coordinate = package.coordinate();
        let descriptor = OwnerDescriptor {
            owner_id: coordinate.id().owner_id().clone(),
            version: coordinate.version().clone(),
            kind: OwnerKind::Plugin,
            defaults: BTreeMap::new(),
            secret_paths: BTreeSet::new(),
        };
        let _ = catalog.insert(descriptor);
    }
    Ok(catalog)
}

const BUILTIN_OWNERS: [(&str, OwnerKind); 8] = [
    ("core.dashboard", OwnerKind::Screen),
    ("core.errors", OwnerKind::Screen),
    ("core.repositories", OwnerKind::Screen),
    ("core.settings", OwnerKind::Screen),
    ("core.terminals", OwnerKind::Screen),
    ("github.actions", OwnerKind::Screen),
    ("github.issues", OwnerKind::Screen),
    ("github.pull-requests", OwnerKind::Screen),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_definition_is_a_published_agent_owner() {
        let catalog = builtin_owner_catalog()
            .unwrap_or_else(|error| panic!("built-in owner catalog must publish: {error}"));

        for definition in crate::domain::agent_definition::AgentDefinition::shipped() {
            let owner_id = Id::parse(definition.id.as_str())
                .unwrap_or_else(|error| panic!("definition id must be an owner id: {error}"));
            assert!(
                catalog
                    .get(&owner_id)
                    .is_some_and(|owner| owner.kind == OwnerKind::Agent),
                "{} must publish as an agent owner",
                definition.id.as_str()
            );
        }
    }
}
