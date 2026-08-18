//! Why one screen definition could not become an internal descriptor
//! (issue #385, CW05-04).
//!
//! Each reason carries the classification the composition layer needs to pick a
//! `CFG` code alongside its `SCR-E301`, so the mapping from "what went wrong" to
//! "which configuration rule was broken" lives with the reasons themselves
//! rather than in a match at the call site that could drift out of step.

use crate::domain::plugin::field::FieldError;
use crate::domain::plugin::surface::ConfigSchemaError;
use crate::persistence::diagnostic::CfgCode;

use super::ids::IdError;
use super::intern::InternExhausted;
use super::panel_types::PanelTypeError;
use super::resource_schemas::ResourceSchemaError;
use super::validate::DescriptorError;

/// A screen definition that parsed but could not be lowered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweringError {
    /// The identifier table refused to admit any more distinct text.
    Interning(InternExhausted),
    /// One declared identifier violates the closed grammar.
    Identifier {
        /// Which field carried it.
        field: &'static str,
        /// The violated rule.
        reason: IdError,
    },
    /// The declared screen identity does not match the file it was declared in.
    IdentityMismatch {
        /// The identity the file name requires.
        expected: String,
    },
    /// The declared panel type was refused.
    PanelType(PanelTypeError),
    /// A binding names an action or context the immutable registry does not
    /// publish.
    UnknownBinding {
        /// Which half was unresolvable.
        field: &'static str,
        /// The unresolvable name, which is an identifier rather than a value.
        declared: String,
    },
    /// An activation field name is not a well-formed identifier.
    ActivationName {
        /// The offending name, which is an identifier rather than a value.
        name: String,
    },
    /// A resource identifier is not well formed.
    ResourceIdentifier {
        /// Which resource field carried it.
        field: &'static str,
    },
    /// One resource field declaration is inconsistent.
    ResourceField(FieldError),
    /// A resource field collection is inconsistent.
    ResourceFields(ConfigSchemaError),
    /// A resource schema is inconsistent.
    ResourceSchema(ResourceSchemaError),
    /// A port's resource owner is not a well-formed identifier.
    ResourceOwner {
        /// The invalid owner spelling.
        owner: String,
    },
    /// A schema-1 port names a type with no historical owner mapping.
    LegacyResourceOwner {
        /// The declaration's type identifier.
        type_id: String,
    },
    /// A configuration key is not a well-formed identifier.
    ConfigKey {
        /// The offending key, which is a name rather than a value.
        key: String,
    },
    /// A configuration value is of a kind panel configuration does not carry.
    ConfigValue {
        /// The offending TOML kind.
        kind: &'static str,
    },
    /// A relationship endpoint is not spelled `<panel>.<port>`.
    PortReference,
    /// A size, minimum, or maximum reached lowering as zero, which parsing
    /// should already have refused.
    ZeroExtent {
        /// Which field carried it.
        field: &'static str,
    },
    /// The lowered descriptor violates a structural invariant.
    Descriptor(DescriptorError),
}

impl LoweringError {
    /// Which configuration rule family this failure belongs to.
    ///
    /// Ownership failures — a definition claiming an identity or a panel type
    /// that is not its to claim — are `CFG-E005`. Everything else is a
    /// reference or bound failure, which is `CFG-E006`.
    #[must_use]
    pub const fn cfg_code(&self) -> CfgCode {
        match self {
            Self::IdentityMismatch { .. } | Self::PanelType(_) => CfgCode::E005,
            _ => CfgCode::E006,
        }
    }
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interning(error) => write!(formatter, "{error}"),
            Self::Identifier { field, reason } => write!(formatter, "{field}: {reason}"),
            Self::IdentityMismatch { expected } => {
                write!(formatter, "declared screen identity must be {expected:?}")
            }
            Self::PanelType(error) => write!(formatter, "{error}"),
            Self::UnknownBinding { field, declared } => write!(
                formatter,
                "bindings.{field} {declared:?} is not published by the registry"
            ),
            Self::ActivationName { name } => write!(
                formatter,
                "activation field name {name:?} is not a valid identifier"
            ),
            Self::ResourceIdentifier { field } => {
                write!(formatter, "{field} is not a valid resource identifier")
            }
            Self::ResourceField(error) => write!(formatter, "resource field: {error}"),
            Self::ResourceFields(error) => write!(formatter, "resource fields: {error}"),
            Self::ResourceSchema(error) => write!(formatter, "resource schema: {error}"),
            Self::ResourceOwner { owner } => {
                write!(
                    formatter,
                    "resource owner {owner:?} is not a valid identifier"
                )
            }
            Self::LegacyResourceOwner { type_id } => write!(
                formatter,
                "schema-1 resource type {type_id:?} has no historical owner mapping"
            ),
            Self::ConfigKey { key } => {
                write!(formatter, "config key {key:?} is not a valid identifier")
            }
            Self::ConfigValue { kind } => {
                write!(formatter, "panel config does not carry {kind} values")
            }
            Self::PortReference => {
                formatter.write_str("port reference must be spelled '<panel>.<port>'")
            }
            Self::ZeroExtent { field } => write!(formatter, "{field} must be at least 1"),
            Self::Descriptor(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for LoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Interning(error) => Some(error),
            Self::PanelType(error) => Some(error),
            Self::ResourceField(error) => Some(error),
            Self::ResourceFields(error) => Some(error),
            Self::ResourceSchema(error) => Some(error),
            Self::Descriptor(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InternExhausted> for LoweringError {
    fn from(error: InternExhausted) -> Self {
        Self::Interning(error)
    }
}

impl From<PanelTypeError> for LoweringError {
    fn from(error: PanelTypeError) -> Self {
        Self::PanelType(error)
    }
}

impl From<DescriptorError> for LoweringError {
    fn from(error: DescriptorError) -> Self {
        Self::Descriptor(error)
    }
}
