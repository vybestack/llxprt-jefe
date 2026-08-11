use super::{ConfigValueErrorKind, field_visible, validate_config, validate_field_value};
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope, Scalar};
use crate::domain::plugin::surface::ConfigSchema;
use crate::domain::{CanonicalDecimal, Id, SecretRef, TypedMap, TypedValue};
use crate::test_support::Must;

fn id(value: &str) -> Id {
    Id::parse(value).must("test id")
}

fn field(kind: FieldKind) -> Field {
    Field::parse(field_draft(kind)).must("field")
}

#[test]
fn every_closed_field_type_accepts_only_its_typed_value() {
    let cases = [
        (FieldKind::Boolean, TypedValue::Bool(true)),
        (FieldKind::String, TypedValue::String("text".to_owned())),
        (FieldKind::Integer, TypedValue::Integer(7)),
        (
            FieldKind::FiniteNumber,
            TypedValue::Decimal(CanonicalDecimal::parse("1.5").must("decimal")),
        ),
        (FieldKind::Path, TypedValue::String("/tmp".to_owned())),
        (
            FieldKind::StringList,
            TypedValue::List(vec![TypedValue::String("a".to_owned())]),
        ),
        (
            FieldKind::SecretReference,
            TypedValue::SecretRef(SecretRef {
                env: crate::domain::plugin::SecretReference::parse("TOKEN")
                    .must("secret environment fixture"),
            }),
        ),
    ];
    for (kind, value) in cases {
        assert_eq!(validate_field_value(&field(kind), &value), Ok(()));
        assert_eq!(
            validate_field_value(&field(kind), &TypedValue::Map(TypedMap::new())),
            Err(ConfigValueErrorKind::Type)
        );
    }
}

#[test]
fn inclusive_bounds_enum_choices_and_unique_lists_are_enforced() {
    let bounded = Field::parse(FieldDraft {
        min: Some(Scalar::Integer(2)),
        max: Some(Scalar::Integer(3)),
        ..field_draft(FieldKind::String)
    })
    .must("bounded field");
    assert_eq!(
        validate_field_value(&bounded, &TypedValue::String("ab".to_owned())),
        Ok(())
    );
    assert_eq!(
        validate_field_value(&bounded, &TypedValue::String("a".to_owned())),
        Err(ConfigValueErrorKind::BelowMinimum)
    );

    let enum_field = Field::parse(FieldDraft {
        choices: vec![
            Scalar::Text("one".to_owned()),
            Scalar::Text("two".to_owned()),
        ],
        ..field_draft(FieldKind::Enum)
    })
    .must("enum field");
    assert_eq!(
        validate_field_value(&enum_field, &TypedValue::String("three".to_owned())),
        Err(ConfigValueErrorKind::Choice)
    );

    let unique = Field::parse(FieldDraft {
        unique: true,
        ..field_draft(FieldKind::StringList)
    })
    .must("unique field");
    assert_eq!(
        validate_field_value(
            &unique,
            &TypedValue::List(vec![
                TypedValue::String("same".to_owned()),
                TypedValue::String("same".to_owned()),
            ])
        ),
        Err(ConfigValueErrorKind::Duplicate)
    );
}

#[test]
fn path_values_accept_4096_bytes_and_reject_4097() {
    let path = field(FieldKind::Path);
    assert_eq!(
        validate_field_value(
            &path,
            &TypedValue::String("x".repeat(crate::domain::plugin::field::PATH_VALUE_BYTE_LIMIT))
        ),
        Ok(())
    );
    assert_eq!(
        validate_field_value(
            &path,
            &TypedValue::String(
                "x".repeat(crate::domain::plugin::field::PATH_VALUE_BYTE_LIMIT + 1)
            )
        ),
        Err(ConfigValueErrorKind::AboveMaximum)
    );
}

#[test]
fn visibility_is_a_sibling_present_truthy_gate_and_required_is_visible_only() {
    let gate = Field::parse(FieldDraft {
        id: id("enabled"),
        label: "Enabled".to_owned(),
        kind: FieldKind::Boolean,
        required: false,
        ..field_draft(FieldKind::Boolean)
    })
    .must("gate");
    let dependent = Field::parse(FieldDraft {
        visible_when: Some(id("enabled")),
        ..field_draft(FieldKind::String)
    })
    .must("dependent");
    let schema = ConfigSchema::parse(1, vec![gate, dependent.clone()]).must("schema");
    let mut values = TypedMap::new();
    values.insert(id("enabled"), TypedValue::Bool(false));
    assert!(!field_visible(&dependent, schema.fields(), &values));
    assert!(validate_config(&schema, &values).is_empty());
    values.insert(id("enabled"), TypedValue::Bool(true));
    assert!(field_visible(&dependent, schema.fields(), &values));
    assert_eq!(
        validate_config(&schema, &values)[0].reason,
        ConfigValueErrorKind::Required
    );
}

#[test]
fn visibility_uses_effective_defaults_and_transitive_gates() {
    let gate = Field::parse(FieldDraft {
        id: id("gate"),
        label: "Gate".to_owned(),
        kind: FieldKind::Boolean,
        default: Some(TypedValue::Bool(true)),
        ..field_draft(FieldKind::Boolean)
    })
    .must("gate");
    let middle = Field::parse(FieldDraft {
        id: id("middle"),
        label: "Middle".to_owned(),
        kind: FieldKind::Boolean,
        default: Some(TypedValue::Bool(true)),
        visible_when: Some(id("gate")),
        ..field_draft(FieldKind::Boolean)
    })
    .must("middle");
    let dependent = Field::parse(FieldDraft {
        id: id("dependent"),
        label: "Dependent".to_owned(),
        required: true,
        visible_when: Some(id("middle")),
        ..field_draft(FieldKind::String)
    })
    .must("dependent");
    let schema = ConfigSchema::parse(1, vec![gate, middle, dependent]).must("schema");

    assert_eq!(
        validate_config(&schema, &TypedMap::new())[0].reason,
        ConfigValueErrorKind::Required
    );

    let mut values = TypedMap::new();
    values.insert(id("gate"), TypedValue::Bool(false));
    values.insert(id("middle"), TypedValue::Bool(true));
    assert!(validate_config(&schema, &values).is_empty());
}

#[test]
fn complete_config_rejects_unknown_fields_without_echoing_values() {
    let schema = ConfigSchema::parse(1, vec![]).must("schema");
    let mut values = TypedMap::new();
    values.insert(id("unknown"), TypedValue::String("secret-value".to_owned()));
    let errors = validate_config(&schema, &values);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field, id("unknown"));
    assert_eq!(errors[0].reason, ConfigValueErrorKind::Unknown);
}

fn field_draft(kind: FieldKind) -> FieldDraft {
    FieldDraft {
        id: id("value"),
        label: "Value".to_owned(),
        description: None,
        kind,
        required: true,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    }
}

#[test]
fn effective_values_fills_declared_defaults_for_unset_fields() {
    use crate::domain::plugin_config::effective_values;
    let field = Field::parse(FieldDraft {
        default: Some(TypedValue::Integer(42)),
        ..field_draft(FieldKind::Integer)
    })
    .must("field with default");
    let schema = ConfigSchema::parse(1, vec![field]).must("schema");
    let effective = effective_values(&schema, &TypedMap::new());
    assert_eq!(effective.get(&id("value")), Some(&TypedValue::Integer(42)));
}

#[test]
fn effective_values_preserves_user_values_over_defaults() {
    use crate::domain::plugin_config::effective_values;
    let field = Field::parse(FieldDraft {
        default: Some(TypedValue::Integer(42)),
        ..field_draft(FieldKind::Integer)
    })
    .must("field with default");
    let schema = ConfigSchema::parse(1, vec![field]).must("schema");
    let mut values = TypedMap::new();
    values.insert(id("value"), TypedValue::Integer(7));
    let effective = effective_values(&schema, &values);
    assert_eq!(
        effective.get(&id("value")),
        Some(&TypedValue::Integer(7)),
        "a user value overrides the default"
    );
}

#[test]
fn a_required_visible_field_with_a_valid_default_is_not_missing() {
    // A required field with a default must not be reported missing when no
    // user value is supplied: the effective-default rule fills it in.
    let field = Field::parse(FieldDraft {
        default: Some(TypedValue::String("fallback".to_owned())),
        ..field_draft(FieldKind::String)
    })
    .must("field with default");
    let schema = ConfigSchema::parse(1, vec![field]).must("schema");
    let values = TypedMap::new();
    assert!(
        validate_config(&schema, &values).is_empty(),
        "a required field with a valid default must not be reported missing"
    );
}

#[test]
fn a_required_visible_field_without_a_default_is_missing() {
    let schema = ConfigSchema::parse(1, vec![field(FieldKind::String)]).must("schema");
    let values = TypedMap::new();
    let errors = validate_config(&schema, &values);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].reason, ConfigValueErrorKind::Required);
}
