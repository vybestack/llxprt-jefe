//! Pure plugin configuration validation and visibility projection (issue #391).
//!
//! This is the sole runtime authority for deciding whether typed plugin values
//! satisfy a selected manifest schema. It performs no I/O, secret resolution,
//! persistence, or UI work; Settings and panel forms consume its deterministic
//! field-scoped results.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;

use super::plugin::field::{Field, FieldKind, PATH_VALUE_BYTE_LIMIT, Scalar};
use super::plugin::surface::ConfigSchema;
use super::{Id, TypedMap, TypedValue};

/// Compute the effective configuration values by filling declared defaults.
///
/// This is the single pure rule both validation and projection consume: a
/// field without a user-supplied value but with a declared default is treated
/// as carrying its default. Secret-reference defaults remain references and
/// are never resolved here.
#[must_use]
pub fn effective_values(schema: &ConfigSchema, values: &TypedMap) -> TypedMap {
    effective_field_values(schema.fields(), values)
}

/// One validation failure adjacent to a declared field or unknown value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValueError {
    /// The field that failed validation.
    pub field: Id,
    /// Stable operator-facing reason without echoing the value.
    pub reason: ConfigValueErrorKind,
}

/// Why one typed plugin value is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigValueErrorKind {
    /// A required visible field has no value.
    Required,
    /// A value has the wrong closed field type.
    Type,
    /// A numeric value or string/list length is below `min`.
    BelowMinimum,
    /// A numeric value or string/list length is above `max`.
    AboveMaximum,
    /// An enum value is not one of the declared choices.
    Choice,
    /// A string-list requiring unique entries contains a duplicate.
    Duplicate,
    /// The map contains a field the schema does not declare.
    Unknown,
}

impl fmt::Display for ConfigValueErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Required => "required value is missing",
            Self::Type => "value has the wrong type",
            Self::BelowMinimum => "value is below the minimum",
            Self::AboveMaximum => "value is above the maximum",
            Self::Choice => "value is not an allowed choice",
            Self::Duplicate => "list contains a duplicate value",
            Self::Unknown => "field is not declared by the schema",
        })
    }
}

/// Validate a complete candidate configuration against one selected schema.
///
/// Invisible fields are preserved but are not required. Values that are present
/// are always validated, including dormant-by-visibility values, so making a
/// gate true cannot expose an already-invalid candidate. A visible required
/// field with a valid default is never reported missing: the
/// [`effective_values`] rule fills it in before the required-ness check.
#[must_use]
pub fn validate_config(schema: &ConfigSchema, values: &TypedMap) -> Vec<ConfigValueError> {
    validate_fields(schema.fields(), values)
}

/// Validate a typed map against one closed field declaration list.
///
/// Provider event argument schemas use the same field grammar as plugin config,
/// so they share this authority rather than reimplementing required/default,
/// visibility, type, and constraint rules in the panel reducer.
#[must_use]
pub fn validate_fields(fields: &[Field], values: &TypedMap) -> Vec<ConfigValueError> {
    let mut errors = Vec::new();
    for id in values.keys() {
        if !fields.iter().any(|field| field.id() == id) {
            errors.push(error(id, ConfigValueErrorKind::Unknown));
        }
    }
    let effective = effective_field_values(fields, values);
    for field in fields {
        if !effective.contains_key(field.id())
            && field.required()
            && field_visible_inner(field, fields, &effective, &mut BTreeSet::new())
        {
            errors.push(error(field.id(), ConfigValueErrorKind::Required));
            continue;
        }
        if let Some(value) = values.get(field.id())
            && let Err(reason) = validate_field_value(field, value)
        {
            errors.push(error(field.id(), reason));
        }
    }
    errors
}

/// Validate one value against one field declaration.
///
/// Used by generated Settings controls and provider-panel form events so those
/// surfaces cannot drift into separate interpretations of enum, range, or list
/// constraints.
pub fn validate_field_value(field: &Field, value: &TypedValue) -> Result<(), ConfigValueErrorKind> {
    let comparable = value_scalar(field.kind(), value)?;
    if field.kind() == FieldKind::Path
        && matches!(value, TypedValue::String(path) if path.len() > PATH_VALUE_BYTE_LIMIT)
    {
        return Err(ConfigValueErrorKind::AboveMaximum);
    }
    validate_bounds(field, value, comparable.as_ref())?;
    validate_choice(field, comparable.as_ref())?;
    validate_unique(field, value)
}

/// Whether the field is currently visible under the sibling present/truthy gate.
#[must_use]
pub fn field_visible(field: &Field, fields: &[Field], values: &TypedMap) -> bool {
    let effective = effective_field_values(fields, values);
    field_visible_inner(field, fields, &effective, &mut BTreeSet::new())
}

fn effective_field_values(fields: &[Field], values: &TypedMap) -> TypedMap {
    let mut effective = values.clone();
    for field in fields {
        if !effective.contains_key(field.id())
            && let Some(default) = field.default()
        {
            effective.insert(field.id().clone(), default.clone());
        }
    }
    effective
}

fn field_visible_inner(
    field: &Field,
    fields: &[Field],
    effective: &TypedMap,
    visiting: &mut BTreeSet<Id>,
) -> bool {
    let Some(gate_id) = field.visible_when() else {
        return true;
    };
    if !visiting.insert(field.id().clone()) {
        return false;
    }
    let visible = fields
        .iter()
        .find(|candidate| candidate.id() == gate_id)
        .is_some_and(|gate| {
            field_visible_inner(gate, fields, effective, visiting)
                && effective.get(gate_id).is_some_and(value_truthy)
        });
    visiting.remove(field.id());
    visible
}

fn error(field: &Id, reason: ConfigValueErrorKind) -> ConfigValueError {
    ConfigValueError {
        field: field.clone(),
        reason,
    }
}

fn value_scalar(
    kind: FieldKind,
    value: &TypedValue,
) -> Result<Option<Scalar>, ConfigValueErrorKind> {
    match (kind, value) {
        (FieldKind::Boolean, TypedValue::Bool(value)) => Ok(Some(Scalar::Bool(*value))),
        (FieldKind::String | FieldKind::Enum | FieldKind::Path, TypedValue::String(value)) => {
            Ok(Some(Scalar::Text(value.clone())))
        }
        (FieldKind::Integer | FieldKind::FiniteNumber, TypedValue::Integer(value)) => {
            Ok(Some(Scalar::Integer(*value)))
        }
        (FieldKind::FiniteNumber, TypedValue::Decimal(value)) => {
            Ok(Some(Scalar::Decimal(value.clone())))
        }
        (FieldKind::StringList, TypedValue::List(values))
            if values
                .iter()
                .all(|value| matches!(value, TypedValue::String(_))) =>
        {
            Ok(None)
        }
        (FieldKind::SecretReference, TypedValue::SecretRef(_)) => Ok(None),
        _ => Err(ConfigValueErrorKind::Type),
    }
}

fn validate_bounds(
    field: &Field,
    value: &TypedValue,
    scalar: Option<&Scalar>,
) -> Result<(), ConfigValueErrorKind> {
    let measured = match field.kind() {
        FieldKind::String => match value {
            TypedValue::String(value) => {
                Scalar::Integer(i64::try_from(value.len()).unwrap_or(i64::MAX))
            }
            _ => return Err(ConfigValueErrorKind::Type),
        },
        FieldKind::StringList => match value {
            TypedValue::List(values) => {
                Scalar::Integer(i64::try_from(values.len()).unwrap_or(i64::MAX))
            }
            _ => return Err(ConfigValueErrorKind::Type),
        },
        FieldKind::Integer | FieldKind::FiniteNumber => {
            scalar.cloned().ok_or(ConfigValueErrorKind::Type)?
        }
        _ => return Ok(()),
    };
    if field
        .min()
        .is_some_and(|bound| measured.numeric_cmp(bound) == Some(Ordering::Less))
    {
        return Err(ConfigValueErrorKind::BelowMinimum);
    }
    if field
        .max()
        .is_some_and(|bound| measured.numeric_cmp(bound) == Some(Ordering::Greater))
    {
        return Err(ConfigValueErrorKind::AboveMaximum);
    }
    Ok(())
}

fn validate_choice(field: &Field, scalar: Option<&Scalar>) -> Result<(), ConfigValueErrorKind> {
    if field.kind() == FieldKind::Enum
        && !scalar.is_some_and(|value| field.choices().contains(value))
    {
        return Err(ConfigValueErrorKind::Choice);
    }
    Ok(())
}

fn validate_unique(field: &Field, value: &TypedValue) -> Result<(), ConfigValueErrorKind> {
    if !field.unique() {
        return Ok(());
    }
    let TypedValue::List(values) = value else {
        return Err(ConfigValueErrorKind::Type);
    };
    for (index, value) in values.iter().enumerate() {
        if values[..index].contains(value) {
            return Err(ConfigValueErrorKind::Duplicate);
        }
    }
    Ok(())
}

fn value_truthy(value: &TypedValue) -> bool {
    match value {
        TypedValue::Bool(value) => *value,
        TypedValue::String(value) => !value.is_empty(),
        TypedValue::Integer(value) => *value != 0,
        TypedValue::Decimal(value) => value.as_str() != "0",
        TypedValue::Datetime(_) | TypedValue::SecretRef(_) => true,
        TypedValue::List(value) => !value.is_empty(),
        TypedValue::Map(value) => !value.is_empty(),
    }
}

#[cfg(test)]
#[path = "plugin_config_tests.rs"]
mod tests;
