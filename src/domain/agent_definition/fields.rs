//! Closed field and emitter definitions (issue #382 CW-02).
//!
//! A [`Field`] is a typed form field; an [`Emitter`] maps a typed field value
//! into an argv/env element. Generated forms and launch plans are pure
//! projections over these definitions. There is no generic JSON value, shell
//! template, token splitting, setup command, script, or raw-argument field.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::limits::{CHOICE_LIMIT, FIELD_ID_BYTE_LIMIT, STRING_VALUE_BYTE_LIMIT};

/// Kind of a typed form value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// Required boolean.
    Boolean,
    /// Optional boolean with an explicit default.
    OptionalBoolean,
    /// String value.
    String,
    /// Integer value with optional bounds.
    Integer,
    /// Enum value constrained to `choices`.
    Enum,
    /// Filesystem path value.
    Path,
    /// List of string values.
    StringList,
}

impl FieldKind {
    /// Whether this kind is numeric (Integer).
    #[must_use]
    pub const fn is_numeric(self) -> bool {
        matches!(self, Self::Integer)
    }

    /// Whether this kind carries a list value.
    #[must_use]
    pub const fn is_list(self) -> bool {
        matches!(self, Self::StringList)
    }
}

/// Default-free typed field value (the closed value sum).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    /// Boolean value (for `Boolean` and `OptionalBoolean` fields).
    Boolean(bool),
    /// Optional boolean value (for `OptionalBoolean` fields).
    OptionalBoolean(Option<bool>),
    /// String value (for `String`, `Enum`, and `Path` fields).
    String(String),
    /// Integer value (for `Integer` fields).
    Integer(i64),
    /// Path value serialized as a string (for `Path` fields).
    Path(String),
    /// List of strings (for `StringList` fields).
    StringList(Vec<String>),
}

impl FieldValue {
    /// Render as a string for argv emission.
    #[must_use]
    pub fn as_arg_string(&self) -> Option<String> {
        match self {
            Self::Boolean(b) | Self::OptionalBoolean(Some(b)) => Some(b.to_string()),
            Self::OptionalBoolean(None) | Self::StringList(_) => None,
            Self::String(s) | Self::Path(s) => Some(s.clone()),
            Self::Integer(i) => Some(i.to_string()),
        }
    }

    /// Whether this value matches the given field kind.
    #[must_use]
    pub fn matches_kind(&self, kind: FieldKind) -> bool {
        matches!(
            (self, kind),
            (Self::Boolean(_), FieldKind::Boolean)
                | (Self::OptionalBoolean(_), FieldKind::OptionalBoolean)
                | (Self::String(_), FieldKind::String | FieldKind::Enum)
                | (Self::Integer(_), FieldKind::Integer)
                | (Self::Path(_), FieldKind::Path)
                | (Self::StringList(_), FieldKind::StringList)
        )
    }
}

/// One typed form field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Field identifier, unique within its scope.
    pub id: String,
    /// Typed value kind.
    pub kind: FieldKind,
    /// Whether the field is required.
    #[serde(default)]
    pub required: bool,
    /// Optional default value, must match `kind`.
    #[serde(default)]
    pub default: Option<FieldValue>,
    /// Inclusive minimum for `Integer` fields.
    #[serde(default)]
    pub minimum: Option<i64>,
    /// Inclusive maximum for `Integer` fields.
    #[serde(default)]
    pub maximum: Option<i64>,
    /// Allowed values for `Enum` fields (0..=64).
    #[serde(default)]
    pub choices: Vec<String>,
    /// Sibling field id that gates this field's visibility.
    #[serde(default)]
    pub visible_when: Option<String>,
    /// Whether this field participates in the launch signature.
    #[serde(default)]
    pub launch_signature: bool,
}

impl Field {
    /// Validate this field against the closed bounds and consistency rules.
    ///
    /// # Errors
    ///
    /// Returns [`FieldValidateError`] for id bounds, choice bounds, enum
    /// consistency, default/kind mismatch, or integer-bounds/kind mismatch.
    pub fn validate(&self) -> Result<(), FieldValidateError> {
        if self.id.is_empty() || self.id.len() > FIELD_ID_BYTE_LIMIT {
            return Err(FieldValidateError::IdBounds {
                bytes: self.id.len(),
            });
        }
        if self.choices.len() > CHOICE_LIMIT {
            return Err(FieldValidateError::TooManyChoices {
                len: self.choices.len(),
            });
        }
        if self.kind == FieldKind::Enum && self.choices.is_empty() {
            return Err(FieldValidateError::EnumChoicesRequired);
        }
        if self.kind == FieldKind::Enum {
            for choice in &self.choices {
                if choice.is_empty() || choice.len() > STRING_VALUE_BYTE_LIMIT {
                    return Err(FieldValidateError::ChoiceLength {
                        bytes: choice.len(),
                    });
                }
            }
        }
        if let Some(default) = &self.default {
            if !default.matches_kind(self.kind) {
                return Err(FieldValidateError::DefaultKindMismatch);
            }
            if self.kind == FieldKind::Enum {
                let FieldValue::String(value) = default else {
                    return Err(FieldValidateError::DefaultKindMismatch);
                };
                if !self.choices.iter().any(|choice| choice == value) {
                    return Err(FieldValidateError::DefaultNotInChoices);
                }
            }
        }
        let has_integer_bounds = self.minimum.is_some() || self.maximum.is_some();
        if has_integer_bounds && self.kind != FieldKind::Integer {
            return Err(FieldValidateError::IntegerBoundsIncompatible);
        }
        if let (Some(min), Some(max)) = (self.minimum, self.maximum)
            && min > max
        {
            return Err(FieldValidateError::InvertedBounds { min, max });
        }
        Ok(())
    }
}

/// Field validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValidateError {
    /// Field id is outside 1..=128 bytes.
    IdBounds {
        /// Actual byte length.
        bytes: usize,
    },
    /// Choices exceed 64.
    TooManyChoices {
        /// Actual count.
        len: usize,
    },
    /// Enum field has no choices.
    EnumChoicesRequired,
    /// A choice value is outside 1..=4096 bytes.
    ChoiceLength {
        /// Actual bytes.
        bytes: usize,
    },
    /// Default value does not match the field kind.
    DefaultKindMismatch,
    /// Enum default is not in the choices list.
    DefaultNotInChoices,
    /// Integer bounds set on a non-integer field.
    IntegerBoundsIncompatible,
    /// Minimum exceeds maximum.
    InvertedBounds {
        /// The minimum.
        min: i64,
        /// The maximum.
        max: i64,
    },
}

impl fmt::Display for FieldValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdBounds { bytes } => {
                write!(f, "field id must be 1..=128 bytes, found {bytes}")
            }
            Self::TooManyChoices { len } => {
                write!(f, "field choices must be 0..=64, found {len}")
            }
            Self::EnumChoicesRequired => "enum field requires choices".fmt(f),
            Self::ChoiceLength { bytes } => {
                write!(f, "enum choice must be 1..=4096 bytes, found {bytes}")
            }
            Self::DefaultKindMismatch => "default value does not match field kind".fmt(f),
            Self::DefaultNotInChoices => "enum default is not in choices".fmt(f),
            Self::IntegerBoundsIncompatible => "integer bounds require an integer field".fmt(f),
            Self::InvertedBounds { min, max } => {
                write!(f, "minimum {min} exceeds maximum {max}")
            }
        }
    }
}

impl std::error::Error for FieldValidateError {}

/// Emitter kind: how a field value becomes an argv/env element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Emitter {
    /// A fixed argv element.
    Fixed {
        /// The literal argv element.
        value: String,
    },
    /// A boolean flag emitted only when the field is true.
    Flag {
        /// Field id to read.
        field: String,
    },
    /// An option `--name <field-value>`.
    Option {
        /// Option name (e.g. `--model`).
        name: String,
        /// Field id to read.
        field: String,
    },
    /// A boolean option with distinct true/false values.
    BooleanOption {
        /// Option name.
        name: String,
        /// Field id to read.
        field: String,
        /// Value emitted when the field is true.
        true_value: String,
        /// Value emitted when the field is false, if any.
        #[serde(default)]
        false_value: Option<String>,
    },
    /// A repeated option, one per string-list element.
    RepeatedOption {
        /// Option name.
        name: String,
        /// Field id to read.
        field: String,
    },
    /// A positional argv element from a field value.
    Positional {
        /// Field id to read.
        field: String,
    },
    /// An environment variable `name=<field-value>`.
    Environment {
        /// Environment variable name.
        name: String,
        /// Field id to read.
        field: String,
    },
}

impl Emitter {
    /// Field id referenced by this emitter, if any (fixed emitters reference
    /// none).
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Fixed { .. } => None,
            Self::Flag { field }
            | Self::Option { field, .. }
            | Self::BooleanOption { field, .. }
            | Self::RepeatedOption { field, .. }
            | Self::Positional { field }
            | Self::Environment { field, .. } => Some(field),
        }
    }

    /// Validate this emitter against the closed bounds.
    ///
    /// # Errors
    ///
    /// Returns [`EmitterValidateError`] for oversize names or empty field
    /// references.
    pub fn validate(&self) -> Result<(), EmitterValidateError> {
        match self {
            Self::Fixed { value } => validate_bounded_string(value, "emitter fixed value")?,
            Self::Flag { field } | Self::Positional { field } => {
                validate_field_ref(field)?;
            }
            Self::Option { name, field }
            | Self::RepeatedOption { name, field }
            | Self::Environment { name, field } => {
                validate_bounded_string(name, "emitter name")?;
                validate_field_ref(field)?;
            }
            Self::BooleanOption {
                name,
                field,
                true_value,
                false_value,
            } => {
                validate_bounded_string(name, "emitter name")?;
                validate_field_ref(field)?;
                validate_bounded_string(true_value, "emitter true value")?;
                if let Some(false_value) = false_value {
                    validate_bounded_string(false_value, "emitter false value")?;
                }
            }
        }
        Ok(())
    }
}

fn validate_bounded_string(value: &str, what: &str) -> Result<(), EmitterValidateError> {
    if value.is_empty() {
        return Err(EmitterValidateError::EmptyString {
            what: what.to_string(),
        });
    }
    if value.len() > STRING_VALUE_BYTE_LIMIT {
        return Err(EmitterValidateError::StringTooLong {
            what: what.to_string(),
            bytes: value.len(),
        });
    }
    Ok(())
}

fn validate_field_ref(field: &str) -> Result<(), EmitterValidateError> {
    if field.is_empty() || field.len() > FIELD_ID_BYTE_LIMIT {
        return Err(EmitterValidateError::FieldRefBounds { bytes: field.len() });
    }
    Ok(())
}

/// Emitter validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitterValidateError {
    /// A bounded string is empty.
    EmptyString {
        /// What was empty.
        what: String,
    },
    /// A bounded string exceeds the limit.
    StringTooLong {
        /// What was too long.
        what: String,
        /// Actual byte length.
        bytes: usize,
    },
    /// A field reference is outside 1..=128 bytes.
    FieldRefBounds {
        /// Actual bytes.
        bytes: usize,
    },
}

impl fmt::Display for EmitterValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyString { what } => write!(f, "{what} must not be empty"),
            Self::StringTooLong { what, bytes } => write!(f, "{what} exceeds {bytes} bytes"),
            Self::FieldRefBounds { bytes } => {
                write!(
                    f,
                    "emitter field reference must be 1..=128 bytes, found {bytes}"
                )
            }
        }
    }
}

impl std::error::Error for EmitterValidateError {}

#[cfg(test)]
#[path = "fields_tests.rs"]
mod tests;
