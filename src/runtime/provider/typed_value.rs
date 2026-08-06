//! Typed-value, field-declaration, and environment-name mapping for the
//! action-provider protocol readers (issue #390 CW-10, Slice A).
//!
//! This module maps the shared bounded reader's ordered tree onto the closed
//! domain value types ([`TypedValue`], [`TypedMap`], [`SecretRef`]), the
//! continuation-schema [`Field`] declarations, and the environment-name-keyed
//! string maps of the `configure` payload. It composes the generic primitives
//! in [`super::object_reader`] and carries no process, state, effect, or
//! persistence.

use std::collections::BTreeMap;

use crate::domain::bounded_json::BoundedJson;
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope, Scalar};
use crate::domain::plugin::limits::FIELD_CHOICE_LIMIT;
use crate::domain::{CanonicalDateTime, CanonicalDecimal, Id, SecretRef, TypedMap, TypedValue};

use super::error::ProviderError;
use super::identifiers::EnvName;
use super::object_reader::{
    array, closed_object, find, read_bool, read_enum, read_id, read_parsed_string, read_string,
    require, type_mismatch,
};

/// Maximum elements in one typed-value list (matches the framing array bound).
const TYPED_LIST_ELEMENT_LIMIT: usize = 4096;

const TYPED_VALUE_KEYS: [&str; 2] = ["type", "value"];
const SECRET_REF_KEYS: [&str; 1] = ["id"];
const FIELD_DECLARATION_KEYS: [&str; 9] = [
    "id",
    "kind",
    "required",
    "default",
    "minimum",
    "maximum",
    "choices",
    "visible_when",
    "restart",
];

/// Map a bounded object onto a closed typed map.
pub(super) fn read_typed_map(value: &BoundedJson, path: &str) -> Result<TypedMap, ProviderError> {
    let members = value
        .as_object()
        .ok_or_else(|| type_mismatch(path, "object"))?;
    let mut map = TypedMap::new();
    for (key, entry) in members {
        let id = Id::parse(key).map_err(|error| ProviderError::InvalidValue {
            path: format!("{path}.{key}"),
            reason: error.to_string(),
        })?;
        let typed = read_typed_value(entry, &format!("{path}.{key}"))?;
        map.insert(id, typed);
    }
    Ok(map)
}

/// Map a bounded object `{type, value}` onto a closed typed value.
fn read_typed_value(value: &BoundedJson, path: &str) -> Result<TypedValue, ProviderError> {
    let members = closed_object(value, path, &TYPED_VALUE_KEYS)?;
    let kind = read_string(members, path, "type")?;
    let content_path = format!("{path}.value");
    let content = require(members, path, "value")?;
    match kind {
        "string" => content
            .as_str()
            .map(|text| TypedValue::String(text.to_owned()))
            .ok_or_else(|| type_mismatch(&content_path, "string")),
        "bool" => content
            .as_bool()
            .map(TypedValue::Bool)
            .ok_or_else(|| type_mismatch(&content_path, "boolean")),
        "integer" => content
            .as_int()
            .map(TypedValue::Integer)
            .ok_or_else(|| type_mismatch(&content_path, "integer")),
        "decimal" => read_parsed_string(content, &content_path, CanonicalDecimal::parse)
            .map(TypedValue::Decimal),
        "datetime" => read_parsed_string(content, &content_path, CanonicalDateTime::parse)
            .map(TypedValue::Datetime),
        "list" => {
            let elements = array(content, &content_path, TYPED_LIST_ELEMENT_LIMIT)?;
            elements
                .iter()
                .enumerate()
                .map(|(index, element)| {
                    read_typed_value(element, &format!("{content_path}[{index}]"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(TypedValue::List)
        }
        "map" => read_typed_map(content, &content_path).map(TypedValue::Map),
        "secret_ref" => read_secret_ref(content, &content_path).map(TypedValue::SecretRef),
        other => Err(ProviderError::UnknownValue {
            path: format!("{path}.type"),
            value: other.to_owned(),
        }),
    }
}

fn read_secret_ref(value: &BoundedJson, path: &str) -> Result<SecretRef, ProviderError> {
    let members = closed_object(value, path, &SECRET_REF_KEYS)?;
    Ok(SecretRef {
        id: read_id(members, path, "id")?,
    })
}

/// Map a bounded object onto an environment-name-keyed string map.
pub(super) fn read_env_string_map(
    value: &BoundedJson,
    path: &str,
) -> Result<BTreeMap<EnvName, String>, ProviderError> {
    let members = value
        .as_object()
        .ok_or_else(|| type_mismatch(path, "object"))?;
    let mut map = BTreeMap::new();
    for (key, entry) in members {
        let name = EnvName::parse(key).map_err(|_| ProviderError::InvalidValue {
            path: format!("{path}.{key}"),
            reason: "not a valid environment-variable name".to_owned(),
        })?;
        let text = entry
            .as_str()
            .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "string"))?;
        map.insert(name, text.to_owned());
    }
    Ok(map)
}

/// Map one continuation-schema field declaration, reusing the domain validator.
pub(super) fn read_field_declaration(
    value: &BoundedJson,
    path: &str,
) -> Result<Field, ProviderError> {
    let members = closed_object(value, path, &FIELD_DECLARATION_KEYS)?;
    let kind = read_enum(members, path, "kind", FieldKind::from_wire)?;
    let choices = match find(members, "choices") {
        Some(entry) => read_scalars(entry, &format!("{path}.choices"))?,
        None => Vec::new(),
    };
    let draft = FieldDraft {
        id: read_id(members, path, "id")?,
        kind,
        required: read_bool(members, path, "required")?,
        default: read_scalar_option(members, path, "default")?,
        minimum: read_scalar_option(members, path, "minimum")?,
        maximum: read_scalar_option(members, path, "maximum")?,
        choices,
        visible_when: match find(members, "visible_when") {
            Some(_) => Some(read_id(members, path, "visible_when")?),
            None => None,
        },
        restart: read_enum(members, path, "restart", RestartScope::from_wire)?,
    };
    Field::parse(draft).map_err(|error| ProviderError::InvalidValue {
        path: path.to_owned(),
        reason: error.to_string(),
    })
}

fn read_scalars(value: &BoundedJson, path: &str) -> Result<Vec<Scalar>, ProviderError> {
    array(value, path, FIELD_CHOICE_LIMIT)?
        .iter()
        .map(|entry| read_scalar(entry, path))
        .collect()
}

fn read_scalar_option(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<Option<Scalar>, ProviderError> {
    find(members, key)
        .map(|entry| read_scalar(entry, &format!("{path}.{key}")))
        .transpose()
}

fn read_scalar(value: &BoundedJson, path: &str) -> Result<Scalar, ProviderError> {
    match value {
        BoundedJson::Bool(flag) => Ok(Scalar::Bool(*flag)),
        BoundedJson::Int(number) => Ok(Scalar::Integer(*number)),
        BoundedJson::Number(decimal) => Ok(Scalar::Decimal(decimal.clone())),
        BoundedJson::Str(text) => Ok(Scalar::Text(text.clone())),
        _ => Err(type_mismatch(path, "scalar")),
    }
}
