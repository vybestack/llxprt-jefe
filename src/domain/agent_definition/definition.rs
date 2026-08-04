//! The closed [`AgentDefinition`] and its strict deserialization (issue #382).
//!
//! This module owns the closed definition contract value type and its public
//! entry points. The heavy lifting lives in focused sibling modules:
//! - [`super::reader`] maps raw bytes to the typed value via the bounded JSON
//!   reader and rejects unknown/duplicate fields at the JSON level.
//! - [`super::validation`] checks every cross-field and graph invariant.
//! - [`super::canonical`] produces the canonical byte serialization used for
//!   the content SHA-256 digest.
//!
//! A definition deserializes only after strict validation; there is no public
//! `from_validated` bypass.

use super::fields::Emitter;
use super::fields::Field;
use super::probe::ProbeSpec;
use super::reader::read_definition;
use super::sha256::DefinitionSha256;
use super::type_id::AgentTypeId;
use super::types::{OperationMatrix, TargetMatrix};

/// Schema version of the shipped definition serialization.
pub const DEFINITION_SCHEMA: u16 = 1;

/// A validated, immutable agent definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDefinition {
    /// Schema version (always [`DEFINITION_SCHEMA`]).
    pub schema: u16,
    /// Stable, validated agent-type id.
    pub id: AgentTypeId,
    /// Human-readable display name (1..=256 bytes).
    pub display_name: String,
    /// Release the shipped argv mappings were authored against.
    ///
    /// Documentation only. It is displayed beside the resolved installation
    /// and is never parsed, compared, or used to gate anything (issue #657):
    /// support is decided by whether the executable can be reached, not by
    /// its version.
    pub minimum_version: String,
    /// Ordered executable candidates (1..=8).
    pub candidates: Vec<super::type_id::ExecutableCandidate>,
    /// Closed probe specification.
    pub probe: ProbeSpec,
    /// Per-operation support matrix.
    pub operations: OperationMatrix,
    /// Per-target support matrix.
    pub targets: TargetMatrix,
    /// Repository-scope form fields (0..=64).
    pub repository_fields: Vec<Field>,
    /// Agent-scope form fields (0..=64).
    pub agent_fields: Vec<Field>,
    /// Ordered argv/env emitters (0..=128).
    pub emitters: Vec<Emitter>,
}

impl AgentDefinition {
    /// Strictly deserialize and validate a definition from raw bytes.
    ///
    /// The bytes are parsed by the bounded strict JSON reader (rejecting
    /// duplicate keys, unknown fields, non-integer numbers, overlong strings,
    /// and depth/map/array bounds), mapped into the typed value, and then
    /// validated against every closed-schema rule. A definition is returned
    /// only when it strictly validates.
    ///
    /// # Errors
    ///
    /// Returns the first [`DefinitionError`] (`AGT-E201`) for any closed-schema
    /// violation.
    pub fn from_bytes(input: &[u8]) -> Result<Self, super::diagnostics::DefinitionError> {
        read_definition(input)
    }

    /// Validate this definition against every closed-schema rule.
    ///
    /// # Errors
    ///
    /// Returns the first [`DefinitionError`] for any violation.
    pub fn validate(&self) -> Result<(), super::diagnostics::DefinitionError> {
        super::validation::validate_definition(self)
    }

    /// The four shipped definitions in canonical ID order.
    ///
    /// Product tokens live only in [`super::shipped`]. Each definition carries
    /// its fixture-proven mappings and content SHA-256.
    #[must_use]
    pub fn shipped() -> Vec<Self> {
        super::shipped::shipped_definitions()
    }

    /// Content SHA-256 of this definition's canonical serialization.
    ///
    /// Canonical serialization is produced via sorted-key JSON so the digest is
    /// stable across builds. No secret or display-only value is excluded here
    /// because the definition contains neither.
    #[must_use]
    pub fn sha256(&self) -> DefinitionSha256 {
        let value = super::canonical::definition_to_json(self);
        let bytes = super::canonical::canonical_json_bytes(&value);
        DefinitionSha256::digest(&bytes)
    }
}

#[cfg(test)]
#[path = "definition_tests.rs"]
mod tests;
