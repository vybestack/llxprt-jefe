//! Exact typed resource context projected from one open screen instance.

use std::fmt;

use super::AppState;
use crate::domain::{ConfigContractError, Id, TypedMap, TypedPortValue, TypedValue};
use crate::workbench::{PortDescriptor, PortRef, PortValue, ResourceSchemaError, ScreenIdentity};

const OWNER_ID: &str = "owner-id";
const TYPE_ID: &str = "type-id";
const SCHEMA_VERSION: &str = "schema-version";
const SEMANTIC_KEY: &str = "semantic-key";
const VALUE: &str = "value";

/// Failure to project one exact, schema-validated provider action context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderActionContextError {
    /// The open screen identity is not present in the committed registry.
    UnknownScreen { screen: ScreenIdentity },
    /// A declared resource port has no runtime identity for this instance.
    MissingPortRuntime { port: PortRef },
    /// A required resource input has no current value.
    MissingRequiredResource { port: PortRef },
    /// Policy values are not exact resource snapshots.
    NonResourceValue { port: PortRef },
    /// The retained value does not match its immutable port declaration.
    PortTypeMismatch { port: PortRef },
    /// The immutable resource schema rejected the retained value.
    InvalidResource {
        port: PortRef,
        source: ResourceSchemaError,
    },
    /// A compiled context field or port key is not a valid typed-map identifier.
    InvalidIdentifier {
        value: String,
        source: ConfigContractError,
    },
    /// Resource schema versions must fit the closed typed integer transport.
    SchemaVersionOutOfRange { port: PortRef, version: u64 },
}

impl fmt::Display for ProviderActionContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScreen { screen } => {
                write!(formatter, "open screen {screen} is not published")
            }
            Self::MissingPortRuntime { port } => {
                write!(formatter, "resource port {port} has no instance runtime")
            }
            Self::MissingRequiredResource { port } => {
                write!(formatter, "required resource port {port} is absent")
            }
            Self::NonResourceValue { port } => {
                write!(formatter, "resource port {port} contains a policy value")
            }
            Self::PortTypeMismatch { port } => {
                write!(formatter, "resource value does not match port {port}")
            }
            Self::InvalidResource { port, source } => {
                write!(formatter, "resource port {port} is invalid: {source}")
            }
            Self::InvalidIdentifier { value, source } => {
                write!(
                    formatter,
                    "provider context identifier {value:?} is invalid: {source}"
                )
            }
            Self::SchemaVersionOutOfRange { port, version } => write!(
                formatter,
                "resource port {port} schema version {version} exceeds the typed integer range"
            ),
        }
    }
}

impl std::error::Error for ProviderActionContextError {}

/// Project all current declared resource values for the exact open instance.
///
/// Context keys are declared `panel.port` identities. Each value is a closed
/// map containing immutable owner/type/version/key metadata and the exact
/// schema-validated resource payload. Absent optional ports are omitted;
/// required inputs and policy values fail before provider invocation.
pub fn project_current_context(state: &AppState) -> Result<TypedMap, ProviderActionContextError> {
    let current = state.nav.current();
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(current.screen)
        .ok_or(ProviderActionContextError::UnknownScreen {
            screen: current.screen,
        })?;
    let mut context = TypedMap::new();
    for panel in &descriptor.panels {
        for declared in &panel.ports {
            let port = PortRef {
                panel: panel.id,
                port: declared.id,
            };
            if let Some(value) = project_port_context(state, declared, port)? {
                context.insert(context_id(&port.to_string())?, value);
            }
        }
    }
    Ok(context)
}

fn project_port_context(
    state: &AppState,
    declared: &PortDescriptor,
    port: PortRef,
) -> Result<Option<TypedValue>, ProviderActionContextError> {
    let current = state.nav.current();
    let runtime = current
        .relationships()
        .ok_or(ProviderActionContextError::MissingPortRuntime { port })?;
    let key = runtime
        .port_key(&port)
        .ok_or(ProviderActionContextError::MissingPortRuntime { port })?;
    match current.relationship_state().value(&key) {
        PortValue::Absent => project_absent_resource(declared, port),
        PortValue::All => Err(ProviderActionContextError::NonResourceValue { port }),
        PortValue::Typed(value) => project_typed_resource(state, declared, port, value).map(Some),
    }
}

fn project_absent_resource(
    declared: &PortDescriptor,
    port: PortRef,
) -> Result<Option<TypedValue>, ProviderActionContextError> {
    if declared.required {
        Err(ProviderActionContextError::MissingRequiredResource { port })
    } else {
        Ok(None)
    }
}
fn project_typed_resource(
    state: &AppState,
    declared: &PortDescriptor,
    port: PortRef,
    value: TypedPortValue,
) -> Result<TypedValue, ProviderActionContextError> {
    if declared.type_id.name() != value.type_id.as_str()
        || declared.type_id.version() != value.schema_version
    {
        return Err(ProviderActionContextError::PortTypeMismatch { port });
    }
    state
        .published_workbench()
        .resource_schemas()
        .validate(&declared.owner_id, &value)
        .map_err(|source| ProviderActionContextError::InvalidResource { port, source })?;
    let schema_version = i64::try_from(value.schema_version).map_err(|_| {
        ProviderActionContextError::SchemaVersionOutOfRange {
            port,
            version: value.schema_version,
        }
    })?;
    Ok(TypedValue::Map(
        [
            (
                context_id(OWNER_ID)?,
                TypedValue::String(declared.owner_id.to_string()),
            ),
            (
                context_id(TYPE_ID)?,
                TypedValue::String(value.type_id.to_string()),
            ),
            (
                context_id(SCHEMA_VERSION)?,
                TypedValue::Integer(schema_version),
            ),
            (
                context_id(SEMANTIC_KEY)?,
                TypedValue::String(value.semantic_key),
            ),
            (context_id(VALUE)?, TypedValue::Map(value.value)),
        ]
        .into(),
    ))
}

fn context_id(value: &str) -> Result<Id, ProviderActionContextError> {
    Id::parse(value).map_err(|source| ProviderActionContextError::InvalidIdentifier {
        value: value.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::{PanelId, PortDirection, PortId, VersionedTypeId};

    fn required_port(direction: PortDirection) -> (PortDescriptor, PortRef) {
        let id =
            PortId::parse("resource").unwrap_or_else(|error| panic!("fixture port id: {error}"));
        let panel =
            PanelId::parse("panel").unwrap_or_else(|error| panic!("fixture panel id: {error}"));
        (
            PortDescriptor {
                id,
                owner_id: Id::parse("owner")
                    .unwrap_or_else(|error| panic!("fixture owner: {error}")),
                direction,
                type_id: VersionedTypeId::parse("resource@1")
                    .unwrap_or_else(|error| panic!("fixture type: {error}")),
                required: true,
                retained: false,
            },
            PortRef { panel, port: id },
        )
    }

    #[test]
    fn absent_required_output_refuses_provider_context_projection() {
        let (declared, port) = required_port(PortDirection::Output);

        assert_eq!(
            project_absent_resource(&declared, port),
            Err(ProviderActionContextError::MissingRequiredResource { port })
        );
    }
}
