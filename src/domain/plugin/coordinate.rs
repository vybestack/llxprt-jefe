//! Exact package identity and the CW-09 listing order
//! (issue #389, acceptance rows D3 and C1).
//!
//! A [`PackageCoordinate`] is the `(plugin id, canonical version)` pair that
//! names one installed package directory, `<root>/<plugin-id>/<version>/`.
//!
//! The version is [`CanonicalSemver`], the existing closed configuration
//! contract, so a package version and a settings owner version are the same
//! validated value. That type already implements exactly the rule CW-09
//! requires: `precedence_cmp` follows SemVer 2.0.0 precedence and ignores
//! build metadata, while equality retains the original bytes. Two versions
//! differing only by build metadata therefore have equal precedence but are
//! **distinct packages** that coexist side by side and require exact
//! selection.
//!
//! Because the listing order is deliberately not a plain ascending order — it
//! is identifier ascending, then precedence **descending**, then exact version
//! bytes ascending — it is exposed as [`PackageCoordinate::listing_cmp`]
//! rather than as a surprising `Ord` implementation.

use std::cmp::Ordering;
use std::fmt;

use super::plugin_id::{PluginId, PluginIdError};
use crate::domain::{CanonicalSemver, ConfigContractError};

/// The exact identity of one installed package version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageCoordinate {
    id: PluginId,
    version: CanonicalSemver,
}

impl PackageCoordinate {
    /// Build a coordinate from already-validated parts.
    #[must_use]
    pub const fn new(id: PluginId, version: CanonicalSemver) -> Self {
        Self { id, version }
    }

    /// Parse a coordinate from the identifier and version directory names.
    ///
    /// # Errors
    ///
    /// Returns [`PackageCoordinateError`] when the identifier fails
    /// [`PluginId::parse`] or the version is not canonical SemVer.
    pub fn parse(id: &str, version: &str) -> Result<Self, PackageCoordinateError> {
        let id = PluginId::parse(id).map_err(PackageCoordinateError::Id)?;
        let version = CanonicalSemver::parse(version).map_err(PackageCoordinateError::Version)?;
        Ok(Self::new(id, version))
    }

    /// Borrow the package identifier.
    #[must_use]
    pub const fn id(&self) -> &PluginId {
        &self.id
    }

    /// Borrow the exact canonical version.
    #[must_use]
    pub const fn version(&self) -> &CanonicalSemver {
        &self.version
    }

    /// Order two coordinates the way `jefe plugin list` presents them.
    ///
    /// Identifier ascending, then SemVer precedence descending so the
    /// highest-precedence version leads, then exact version bytes ascending so
    /// build-metadata-only variants have a stable, deterministic order.
    #[must_use]
    pub fn listing_cmp(left: &Self, right: &Self) -> Ordering {
        left.id
            .cmp(&right.id)
            .then_with(|| right.version.precedence_cmp(&left.version))
            .then_with(|| left.version.as_str().cmp(right.version.as_str()))
    }
}

impl fmt::Display for PackageCoordinate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.id.as_str(), self.version.as_str())
    }
}

/// Why a package coordinate could not be built from directory names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageCoordinateError {
    /// The identifier component is not a valid plugin id.
    Id(PluginIdError),
    /// The version component is not canonical SemVer.
    Version(ConfigContractError),
}

impl fmt::Display for PackageCoordinateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(error) => error.fmt(formatter),
            Self::Version(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for PackageCoordinateError {}

#[cfg(test)]
#[path = "coordinate_tests.rs"]
mod tests;
