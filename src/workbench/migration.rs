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

use super::ids::ScreenId;
use super::screens::ScreenRegistry;

/// Every legacy screen value, paired with the stable identity it maps to.
///
/// Each legacy value appears exactly once. This table is the migration: adding
/// a legacy value without a target, or listing one twice, fails the migration
/// matrix test.
pub const LEGACY_SCREEN_VALUES: [(&str, &str); 5] = [
    ("Dashboard", "core.dashboard"),
    ("Split", "core.repositories"),
    ("DashboardIssues", "github.issues"),
    ("DashboardPullRequests", "github.pull-requests"),
    ("DashboardActions", "github.actions"),
];

/// Outcome of translating one legacy screen value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// The legacy value was recognised and mapped.
    Mapped(ScreenId),
    /// The legacy value was absent, unrecognised, or names a screen the
    /// registry does not contain; the compiled initial screen was selected.
    FellBackToInitial(ScreenId),
}

impl MigrationOutcome {
    /// The screen identity to use, whichever path produced it.
    #[must_use]
    pub const fn screen_id(&self) -> &ScreenId {
        match self {
            Self::Mapped(id) | Self::FellBackToInitial(id) => id,
        }
    }
}

/// Translate one legacy screen value into a stable screen identity.
///
/// `legacy` is the persisted value, or `None` when the document predates the
/// field. Returns `None` only when the registry itself is empty, which
/// validation prevents for the shipped table.
#[must_use]
pub fn migrate_legacy_screen_value(
    legacy: Option<&str>,
    registry: &ScreenRegistry,
) -> Option<MigrationOutcome> {
    let mapped = legacy.and_then(|value| {
        LEGACY_SCREEN_VALUES
            .iter()
            .find(|(legacy_value, _)| *legacy_value == value)
            .map(|(_, stable)| *stable)
    });

    if let Some(stable) = mapped
        && let Ok(id) = ScreenId::parse(stable)
        && registry.get(&id).is_some()
    {
        return Some(MigrationOutcome::Mapped(id));
    }

    if let Some(value) = legacy {
        tracing::warn!(
            legacy_screen_value = value,
            "unrecognised persisted screen value; falling back to the initial screen"
        );
    }

    registry
        .initial_screen()
        .map(|screen| MigrationOutcome::FellBackToInitial(screen.id.clone()))
}
