//! Behavioral tests for the bounded strict JSON reader (issue #380, CW00-01/10).

use super::super::error::HarCode;
use super::{JsonValue, parse_json};

fn parse(text: &str) -> Result<JsonValue, super::super::error::HarnessError> {
    parse_json(text.as_bytes())
}

#[test]
fn parses_object_preserving_order() {
    let value = parse(r#"{"b":1,"a":{"x":[true,false,null,"s"]}}"#)
        .unwrap_or_else(|err| panic!("should parse: {err}"));
    let members = value.as_object().unwrap_or_else(|| panic!("object"));
    assert_eq!(members[0].0, "b");
    assert_eq!(members[0].1, JsonValue::Int(1));
    assert_eq!(members[1].0, "a");
}

#[test]
fn rejects_duplicate_keys_as_e001() {
    let err = parse(r#"{"a":1,"a":2}"#)
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn rejects_non_integer_numbers_as_e001() {
    for doc in [r#"{"a":1.5}"#, r#"{"a":1e3}"#, r#"{"a":01}"#] {
        let err = parse(doc)
            .err()
            .unwrap_or_else(|| panic!("must fail: {doc}"));
        assert_eq!(err.code, HarCode::E001, "{doc}");
    }
}

#[test]
fn rejects_trailing_data_and_syntax_errors() {
    for doc in ["{} x", "{", "[1,]", r#"{"a"}"#, "nul"] {
        let err = parse(doc)
            .err()
            .unwrap_or_else(|| panic!("must fail: {doc}"));
        assert_eq!(err.code, HarCode::E001, "{doc}");
    }
}

#[test]
fn rejects_invalid_utf8_input() {
    let err = parse_json(&[b'{', 0xFF, b'}'])
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E001);
}

#[test]
fn depth_at_limit_parses_and_plus_one_is_e002() {
    // Depth counts nested containers; 16 nested arrays is at-limit.
    let at_limit = format!("{}1{}", "[".repeat(16), "]".repeat(16));
    parse(&at_limit).unwrap_or_else(|err| panic!("at-limit should parse: {err}"));
    let over = format!("{}1{}", "[".repeat(17), "]".repeat(17));
    let err = parse(&over).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E002);
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn object_members_at_limit_parse_and_plus_one_is_e002() {
    let build = |count: usize| {
        let members: Vec<String> = (0..count).map(|i| format!("\"k{i}\":1")).collect();
        format!("{{{}}}", members.join(","))
    };
    parse(&build(256)).unwrap_or_else(|err| panic!("at-limit should parse: {err}"));
    let err = parse(&build(257))
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E002);
}

#[test]
fn array_elements_at_limit_parse_and_plus_one_is_e002() {
    let build = |count: usize| format!("[{}]", vec!["1"; count].join(","));
    parse(&build(1024)).unwrap_or_else(|err| panic!("at-limit should parse: {err}"));
    let err = parse(&build(1025))
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E002);
}

#[test]
fn input_bytes_over_limit_is_e002() {
    let mut doc = String::from("[\"");
    doc.push_str(&"a".repeat(1_048_576));
    doc.push_str("\"]");
    let err = parse(&doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E002);
}

#[test]
fn string_over_limit_is_e002() {
    let mut doc = String::from("\"");
    doc.push_str(&"a".repeat(262_145));
    doc.push('"');
    let err = parse(&doc).err().unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code, HarCode::E002);
}

#[test]
fn decodes_escapes_and_surrogate_pairs() {
    let value =
        parse(r#""a\n\t\u0041\ud83d\ude00""#).unwrap_or_else(|err| panic!("should parse: {err}"));
    assert_eq!(value, JsonValue::Str("a\n\tA😀".to_string()));
}

#[test]
fn rejects_unpaired_surrogates_and_bad_escapes() {
    for doc in [r#""\ud83d""#, r#""\udc00""#, r#""\q""#, "\"\u{1}\""] {
        let err = parse(doc)
            .err()
            .unwrap_or_else(|| panic!("must fail: {doc}"));
        assert_eq!(err.code, HarCode::E001, "{doc}");
    }
}

#[test]
fn exit_codes_map_per_contract() {
    use super::super::error::HarnessError;
    assert_eq!(HarnessError::syntax("x").exit_code(), 2);
    assert_eq!(HarnessError::limit("x").exit_code(), 2);
    assert_eq!(HarnessError::interpolation("x").exit_code(), 2);
    assert_eq!(HarnessError::containment("x").exit_code(), 4);
    assert_eq!(HarnessError::process("x").exit_code(), 4);
    assert_eq!(HarnessError::assertion("x").exit_code(), 4);
    assert_eq!(HarnessError::cleanup("x").exit_code(), 4);
    assert_eq!(HarnessError::wait_timeout("x").exit_code(), 124);
}
