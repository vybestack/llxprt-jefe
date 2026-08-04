//! Unit tests for closed field and emitter validation.

use super::*;

fn string_field(id: &str) -> Field {
    Field {
        id: id.to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: None,
        launch_signature: true,
    }
}

fn enum_field(id: &str, choices: &[&str]) -> Field {
    Field {
        id: id.to_string(),
        kind: FieldKind::Enum,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: choices.iter().map(|c| (*c).to_string()).collect(),
        visible_when: None,
        launch_signature: true,
    }
}

fn integer_field(id: &str) -> Field {
    Field {
        id: id.to_string(),
        kind: FieldKind::Integer,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: None,
        launch_signature: true,
    }
}

#[test]
fn valid_string_field_passes() {
    assert!(string_field("model").validate().is_ok());
}

#[test]
fn id_bounds_n_and_n_plus_one() {
    let exactly = str::repeat("a", FIELD_ID_BYTE_LIMIT);
    let too_long = str::repeat("a", FIELD_ID_BYTE_LIMIT + 1);
    let mut field = string_field(&exactly);
    assert!(field.validate().is_ok(), "128 bytes accepted at N");
    field.id = too_long;
    assert!(field.validate().is_err(), "129 bytes rejected at N+1");
}

#[test]
fn empty_id_rejected() {
    let field = string_field("");
    assert!(field.validate().is_err(), "empty id rejected");
}

#[test]
fn enum_field_requires_choices() {
    let mut field = enum_field("perm", &[]);
    field.kind = FieldKind::Enum;
    field.choices = vec![];
    assert!(field.validate().is_err(), "enum with no choices rejected");
}

#[test]
fn enum_field_with_choices_passes() {
    let field = enum_field("perm", &["default", "acceptEdits"]);
    assert!(field.validate().is_ok(), "enum with choices passes");
}

#[test]
fn choices_over_n_rejected() {
    let choices: Vec<String> = (0..=CHOICE_LIMIT).map(|i| format!("c{i}")).collect();
    let field = Field {
        id: "perm".to_string(),
        kind: FieldKind::Enum,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices,
        visible_when: None,
        launch_signature: true,
    };
    assert!(field.validate().is_err(), "choices over N rejected");
}

#[test]
fn default_must_match_kind() {
    let mut field = string_field("model");
    field.default = Some(FieldValue::Integer(7));
    assert!(
        field.validate().is_err(),
        "integer default on string field rejected"
    );
}

#[test]
fn enum_default_must_be_in_choices() {
    let mut field = enum_field("perm", &["default", "acceptEdits"]);
    field.default = Some(FieldValue::String("bogus".to_string()));
    assert!(
        field.validate().is_err(),
        "enum default not in choices rejected"
    );
}

#[test]
fn enum_default_in_choices_passes() {
    let mut field = enum_field("perm", &["default", "acceptEdits"]);
    field.default = Some(FieldValue::String("default".to_string()));
    assert!(field.validate().is_ok(), "enum default in choices passes");
}

#[test]
fn integer_bounds_only_on_integer_field() {
    let mut field = string_field("model");
    field.minimum = Some(0);
    assert!(
        field.validate().is_err(),
        "integer bounds on string field rejected"
    );
}

#[test]
fn integer_field_with_bounds_passes() {
    let mut field = integer_field("count");
    field.minimum = Some(0);
    field.maximum = Some(10);
    assert!(
        field.validate().is_ok(),
        "integer bounds on integer field passes"
    );
}

#[test]
fn inverted_bounds_rejected() {
    let mut field = integer_field("count");
    field.minimum = Some(10);
    field.maximum = Some(0);
    assert!(field.validate().is_err(), "inverted bounds rejected");
}

#[test]
fn emitter_fixed_validates() {
    let emitter = Emitter::Fixed {
        value: "--resume".to_string(),
    };
    assert!(emitter.validate().is_ok());
}

#[test]
fn emitter_fixed_empty_rejected() {
    let emitter = Emitter::Fixed {
        value: String::new(),
    };
    assert!(emitter.validate().is_err(), "empty fixed value rejected");
}

#[test]
fn emitter_option_validates() {
    let emitter = Emitter::Option {
        name: "--model".to_string(),
        field: "model".to_string(),
    };
    assert!(emitter.validate().is_ok());
}

#[test]
fn emitter_field_ref_empty_rejected() {
    let emitter = Emitter::Flag {
        name: "--flag".to_string(),
        field: String::new(),
    };
    assert!(emitter.validate().is_err(), "empty field ref rejected");
}

#[test]
fn field_value_matches_kind() {
    assert!(FieldValue::Integer(7).matches_kind(FieldKind::Integer));
    assert!(FieldValue::String("x".into()).matches_kind(FieldKind::String));
    assert!(FieldValue::String("x".into()).matches_kind(FieldKind::Enum));
    assert!(!FieldValue::Integer(7).matches_kind(FieldKind::String));
}

#[test]
fn field_value_as_arg_string() {
    assert_eq!(
        FieldValue::Boolean(true).as_arg_string(),
        Some("true".to_string())
    );
    assert_eq!(
        FieldValue::Integer(42).as_arg_string(),
        Some("42".to_string())
    );
    assert_eq!(
        FieldValue::StringList(vec!["a".into()]).as_arg_string(),
        None
    );
}
