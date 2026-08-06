//! Manifest element readers (issue #389 CW-09, acceptance rows D4 and D5).
//!
//! Each function maps one closed object onto its typed declaration. Every
//! object names the keys it admits, so an unknown field is rejected here rather
//! than silently ignored, and every enum is resolved through its exact
//! lower-kebab-case wire name.

use super::action::{Action, ActionConfirmation, ActionDraft, ActionOutcome};
use super::field::{Field, FieldDraft, FieldKind, RestartScope, Scalar};
use super::limits::{
    ACTION_ARGUMENT_LIMIT, ACTION_CONTEXT_LIMIT, CONFIG_FIELD_LIMIT, FIELD_CHOICE_LIMIT,
    PANEL_PORT_LIMIT, ROUTE_ACTIVATION_FIELD_LIMIT, SCREEN_ID_LIMIT,
};
use super::manifest::PluginDefaults;
use super::provider::{Provider, ProviderMode};
use super::reader::{
    ManifestReadError, array, closed_object, declaration_error, optional, read_bool, read_enum,
    read_enum_array, read_id, read_string, read_u32, read_with, require,
};
use super::surface::{
    ConfigSchema, EventKind, ModelKind, Panel, PanelDraft, Port, Route, RouteDraft,
    ScreenContribution,
};
use super::values::{HostTriple, RelativePath};
use crate::domain::Id;
use crate::domain::bounded_json::BoundedJson;

const PROVIDER_KEYS: [&str; 2] = ["mode", "binaries"];
const CONFIG_KEYS: [&str; 2] = ["schema_version", "fields"];
const FIELD_KEYS: [&str; 9] = [
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
const ACTION_KEYS: [&str; 11] = [
    "id",
    "label",
    "description",
    "category",
    "contexts",
    "arguments",
    "timeout_seconds",
    "destructive",
    "confirmation",
    "handler",
    "allowed_outcomes",
];
const PANEL_KEYS: [&str; 5] = ["id", "model_kinds", "event_kinds", "handler", "ports"];
const PORT_KEYS: [&str; 1] = ["id"];
const ROUTE_KEYS: [&str; 3] = ["id", "activation_fields", "target_screen"];
const SCREEN_KEYS: [&str; 2] = ["path", "screen_ids"];
const DEFAULTS_KEYS: [&str; 3] = ["actions_enabled", "screens_enabled", "config"];

/// Read the provider declaration and its host-triple binary map.
pub(super) fn read_provider(value: &BoundedJson) -> Result<Provider, ManifestReadError> {
    let members = closed_object(value, "provider", &PROVIDER_KEYS)?;
    let mode = read_enum(members, "provider", "mode", ProviderMode::from_wire)?;
    let declared = closed_binaries(require(members, "provider", "binaries")?)?;
    Provider::parse(mode, declared).map_err(declaration_error("provider"))
}

/// Read `binaries` as validated `(host triple, relative path)` pairs.
///
/// The key set is open by nature — it is one entry per supported target — so
/// each key is validated as a host triple instead of matched against a list.
fn closed_binaries(
    value: &BoundedJson,
) -> Result<Vec<(HostTriple, RelativePath)>, ManifestReadError> {
    let members = value
        .as_object()
        .ok_or_else(|| ManifestReadError::TypeMismatch {
            path: "provider.binaries".to_owned(),
            expected: "object",
        })?;
    members
        .iter()
        .map(|(key, entry)| {
            let triple =
                HostTriple::parse(key).map_err(|error| ManifestReadError::InvalidValue {
                    path: "provider.binaries".to_owned(),
                    reason: error.to_string(),
                })?;
            let text = entry
                .as_str()
                .ok_or_else(|| ManifestReadError::TypeMismatch {
                    path: format!("provider.binaries.{key}"),
                    expected: "string",
                })?;
            let path =
                RelativePath::parse(text).map_err(|error| ManifestReadError::InvalidValue {
                    path: format!("provider.binaries.{key}"),
                    reason: error.to_string(),
                })?;
            Ok((triple, path))
        })
        .collect()
}

/// Read the configuration schema.
pub(super) fn read_config_schema(value: &BoundedJson) -> Result<ConfigSchema, ManifestReadError> {
    let members = closed_object(value, "config", &CONFIG_KEYS)?;
    let version = read_u32(members, "config", "schema_version")?;
    let fields = array(
        require(members, "config", "fields")?,
        "config.fields",
        CONFIG_FIELD_LIMIT,
    )?
    .iter()
    .map(|entry| read_field(entry, "config.fields"))
    .collect::<Result<Vec<_>, _>>()?;
    ConfigSchema::parse(version, fields).map_err(declaration_error("config"))
}

/// Read one field declaration.
fn read_field(value: &BoundedJson, path: &str) -> Result<Field, ManifestReadError> {
    let members = closed_object(value, path, &FIELD_KEYS)?;
    let kind = read_enum(members, path, "kind", FieldKind::from_wire)?;
    let choices = optional(members, "choices")
        .map(|entry| read_scalars(entry, &format!("{path}.choices"), FIELD_CHOICE_LIMIT))
        .transpose()?
        .unwrap_or_default();
    let draft = FieldDraft {
        id: read_id(members, path, "id")?,
        kind,
        required: read_bool(members, path, "required")?,
        default: read_scalar_option(members, path, "default")?,
        minimum: read_scalar_option(members, path, "minimum")?,
        maximum: read_scalar_option(members, path, "maximum")?,
        choices,
        visible_when: optional(members, "visible_when")
            .map(|_| read_id(members, path, "visible_when"))
            .transpose()?,
        restart: read_enum(members, path, "restart", RestartScope::from_wire)?,
    };
    Field::parse(draft).map_err(declaration_error(path))
}

/// Read an optional scalar member.
fn read_scalar_option(
    members: &[(String, BoundedJson)],
    path: &str,
    key: &str,
) -> Result<Option<Scalar>, ManifestReadError> {
    optional(members, key)
        .map(|entry| scalar(entry, &format!("{path}.{key}")))
        .transpose()
}

/// Read an array of scalars.
fn read_scalars(
    value: &BoundedJson,
    path: &str,
    limit: usize,
) -> Result<Vec<Scalar>, ManifestReadError> {
    array(value, path, limit)?
        .iter()
        .map(|entry| scalar(entry, path))
        .collect()
}

/// Lower one JSON value onto a declared scalar.
fn scalar(value: &BoundedJson, path: &str) -> Result<Scalar, ManifestReadError> {
    match value {
        BoundedJson::Bool(flag) => Ok(Scalar::Bool(*flag)),
        BoundedJson::Int(number) => Ok(Scalar::Integer(*number)),
        BoundedJson::Number(decimal) => Ok(Scalar::Decimal(decimal.clone())),
        BoundedJson::Str(text) => Ok(Scalar::Text(text.clone())),
        BoundedJson::Null | BoundedJson::Array(_) | BoundedJson::Object(_) => {
            Err(ManifestReadError::TypeMismatch {
                path: path.to_owned(),
                expected: "scalar",
            })
        }
    }
}

/// Read one action declaration.
pub(super) fn read_action(value: &BoundedJson) -> Result<Action, ManifestReadError> {
    let path = "actions";
    let members = closed_object(value, path, &ACTION_KEYS)?;
    let draft = ActionDraft {
        id: read_id(members, path, "id")?,
        label: read_string(members, path, "label")?.to_owned(),
        description: read_string(members, path, "description")?.to_owned(),
        category: read_id(members, path, "category")?,
        contexts: read_ids(
            require(members, path, "contexts")?,
            "actions.contexts",
            ACTION_CONTEXT_LIMIT,
        )?,
        arguments: array(
            require(members, path, "arguments")?,
            "actions.arguments",
            ACTION_ARGUMENT_LIMIT,
        )?
        .iter()
        .map(|entry| read_field(entry, "actions.arguments"))
        .collect::<Result<Vec<_>, _>>()?,
        timeout_seconds: read_u32(members, path, "timeout_seconds")?,
        destructive: read_bool(members, path, "destructive")?,
        confirmation: read_enum(members, path, "confirmation", ActionConfirmation::from_wire)?,
        handler: read_id(members, path, "handler")?,
        allowed_outcomes: read_enum_array(
            require(members, path, "allowed_outcomes")?,
            "actions.allowed_outcomes",
            ActionOutcome::ALL.len(),
            ActionOutcome::from_wire,
        )?,
    };
    Action::parse(draft).map_err(declaration_error(path))
}

/// Read one panel declaration.
pub(super) fn read_panel(value: &BoundedJson) -> Result<Panel, ManifestReadError> {
    let path = "panels";
    let members = closed_object(value, path, &PANEL_KEYS)?;
    let draft = PanelDraft {
        id: read_id(members, path, "id")?,
        model_kinds: read_enum_array(
            require(members, path, "model_kinds")?,
            "panels.model_kinds",
            ModelKind::ALL.len(),
            ModelKind::from_wire,
        )?,
        event_kinds: read_enum_array(
            require(members, path, "event_kinds")?,
            "panels.event_kinds",
            EventKind::ALL.len(),
            EventKind::from_wire,
        )?,
        handler: read_id(members, path, "handler")?,
        ports: array(
            require(members, path, "ports")?,
            "panels.ports",
            PANEL_PORT_LIMIT,
        )?
        .iter()
        .map(read_port)
        .collect::<Result<Vec<_>, _>>()?,
    };
    Panel::parse(draft).map_err(declaration_error(path))
}

/// Read one port declaration.
fn read_port(value: &BoundedJson) -> Result<Port, ManifestReadError> {
    let members = closed_object(value, "panels.ports", &PORT_KEYS)?;
    Ok(Port::new(read_id(members, "panels.ports", "id")?))
}

/// Read one route declaration.
pub(super) fn read_route(value: &BoundedJson) -> Result<Route, ManifestReadError> {
    let path = "routes";
    let members = closed_object(value, path, &ROUTE_KEYS)?;
    let draft = RouteDraft {
        id: read_id(members, path, "id")?,
        activation_fields: array(
            require(members, path, "activation_fields")?,
            "routes.activation_fields",
            ROUTE_ACTIVATION_FIELD_LIMIT,
        )?
        .iter()
        .map(|entry| read_field(entry, "routes.activation_fields"))
        .collect::<Result<Vec<_>, _>>()?,
        target_screen: read_id(members, path, "target_screen")?,
    };
    Route::parse(draft).map_err(declaration_error(path))
}

/// Read one screen contribution.
pub(super) fn read_screen(value: &BoundedJson) -> Result<ScreenContribution, ManifestReadError> {
    let path = "screens";
    let members = closed_object(value, path, &SCREEN_KEYS)?;
    let file = read_with(members, path, "path", RelativePath::parse)?;
    let ids = read_ids(
        require(members, path, "screen_ids")?,
        "screens.screen_ids",
        SCREEN_ID_LIMIT,
    )?;
    ScreenContribution::parse(file, ids).map_err(declaration_error(path))
}

/// Read the package defaults.
pub(super) fn read_defaults(value: &BoundedJson) -> Result<PluginDefaults, ManifestReadError> {
    let path = "defaults";
    let members = closed_object(value, path, &DEFAULTS_KEYS)?;
    let list = |key: &str, limit: usize| match optional(members, key) {
        Some(entry) => read_ids(entry, &format!("defaults.{key}"), limit),
        None => Ok(Vec::new()),
    };
    let config = match optional(members, "config") {
        Some(entry) => read_config_defaults(entry)?,
        None => Vec::new(),
    };
    Ok(PluginDefaults {
        actions_enabled: list("actions_enabled", super::limits::ACTION_LIMIT)?,
        screens_enabled: list("screens_enabled", SCREEN_ID_LIMIT)?,
        config,
    })
}

/// Read the default configuration object as validated `(field, value)` pairs.
fn read_config_defaults(value: &BoundedJson) -> Result<Vec<(Id, Scalar)>, ManifestReadError> {
    let members = value
        .as_object()
        .ok_or_else(|| ManifestReadError::TypeMismatch {
            path: "defaults.config".to_owned(),
            expected: "object",
        })?;
    members
        .iter()
        .map(|(key, entry)| {
            let id = Id::parse(key).map_err(|error| ManifestReadError::InvalidValue {
                path: "defaults.config".to_owned(),
                reason: error.to_string(),
            })?;
            Ok((id, scalar(entry, &format!("defaults.config.{key}"))?))
        })
        .collect()
}

/// Read an array of configuration identifiers.
fn read_ids(value: &BoundedJson, path: &str, limit: usize) -> Result<Vec<Id>, ManifestReadError> {
    array(value, path, limit)?
        .iter()
        .map(|entry| {
            let text = entry
                .as_str()
                .ok_or_else(|| ManifestReadError::TypeMismatch {
                    path: path.to_owned(),
                    expected: "string",
                })?;
            Id::parse(text).map_err(|error| ManifestReadError::InvalidValue {
                path: path.to_owned(),
                reason: error.to_string(),
            })
        })
        .collect()
}
