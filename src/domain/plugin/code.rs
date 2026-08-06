//! Stable `PLG-Ennn` diagnostic codes for the plugin package subsystem
//! (issue #389 CW-09, acceptance row D9).
//!
//! These codes are the operator-visible contract shared by the inventory, the
//! install adapter, the CLI, and the Plugins UI: the same condition renders the
//! same code in a list row, a command's stderr, and a recovery panel. The text
//! is stable, so it is asserted by goldens rather than reformatted at each call
//! site.
//!
//! Only conditions that CW-09 requires an operator to recognize across layers
//! carry a code. Purely local validation reasons stay typed reason enums on
//! their own error types; they do not inflate this taxonomy.

use std::fmt;

/// A stable operator-visible plugin diagnostic code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PluginCode {
    /// `PLG-E501` — two physically distinct packages claim one
    /// `(plugin id, canonical version)` identity.
    ///
    /// Root precedence never resolves this collision, so neither package is
    /// selected and neither publishes.
    Ambiguous,
    /// `PLG-E503` — an install committed its atomic rename but the final
    /// parent sync did not confirm.
    ///
    /// The durable result is indeterminate, so the inventory is rescanned from
    /// the physical tree and nothing is ever overwritten to "fix" it.
    IndeterminateCommit,
}

impl PluginCode {
    /// Every code, in ascending numeric order.
    pub const ALL: [Self; 2] = [Self::Ambiguous, Self::IndeterminateCommit];

    /// The exact stable code text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ambiguous => "PLG-E501",
            Self::IndeterminateCommit => "PLG-E503",
        }
    }

    /// Short operator-facing summary of the condition.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::Ambiguous => {
                "two physically distinct packages claim one id and version; neither is selected"
            }
            Self::IndeterminateCommit => {
                "the install renamed but its final parent sync is unconfirmed; rescan before retrying"
            }
        }
    }
}

impl fmt::Display for PluginCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
