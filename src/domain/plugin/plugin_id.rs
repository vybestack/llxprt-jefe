//! Validated plugin package identifier (issue #389 CW-09, acceptance row D2).
//!
//! A [`PluginId`] is the vendor-qualified identity of an installed package. It
//! is the directory name under a package root (`<root>/<plugin-id>/…`) and it
//! is simultaneously the configuration owner id under which the package's
//! trust and configuration persist (`plugins.<plugin-id>`).
//!
//! Because both roles must agree, the grammar and byte bound are delegated to
//! [`Id`], the existing closed configuration-identifier contract, rather than
//! restated here. `PluginId` adds only the two rules that are specific to
//! packages: an identifier must carry at least
//! [`PLUGIN_ID_MINIMUM_LABELS`] dot-separated labels, and its first label may
//! not be one of the [`RESERVED_FIRST_LABELS`] namespaces owned by this
//! executable.

use std::fmt;

use super::limits::{PLUGIN_ID_BYTE_LIMIT, PLUGIN_ID_MINIMUM_LABELS, RESERVED_FIRST_LABELS};
use crate::domain::Id;

/// A validated, vendor-qualified plugin package identifier.
///
/// Construction is via [`PluginId::parse`] only; the inner value is private so
/// an unvalidated string can never become a `PluginId`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginId(Id);

impl PluginId {
    /// Parse and validate a plugin package identifier.
    ///
    /// # Errors
    ///
    /// Returns [`PluginIdError`] when the value exceeds
    /// [`PLUGIN_ID_BYTE_LIMIT`] bytes, fails the configuration-identifier
    /// grammar, carries fewer than [`PLUGIN_ID_MINIMUM_LABELS`] labels, or
    /// claims a reserved first label.
    pub fn parse(value: &str) -> Result<Self, PluginIdError> {
        let reject = |reason| {
            Err(PluginIdError {
                raw: value.to_owned(),
                reason,
            })
        };
        if value.len() > PLUGIN_ID_BYTE_LIMIT {
            return reject(PluginIdErrorReason::Length);
        }
        let Ok(id) = Id::parse(value) else {
            return reject(PluginIdErrorReason::Grammar);
        };
        let mut labels = value.split('.');
        let Some(first) = labels.next() else {
            return reject(PluginIdErrorReason::Grammar);
        };
        if labels.count() + 1 < PLUGIN_ID_MINIMUM_LABELS {
            return reject(PluginIdErrorReason::TooFewLabels);
        }
        if RESERVED_FIRST_LABELS.contains(&first) {
            return reject(PluginIdErrorReason::ReservedPrefix);
        }
        Ok(Self(id))
    }

    /// Borrow the exact validated identifier bytes.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Borrow this identifier as the configuration owner id it also names.
    ///
    /// The package's trust and configuration persist under `plugins.<id>`, so
    /// the package identity and the settings owner identity are the same
    /// validated value rather than two parallel parses of the same text.
    #[must_use]
    pub fn owner_id(&self) -> &Id {
        &self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A rejected plugin identifier and the categorized reason it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginIdError {
    /// The raw value that failed validation.
    pub raw: String,
    /// Why the value was rejected.
    pub reason: PluginIdErrorReason,
}

/// Categorized reason a plugin identifier failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginIdErrorReason {
    /// Longer than [`PLUGIN_ID_BYTE_LIMIT`] bytes.
    Length,
    /// Empty, or outside `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`.
    Grammar,
    /// Fewer than [`PLUGIN_ID_MINIMUM_LABELS`] dot-separated labels.
    TooFewLabels,
    /// The first label is one of [`RESERVED_FIRST_LABELS`].
    ReservedPrefix,
}

impl PluginIdErrorReason {
    /// Human-readable reason text.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Length => "longer than 128 bytes",
            Self::Grammar => "not a lowercase ASCII configuration identifier",
            Self::TooFewLabels => "needs at least two dot-separated labels",
            Self::ReservedPrefix => "claims a reserved first label",
        }
    }
}

impl fmt::Display for PluginIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid plugin id {:?}: {}",
            self.raw,
            self.reason.message()
        )
    }
}

impl std::error::Error for PluginIdError {}

#[cfg(test)]
#[path = "plugin_id_tests.rs"]
mod tests;
