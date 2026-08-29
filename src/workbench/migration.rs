//! One-way migration from legacy screen values to stable screen identities
//! (issue #384, CW04-09).
//!
//! This module is the only place the legacy screen vocabulary is named. The
//! mapping is one-way by design: legacy values are read, translated once, and
//! never written back. Runtime code downstream of this module works exclusively
//! with [`ScreenId`], so a legacy ordinal can never leak into layout, focus, or
//! rendering decisions.
//!
//! An unrecognised legacy value is not a fatal condition — it warns and selects
//! the compiled initial screen, because a single unreadable field must not cost
//! the user the rest of their restored session.

use super::ids::ScreenIdentity;
use super::screens::ScreenRegistry;

/// Every legacy screen value, paired with the stable identity it maps to.
///
/// Each legacy value appears exactly once. This table is the migration: adding
/// a legacy value without a target, or listing one twice, fails the migration
/// matrix test.
///
/// Note that the current durable slot is an `Id`, which must start lowercase,
/// so these CamelCase names cannot appear in a document written by any shipped
/// version. The table is the one-way translation for any value that does reach
/// the reader carrying the legacy vocabulary; a value matching nothing here
/// falls back rather than being treated as a second supported encoding.
pub const LEGACY_SCREEN_VALUES: [(&str, &str); 7] = [
    ("Dashboard", "core.dashboard"),
    ("Split", "core.repositories"),
    ("DashboardIssues", "github.issues"),
    ("DashboardPullRequests", "github.pull-requests"),
    ("DashboardActions", "github.actions"),
    ("DashboardErrors", "core.errors"),
    ("DashboardTerminals", "core.terminals"),
];

/// Outcome of translating one legacy screen value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// The legacy value was recognised and mapped.
    Mapped(ScreenIdentity),
    /// The legacy value was absent, unrecognised, or names a screen the
    /// registry does not contain; the published initial screen was selected.
    FellBackToInitial(ScreenIdentity),
}

impl MigrationOutcome {
    /// The screen identity to use, whichever path produced it.
    #[must_use]
    pub const fn screen_id(self) -> ScreenIdentity {
        match self {
            Self::Mapped(id) | Self::FellBackToInitial(id) => id,
        }
    }
}

/// Translate a persisted screen value into a stable screen identity.
///
/// `persisted` is the value read from the durable document, or `None` when the
/// document does not carry one. A stable identity resolves directly; a legacy
/// variant name is translated here and nowhere else. Returns `None` only when
/// the registry is empty, which validation prevents for the shipped table.
#[must_use]
pub fn migrate_persisted_screen_value(
    persisted: Option<&str>,
    registry: &ScreenRegistry,
) -> Option<MigrationOutcome> {
    let resolved = persisted.and_then(|value| {
        registry
            .resolve(value)
            .or_else(|| resolve_legacy(value, registry))
    });

    if let Some(id) = resolved {
        return Some(MigrationOutcome::Mapped(id));
    }

    if let Some(value) = persisted {
        tracing::warn!(
            persisted_screen_value = value,
            "unrecognised persisted screen value; falling back to the initial screen"
        );
    }

    registry
        .initial_screen()
        .map(|screen| MigrationOutcome::FellBackToInitial(screen.id))
}

/// Map one legacy variant name onto its stable identity.
fn resolve_legacy(value: &str, registry: &ScreenRegistry) -> Option<ScreenIdentity> {
    LEGACY_SCREEN_VALUES
        .iter()
        .find(|(legacy_value, _)| *legacy_value == value)
        .and_then(|(_, stable)| registry.resolve(stable))
}
