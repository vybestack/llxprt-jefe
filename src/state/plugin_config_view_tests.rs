//! Behavioral tests for generated plugin config projection
//! (issue #391, CW11-06/CW11-07).
//!
//! @requirement CW11-06
//! @requirement CW11-07

use crate::domain::plugin::SecretReference;
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope, Scalar};
use crate::domain::plugin::surface::ConfigSchema;
use crate::domain::plugin_config::ConfigValueErrorKind;
use crate::domain::{CanonicalDecimal, Id, SecretRef, TypedMap, TypedValue};

use super::{PluginConfigControl, PluginConfigError, project_plugin_config};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("test id: {error}"))
}

/// Build a field draft for `value` with the given kind and sensible defaults.
fn draft(kind: FieldKind) -> FieldDraft {
    FieldDraft {
        id: id("value"),
        label: "Value".to_owned(),
        description: None,
        kind,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: Vec::new(),
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    }
}

/// Build a schema with one field, replacing the `value` field declaration.
fn one_field(field: FieldDraft) -> ConfigSchema {
    ConfigSchema::parse(
        1,
        vec![Field::parse(field).unwrap_or_else(|error| panic!("field fixture: {error}"))],
    )
    .unwrap_or_else(|error| panic!("schema fixture: {error}"))
}

fn decimal(text: &str) -> CanonicalDecimal {
    CanonicalDecimal::parse(text).unwrap_or_else(|error| panic!("decimal fixture: {error}"))
}

fn projected_control(kind: FieldKind, value: TypedValue) -> PluginConfigControl {
    let schema = one_field(draft(kind));
    let mut values = TypedMap::new();
    values.insert(id("value"), value);
    project_plugin_config(&schema, &values)[0].control.clone()
}

#[test]
fn scalar_kinds_emit_typed_controls() {
    assert_eq!(
        projected_control(FieldKind::Boolean, TypedValue::Bool(true)),
        PluginConfigControl::Boolean { value: true }
    );
    for kind in [FieldKind::String, FieldKind::Path] {
        assert_eq!(
            projected_control(kind, TypedValue::String("hello".to_owned())),
            PluginConfigControl::Scalar {
                value: "hello".to_owned()
            }
        );
    }
    assert_eq!(
        projected_control(FieldKind::Integer, TypedValue::Integer(42)),
        PluginConfigControl::Scalar {
            value: "42".to_owned()
        }
    );
    assert_eq!(
        projected_control(FieldKind::FiniteNumber, TypedValue::Decimal(decimal("1.5"))),
        PluginConfigControl::Scalar {
            value: "1.5".to_owned()
        }
    );
}

#[test]
fn list_enum_and_secret_emit_typed_controls() {
    let list = TypedValue::List(vec![
        TypedValue::String("a".to_owned()),
        TypedValue::String("b".to_owned()),
    ]);
    assert_eq!(
        projected_control(FieldKind::StringList, list),
        PluginConfigControl::Scalar {
            value: "a, b".to_owned()
        }
    );
    let schema = one_field(FieldDraft {
        choices: vec![
            Scalar::Text("one".to_owned()),
            Scalar::Text("two".to_owned()),
        ],
        ..draft(FieldKind::Enum)
    });
    let mut values = TypedMap::new();
    values.insert(id("value"), TypedValue::String("two".to_owned()));
    assert_eq!(
        project_plugin_config(&schema, &values)[0].control,
        PluginConfigControl::Enum {
            selected: Some("two".to_owned()),
            choices: vec!["one".to_owned(), "two".to_owned()],
        }
    );
    let secret = TypedValue::SecretRef(SecretRef {
        env: SecretReference::parse("API_TOKEN")
            .unwrap_or_else(|error| panic!("secret fixture: {error}")),
    });
    assert_eq!(
        projected_control(FieldKind::SecretReference, secret),
        PluginConfigControl::SecretReference {
            set: true,
            env: Some("API_TOKEN".to_owned()),
        }
    );
}

#[test]
fn labels_descriptions_defaults_bounds_choices_unique_restart_are_projected() {
    let schema = one_field(FieldDraft {
        label: "API endpoint".to_owned(),
        description: Some("Where requests go".to_owned()),
        kind: FieldKind::String,
        required: true,
        default: Some(TypedValue::String("https://example.com".to_owned())),
        min: Some(Scalar::Integer(8)),
        max: Some(Scalar::Integer(1024)),
        restart: RestartScope::Provider,
        ..draft(FieldKind::String)
    });
    let row = &project_plugin_config(&schema, &TypedMap::new())[0];
    assert_eq!(row.label, "API endpoint");
    assert_eq!(row.description.as_deref(), Some("Where requests go"));
    assert!(row.required);
    assert_eq!(row.default.as_deref(), Some("https://example.com"));
    assert_eq!(row.min.as_deref(), Some("8"));
    assert_eq!(row.max.as_deref(), Some("1024"));
    assert_eq!(row.restart, RestartScope::Provider);
}

#[test]
fn enum_choices_and_unique_flag_are_projected() {
    // `unique` is only legal on string-list, so a list field declares it.
    let schema = ConfigSchema::parse(
        1,
        vec![
            Field::parse(FieldDraft {
                choices: Vec::new(),
                unique: true,
                ..draft(FieldKind::StringList)
            })
            .unwrap_or_else(|error| panic!("field fixture: {error}")),
        ],
    )
    .unwrap_or_else(|error| panic!("schema fixture: {error}"));
    let row = &project_plugin_config(&schema, &TypedMap::new())[0];
    assert!(row.unique);
    // Choices belong to enum only; a string-list carries none.
    assert!(row.choices.is_empty());

    // An enum field projects its choices.
    let schema = one_field(FieldDraft {
        choices: vec![
            Scalar::Text("alpha".to_owned()),
            Scalar::Text("beta".to_owned()),
        ],
        ..draft(FieldKind::Enum)
    });
    let row = &project_plugin_config(&schema, &TypedMap::new())[0];
    assert_eq!(row.choices, vec!["alpha".to_owned(), "beta".to_owned()]);
}

#[test]
fn required_visible_missing_field_reports_required_error() {
    let schema = one_field(FieldDraft {
        required: true,
        ..draft(FieldKind::String)
    });
    let row = &project_plugin_config(&schema, &TypedMap::new())[0];
    assert_eq!(
        row.error,
        Some(PluginConfigError {
            reason: ConfigValueErrorKind::Required,
        })
    );

    let mut values = TypedMap::new();
    values.insert(id("value"), TypedValue::String("ok".to_owned()));
    let row = &project_plugin_config(&schema, &values)[0];
    assert!(row.error.is_none());
}

#[test]
fn adjacent_errors_appear_for_invalid_values() {
    // Below minimum length.
    let schema = one_field(FieldDraft {
        min: Some(Scalar::Integer(5)),
        ..draft(FieldKind::String)
    });
    let mut values = TypedMap::new();
    values.insert(id("value"), TypedValue::String("ab".to_owned()));
    let row = &project_plugin_config(&schema, &values)[0];
    assert_eq!(
        row.error,
        Some(PluginConfigError {
            reason: ConfigValueErrorKind::BelowMinimum
        })
    );

    // Enum choice violation.
    let schema = one_field(FieldDraft {
        choices: vec![Scalar::Text("one".to_owned())],
        ..draft(FieldKind::Enum)
    });
    let mut values = TypedMap::new();
    values.insert(id("value"), TypedValue::String("two".to_owned()));
    let row = &project_plugin_config(&schema, &values)[0];
    assert_eq!(
        row.error,
        Some(PluginConfigError {
            reason: ConfigValueErrorKind::Choice
        })
    );

    // Duplicate in a unique list.
    let schema = one_field(FieldDraft {
        unique: true,
        ..draft(FieldKind::StringList)
    });
    let mut values = TypedMap::new();
    values.insert(
        id("value"),
        TypedValue::List(vec![
            TypedValue::String("dup".to_owned()),
            TypedValue::String("dup".to_owned()),
        ]),
    );
    let row = &project_plugin_config(&schema, &values)[0];
    assert_eq!(
        row.error,
        Some(PluginConfigError {
            reason: ConfigValueErrorKind::Duplicate
        })
    );

    // Type mismatch.
    let schema = one_field(draft(FieldKind::Integer));
    let mut values = TypedMap::new();
    values.insert(id("value"), TypedValue::String("not-int".to_owned()));
    let row = &project_plugin_config(&schema, &values)[0];
    assert_eq!(
        row.error,
        Some(PluginConfigError {
            reason: ConfigValueErrorKind::Type
        })
    );
}

#[test]
fn above_maximum_error_is_projected_adjacent_to_the_field() {
    let schema = one_field(FieldDraft {
        max: Some(Scalar::Integer(3)),
        ..draft(FieldKind::String)
    });
    let mut values = TypedMap::new();
    values.insert(id("value"), TypedValue::String("abcd".to_owned()));

    let row = &project_plugin_config(&schema, &values)[0];

    assert_eq!(
        row.error,
        Some(PluginConfigError {
            reason: ConfigValueErrorKind::AboveMaximum
        })
    );
}

#[test]
fn hidden_field_is_marked_hidden_by_visibility_gate() {
    let gate = Field::parse(FieldDraft {
        id: id("enabled"),
        label: "Enabled".to_owned(),
        kind: FieldKind::Boolean,
        ..draft(FieldKind::Boolean)
    })
    .unwrap_or_else(|error| panic!("gate fixture: {error}"));
    let dependent = Field::parse(FieldDraft {
        id: id("dep"),
        label: "Dependent".to_owned(),
        kind: FieldKind::String,
        visible_when: Some(id("enabled")),
        ..draft(FieldKind::String)
    })
    .unwrap_or_else(|error| panic!("dependent fixture: {error}"));
    let schema =
        ConfigSchema::parse(1, vec![gate, dependent]).unwrap_or_else(|error| panic!("{error}"));

    // Gate false → dependent hidden.
    let mut values = TypedMap::new();
    values.insert(id("enabled"), TypedValue::Bool(false));
    let rows = project_plugin_config(&schema, &values);
    let dependent_row = rows
        .iter()
        .find(|row| row.field_id == id("dep"))
        .unwrap_or_else(|| panic!("dependent row"));
    assert!(!dependent_row.visible);
    assert_eq!(dependent_row.control, PluginConfigControl::Hidden);

    // Gate true → dependent visible and editable.
    values.insert(id("enabled"), TypedValue::Bool(true));
    let rows = project_plugin_config(&schema, &values);
    let dependent_row = rows
        .iter()
        .find(|row| row.field_id == id("dep"))
        .unwrap_or_else(|| panic!("dependent row"));
    assert!(dependent_row.visible);
    assert!(matches!(
        dependent_row.control,
        PluginConfigControl::Scalar { .. }
    ));
}

#[test]
fn secret_reference_shows_env_name_only_and_never_bytes() {
    let schema = one_field(draft(FieldKind::SecretReference));

    // Set → env name, no resolved bytes.
    let mut values = TypedMap::new();
    values.insert(
        id("value"),
        TypedValue::SecretRef(SecretRef {
            env: SecretReference::parse("DB_PASSWORD")
                .unwrap_or_else(|error| panic!("secret fixture: {error}")),
        }),
    );
    let row = &project_plugin_config(&schema, &values)[0];
    match &row.control {
        PluginConfigControl::SecretReference { set, env } => {
            assert!(*set);
            assert_eq!(env.as_deref(), Some("DB_PASSWORD"));
        }
        other => panic!("expected SecretReference control, got {other:?}"),
    }

    // Unset → set false, no env.
    let row = &project_plugin_config(&schema, &TypedMap::new())[0];
    assert_eq!(
        row.control,
        PluginConfigControl::SecretReference {
            set: false,
            env: None,
        }
    );
}

#[test]
fn boolean_defaults_when_no_value_set() {
    let schema = one_field(FieldDraft {
        default: Some(TypedValue::Bool(true)),
        ..draft(FieldKind::Boolean)
    });
    let row = &project_plugin_config(&schema, &TypedMap::new())[0];
    assert_eq!(row.control, PluginConfigControl::Boolean { value: true });
}

#[test]
fn empty_scalar_shows_no_value() {
    let schema = one_field(draft(FieldKind::String));
    let row = &project_plugin_config(&schema, &TypedMap::new())[0];
    assert_eq!(
        row.control,
        PluginConfigControl::Scalar {
            value: String::new()
        }
    );
    assert!(row.error.is_none());
}
