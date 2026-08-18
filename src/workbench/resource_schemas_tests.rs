use std::collections::BTreeMap;

use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::plugin::surface::ConfigSchema;
use crate::domain::{Id, TypedPortValue, TypedValue};

use super::{ResourceSchema, ResourceSchemaError, ResourceSchemaRegistry};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| unreachable!("valid fixture id: {error}"))
}

fn issue_schema(owner: &str) -> ResourceSchema {
    let number = Field::parse(FieldDraft {
        id: id("number"),
        label: "Number".to_owned(),
        description: None,
        kind: FieldKind::Integer,
        required: true,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| unreachable!("valid fixture field: {error}"));
    let fields = ConfigSchema::parse(1, vec![number])
        .unwrap_or_else(|error| unreachable!("valid fixture schema: {error}"));
    ResourceSchema::new(id(owner), id("github.issue"), 1, id("number"), fields)
        .unwrap_or_else(|error| unreachable!("valid fixture resource schema: {error}"))
}

fn registry() -> ResourceSchemaRegistry {
    ResourceSchemaRegistry::publish(vec![issue_schema("jefe")])
        .unwrap_or_else(|error| unreachable!("valid fixture registry: {error}"))
}

fn issue_value(version: u64, key: &str, number: TypedValue) -> TypedPortValue {
    TypedPortValue {
        type_id: id("github.issue"),
        schema_version: version,
        semantic_key: key.to_owned(),
        value: BTreeMap::from([(id("number"), number)]),
    }
}

#[test]
fn a_published_resource_schema_validates_all_typed_port_value_fields() {
    let registry = registry();
    let owner = id("jefe");

    assert_eq!(
        registry.validate(&owner, &issue_value(1, "42", TypedValue::Integer(42))),
        Ok(())
    );
    assert!(matches!(
        registry.validate(&owner, &issue_value(2, "42", TypedValue::Integer(42))),
        Err(ResourceSchemaError::VersionMismatch { .. })
    ));
    assert!(matches!(
        registry.validate(
            &id("vendor.extension"),
            &issue_value(1, "42", TypedValue::Integer(42))
        ),
        Err(ResourceSchemaError::OwnerMismatch { .. })
    ));
    assert!(matches!(
        registry.validate(&owner, &issue_value(1, "43", TypedValue::Integer(42))),
        Err(ResourceSchemaError::SemanticKeyMismatch { .. })
    ));
    assert!(matches!(
        registry.validate(
            &owner,
            &TypedPortValue {
                type_id: id("github.unknown"),
                schema_version: 1,
                semantic_key: "42".to_owned(),
                value: BTreeMap::new(),
            }
        ),
        Err(ResourceSchemaError::UnknownType { .. })
    ));
}

#[test]
fn unknown_fields_and_wrong_payload_types_are_rejected() {
    let registry = registry();
    let owner = id("jefe");
    let mut unknown = issue_value(1, "42", TypedValue::Integer(42));
    unknown
        .value
        .insert(id("extra"), TypedValue::String("smuggled".to_owned()));

    assert!(matches!(
        registry.validate(&owner, &unknown),
        Err(ResourceSchemaError::InvalidFields { .. })
    ));
    assert!(matches!(
        registry.validate(
            &owner,
            &issue_value(1, "42", TypedValue::String("42".to_owned()))
        ),
        Err(ResourceSchemaError::InvalidFields { .. })
    ));
}

#[test]
fn duplicate_type_versions_and_zero_versions_fail_publication() {
    let schema = issue_schema("jefe");
    assert!(matches!(
        ResourceSchemaRegistry::publish(vec![schema.clone(), schema]),
        Err(ResourceSchemaError::DuplicateSchema { .. })
    ));

    let fields = ConfigSchema::parse(1, Vec::new())
        .unwrap_or_else(|error| unreachable!("valid empty fixture schema: {error}"));
    assert!(matches!(
        ResourceSchema::new(id("jefe"), id("github.issue"), 0, id("number"), fields),
        Err(ResourceSchemaError::InvalidVersion { version: 0 })
    ));
}
