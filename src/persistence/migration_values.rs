//! Canonical typed-value and identity helpers for one-way state migration.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::domain::{CanonicalDecimal, Id, Sha256Digest, TypedMap, TypedValue};
use crate::persistence::sha256::Sha256;

pub(super) fn required_id(value: &str) -> Result<Id, String> {
    Id::parse(value).map_err(|error| error.to_string())
}

pub(super) fn type_id(value: Option<&str>) -> Result<Id, String> {
    let normalized = match value.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("llxprt") => "core.llxprt",
        Some("code_puppy" | "code-puppy" | "codepuppy") => "core.code-puppy",
        Some(other) => return Err(format!("unsupported schema-1 agent kind {other}")),
    };
    required_id(normalized)
}

pub(super) fn stable_id(prefix: &str, parts: &[&str]) -> Result<Id, String> {
    required_id(&format!("{prefix}.{}", hash_parts(parts)))
}

pub(super) fn digest_parts(parts: &[&str]) -> Result<Sha256Digest, String> {
    Sha256Digest::parse(&hash_parts(parts)).map_err(|error| error.to_string())
}

pub(super) fn digest_bytes(bytes: &[u8]) -> Result<Sha256Digest, String> {
    Sha256Digest::parse(&Sha256::digest(bytes).to_string()).map_err(|error| error.to_string())
}

fn hash_parts(parts: &[&str]) -> String {
    let mut encoded = Vec::new();
    for part in parts {
        let length = u64::try_from(part.len()).unwrap_or(u64::MAX);
        encoded.extend_from_slice(&length.to_be_bytes());
        encoded.extend_from_slice(part.as_bytes());
    }
    Sha256::digest(&encoded).to_string()
}

pub(super) fn typed_map_hash(values: &TypedMap) -> Result<Sha256Digest, String> {
    let encoded = serde_json::to_vec(values).map_err(|error| error.to_string())?;
    digest_bytes(&encoded)
}

pub(super) fn insert_json(values: &mut TypedMap, key: &str, value: Value) -> Result<(), String> {
    let key = normalized_key(key)?;
    if let Some(value) = json_to_typed(value)? {
        let duplicate_key = key.to_string();
        if values.insert(key, value).is_some() {
            return Err(format!("duplicate typed value key {duplicate_key}"));
        }
    }
    Ok(())
}

pub(super) fn json_map_to_typed(value: Value) -> Result<TypedMap, String> {
    let Value::Object(entries) = value else {
        return Err("typed map source is not an object".to_owned());
    };
    let mut values = BTreeMap::new();
    for (key, value) in entries {
        insert_json(&mut values, &key, value)?;
    }
    Ok(values)
}

fn json_to_typed(value: Value) -> Result<Option<TypedValue>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Bool(value) => Ok(Some(TypedValue::Bool(value))),
        Value::String(value) => Ok(Some(TypedValue::String(value))),
        Value::Number(value) => number_to_typed(&value).map(Some),
        Value::Array(values) => list_to_typed(values).map(|value| Some(TypedValue::List(value))),
        Value::Object(_) => json_map_to_typed(value).map(|value| Some(TypedValue::Map(value))),
    }
}

fn number_to_typed(value: &serde_json::Number) -> Result<TypedValue, String> {
    if let Some(value) = value.as_i64() {
        return Ok(TypedValue::Integer(value));
    }
    let text = value.to_string();
    CanonicalDecimal::parse(&text)
        .map(TypedValue::Decimal)
        .map_err(|error| error.to_string())
}

fn list_to_typed(values: Vec<Value>) -> Result<Vec<TypedValue>, String> {
    values
        .into_iter()
        .map(|value| {
            json_to_typed(value)?.ok_or_else(|| "null is not a typed list value".to_owned())
        })
        .collect()
}

fn normalized_key(value: &str) -> Result<Id, String> {
    let normalized = value.replace('_', "-");
    required_id(&normalized)
}

pub(super) fn canonical_remote_target(
    login_user: &str,
    host: &str,
    port: u16,
    run_as_user: &str,
    base_dir: &str,
) -> String {
    length_prefixed_text(&[
        login_user.trim(),
        &host.trim().to_ascii_lowercase(),
        &port.to_string(),
        run_as_user.trim(),
        &normalize_remote_path(base_dir),
    ])
}

fn length_prefixed_text(parts: &[&str]) -> String {
    let mut encoded = String::new();
    for part in parts {
        encoded.push_str(&part.len().to_string());
        encoded.push(':');
        encoded.push_str(part);
    }
    encoded
}

pub(super) fn normalize_remote_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                let _ = components.pop();
            }
            value => components.push(value),
        }
    }
    let joined = components.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}
