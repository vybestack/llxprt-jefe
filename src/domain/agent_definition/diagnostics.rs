//! Typed `AGT-E201` validation diagnostics for the closed definition contract.
//!
//! Every closed-schema rule from issue #382 has a dedicated diagnostic variant
//! so callers receive a precise reason for each rejected definition. The
//! diagnostics are pure values: they never reach into I/O and never carry
//! secret material. The public entry point is [`DefinitionError`], returned by
//! [`super::definition::AgentDefinition::from_bytes`] (strict deserialize) and
//! [`super::definition::AgentDefinition::validate`].

use std::fmt;

use super::probe::ProbeValidateError;

/// One categorized validation failure for a closed agent definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionError {
    /// Schema version is not 1.
    SchemaVersion { found: u16 },
    /// Agent-type id failed grammar or length validation.
    InvalidTypeId(super::type_id::AgentTypeIdError),
    /// Display name length is outside 1..=256 bytes.
    DisplayNameLength { bytes: usize },
    /// Candidates length is outside 1..=8.
    CandidateBounds { len: usize },
    /// A duplicate candidate kind+value pair was declared.
    DuplicateCandidate { index: usize },
    /// Repository fields exceed the per-scope bound.
    RepositoryFieldBounds { len: usize },
    /// Agent fields exceed the per-scope bound.
    AgentFieldBounds { len: usize },
    /// Combined form fields exceed the 128 total bound.
    TotalFieldBounds { len: usize },
    /// Emitters exceed the 128 bound.
    EmitterBounds { len: usize },
    /// A field-level validation failed.
    Field {
        index: usize,
        error: super::fields::FieldValidateError,
    },
    /// A duplicate field id was declared within one scope.
    DuplicateFieldId {
        scope: FieldScope,
        id: String,
        index: usize,
    },
    /// An enum field has no choices while its kind is `Enum`.
    EnumChoicesRequired { index: usize },
    /// A default value does not match the field kind.
    DefaultKindMismatch { index: usize },
    /// Integer bounds are inverted or incompatible with the field kind.
    IntegerBoundsIncompatible { index: usize },
    /// A `visible_when` reference does not resolve to a sibling field.
    UnknownVisibleWhen { index: usize, id: String },
    /// The sibling visibility graph contains a cycle.
    VisibilityCycle { path: Vec<String> },
    /// An emitter references an unknown field id.
    UnknownEmitterField { index: usize, field: String },
    /// A duplicate emitter field id was declared.
    DuplicateEmitterField { index: usize, field: String },
    /// An emitter name exceeds the path byte limit.
    EmitterNameTooLong { index: usize, bytes: usize },
    /// The probe spec failed its closed validation.
    Probe(Box<ProbeValidateError>),
    /// A capability id failed grammar validation.
    InvalidCapabilityId { index: usize, id: String },
    /// Duplicate required capability ids were declared.
    DuplicateCapability { index: usize, id: String },
    /// Required capabilities exceed the 32 bound.
    CapabilityBounds { len: usize },
    /// The serialized definition carried an unknown field (closed schema).
    UnknownField { field: String },
    /// The serialized definition carried a duplicate field.
    DuplicateJsonField { field: String },
}

/// Which scope a field belongs to (used by duplicate-field diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldScope {
    /// Repository-scope field.
    Repository,
    /// Agent-scope field.
    Agent,
}

impl fmt::Display for FieldScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Repository => "repository",
            Self::Agent => "agent",
        })
    }
}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl DefinitionError {
    fn message(&self) -> String {
        match self {
            Self::SchemaVersion { found } => {
                format!("AGT-E201: schema version must be 1, found {found}")
            }
            Self::InvalidTypeId(err) => format!("AGT-E201: invalid agent type id: {err}"),
            Self::DisplayNameLength { bytes } => display_name_length(*bytes),
            Self::CandidateBounds { len } => {
                format!("AGT-E201: candidates must be 1..=8, found {len}")
            }
            Self::DuplicateCandidate { index } => {
                format!("AGT-E201: duplicate candidate at index {index}")
            }
            Self::RepositoryFieldBounds { len } => {
                format!("AGT-E201: repository_fields must be 0..=64, found {len}")
            }
            Self::AgentFieldBounds { len } => {
                format!("AGT-E201: agent_fields must be 0..=64, found {len}")
            }
            Self::TotalFieldBounds { len } => total_field_bounds_message(*len),
            Self::EmitterBounds { len } => {
                format!("AGT-E201: emitters must be 0..=128, found {len}")
            }
            Self::Field { index, error } => {
                format!("AGT-E201: field at index {index} invalid: {error}")
            }
            Self::DuplicateFieldId { scope, id, index } => {
                format!("AGT-E201: duplicate {scope} field id {id:?} at index {index}")
            }
            Self::EnumChoicesRequired { index } => enum_choices_message(*index),
            Self::DefaultKindMismatch { index } => default_kind_mismatch(*index),
            Self::IntegerBoundsIncompatible { index } => integer_bounds_incompatible(*index),
            Self::UnknownVisibleWhen { index, id } => unknown_visible_when(*index, id),
            Self::VisibilityCycle { path } => {
                format!("AGT-E201: visibility cycle: {}", path.join(" -> "))
            }
            Self::UnknownEmitterField { index, field } => unknown_emitter_field(*index, field),
            Self::DuplicateEmitterField { index, field } => duplicate_emitter_field(*index, field),
            Self::EmitterNameTooLong { index, bytes } => {
                format!("AGT-E201: emitter name at index {index} exceeds {bytes} bytes")
            }
            Self::Probe(err) => format!("AGT-E201: probe spec invalid: {err}"),
            Self::InvalidCapabilityId { index, id } => {
                format!("AGT-E201: invalid capability id at index {index}: {id:?}")
            }
            Self::DuplicateCapability { index, id } => {
                format!("AGT-E201: duplicate capability {id:?} at index {index}")
            }
            Self::CapabilityBounds { len } => {
                format!("AGT-E201: required capabilities must be 0..=32, found {len}")
            }
            Self::UnknownField { field } => format!("AGT-E201: unknown field {field:?}"),
            Self::DuplicateJsonField { field } => {
                format!("AGT-E201: duplicate JSON field {field:?}")
            }
        }
    }
}

fn display_name_length(bytes: usize) -> String {
    format!("AGT-E201: display_name must be 1..=256 bytes, found {bytes}")
}

fn total_field_bounds_message(len: usize) -> String {
    format!("AGT-E201: combined form fields must be 0..=128, found {len}")
}

fn enum_choices_message(index: usize) -> String {
    format!("AGT-E201: enum field at index {index} requires choices")
}

fn default_kind_mismatch(index: usize) -> String {
    format!("AGT-E201: default value at index {index} does not match field kind")
}

fn integer_bounds_incompatible(index: usize) -> String {
    format!("AGT-E201: integer bounds at index {index} incompatible with field kind")
}

fn unknown_visible_when(index: usize, id: &String) -> String {
    format!("AGT-E201: field at index {index} references unknown visible_when sibling {id:?}")
}

fn unknown_emitter_field(index: usize, field: &String) -> String {
    format!("AGT-E201: emitter at index {index} references unknown field {field:?}")
}

fn duplicate_emitter_field(index: usize, field: &String) -> String {
    format!("AGT-E201: duplicate emitter field {field:?} at index {index}")
}

impl std::error::Error for DefinitionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidTypeId(err) => Some(err),
            Self::Field { error, .. } => Some(error),
            Self::Probe(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<ProbeValidateError> for DefinitionError {
    fn from(err: ProbeValidateError) -> Self {
        Self::Probe(Box::new(err))
    }
}

impl From<super::type_id::AgentTypeIdError> for DefinitionError {
    fn from(err: super::type_id::AgentTypeIdError) -> Self {
        Self::InvalidTypeId(err)
    }
}
