//! Unit tests for RFC 6901 JSON-pointer validation and evaluation.

use super::super::bounded_json::BoundedJson;
use super::*;

#[test]
fn empty_pointer_is_root() {
    let pointer =
        JsonPointer::parse("").unwrap_or_else(|error| panic!("empty pointer must parse: {error}"));
    assert!(pointer.tokens().is_empty());
    let root = BoundedJson::Int(7);
    assert_eq!(pointer.evaluate(&root), Some(&BoundedJson::Int(7)));
}

#[test]
fn leading_slash_object_access() {
    let pointer = JsonPointer::parse("/identity")
        .unwrap_or_else(|error| panic!("valid pointer must parse: {error}"));
    assert_eq!(pointer.tokens(), &["identity".to_string()]);
    let root = BoundedJson::Object(vec![("identity".to_string(), BoundedJson::Int(1))]);
    assert_eq!(pointer.evaluate(&root), Some(&BoundedJson::Int(1)));
}

#[test]
fn nested_object_access() {
    let pointer = JsonPointer::parse("/data/version")
        .unwrap_or_else(|error| panic!("valid pointer must parse: {error}"));
    let root = BoundedJson::Object(vec![(
        "data".to_string(),
        BoundedJson::Object(vec![("version".to_string(), BoundedJson::Int(2))]),
    )]);
    assert_eq!(pointer.evaluate(&root), Some(&BoundedJson::Int(2)));
}

#[test]
fn array_index_access() {
    let pointer = JsonPointer::parse("/capabilities/0")
        .unwrap_or_else(|error| panic!("valid pointer must parse: {error}"));
    let root = BoundedJson::Object(vec![(
        "capabilities".to_string(),
        BoundedJson::Array(vec![BoundedJson::Str("a".to_string())]),
    )]);
    assert_eq!(
        pointer.evaluate(&root),
        Some(&BoundedJson::Str("a".to_string()))
    );
}

#[test]
fn tilde_escapes_decode() {
    let pointer = JsonPointer::parse("/a~1b~0c")
        .unwrap_or_else(|error| panic!("valid escapes must parse: {error}"));
    assert_eq!(pointer.tokens(), &["a/b~c".to_string()]);
}

#[test]
fn missing_slash_rejected() {
    let result = JsonPointer::parse("identity");
    assert!(result.is_err(), "non-empty pointer must start with '/'");
}

#[test]
fn invalid_tilde_escape_rejected() {
    let result = JsonPointer::parse("/a~2b");
    assert!(result.is_err(), "invalid '~' escape rejected");
}

#[test]
fn evaluate_missing_key_returns_none() {
    let pointer = JsonPointer::parse("/missing")
        .unwrap_or_else(|error| panic!("valid pointer must parse: {error}"));
    let root = BoundedJson::Object(vec![("identity".to_string(), BoundedJson::Int(1))]);
    assert_eq!(pointer.evaluate(&root), None);
}
