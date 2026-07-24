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
    Ok(catalog)
}

const BUILTIN_OWNERS: [(&str, OwnerKind); 9] = [
    ("core.code-puppy", OwnerKind::Agent),
    ("core.dashboard", OwnerKind::Screen),
    ("core.errors", OwnerKind::Screen),
    ("core.llxprt", OwnerKind::Agent),
    ("core.repositories", OwnerKind::Screen),
    ("core.terminals", OwnerKind::Screen),
    ("github.actions", OwnerKind::Screen),
    ("github.issues", OwnerKind::Screen),
    ("github.pull-requests", OwnerKind::Screen),
];
