//! Contract tests for schema-2 configuration value types.

use std::cmp::Ordering;

use super::{CanonicalDecimal, CanonicalSemver, Id, TypedValue};

#[test]
fn id_enforces_grammar_and_inclusive_byte_bound() {
    let at_limit = format!("a{}", "0".repeat(127));
    assert!(Id::parse(&at_limit).is_ok());
    assert!(Id::parse(&format!("a{}", "0".repeat(128))).is_err());

    for invalid in ["", "A", "0agent", "agent_1", "agent..one", "agent-"] {
        assert!(Id::parse(invalid).is_err(), "{invalid:?} must be rejected");
    }
    assert_eq!(
        Id::parse("core.code-puppy").map(|id| id.to_string()),
        Ok("core.code-puppy".to_owned())
    );
}

#[test]
fn typed_values_use_a_closed_externally_tagged_wire_shape() {
    let value = TypedValue::String("green-screen".to_owned());
    let Ok(encoded) = serde_json::to_string(&value) else {
        panic!("typed value must serialize");
    };
    assert_eq!(encoded, r#"{"type":"string","value":"green-screen"}"#);
    assert!(serde_json::from_str::<TypedValue>("null").is_err());
    assert!(serde_json::from_str::<TypedValue>(r#"{"arbitrary":true}"#).is_err());
}

#[test]
fn canonical_decimal_rejects_noncanonical_and_nonfinite_text() {
    for valid in ["0", "1", "-1", "0.5", "-10.25"] {
        assert!(
            CanonicalDecimal::parse(valid).is_ok(),
            "{valid:?} must parse"
        );
    }
    for invalid in ["-0", "01", "1.0", "1.", ".5", "1e3", "NaN", "inf"] {
        assert!(
            CanonicalDecimal::parse(invalid).is_err(),
            "{invalid:?} must be rejected"
        );
    }
}

#[test]
fn canonical_semver_validates_and_compares_precedence_without_build_metadata() {
    let alpha = CanonicalSemver::parse("1.0.0-alpha.1");
    let release = CanonicalSemver::parse("1.0.0+build.7");
    let same_release = CanonicalSemver::parse("1.0.0+build.8");
    let (Ok(alpha), Ok(release), Ok(same_release)) = (alpha, release, same_release) else {
        panic!("valid semantic versions must parse");
    };
    assert_eq!(alpha.precedence_cmp(&release), Ordering::Less);
    assert_eq!(release.precedence_cmp(&same_release), Ordering::Equal);

    for invalid in ["1", "1.0", "01.0.0", "1.0.0-01", "1.0.0+", "1.0.0-"] {
        assert!(
            CanonicalSemver::parse(invalid).is_err(),
            "{invalid:?} must be rejected"
        );
    }
}
