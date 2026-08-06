//! Bounds and policy table for the shared bounded JSON reader (issue #389 D1).

use super::*;

const INTEGER_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    document_bytes: 1_024,
    depth: 4,
    object_members: 3,
    array_elements: 3,
    string_bytes: 8,
    numbers: NumberPolicy::IntegerOnly,
};

const FINITE_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    numbers: NumberPolicy::Finite,
    ..INTEGER_LIMITS
};

fn parsed(text: &str, limits: &BoundedJsonLimits) -> BoundedJson {
    parse(text.as_bytes(), limits).unwrap_or_else(|error| panic!("{text} must parse: {error}"))
}

fn rejected(text: &str, limits: &BoundedJsonLimits) -> BoundedJsonError {
    parse(text.as_bytes(), limits)
        .err()
        .unwrap_or_else(|| panic!("{text} must be rejected"))
}

#[test]
fn a_duplicate_object_key_names_the_offending_key() {
    assert_eq!(
        rejected(r#"{"a":1,"a":2}"#, &INTEGER_LIMITS),
        BoundedJsonError::DuplicateKey {
            key: "a".to_owned()
        }
    );
}

#[test]
fn object_order_is_preserved() {
    let value = parsed(r#"{"b":1,"a":2}"#, &INTEGER_LIMITS);
    let members = value
        .as_object()
        .unwrap_or_else(|| panic!("must be an object"));
    let keys: Vec<&str> = members.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, vec!["b", "a"]);
}

#[test]
fn each_bound_accepts_its_limit_and_rejects_one_more() {
    assert!(parse(br#"{"a":1,"b":2,"c":3}"#, &INTEGER_LIMITS).is_ok());
    assert_eq!(
        rejected(r#"{"a":1,"b":2,"c":3,"d":4}"#, &INTEGER_LIMITS),
        BoundedJsonError::ObjectTooLarge { limit: 3 }
    );

    assert!(parse(b"[1,2,3]", &INTEGER_LIMITS).is_ok());
    assert_eq!(
        rejected("[1,2,3,4]", &INTEGER_LIMITS),
        BoundedJsonError::ArrayTooLarge { limit: 3 }
    );

    assert!(parse(br#""12345678""#, &INTEGER_LIMITS).is_ok());
    assert_eq!(
        rejected(r#""123456789""#, &INTEGER_LIMITS),
        BoundedJsonError::StringTooLong { limit: 8 }
    );

    // The bound counts permitted nesting levels, so four nested arrays are at
    // the limit and a fifth is over it.
    assert!(parse(b"[[[[1]]]]", &INTEGER_LIMITS).is_ok());
    assert_eq!(
        rejected("[[[[[1]]]]]", &INTEGER_LIMITS),
        BoundedJsonError::DepthExceeded { limit: 4 }
    );
}

#[test]
fn a_document_over_the_byte_bound_is_rejected_before_parsing() {
    let limits = BoundedJsonLimits {
        document_bytes: 4,
        ..INTEGER_LIMITS
    };
    assert_eq!(
        rejected("[1,2,3]", &limits),
        BoundedJsonError::DocumentTooLarge { bytes: 7, limit: 4 }
    );
}

#[test]
fn integer_only_schemas_reject_fractions_and_exponents() {
    for text in ["1.5", "1e3", "1E3", "1.0", "-0.5"] {
        assert!(
            matches!(
                rejected(text, &INTEGER_LIMITS),
                BoundedJsonError::Syntax { .. }
            ),
            "{text} must be rejected by an integer-only schema"
        );
    }
}

#[test]
fn finite_schemas_admit_canonical_decimals() {
    assert_eq!(
        parsed("1.5", &FINITE_LIMITS)
            .as_decimal()
            .map(CanonicalDecimal::as_str),
        Some("1.5")
    );
    assert_eq!(
        parsed("-0.25", &FINITE_LIMITS)
            .as_decimal()
            .map(CanonicalDecimal::as_str),
        Some("-0.25")
    );
    assert_eq!(parsed("7", &FINITE_LIMITS).as_int(), Some(7));
    assert_eq!(parsed("-7", &FINITE_LIMITS).as_int(), Some(-7));
    assert_eq!(parsed("0", &FINITE_LIMITS).as_int(), Some(0));
}

#[test]
fn finite_schemas_reject_non_canonical_and_non_finite_spellings() {
    // Trailing fraction zeroes, negative zero, and exponents are second
    // spellings of a value the canonical form already has.
    for text in ["1.50", "-0.0", "1e3", "1.5e3", "0.5e-1"] {
        assert!(
            parse(text.as_bytes(), &FINITE_LIMITS).is_err(),
            "{text} is not canonical and must be rejected"
        );
    }
    // JSON has no NaN or infinity literal, so both are syntax errors rather
    // than values that must be filtered afterwards.
    for text in ["NaN", "Infinity", "-Infinity"] {
        assert!(
            parse(text.as_bytes(), &FINITE_LIMITS).is_err(),
            "{text} must never parse"
        );
    }
}

#[test]
fn a_literal_that_rounds_to_infinity_is_not_a_finite_decimal() {
    let huge = format!("{}.5", "9".repeat(400));
    let limits = BoundedJsonLimits {
        document_bytes: 4_096,
        ..FINITE_LIMITS
    };
    assert_eq!(
        parse(huge.as_bytes(), &limits),
        Err(BoundedJsonError::NumberNotAdmitted { text: huge })
    );
}

#[test]
fn leading_zeros_and_bare_fractions_are_rejected() {
    for text in ["01", "-01", "00", "1.", ".5", "-"] {
        assert!(
            parse(text.as_bytes(), &FINITE_LIMITS).is_err(),
            "{text} must be rejected"
        );
    }
}

#[test]
fn an_integer_outside_the_signed_range_is_not_admitted() {
    let text = "9223372036854775808";
    assert_eq!(
        rejected(text, &INTEGER_LIMITS),
        BoundedJsonError::NumberNotAdmitted {
            text: text.to_owned()
        }
    );
}

#[test]
fn trailing_data_after_the_top_level_value_is_rejected() {
    assert_eq!(
        rejected("{} {}", &INTEGER_LIMITS),
        BoundedJsonError::TrailingData { offset: 3 }
    );
}

#[test]
fn strings_reject_control_characters_and_unpaired_surrogates() {
    for text in [
        "\"a\u{1}b\"",
        r#""\uD800""#,
        r#""\uDC00""#,
        r#""\q""#,
        "\"a",
    ] {
        assert!(
            matches!(
                rejected(text, &INTEGER_LIMITS),
                BoundedJsonError::Syntax { .. }
            ),
            "{text:?} must be rejected"
        );
    }
}

#[test]
fn a_valid_surrogate_pair_decodes() {
    assert_eq!(
        parsed(r#""\uD83D\uDE00""#, &INTEGER_LIMITS).as_str(),
        Some("\u{1F600}")
    );
}

#[test]
fn a_non_utf8_document_is_rejected() {
    assert_eq!(
        parse(&[0xFF, 0xFE], &INTEGER_LIMITS),
        Err(BoundedJsonError::NotUtf8)
    );
}

#[test]
fn null_and_booleans_round_trip() {
    assert!(parsed("null", &INTEGER_LIMITS).is_null());
    assert_eq!(parsed("true", &INTEGER_LIMITS).as_bool(), Some(true));
    assert_eq!(parsed("false", &INTEGER_LIMITS).as_bool(), Some(false));
}
