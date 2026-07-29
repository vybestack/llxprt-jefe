//! Unit tests for the bounded strict JSON reader.

use super::*;

#[test]
fn parses_simple_object() {
    let json = br#"{"a":1,"b":"x"}"#;
    let value = parse_definition_json(json).unwrap_or_else(|error| panic!("valid object: {error}"));
    let Some(object) = value.as_object() else {
        panic!("parsed value must be an object");
    };
    assert_eq!(object.len(), 2);
    assert_eq!(object[0].0, "a");
    assert_eq!(object[1].0, "b");
}

#[test]
fn rejects_duplicate_keys() {
    let json = br#"{"a":1,"a":2}"#;
    let result = parse_definition_json(json);
    assert!(matches!(result, Err(DefinitionError::DuplicateJsonField { field }) if field == "a"));
}

#[test]
fn rejects_unknown_prefix_as_value_error() {
    let json = br#"{"a":1.5}"#;
    let result = parse_definition_json(json);
    assert!(result.is_err(), "fractional numbers rejected");
}

#[test]
fn rejects_leading_zeros() {
    let json = br#"{"a":007}"#;
    let result = parse_definition_json(json);
    assert!(result.is_err(), "leading zeros rejected");
}

#[test]
fn rejects_trailing_data() {
    let json = br"{}x";
    let result = parse_definition_json(json);
    assert!(result.is_err(), "trailing data rejected");
}

#[test]
fn rejects_unterminated_string() {
    let json = br#"{"a":"unterminated}"#;
    let result = parse_definition_json(json);
    assert!(result.is_err(), "unterminated string rejected");
}

#[test]
fn parses_nested_array_and_object() {
    let json = br#"{"arr":[1,2,3],"obj":{"x":true}}"#;
    let value = parse_definition_json(json).unwrap_or_else(|error| panic!("valid: {error}"));
    let Some(object) = value.as_object() else {
        panic!("parsed value must be an object");
    };
    assert!(object.iter().any(|(k, _)| k == "arr"));
    assert!(object.iter().any(|(k, _)| k == "obj"));
}

#[test]
fn rejects_exponent_number() {
    let json = br#"{"a":1e2}"#;
    let result = parse_definition_json(json);
    assert!(result.is_err(), "exponent rejected");
}

#[test]
fn parses_null_bool_and_escape() {
    let json = br#"{"n":null,"t":true,"f":false,"s":"a\nb"}"#;
    let value = parse_definition_json(json).unwrap_or_else(|error| panic!("valid: {error}"));
    let Some(object) = value.as_object() else {
        panic!("parsed value must be an object");
    };
    assert!(object.iter().any(|(_, v)| v.is_null()));
}
