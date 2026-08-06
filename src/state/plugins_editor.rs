//! Pure projection of the plugin inventory for the Settings Plugins section
//! (issue #389 CW-09, acceptance rows U1–U3).
//!
//! The section is a presenter over an immutable snapshot. It never scans a
//! root, never installs, never writes, and never starts a provider: the
//! snapshot is bound when the screen opens and the rows are a function of it
//! plus the live draft. A scan finishing underneath the screen must not make
//! the list move while the operator is choosing from it.
//!
//! Every state the section can show is a distinct [`PluginRowState`] rather
//! than a colour, so the seven required renderings differ in text and remain
//! legible without colour.

use std::fmt;

use crate::domain::plugin::PluginCode;

/// What one Plugins row is reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginRowState {
    /// Installed and usable here.
    Installed,
    /// Installed and valid, but no provider binary matches this host.
    UnsupportedPlatform {
        /// Which host has no binary.
        reason: String,
    },
    /// Two physically distinct packages claim this identity.
    Ambiguous {
        /// The stable operator-visible code.
        code: PluginCode,
        /// The physical paths that collide.
        paths: Vec<String>,
    },
    /// Installed but unusable, with the reason it cannot be read.
    Unavailable {
        /// Why the package cannot be used.
        reason: String,
    },
}

impl PluginRowState {
    /// The short status text this state shows, without colour.
    #[must_use]
    pub fn status(&self) -> String {
        match self {
            Self::Installed => "installed".to_owned(),
            Self::UnsupportedPlatform { .. } => "Unsupported platform".to_owned(),
            Self::Ambiguous { code, .. } => format!("Ambiguous {code}"),
            Self::Unavailable { .. } => "unavailable".to_owned(),
        }
    }

    /// The second line explaining this state, when it has one.
    #[must_use]
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::Installed => None,
            Self::UnsupportedPlatform { reason } | Self::Unavailable { reason } => {
                Some(reason.clone())
            }
            Self::Ambiguous { paths, .. } => {
                Some(format!("{} physical package paths", paths.len()))
            }
        }
    }

    /// Whether trust can be granted to this row at all.
    ///
    /// An ambiguous or unreadable package cannot be trusted, because there is
    /// no single thing to trust.
    #[must_use]
    pub const fn is_selectable(&self) -> bool {
        matches!(self, Self::Installed | Self::UnsupportedPlatform { .. })
    }
}

/// One immutable snapshot row the Plugins section projects from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSnapshotRow {
    /// The package identifier.
    pub id: String,
    /// The operator-facing name, when a manifest could be read.
    pub display_name: String,
    /// The version this row is about.
    pub version: String,
    /// Every installed version of this package, in listing order.
    pub versions: Vec<String>,
    /// Which root the package was selected from.
    pub root: String,
    /// What this row is reporting.
    pub state: PluginRowState,
}

/// One rendered Plugins row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRow {
    /// The package identifier.
    pub id: String,
    /// The label: name and version.
    pub label: String,
    /// Whether the draft currently trusts this package.
    pub enabled: bool,
    /// The status text.
    pub status: String,
    /// The explanatory second line, when there is one.
    pub detail: Option<String>,
    /// Every installed version, for the exact-version chooser.
    pub versions: Vec<String>,
    /// Whether trust can be toggled on this row.
    pub selectable: bool,
}

impl fmt::Display for PluginRow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {} {}",
            self.label,
            if self.enabled { "enabled" } else { "disabled" },
            self.status
        )
    }
}

/// Project the Plugins rows from an immutable snapshot plus the live draft.
///
/// `trusted` answers whether the draft currently enables a package id, so an
/// unsaved toggle shows immediately without the projection reading settings
/// itself.
#[must_use]
pub fn project_plugins(
    snapshot: &[PluginSnapshotRow],
    trusted: &dyn Fn(&str) -> bool,
) -> Vec<PluginRow> {
    snapshot
        .iter()
        .map(|row| {
            let selectable = row.state.is_selectable();
            PluginRow {
                id: row.id.clone(),
                label: format!("{} {}", row.display_name, row.version),
                // A package that cannot be selected is never shown as trusted,
                // because trusting it could not take effect.
                enabled: selectable && trusted(&row.id),
                status: row.state.status(),
                detail: row.state.detail(),
                versions: row.versions.clone(),
                selectable,
            }
        })
        .collect()
}

/// The trust confirmation an operator must accept before a package may run.
///
/// The wording is deliberately concrete about the consequence: the provider is
/// not sandboxed, and it runs with the operator's own privileges.
pub const TRUST_CONFIRMATION: &str = "Provider runs unsandboxed as you.";

/// The recovery line shown when a selected package is broken.
///
/// It states the process count explicitly, because the whole point of the
/// recovery state is that a broken package did not cause anything to run.
pub const RECOVERY_PROCESS_NOTICE: &str = "provider processes started: 0";

#[cfg(test)]
#[path = "plugins_editor_tests.rs"]
mod tests;
