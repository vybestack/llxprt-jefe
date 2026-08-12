//! Read `plugin.json` bytes into a validated [`Manifest`]
//! (issue #389 CW-09, acceptance rows D4 and D5).
//!
//! Parsing runs in three stages, each of which can only reject:
//!
//! 1. the shared bounded reader enforces byte, depth, member, element and
//!    string bounds and rejects duplicate keys;
//! 2. this module maps the ordered tree onto the closed schema, rejecting any
//!    field the schema does not name and any enum spelling that is not exactly
//!    the declared lower-kebab-case one;
//! 3. the typed declarations validate themselves and then each other through
//!    [`Manifest::parse`].
//!
//! Nothing here opens a file or starts a process; it takes bytes and returns
//! declarations or a diagnostic.

use std::fmt;

use super::limits::{
    ACTION_LIMIT, MANIFEST_BYTE_LIMIT, PANEL_LIMIT, ROUTE_LIMIT, SCREEN_CONTRIBUTION_LIMIT,
};
use super::manifest::{Manifest, ManifestDraft, ManifestError};
use super::plugin_id::PluginId;
use crate::domain::bounded_json::{
    BoundedJson, BoundedJsonError, BoundedJsonLimits, NumberPolicy, parse,
};
use crate::domain::{CanonicalSemver, Id};

/// Bounds the plugin manifest places on its own JSON.
///
/// Numbers are finite rather than integer-only, because a configuration field
/// may declare a fractional default or bound.
const MANIFEST_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    document_bytes: MANIFEST_BYTE_LIMIT,
    depth: 16,
    object_members: 256,
    array_elements: 1_024,
    string_bytes: 4_096,
    numbers: NumberPolicy::Finite,
};

/// Keys the manifest object admits.
const MANIFEST_KEYS: [&str; 13] = [
    "manifest_schema",
    "id",
    "version",
    "display_name",
    "host_api",
    "protocol",
    "provider",
    "config",
    "actions",
    "panels",
    "routes",
    "screens",
    "defaults",
];

/// Keys the `host_api` object admits.
const HOST_API_KEYS: [&str; 2] = ["minimum", "maximum"];

/// Read and validate a complete manifest document.
///
/// # Errors
///
/// Returns [`ManifestReadError`] for a bounded-reader failure, an unknown or
/// missing field, a wrong JSON type, an unrecognized enum spelling, an invalid
/// value, or a cross-declaration validation failure.
pub fn read_manifest(input: &[u8]) -> Result<Manifest, ManifestReadError> {
    let document = parse(input, &MANIFEST_LIMITS).map_err(ManifestReadError::Json)?;
    let members = closed_object(&document, "manifest", &MANIFEST_KEYS)?;
    let host_api = closed_object(
        require(members, "manifest", "host_api")?,
        "host_api",
        &HOST_API_KEYS,
    )?;
    let config = optional(members, "config")
        .map(super::reader_parts::read_config_schema)
        .transpose()?;
    let defaults = optional(members, "defaults")
        .map(|value| super::reader_parts::read_defaults(value, config.as_ref()))
        .transpose()?;
    let draft = ManifestDraft {
        manifest_schema: read_u32(members, "manifest", "manifest_schema")?,
        id: read_with(members, "manifest", "id", PluginId::parse)?,
        version: read_with(members, "manifest", "version", CanonicalSemver::parse)?,
        display_name: read_string(members, "manifest", "display_name")?.to_owned(),
        host_api_minimum: read_with(host_api, "host_api", "minimum", CanonicalSemver::parse)?,
        host_api_maximum: read_with(host_api, "host_api", "maximum", CanonicalSemver::parse)?,
        protocol: read_u32(members, "manifest", "protocol")?,
        provider: super::reader_parts::read_provider(require(members, "manifest", "provider")?)?,
        config,
        actions: read_each(
            members,
            "actions",
            ACTION_LIMIT,
            super::reader_parts::read_action,
        )?,
        panels: read_each(
            members,
            "panels",
            PANEL_LIMIT,
            super::reader_parts::read_panel,
        )?,
        routes: read_each(
            members,
            "routes",
            ROUTE_LIMIT,
            super::reader_parts::read_route,
        )?,
        screens: read_each(
            members,
            "screens",
            SCREEN_CONTRIBUTION_LIMIT,
            super::reader_parts::read_screen,
        )?,
        defaults,
    };
    Manifest::parse(draft).map_err(ManifestReadError::Manifest)
}

/// Borrow an object's members, rejecting any key the schema does not name.
pub(super) fn closed_object<'a>(
    value: &'a BoundedJson,
    path: &str,
    allowed: &[&str],
) -> Result<&'a [(String, BoundedJson)], ManifestReadError> {
    let members = value
        .as_object()
        .ok_or_else(|| ManifestReadError::TypeMismatch {
            path: path.to_owned(),
            expected: "object",
        })?;
    for (key, _) in members {
        if !allowed.contains(&key.as_str()) {
            return Err(ManifestReadError::UnknownField {
                path: path.to_owned(),
                field: key.clone(),
            });
        }
    }
    Ok(members)
}

/// Borrow an optional member.
pub(super) fn optional<'a>(
    members: &'a [(String, BoundedJson)],
    key: &str,
) -> Option<&'a BoundedJson> {
    members
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

/// Borrow a required member.
pub(super) fn require<'a>(
    members: &'a [(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<&'a BoundedJson, ManifestReadError> {
    optional(members, key).ok_or_else(|| ManifestReadError::MissingField {
        path: path.to_owned(),
        field: key.to_owned(),
    })
}

/// Read a required string member.
pub(super) fn read_string<'a>(
    members: &'a [(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<&'a str, ManifestReadError> {
    require(members, path, key)?
        .as_str()
        .ok_or_else(|| ManifestReadError::TypeMismatch {
            path: format!("{path}.{key}"),
            expected: "string",
        })
}

/// Read a required boolean member.
pub(super) fn read_bool(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<bool, ManifestReadError> {
    require(members, path, key)?
        .as_bool()
        .ok_or_else(|| ManifestReadError::TypeMismatch {
            path: format!("{path}.{key}"),
            expected: "boolean",
        })
}

/// Read a required non-negative integer member as `u32`.
pub(super) fn read_u32(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<u32, ManifestReadError> {
    let raw =
        require(members, path, key)?
            .as_int()
            .ok_or_else(|| ManifestReadError::TypeMismatch {
                path: format!("{path}.{key}"),
                expected: "integer",
            })?;
    u32::try_from(raw).map_err(|_| ManifestReadError::InvalidValue {
        path: format!("{path}.{key}"),
        reason: format!("{raw} is outside the unsigned 32-bit range"),
    })
}

/// Read a required non-negative integer member as `u64`.
pub(super) fn read_u64(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<u64, ManifestReadError> {
    let raw =
        require(members, path, key)?
            .as_int()
            .ok_or_else(|| ManifestReadError::TypeMismatch {
                path: format!("{path}.{key}"),
                expected: "integer",
            })?;
    u64::try_from(raw).map_err(|_| ManifestReadError::InvalidValue {
        path: format!("{path}.{key}"),
        reason: format!("{raw} is outside the unsigned 64-bit range"),
    })
}

/// Read a required string member through a validating parser.
pub(super) fn read_with<T, E: fmt::Display>(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
    parser: impl Fn(&str) -> Result<T, E>,
) -> Result<T, ManifestReadError> {
    let text = read_string(members, path, key)?;
    parser(text).map_err(|error| ManifestReadError::InvalidValue {
        path: format!("{path}.{key}"),
        reason: error.to_string(),
    })
}

/// Read a required string member as a configuration identifier.
pub(super) fn read_id(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<Id, ManifestReadError> {
    read_with(members, path, key, Id::parse)
}

/// Borrow an array member, rejecting a wrong type or an exceeded bound.
pub(super) fn array<'a>(
    value: &'a BoundedJson,
    path: &str,
    limit: usize,
) -> Result<&'a [BoundedJson], ManifestReadError> {
    let elements = value
        .as_array()
        .ok_or_else(|| ManifestReadError::TypeMismatch {
            path: path.to_owned(),
            expected: "array",
        })?;
    if elements.len() > limit {
        return Err(ManifestReadError::InvalidValue {
            path: path.to_owned(),
            reason: format!("{} entries exceeds the {limit} limit", elements.len()),
        });
    }
    Ok(elements)
}

/// Read a required array member, mapping each element.
fn read_each<T>(
    members: &[(String, BoundedJson)],
    key: &str,
    limit: usize,
    element: impl Fn(&BoundedJson) -> Result<T, ManifestReadError>,
) -> Result<Vec<T>, ManifestReadError> {
    array(require(members, "manifest", key)?, key, limit)?
        .iter()
        .map(element)
        .collect()
}

/// Resolve an enum wire name, reporting the offending spelling on failure.
pub(super) fn read_enum<T>(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
    resolve: impl Fn(&str) -> Option<T>,
) -> Result<T, ManifestReadError> {
    let text = read_string(members, path, key)?;
    resolve(text).ok_or_else(|| ManifestReadError::UnknownValue {
        path: format!("{path}.{key}"),
        value: text.to_owned(),
    })
}

/// Resolve every element of an enum array.
pub(super) fn read_enum_array<T>(
    value: &BoundedJson,
    path: &str,
    limit: usize,
    resolve: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, ManifestReadError> {
    array(value, path, limit)?
        .iter()
        .map(|element| {
            let text = element
                .as_str()
                .ok_or_else(|| ManifestReadError::TypeMismatch {
                    path: path.to_owned(),
                    expected: "string",
                })?;
            resolve(text).ok_or_else(|| ManifestReadError::UnknownValue {
                path: path.to_owned(),
                value: text.to_owned(),
            })
        })
        .collect()
}

/// Why a manifest document could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestReadError {
    /// The bounded reader rejected the document.
    Json(BoundedJsonError),
    /// The schema does not name this field.
    UnknownField { path: String, field: String },
    /// A required field is absent.
    MissingField { path: String, field: String },
    /// A value has the wrong JSON type.
    TypeMismatch {
        path: String,
        expected: &'static str,
    },
    /// A string is not one of the declared wire names.
    UnknownValue { path: String, value: String },
    /// A value failed its own validation.
    InvalidValue { path: String, reason: String },
    /// The declarations are individually valid but inconsistent together.
    Manifest(ManifestError),
    /// One declaration failed its own validation.
    Declaration { path: String, reason: String },
}

impl fmt::Display for ManifestReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "malformed manifest: {error}"),
            Self::UnknownField { path, field } => {
                write!(formatter, "{path} has no field {field:?}")
            }
            Self::MissingField { path, field } => {
                write!(formatter, "{path} is missing required field {field:?}")
            }
            Self::TypeMismatch { path, expected } => {
                write!(formatter, "{path} must be {expected}")
            }
            Self::UnknownValue { path, value } => {
                write!(formatter, "{path} does not accept the value {value:?}")
            }
            Self::InvalidValue { path, reason } | Self::Declaration { path, reason } => {
                write!(formatter, "{path}: {reason}")
            }
            Self::Manifest(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ManifestReadError {}

/// Lower a declaration's own validation failure.
pub(super) fn declaration_error<E: fmt::Display>(path: &str) -> impl Fn(E) -> ManifestReadError {
    let path = path.to_owned();
    move |error| ManifestReadError::Declaration {
        path: path.clone(),
        reason: error.to_string(),
    }
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
