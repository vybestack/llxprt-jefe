//! Lowering panel configuration and resolving bindings (issue #385, CW05-02).
//!
//! Panel configuration is the one place a definition supplies data rather than
//! structure, so its value vocabulary is narrowed on the way in. Booleans,
//! integers, strings, lists, and maps are what panel configuration is made of;
//! floats and datetimes are refused because no panel reads one, and admitting a
//! kind nothing consumes would make the contract wider than the behavior.
//!
//! There is no path by which a definition produces a secret reference. The
//! external syntax cannot spell one, and this conversion has no branch that
//! creates one, so `secret_ref` in a config table is an ordinary map key with an
//! ordinary string value and resolves to nothing.
//!
//! Diagnostics from here name keys, actions, and contexts. Those are identifiers
//! from closed grammars, not the values beside them, and an author correcting a
//! large file needs to know which one was wrong.

use crate::domain::action_registry::ActionId;
use crate::domain::input_context::ContextId;
use crate::domain::{Id, TypedMap, TypedValue};

use super::activation::{ActivationField, ActivationKind, ScreenBinding};
use super::lowering_error::LoweringError;
use super::screen_file::{ActivationField as ActivationFieldFile, ActivationKind as KindFile};

/// Convert a panel's declared configuration into the internal typed map.
///
/// # Errors
///
/// Returns [`LoweringError::ConfigKey`] naming a key outside the identifier
/// grammar, or [`LoweringError::ConfigValue`] for a value kind panel
/// configuration does not carry.
pub fn lower_config(declared: &toml::value::Table) -> Result<TypedMap, LoweringError> {
    let mut values = TypedMap::new();
    for (key, value) in declared {
        let id = Id::parse(key).map_err(|_| LoweringError::ConfigKey { key: key.clone() })?;
        values.insert(id, lower_value(value)?);
    }
    Ok(values)
}

fn lower_value(value: &toml::Value) -> Result<TypedValue, LoweringError> {
    match value {
        toml::Value::String(text) => Ok(TypedValue::String(text.clone())),
        toml::Value::Integer(number) => Ok(TypedValue::Integer(*number)),
        toml::Value::Boolean(flag) => Ok(TypedValue::Bool(*flag)),
        toml::Value::Array(elements) => Ok(TypedValue::List(
            elements
                .iter()
                .map(lower_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        toml::Value::Table(table) => Ok(TypedValue::Map(lower_config(table)?)),
        toml::Value::Float(_) => Err(LoweringError::ConfigValue { kind: "float" }),
        toml::Value::Datetime(_) => Err(LoweringError::ConfigValue { kind: "datetime" }),
    }
}

/// Lower the route activation schema a definition declares.
///
/// The schema describes shapes, never values, so nothing here can carry a
/// secret. It is lowered rather than dropped because navigation validates an
/// activation against it, and a consumer that had to re-read the file would be
/// a second parser for a grammar that has one.
///
/// # Errors
///
/// Returns [`LoweringError::ActivationName`] when a field name is not a valid
/// identifier.
pub fn lower_activation(
    declared: &[ActivationFieldFile],
) -> Result<Vec<ActivationField>, LoweringError> {
    declared
        .iter()
        .map(|field| {
            Ok(ActivationField {
                name: Id::parse(&field.name).map_err(|_| LoweringError::ActivationName {
                    name: field.name.clone(),
                })?,
                kind: lower_kind(field),
            })
        })
        .collect()
}

fn lower_kind(field: &ActivationFieldFile) -> ActivationKind {
    match field.kind {
        KindFile::Boolean => ActivationKind::Boolean,
        KindFile::OptionalBoolean => ActivationKind::OptionalBoolean,
        KindFile::String => ActivationKind::Text,
        KindFile::Integer => ActivationKind::Integer,
        // Parsing already requires the permitted list for exactly this kind, so
        // an absent one here would be a parser regression; an enum with no
        // permitted value admits nothing, which is what an empty list means.
        KindFile::Enum => ActivationKind::Enumerated {
            permitted: field.values.clone().unwrap_or_default(),
        },
        KindFile::Path => ActivationKind::Path,
        KindFile::StringList => ActivationKind::TextList,
    }
}

/// Lower the typed binding requests a definition declares.
///
/// Action existence and context membership are deliberately validated against
/// the final composed registry, after provider actions and Settings overrides
/// are known.
///
/// # Errors
///
/// Returns [`LoweringError::UnknownBinding`] when either identifier is malformed.
pub fn lower_bindings(declared: &[(&str, &str)]) -> Result<Vec<ScreenBinding>, LoweringError> {
    declared
        .iter()
        .map(|(context, action)| {
            Ok(ScreenBinding {
                context: ContextId::parse(context).map_err(|_| LoweringError::UnknownBinding {
                    field: "context",
                    declared: (*context).to_owned(),
                })?,
                action: ActionId::parse(action).map_err(|_| LoweringError::UnknownBinding {
                    field: "action",
                    declared: (*action).to_owned(),
                })?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_binding_context_is_refused_during_typed_lowering() {
        let Err(error) = lower_bindings(&[("Bad Context", "vendor.action")]) else {
            panic!("malformed context must fail typed lowering");
        };

        assert!(matches!(
            error,
            LoweringError::UnknownBinding {
                field: "context",
                declared,
            } if declared == "Bad Context"
        ));
    }

    #[test]
    fn malformed_binding_action_is_refused_during_typed_lowering() {
        let Err(error) = lower_bindings(&[("vendor.context", "Bad Action")]) else {
            panic!("malformed action must fail typed lowering");
        };

        assert!(matches!(
            error,
            LoweringError::UnknownBinding {
                field: "action",
                declared,
            } if declared == "Bad Action"
        ));
    }
}
