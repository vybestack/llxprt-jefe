//! Immutable, validated agent-type registry published at the composition
//! boundary (issue #382 CW-02 S2).
//!
//! One [`AgentTypeRegistry`] is constructed once from a set of closed
//! [`AgentDefinition`] values, validated against every closed-schema rule, and
//! then frozen. The registry never mutates after publication: probe
//! generations, availability, and runtime state live in the layers above
//! (S3/S4 and beyond), never inside the registry. Product knowledge lives only
//! in the shipped definition data; this module contains no product tokens.
//!
//! The registry's responsibilities are intentionally narrow:
//! - validate every definition at publication (no later bypass);
//! - expose definitions in canonical (bytewise-stable) ID order;
//! - look up a definition by stable [`AgentTypeId`];
//! - reject duplicate type ids at publication.
//!
//! It owns no `AppState`, no PATH snapshot, and no process spawning. The
//! candidate resolver ([`crate::agent_candidate`]) consumes a borrowed slice
//! of `AgentDefinition`; the registry hands out that slice.

use std::collections::HashMap;

use crate::domain::agent_definition::AgentDefinition;
use crate::domain::agent_definition::definition::DEFINITION_SCHEMA;
use crate::domain::agent_definition::diagnostics::DefinitionError;
use crate::domain::agent_definition::type_id::AgentTypeId;

/// Immutable, validated registry of agent-type definitions.
///
/// Constructed via [`AgentTypeRegistry::publish`] (strict validation) or
/// [`AgentTypeRegistry::shipped`] (the four shipped definitions). Once built it
/// is read-only for its entire lifetime: there is no insert/update/remove.
#[derive(Debug, Clone)]
pub struct AgentTypeRegistry {
    definitions: Vec<AgentDefinition>,
    by_id: HashMap<RegistryKey, usize>,
}

/// Stable hashable key wrapping an `AgentTypeId` for the lookup index.
///
/// `AgentTypeId` already implements `Hash`/`Eq`; this newtype keeps the index
/// private and avoids leaking `HashMap` internals through the public API.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RegistryKey(AgentTypeId);

impl AgentTypeRegistry {
    /// Publish a registry from the given definitions after strict validation.
    ///
    /// Definitions are re-validated against every closed-schema rule and stored
    /// in canonical (bytewise-stable) ID order. Duplicate type ids are
    /// rejected. The returned registry is frozen.
    ///
    /// # Errors
    ///
    /// Returns the first [`DefinitionError`] for any closed-schema violation,
    /// including a duplicate type id (`AGT-E201`).
    pub fn publish(definitions: Vec<AgentDefinition>) -> Result<Self, RegistryPublishError> {
        let mut sorted = definitions;
        sorted.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

        let mut by_id: HashMap<RegistryKey, usize> = HashMap::with_capacity(sorted.len());
        for (index, definition) in sorted.iter().enumerate() {
            if let Err(error) = definition.validate() {
                return Err(RegistryPublishError::Definition(error));
            }
            let key = RegistryKey(definition.id.clone());
            if by_id.insert(key, index).is_some() {
                return Err(RegistryPublishError::DuplicateTypeId {
                    id: definition.id.as_str().to_string(),
                });
            }
            if definition.schema != DEFINITION_SCHEMA {
                return Err(RegistryPublishError::Definition(
                    DefinitionError::SchemaVersion {
                        found: definition.schema,
                    },
                ));
            }
        }
        Ok(Self {
            definitions: sorted,
            by_id,
        })
    }

    /// Publish the four shipped definitions (the only product-token source).
    ///
    /// Equivalent to `AgentTypeRegistry::publish(AgentDefinition::shipped())`
    /// but documents that this is the composition-root entry point.
    ///
    /// # Errors
    ///
    /// Returns a [`RegistryPublishError`] only if the shipped data fails
    /// validation, which would be a programming error in the allowlisted
    /// shipped-data module.
    pub fn shipped() -> Result<Self, RegistryPublishError> {
        Self::publish(AgentDefinition::shipped())
    }

    /// Borrow the definitions in canonical (bytewise-stable) ID order.
    #[must_use]
    pub fn definitions(&self) -> &[AgentDefinition] {
        &self.definitions
    }

    /// Number of published definitions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// Look up a definition by stable agent-type id.
    #[must_use]
    pub fn get(&self, id: &AgentTypeId) -> Option<&AgentDefinition> {
        self.by_id
            .get(&RegistryKey(id.clone()))
            .map(|&i| &self.definitions[i])
    }

    /// Borrow the definition at the given canonical index.
    #[must_use]
    pub fn at(&self, index: usize) -> Option<&AgentDefinition> {
        self.definitions.get(index)
    }
}

/// Whether published settings offer one agent type.
///
/// This is the registry's rule and lives here so that startup composition and
/// the Settings editor cannot answer it differently. An unmentioned type is
/// offered: settings are sparse, and a document that says nothing about a type
/// is saying it is happy with the shipped answer. An identity the
/// configuration grammar cannot even spell is likewise offered rather than
/// silently withdrawn — the definition itself is what declares the type, and
/// nothing in the document contradicts it.
#[must_use]
pub fn agent_type_enabled(
    settings: &crate::persistence::settings_document::PublishedSettings,
    type_id: &AgentTypeId,
) -> bool {
    let Ok(owner_id) = crate::domain::Id::parse(type_id.as_str()) else {
        return true;
    };
    settings
        .agents
        .get(&owner_id)
        .and_then(|owner| owner.enabled)
        .unwrap_or(true)
}

/// Error returned when registry publication fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryPublishError {
    /// A definition failed closed-schema validation (`AGT-E201`).
    Definition(DefinitionError),
    /// Two definitions share a stable type id (`AGT-E201`).
    DuplicateTypeId {
        /// The duplicated id text.
        id: String,
    },
}

impl std::fmt::Display for RegistryPublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definition(error) => write!(f, "registry definition invalid: {error}"),
            Self::DuplicateTypeId { id } => {
                write!(f, "registry duplicate type id: {id}")
            }
        }
    }
}

impl std::error::Error for RegistryPublishError {}

impl From<DefinitionError> for RegistryPublishError {
    fn from(error: DefinitionError) -> Self {
        Self::Definition(error)
    }
}

#[cfg(test)]
#[path = "agent_registry_tests.rs"]
mod tests;
