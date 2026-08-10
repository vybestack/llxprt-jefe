//! Pure projection of generated plugin configuration into Settings rows
//! (issue #391, acceptance rows CW11-06 and CW11-07).
//!
//! This is the iocraft-free view model for generated plugin config fields. It
//! turns one selected package's immutable [`ConfigSchema`] and the published
//! [`TypedMap`] into rows a thin renderer can draw.
//!
//! It owns no validation of its own: it delegates value and visibility checks
//! to [`crate::domain::plugin_config`], the sole authority, and reports that
//! authority's answer adjacent to each field. A secret reference is shown only
//! as its environment-variable name (or that it is unset); resolved bytes are
//! never displayed here.

use crate::domain::plugin::field::{Field, FieldKind, RestartScope, Scalar};
use crate::domain::plugin::surface::ConfigSchema;
use crate::domain::plugin_config::{ConfigValueErrorKind, field_visible, validate_config};
use crate::domain::{Id, TypedMap, TypedValue};

/// One field's rendered control, tagged by what the field lets the user do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginConfigControl {
    /// A boolean field, toggled in place.
    Boolean {
        /// The value the draft currently describes.
        value: bool,
    },
    /// A free scalar (string, integer, finite-number, path, or string-list)
    /// whose current value is shown. Inline free-text editing is the file or a
    /// later property integration; the row never invents a parallel editor.
    Scalar {
        /// The value rendered to text, empty when none is set.
        value: String,
    },
    /// An enum choosing from declared options.
    Enum {
        /// The currently selected choice, when one is set.
        selected: Option<String>,
        /// The declared choices rendered to text.
        choices: Vec<String>,
    },
    /// A secret reference showing its environment-variable name, or that it is
    /// unset. Resolved secret bytes are never carried here.
    SecretReference {
        /// Whether a reference is currently set.
        set: bool,
        /// The environment-variable name, when a reference is set.
        env: Option<String>,
    },
    /// A field hidden by its visibility gate.
    Hidden,
}

/// One field's adjacent validation, when its value is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginConfigError {
    /// The stable reason the config validator reported.
    pub reason: ConfigValueErrorKind,
}

/// One rendered config field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginConfigRow {
    /// The field identifier.
    pub field_id: Id,
    /// The operator-facing label.
    pub label: String,
    /// The longer description, if any.
    pub description: Option<String>,
    /// What the field holds.
    pub kind: FieldKind,
    /// Whether a value must be supplied.
    pub required: bool,
    /// The declared default rendered to text, if any.
    pub default: Option<String>,
    /// The inclusive lower bound rendered to text, if any.
    pub min: Option<String>,
    /// The inclusive upper bound rendered to text, if any.
    pub max: Option<String>,
    /// The declared enum choices rendered to text.
    pub choices: Vec<String>,
    /// Whether list entries must be distinct.
    pub unique: bool,
    /// Whether the field is currently visible under its gate.
    pub visible: bool,
    /// What must restart for a change to take effect.
    pub restart: RestartScope,
    /// The control this field renders.
    pub control: PluginConfigControl,
    /// The field's adjacent validation, when its value is invalid.
    pub error: Option<PluginConfigError>,
}

/// Project one selected owner's config schema and published values into rows.
///
/// The schema is the selected installed package's immutable declaration; the
/// values are the published [`TypedMap`] for that owner. Dormant, absent, or
/// disabled owners never reach this projection: their bytes are preserved
/// untouched elsewhere, and only an active selected owner's config is
/// projected and validated.
#[must_use]
pub fn project_plugin_config(schema: &ConfigSchema, values: &TypedMap) -> Vec<PluginConfigRow> {
    let errors = validate_config(schema, values);
    schema
        .fields()
        .iter()
        .map(|field| {
            let error = errors
                .iter()
                .find(|error| error.field == *field.id())
                .map(|error| PluginConfigError {
                    reason: error.reason,
                });
            project_field(field, schema.fields(), values, error)
        })
        .collect()
}

/// Project one field.
fn project_field(
    field: &Field,
    fields: &[Field],
    values: &TypedMap,
    error: Option<PluginConfigError>,
) -> PluginConfigRow {
    let visible = field_visible(field, fields, values);
    let value = values.get(field.id());
    let control = if visible {
        control_for(field, value)
    } else {
        PluginConfigControl::Hidden
    };
    PluginConfigRow {
        field_id: field.id().clone(),
        label: field.label().to_owned(),
        description: field.description().map(ToOwned::to_owned),
        kind: field.kind(),
        required: field.required(),
        default: field.default().map(typed_value_text),
        min: field.min().map(scalar_text),
        max: field.max().map(scalar_text),
        choices: field.choices().iter().map(scalar_text).collect(),
        unique: field.unique(),
        visible,
        restart: field.restart(),
        control,
        error,
    }
}

/// Build the control one field renders for one value.
fn control_for(field: &Field, value: Option<&TypedValue>) -> PluginConfigControl {
    match field.kind() {
        FieldKind::Boolean => PluginConfigControl::Boolean {
            value: value
                .and_then(|value| match value {
                    TypedValue::Bool(value) => Some(*value),
                    _ => None,
                })
                .or_else(|| match field.default() {
                    Some(TypedValue::Bool(value)) => Some(*value),
                    _ => None,
                })
                .unwrap_or(false),
        },
        FieldKind::Enum => PluginConfigControl::Enum {
            selected: value.and_then(|value| match value {
                TypedValue::String(value) => Some(value.clone()),
                _ => None,
            }),
            choices: field.choices().iter().map(scalar_text).collect(),
        },
        FieldKind::SecretReference => match value {
            Some(TypedValue::SecretRef(reference)) => PluginConfigControl::SecretReference {
                set: true,
                env: Some(reference.env.env().to_owned()),
            },
            _ => PluginConfigControl::SecretReference {
                set: false,
                env: None,
            },
        },
        FieldKind::String
        | FieldKind::Integer
        | FieldKind::FiniteNumber
        | FieldKind::Path
        | FieldKind::StringList => PluginConfigControl::Scalar {
            value: value.map(typed_value_text).unwrap_or_default(),
        },
    }
}

/// Render one scalar declaration to display text.
fn scalar_text(scalar: &Scalar) -> String {
    match scalar {
        Scalar::Bool(value) => value.to_string(),
        Scalar::Integer(value) => value.to_string(),
        Scalar::Decimal(value) => value.as_str().to_owned(),
        Scalar::Text(value) => value.clone(),
    }
}

/// Render one typed value to display text for a scalar control.
///
/// A secret reference reaches this path only for a declared default; even then,
/// the projection carries only the environment-variable name.
fn typed_value_text(value: &TypedValue) -> String {
    match value {
        TypedValue::Bool(value) => value.to_string(),
        TypedValue::Integer(value) => value.to_string(),
        TypedValue::Decimal(value) => value.as_str().to_owned(),
        TypedValue::String(value) => value.clone(),
        TypedValue::List(values) => values
            .iter()
            .map(typed_value_text)
            .collect::<Vec<_>>()
            .join(", "),
        TypedValue::Datetime(value) => value.as_str().to_owned(),
        TypedValue::SecretRef(reference) => reference.env.env().to_owned(),
        TypedValue::Map(_) => String::new(),
    }
}

#[cfg(test)]
#[path = "plugin_config_view_tests.rs"]
mod tests;
