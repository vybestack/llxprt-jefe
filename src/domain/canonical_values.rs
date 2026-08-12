//! Canonical typed-value and identity helpers.
//!
//! Shared by the durable state projections and one-way schema-1 migration.
//! These are pure value transformations over domain types: identifier minting,
//! stable digests, JSON <-> [`TypedMap`] conversion, and canonical remote
//! target encoding. They perform no I/O and depend on nothing outside
//! `domain`, so both `persistence/` (migration) and `state/` (durable
//! projection) can build identical schema-2 values.

use std::collections::BTreeMap;

use serde_json::Value;

use super::sha256::Sha256;
use super::{CanonicalDecimal, Id, Sha256Digest, TypedMap, TypedValue};

use super::agent_definition::{AgentDefinition, DefinitionSha256, FieldValue, Target};

/// Hash the complete canonical launch target identity.
#[must_use]
pub fn launch_target_fingerprint(target: &Target) -> DefinitionSha256 {
    let mut bytes = Vec::new();
    match target {
        Target::Local { canonical_cwd } => {
            bytes.push(b'L');
            append_target_part(&mut bytes, canonical_cwd.to_string_lossy().as_bytes());
        }
        Target::Remote(remote) => {
            bytes.push(b'R');
            append_target_part(&mut bytes, remote.user.as_bytes());
            append_target_part(&mut bytes, remote.host.as_bytes());
            append_target_part(&mut bytes, &remote.port.unwrap_or(22).to_be_bytes());
            append_target_part(&mut bytes, remote.run_as_user.as_bytes());
            append_target_part(
                &mut bytes,
                remote.canonical_cwd.to_string_lossy().as_bytes(),
            );
        }
    }
    DefinitionSha256::digest(&bytes)
}

fn append_target_part(bytes: &mut Vec<u8>, part: &[u8]) {
    bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
    bytes.extend_from_slice(part);
}

/// Parse `value` as an [`Id`], reporting the grammar violation as text.
pub fn required_id(value: &str) -> Result<Id, String> {
    Id::parse(value).map_err(|error| error.to_string())
}

/// Map a schema-1 agent-kind label onto its canonical type identifier.
pub fn type_id(value: Option<&str>) -> Result<Id, String> {
    let normalized = match value.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("llxprt") => "core.llxprt",
        Some("code_puppy" | "code-puppy" | "codepuppy") => "core.code-puppy",
        Some(other) => {
            let parsed = required_id(other)?;
            if super::agent_definition::AgentDefinition::shipped()
                .iter()
                .any(|definition| definition.id.as_str() == parsed.as_str())
            {
                return Ok(parsed);
            }
            return Err(format!("unsupported schema-1 agent kind {other}"));
        }
    };
    required_id(normalized)
}

/// Mint a deterministic `{prefix}.{digest}` identifier from `parts`.
pub fn stable_id(prefix: &str, parts: &[&str]) -> Result<Id, String> {
    required_id(&format!("{prefix}.{}", hash_parts(parts)))
}

/// Digest unambiguously length-prefixed `parts`.
pub fn digest_parts(parts: &[&str]) -> Result<Sha256Digest, String> {
    Sha256Digest::parse(&hash_parts(parts)).map_err(|error| error.to_string())
}

/// Resolve the canonical digest of a shipped agent definition by durable type id.
pub fn shipped_definition_hash(type_id: &Id) -> Result<Sha256Digest, String> {
    let definition = super::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == type_id.as_str())
        .ok_or_else(|| format!("unknown shipped agent definition {}", type_id.as_str()))?;
    Sha256Digest::parse(&definition.sha256().to_hex()).map_err(|error| error.to_string())
}

/// Digest raw `bytes`.
pub fn digest_bytes(bytes: &[u8]) -> Result<Sha256Digest, String> {
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

/// Digest the canonical serialization of a typed map.
pub fn typed_map_hash(values: &TypedMap) -> Result<Sha256Digest, String> {
    let encoded = serde_json::to_vec(values).map_err(|error| error.to_string())?;
    digest_bytes(&encoded)
}

/// Insert one JSON value under a normalized key, dropping nulls.
pub fn insert_json(values: &mut TypedMap, key: &str, value: Value) -> Result<(), String> {
    let key = normalized_key(key)?;
    if let Some(value) = json_to_typed(value)? {
        let duplicate_key = key.to_string();
        if values.insert(key, value).is_some() {
            return Err(format!("duplicate typed value key {duplicate_key}"));
        }
    }
    Ok(())
}

/// Convert a JSON object into a [`TypedMap`], dropping null-valued keys.
pub fn json_map_to_typed(value: Value) -> Result<TypedMap, String> {
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

/// Encode a canonical decimal as JSON without losing precision.
///
/// `serde_json::Number` is f64-backed, so a decimal carrying more significant
/// digits than a double can hold would silently truncate. Such a value is
/// emitted as its exact text instead: losing the JSON number type is
/// recoverable, whereas losing digits is not.
fn decimal_to_json(text: &str) -> Value {
    match text.parse::<serde_json::Number>() {
        Ok(number) if number.to_string() == text => Value::Number(number),
        _ => Value::String(text.to_owned()),
    }
}

/// Convert one [`TypedValue`] back into plain JSON.
#[must_use]
pub fn typed_to_json(value: &TypedValue) -> Value {
    match value {
        TypedValue::String(value) => Value::String(value.clone()),
        TypedValue::Bool(value) => Value::Bool(*value),
        TypedValue::Integer(value) => Value::Number((*value).into()),
        TypedValue::Decimal(value) => decimal_to_json(value.as_str()),
        TypedValue::Datetime(value) => Value::String(value.as_str().to_owned()),
        TypedValue::List(values) => Value::Array(values.iter().map(typed_to_json).collect()),
        TypedValue::Map(values) => {
            let mut object = serde_json::Map::new();
            for (key, value) in values {
                object.insert(key.to_string(), typed_to_json(value));
            }
            Value::Object(object)
        }
        TypedValue::SecretRef(value) => {
            let mut reference = serde_json::Map::new();
            reference.insert("env".to_owned(), Value::String(value.env.env().to_owned()));
            Value::Object(reference)
        }
    }
}

/// Convert a [`TypedMap`] into JSON with runtime (underscore) field names.
///
/// [`json_map_to_typed`] normalizes `_` to `-` to satisfy the [`Id`] grammar;
/// this restores the runtime spelling recursively so serde can decode durable
/// values straight back into runtime structs.
#[must_use]
pub fn typed_map_to_runtime_json(values: &TypedMap) -> Value {
    let mut object = serde_json::Map::new();
    for (key, value) in values {
        object.insert(key.as_str().replace('-', "_"), typed_to_runtime_json(value));
    }
    Value::Object(object)
}

fn typed_to_runtime_json(value: &TypedValue) -> Value {
    match value {
        TypedValue::List(values) => {
            Value::Array(values.iter().map(typed_to_runtime_json).collect())
        }
        TypedValue::Map(values) => typed_map_to_runtime_json(values),
        other => typed_to_json(other),
    }
}

/// Read one field from a typed map using the runtime (underscore) field name.
#[must_use]
pub fn typed_field<'a>(values: &'a TypedMap, field: &str) -> Option<&'a TypedValue> {
    let key = Id::parse(&field.replace('_', "-")).ok()?;
    values.get(&key)
}

/// Canonical local target identity used by migration and current projection.
pub fn canonical_local_target(path: &std::path::Path) -> Result<String, String> {
    let canonical = match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(error.to_string()),
    };
    canonical
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "canonical local target is not valid UTF-8".to_owned())
}

/// Retain only fields declared as launch-signature inputs for an active type.
pub fn launch_signature_values(type_id: &Id, values: &TypedMap) -> Result<TypedMap, String> {
    let definition = super::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == type_id.as_str())
        .ok_or_else(|| format!("unknown shipped agent definition {}", type_id.as_str()))?;
    let mut signature_values = TypedMap::new();
    for field in definition
        .repository_fields
        .iter()
        .chain(definition.agent_fields.iter())
        .filter(|field| field.launch_signature)
    {
        let key = Id::parse(&field.id.replace('_', "-")).map_err(|error| error.to_string())?;
        if let Some(value) = values.get(&key) {
            signature_values.insert(key, value.clone());
        }
    }
    Ok(signature_values)
}

/// Hash effective launch-signature values using definition declaration order.
pub fn launch_value_fingerprint(
    definition: &AgentDefinition,
    values: &TypedMap,
) -> Result<DefinitionSha256, String> {
    let mut buffer = Vec::new();
    for (scope, field) in definition
        .repository_fields
        .iter()
        .map(|field| (b'R', field))
        .chain(definition.agent_fields.iter().map(|field| (b'A', field)))
        .filter(|(_, field)| field.launch_signature)
    {
        buffer.push(scope);
        buffer.extend_from_slice(field.id.as_bytes());
        buffer.push(0);
        if let Some(value) = typed_field(values, &field.id) {
            append_typed_signature_value(&mut buffer, value)?;
        } else if let Some(value) = field.default.as_ref() {
            append_default_signature_value(&mut buffer, value);
        }
        buffer.push(0);
    }
    Ok(DefinitionSha256::digest(&buffer))
}

fn append_typed_signature_value(buffer: &mut Vec<u8>, value: &TypedValue) -> Result<(), String> {
    match value {
        TypedValue::Bool(value) => buffer.push(if *value { b'1' } else { b'0' }),
        TypedValue::String(value) => buffer.extend_from_slice(value.as_bytes()),
        TypedValue::Integer(value) => buffer.extend_from_slice(value.to_string().as_bytes()),
        TypedValue::List(values) => {
            for value in values {
                let TypedValue::String(value) = value else {
                    return Err("launch signature list contains a non-string value".to_owned());
                };
                buffer.extend_from_slice(value.as_bytes());
                buffer.push(b',');
            }
        }
        _ => return Err("unsupported launch signature typed value".to_owned()),
    }
    Ok(())
}

fn append_default_signature_value(buffer: &mut Vec<u8>, value: &FieldValue) {
    match value {
        FieldValue::Boolean(value) | FieldValue::OptionalBoolean(Some(value)) => {
            buffer.push(if *value { b'1' } else { b'0' });
        }
        FieldValue::OptionalBoolean(None) => buffer.push(b'n'),
        FieldValue::String(value) | FieldValue::Path(value) => {
            buffer.extend_from_slice(value.as_bytes());
        }
        FieldValue::Integer(value) => buffer.extend_from_slice(value.to_string().as_bytes()),
        FieldValue::StringList(values) => {
            for value in values {
                buffer.extend_from_slice(value.as_bytes());
                buffer.push(b',');
            }
        }
    }
}

/// Encode the canonical, unambiguous remote target for a repository.
#[must_use]
pub fn canonical_remote_target(
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

/// Decoded components of a [`canonical_remote_target`] string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTargetParts {
    /// SSH login user.
    pub login_user: String,
    /// Lowercased host name.
    pub host: String,
    /// SSH port.
    pub port: u16,
    /// Effective run-as user.
    pub run_as_user: String,
    /// Normalized remote base directory.
    pub base_dir: String,
}

/// Decode a canonical remote target back into its components.
///
/// # Errors
///
/// Returns a description when the encoding is malformed or has the wrong
/// number of fields.
pub fn parse_remote_target(encoded: &str) -> Result<RemoteTargetParts, String> {
    let parts = parse_length_prefixed_text(encoded)?;
    let [login_user, host, port, run_as_user, base_dir] = parts.as_slice() else {
        return Err(format!(
            "remote target must encode 5 fields, found {}",
            parts.len()
        ));
    };
    let port = port
        .parse::<u16>()
        .map_err(|error| format!("remote target port is invalid: {error}"))?;
    Ok(RemoteTargetParts {
        login_user: login_user.clone(),
        host: host.clone(),
        port,
        run_as_user: run_as_user.clone(),
        base_dir: base_dir.clone(),
    })
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

fn parse_length_prefixed_text(encoded: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let mut rest = encoded;
    while !rest.is_empty() {
        let (length, remainder) = rest
            .split_once(':')
            .ok_or_else(|| "remote target field is missing its length prefix".to_owned())?;
        let length = length
            .parse::<usize>()
            .map_err(|error| format!("remote target length prefix is invalid: {error}"))?;
        if remainder.len() < length || !remainder.is_char_boundary(length) {
            return Err("remote target length prefix exceeds the encoded field".to_owned());
        }
        let (value, remainder) = remainder.split_at(length);
        parts.push(value.to_owned());
        rest = remainder;
    }
    Ok(parts)
}

/// Normalize a remote path by resolving `.`/`..` segments textually.
#[must_use]
pub fn normalize_remote_path(path: &str) -> String {
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
