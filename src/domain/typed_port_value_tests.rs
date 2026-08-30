use std::collections::BTreeMap;

use super::{Id, TypedPortValue, TypedValue};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| unreachable!("valid fixture id: {error}"))
}

fn issue_value(number: i64) -> TypedPortValue {
    TypedPortValue {
        type_id: id("github.issue"),
        schema_version: 1,
        semantic_key: number.to_string(),
        value: BTreeMap::from([(id("number"), TypedValue::Integer(number))]),
    }
}

#[test]
fn typed_port_values_expose_the_closed_four_field_contract() {
    let value = issue_value(42);

    assert_eq!(value.type_id.as_str(), "github.issue");
    assert_eq!(value.schema_version, 1);
    assert_eq!(value.semantic_key, "42");
    assert_eq!(
        value.value.get(&id("number")),
        Some(&TypedValue::Integer(42))
    );
}

#[test]
fn all_four_fields_participate_in_value_identity() {
    let value = issue_value(42);
    let mut changed_type = value.clone();
    changed_type.type_id = id("github.pull-request");
    let mut changed_version = value.clone();
    changed_version.schema_version = 2;
    let mut changed_key = value.clone();
    changed_key.semantic_key = "43".to_owned();
    let mut changed_payload = value.clone();
    changed_payload
        .value
        .insert(id("number"), TypedValue::Integer(43));

    assert_ne!(value, changed_type);
    assert_ne!(value, changed_version);
    assert_ne!(value, changed_key);
    assert_ne!(value, changed_payload);
}
