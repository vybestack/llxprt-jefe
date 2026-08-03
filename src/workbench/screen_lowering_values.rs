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

use crate::domain::action_registry::{Action, ActionId};
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

/// The actions the compiled inventory publishes.
///
/// Compiling the inventory is not free, so a screen builds it once and every
/// binding it declares resolves against that one copy.
///
/// # Errors
///
/// Returns [`LoweringError::UnknownBinding`] when the compiled inventory cannot
/// be built, which is a fault in this program rather than in the definition.
pub fn published_actions() -> Result<Vec<Action>, LoweringError> {
    crate::domain::default_action_inventory::compiled_inventory()
        .map(|inventory| inventory.actions)
        .map_err(|_| LoweringError::UnknownBinding {
            field: "action",
            declared: String::new(),
        })
}

/// Check that one binding names an action and a context the inventory
/// publishes.
///
/// The inventory is the sole authority for what actions exist, and a definition
/// resolves against it rather than declaring anything of its own, so a
/// definition can request an action but never invent one.
///
/// # Errors
///
/// Returns [`LoweringError::UnknownBinding`] naming the unresolvable half.
pub fn resolve_binding(
    published: &[Action],
    context: &str,
    action: &str,
) -> Result<(), LoweringError> {
    let declared = published
        .iter()
        .find(|candidate| candidate.id.as_str() == action)
        .ok_or_else(|| LoweringError::UnknownBinding {
            field: "action",
            declared: action.to_owned(),
        })?;
    if declared
        .contexts
        .iter()
        .any(|declared_context| declared_context.as_str() == context)
    {
        return Ok(());
    }
    Err(LoweringError::UnknownBinding {
        field: "context",
        declared: context.to_owned(),
    })
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

/// Lower the binding requests a definition declares, resolving each against the
/// immutable inventory.
///
/// # Errors
///
/// Returns [`LoweringError::UnknownBinding`] naming the unresolvable half.
pub fn lower_bindings(
    published: &[Action],
    declared: &[(&str, &str)],
) -> Result<Vec<ScreenBinding>, LoweringError> {
    declared
        .iter()
        .map(|(context, action)| {
            resolve_binding(published, context, action)?;
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
