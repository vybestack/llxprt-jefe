//! Launch-signature version 1 (issue #382 CW-02).
//!
//! Signature v1 hashes type id, definition SHA-256, launch-signature fields,
//! and target fingerprint. It excludes secrets and display-only fields.
//! Restore requires matching signature plus live tmux/process evidence.

use serde::{Deserialize, Serialize};

use super::sha256::DefinitionSha256;

/// Versioned launch signature over definition, values, and target identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LaunchSignature {
    /// Signature version (always 1 for v1).
    pub version: u64,
    /// Content digest of the agent definition.
    #[serde(default)]
    pub definition_hash: DefinitionSha256,
    /// Content digest of the typed launch-signature field values.
    #[serde(default)]
    pub typed_value_hash: DefinitionSha256,
    /// Content digest of the target fingerprint.
    #[serde(default)]
    pub target_fingerprint: DefinitionSha256,
}

impl LaunchSignature {
    /// Current signature version.
    pub const VERSION: u64 = 1;

    /// Construct a version-1 signature from its three digests.
    #[must_use]
    pub fn v1(
        definition_hash: DefinitionSha256,
        typed_value_hash: DefinitionSha256,
        target_fingerprint: DefinitionSha256,
    ) -> Self {
        Self {
            version: Self::VERSION,
            definition_hash,
            typed_value_hash,
            target_fingerprint,
        }
    }
}

#[cfg(test)]
#[path = "signature_tests.rs"]
mod tests;
