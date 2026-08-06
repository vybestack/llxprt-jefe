//! Generic closed-object and typed-value extraction primitives for the
//! action-provider protocol readers (issue #390 CW-10, Slice A).
//!
//! These helpers operate purely on the shared bounded reader's [`BoundedJson`]
//! tree: borrowing object members, rejecting unknown fields, requiring keys,
//! and reading typed scalars/enums. They carry no protocol-specific knowledge,
//! so the payload and typed-value readers compose them without duplicating the
//! closed-object rules.

use std::fmt;

use crate::domain::Id;
use crate::domain::bounded_json::BoundedJson;

use super::error::ProviderError;

/// Borrow the members of an object after rejecting any unknown field.
pub(super) fn closed_object<'a>(
    value: &'a BoundedJson,
    path: &str,
    allowed: &[&str],
) -> Result<&'a [(String, BoundedJson)], ProviderError> {
    let members = value
        .as_object()
        .ok_or_else(|| type_mismatch(path, "object"))?;
    for (key, _) in members {
        if !allowed.contains(&key.as_str()) {
            return Err(ProviderError::UnknownField {
                path: path.to_owned(),
                field: key.clone(),
            });
        }
    }
    Ok(members)
}

pub(super) fn find<'a>(members: &'a [(String, BoundedJson)], key: &str) -> Option<&'a BoundedJson> {
    members
        .iter()
        .find_map(|(member_key, value)| (member_key == key).then_some(value))
}

pub(super) fn require<'a>(
    members: &'a [(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<&'a BoundedJson, ProviderError> {
    find(members, key).ok_or_else(|| ProviderError::MissingField {
        path: path.to_owned(),
        field: key.to_owned(),
    })
}

pub(super) fn read_string<'a>(
    members: &'a [(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<&'a str, ProviderError> {
    require(members, path, key)?
        .as_str()
        .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "string"))
}

pub(super) fn read_bool(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<bool, ProviderError> {
    require(members, path, key)?
        .as_bool()
        .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "boolean"))
}

pub(super) fn read_u64(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<u64, ProviderError> {
    let raw = require(members, path, key)?
        .as_int()
        .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "integer"))?;
    u64::try_from(raw).map_err(|_| ProviderError::InvalidValue {
        path: format!("{path}.{key}"),
        reason: format!("{raw} is negative"),
    })
}

pub(super) fn read_optional_u64(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<Option<u64>, ProviderError> {
    match find(members, key) {
        Some(value) => {
            let raw = value
                .as_int()
                .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "integer"))?;
            u64::try_from(raw)
                .map(Some)
                .map_err(|_| ProviderError::InvalidValue {
                    path: format!("{path}.{key}"),
                    reason: format!("{raw} is negative"),
                })
        }
        None => Ok(None),
    }
}

pub(super) fn read_u16(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<u16, ProviderError> {
    let raw = require(members, path, key)?
        .as_int()
        .ok_or_else(|| type_mismatch(&format!("{path}.{key}"), "integer"))?;
    u16::try_from(raw).map_err(|_| ProviderError::InvalidValue {
        path: format!("{path}.{key}"),
        reason: format!("{raw} is not a 0..=65535 integer"),
    })
}

pub(super) fn read_with<T, E: fmt::Display>(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
    parser: impl Fn(&str) -> Result<T, E>,
) -> Result<T, ProviderError> {
    let text = read_string(members, path, key)?;
    parser(text).map_err(|error| ProviderError::InvalidValue {
        path: format!("{path}.{key}"),
        reason: error.to_string(),
    })
}

pub(super) fn read_id(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<Id, ProviderError> {
    read_with(members, path, key, Id::parse)
}

pub(super) fn read_parsed_string<T, E: fmt::Display>(
    content: &BoundedJson,
    path: &str,
    parser: impl Fn(&str) -> Result<T, E>,
) -> Result<T, ProviderError> {
    let text = content
        .as_str()
        .ok_or_else(|| type_mismatch(path, "string"))?;
    parser(text).map_err(|error| ProviderError::InvalidValue {
        path: path.to_owned(),
        reason: error.to_string(),
    })
}

pub(super) fn read_enum<T>(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
    resolve: impl Fn(&str) -> Option<T>,
) -> Result<T, ProviderError> {
    let text = read_string(members, path, key)?;
    resolve(text).ok_or_else(|| ProviderError::UnknownValue {
        path: format!("{path}.{key}"),
        value: text.to_owned(),
    })
}

pub(super) fn read_enum_array<T: PartialEq>(
    value: &BoundedJson,
    path: &str,
    limit: usize,
    resolve: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, ProviderError> {
    array(value, path, limit)?
        .iter()
        .map(|element| {
            let text = element
                .as_str()
                .ok_or_else(|| type_mismatch(path, "string"))?;
            resolve(text).ok_or_else(|| ProviderError::UnknownValue {
                path: path.to_owned(),
                value: text.to_owned(),
            })
        })
        .collect()
}

pub(super) fn array<'a>(
    value: &'a BoundedJson,
    path: &str,
    limit: usize,
) -> Result<&'a [BoundedJson], ProviderError> {
    let elements = value
        .as_array()
        .ok_or_else(|| type_mismatch(path, "array"))?;
    if elements.len() > limit {
        return Err(ProviderError::InvalidValue {
            path: path.to_owned(),
            reason: format!("{} entries exceeds the {limit} limit", elements.len()),
        });
    }
    Ok(elements)
}

/// Reject a repeated capability in the ready payload.
pub(super) fn reject_duplicates<T: PartialEq>(
    values: &[T],
    path: &str,
) -> Result<(), ProviderError> {
    for (index, value) in values.iter().enumerate() {
        if values[..index].contains(value) {
            return Err(ProviderError::InvalidValue {
                path: path.to_owned(),
                reason: "a value is declared twice".to_owned(),
            });
        }
    }
    Ok(())
}

pub(super) fn type_mismatch(path: &str, expected: &'static str) -> ProviderError {
    ProviderError::TypeMismatch {
        path: path.to_owned(),
        expected,
    }
}
