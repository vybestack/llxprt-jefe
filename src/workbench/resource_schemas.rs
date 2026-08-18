//! Immutable schemas for typed values carried by screen control ports.

use std::collections::BTreeMap;
use std::fmt;

use crate::domain::plugin::field::{Field, FieldDraft, FieldError, FieldKind, RestartScope};
use crate::domain::plugin::surface::{ConfigSchema, ConfigSchemaError};
use crate::domain::plugin_config::{ConfigValueError, validate_fields};
use crate::domain::{ConfigContractError, Id, TypedPortValue, TypedValue};

/// One immutable resource schema and the owner allowed to publish it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceSchema {
    owner_id: Id,
    type_id: Id,
    schema_version: u64,
    semantic_key_field: Id,
    fields: ConfigSchema,
}

impl ResourceSchema {
    /// Validate one resource schema before candidate publication.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceSchemaError`] for a zero or inconsistent version, or
    /// when the semantic-key field is absent, optional, or not string/integer.
    pub fn new(
        owner_id: Id,
        type_id: Id,
        schema_version: u64,
        semantic_key_field: Id,
        fields: ConfigSchema,
    ) -> Result<Self, ResourceSchemaError> {
        if schema_version == 0 {
            return Err(ResourceSchemaError::InvalidVersion {
                version: schema_version,
            });
        }
        if fields.schema_version() != schema_version {
            return Err(ResourceSchemaError::SchemaVersionMismatch {
                declared: schema_version,
                fields: fields.schema_version(),
            });
        }
        let Some(field) = fields
            .fields()
            .iter()
            .find(|field| field.id() == &semantic_key_field)
        else {
            return Err(ResourceSchemaError::MissingSemanticKeyField {
                field: semantic_key_field,
            });
        };
        if !field.required() || !matches!(field.kind(), FieldKind::String | FieldKind::Integer) {
            return Err(ResourceSchemaError::InvalidSemanticKeyField {
                field: semantic_key_field,
            });
        }
        Ok(Self {
            owner_id,
            type_id,
            schema_version,
            semantic_key_field,
            fields,
        })
    }

    /// The schema owner.
    #[must_use]
    pub const fn owner_id(&self) -> &Id {
        &self.owner_id
    }

    /// The bare resource type identifier.
    #[must_use]
    pub const fn type_id(&self) -> &Id {
        &self.type_id
    }

    /// The exact schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u64 {
        self.schema_version
    }
}

/// Validated immutable catalog used before relationship commits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceSchemaRegistry {
    schemas: BTreeMap<(Id, u64), ResourceSchema>,
}

impl ResourceSchemaRegistry {
    /// Publish a complete schema catalog, rejecting duplicate type versions.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceSchemaError::DuplicateSchema`] when two entries claim
    /// one `(type_id, schema_version)` identity.
    pub fn publish(schemas: Vec<ResourceSchema>) -> Result<Self, ResourceSchemaError> {
        let mut published = BTreeMap::new();
        for schema in schemas {
            let key = (schema.type_id.clone(), schema.schema_version);
            if published.insert(key.clone(), schema).is_some() {
                return Err(ResourceSchemaError::DuplicateSchema {
                    type_id: key.0,
                    schema_version: key.1,
                });
            }
        }
        Ok(Self { schemas: published })
    }

    /// Validate one typed value against its exact published schema and owner.
    ///
    /// # Errors
    ///
    /// Unknown types, versions, owners, fields, payload types, and semantic-key
    /// mismatches are rejected without coercion.
    pub fn validate(
        &self,
        owner_id: &Id,
        value: &TypedPortValue,
    ) -> Result<(), ResourceSchemaError> {
        let Some(schema) = self
            .schemas
            .get(&(value.type_id.clone(), value.schema_version))
        else {
            if self
                .schemas
                .keys()
                .any(|(type_id, _)| type_id == &value.type_id)
            {
                return Err(ResourceSchemaError::VersionMismatch {
                    type_id: value.type_id.clone(),
                    actual: value.schema_version,
                });
            }
            return Err(ResourceSchemaError::UnknownType {
                type_id: value.type_id.clone(),
            });
        };
        if schema.owner_id != *owner_id {
            return Err(ResourceSchemaError::OwnerMismatch {
                expected: schema.owner_id.clone(),
                actual: owner_id.clone(),
            });
        }
        let field_errors = validate_fields(schema.fields.fields(), &value.value);
        if !field_errors.is_empty() {
            return Err(ResourceSchemaError::InvalidFields {
                errors: field_errors,
            });
        }
        let actual_key = semantic_key(&value.value[&schema.semantic_key_field]);
        if actual_key.as_deref() != Some(value.semantic_key.as_str()) {
            return Err(ResourceSchemaError::SemanticKeyMismatch {
                field: schema.semantic_key_field.clone(),
            });
        }
        Ok(())
    }
}

/// Publish the resource schemas owned by compiled screen definitions.
///
/// # Errors
///
/// Returns [`BuiltinResourceSchemaError`] if a compiled declaration violates
/// the same identifier, field, or schema rules applied to external definitions.
pub fn builtin_resource_schemas() -> Result<ResourceSchemaRegistry, BuiltinResourceSchemaError> {
    let declarations = [
        ("github.issues", "github.issue"),
        ("github.pull-requests", "github.pull-request"),
    ];
    let mut schemas = Vec::with_capacity(declarations.len());
    for (owner, type_id) in declarations {
        let semantic_key = Id::parse("semantic-key")?;
        let field = Field::parse(FieldDraft {
            id: semantic_key.clone(),
            label: "Semantic key".to_owned(),
            description: Some("Stable resource identity".to_owned()),
            kind: FieldKind::String,
            required: true,
            default: None,
            min: None,
            max: None,
            choices: Vec::new(),
            unique: false,
            visible_when: None,
            restart: RestartScope::None,
        })?;
        let fields = ConfigSchema::parse(1, vec![field])?;
        schemas.push(ResourceSchema::new(
            Id::parse(owner)?,
            Id::parse(type_id)?,
            1,
            semantic_key,
            fields,
        )?);
    }
    ResourceSchemaRegistry::publish(schemas).map_err(BuiltinResourceSchemaError::Resource)
}

/// A compiled resource schema violated its own closed declaration contract.
#[derive(Debug)]
pub enum BuiltinResourceSchemaError {
    /// A compiled identifier is invalid.
    Identifier(ConfigContractError),
    /// A compiled field is invalid.
    Field(FieldError),
    /// The compiled field collection is invalid.
    Fields(ConfigSchemaError),
    /// The assembled resource schema registry is invalid.
    Resource(ResourceSchemaError),
}

impl From<ConfigContractError> for BuiltinResourceSchemaError {
    fn from(error: ConfigContractError) -> Self {
        Self::Identifier(error)
    }
}

impl From<FieldError> for BuiltinResourceSchemaError {
    fn from(error: FieldError) -> Self {
        Self::Field(error)
    }
}

impl From<ConfigSchemaError> for BuiltinResourceSchemaError {
    fn from(error: ConfigSchemaError) -> Self {
        Self::Fields(error)
    }
}

impl From<ResourceSchemaError> for BuiltinResourceSchemaError {
    fn from(error: ResourceSchemaError) -> Self {
        Self::Resource(error)
    }
}

impl fmt::Display for BuiltinResourceSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(error) => write!(formatter, "compiled resource identifier: {error}"),
            Self::Field(error) => write!(formatter, "compiled resource field: {error}"),
            Self::Fields(error) => write!(formatter, "compiled resource fields: {error}"),
            Self::Resource(error) => write!(formatter, "compiled resource registry: {error}"),
        }
    }
}

impl std::error::Error for BuiltinResourceSchemaError {}

fn semantic_key(value: &TypedValue) -> Option<String> {
    match value {
        TypedValue::String(value) => Some(value.clone()),
        TypedValue::Integer(value) => Some(value.to_string()),
        TypedValue::Bool(_)
        | TypedValue::Decimal(_)
        | TypedValue::Datetime(_)
        | TypedValue::List(_)
        | TypedValue::Map(_)
        | TypedValue::SecretRef(_) => None,
    }
}

/// Failure to publish or validate an immutable resource schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceSchemaError {
    /// Resource schema versions begin at one.
    InvalidVersion { version: u64 },
    /// The field schema declared a different version.
    SchemaVersionMismatch { declared: u64, fields: u64 },
    /// The semantic-key field was not declared.
    MissingSemanticKeyField { field: Id },
    /// The semantic-key field must be required string or integer data.
    InvalidSemanticKeyField { field: Id },
    /// One type/version identity was published twice.
    DuplicateSchema { type_id: Id, schema_version: u64 },
    /// No schema exists for the supplied type.
    UnknownType { type_id: Id },
    /// The type exists, but not at the supplied version.
    VersionMismatch { type_id: Id, actual: u64 },
    /// The publisher does not own this schema.
    OwnerMismatch { expected: Id, actual: Id },
    /// The closed field schema rejected the payload.
    InvalidFields { errors: Vec<ConfigValueError> },
    /// The semantic identity did not match its canonical payload field.
    SemanticKeyMismatch { field: Id },
}

impl fmt::Display for ResourceSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVersion { version } => {
                write!(formatter, "resource schema version {version} is invalid")
            }
            Self::SchemaVersionMismatch { declared, fields } => write!(
                formatter,
                "resource schema version {declared} does not match field schema version {fields}"
            ),
            Self::MissingSemanticKeyField { field } => {
                write!(formatter, "semantic-key field {field} is not declared")
            }
            Self::InvalidSemanticKeyField { field } => write!(
                formatter,
                "semantic-key field {field} must be a required string or integer"
            ),
            Self::DuplicateSchema {
                type_id,
                schema_version,
            } => write!(
                formatter,
                "resource schema {type_id}@{schema_version} is declared twice"
            ),
            Self::UnknownType { type_id } => {
                write!(formatter, "resource type {type_id} is not published")
            }
            Self::VersionMismatch { type_id, actual } => write!(
                formatter,
                "resource type {type_id} version {actual} is not published"
            ),
            Self::OwnerMismatch { expected, actual } => write!(
                formatter,
                "resource schema owner {actual} does not match {expected}"
            ),
            Self::InvalidFields { errors } => {
                write!(
                    formatter,
                    "resource payload has {} invalid fields",
                    errors.len()
                )
            }
            Self::SemanticKeyMismatch { field } => write!(
                formatter,
                "resource semantic key does not match payload field {field}"
            ),
        }
    }
}

impl std::error::Error for ResourceSchemaError {}
