//! Behavioral tests for schema-1 scalar validators (issue #380, CW00-01/10).

use super::super::error::HarCode;
use super::{decode_base64, validate_env_name, validate_id, validate_rel_path, validate_secrets};

#[test]
fn accepts_normal_relative_paths() {
    for path in ["a", "a/b/c.txt", "deep/tree/file", "with-dash/under_score"] {
        validate_rel_path("f", path).unwrap_or_else(|err| panic!("{path} should pass: {err}"));
    }
}

#[test]
fn rejects_forbidden_path_shapes() {
    for path in [
        "", "/abs", "a//b", "a/", "./a", "a/./b", "../a", "a/../b", "a\\b", "a\u{0}b",
    ] {
        let err = validate_rel_path("f", path)
            .err()
            .unwrap_or_else(|| panic!("{path:?} must fail"));
        assert_eq!(err.code(), HarCode::E001, "{path:?}");
    }
}

#[test]
fn path_length_at_limit_passes_and_plus_one_is_e002() {
    let at_limit = "a/".repeat(2047) + "aa";
    assert_eq!(at_limit.len(), 4096);
    validate_rel_path("f", &at_limit).unwrap_or_else(|err| panic!("at-limit should pass: {err}"));
    let over = "a/".repeat(2047) + "aaa";
    let err = validate_rel_path("f", &over)
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code(), HarCode::E002);
}

#[test]
fn env_name_regex_enforced() {
    for name in ["A", "_", "PATH", "JEFE_CONFIG_DIR", "A1_B2"] {
        validate_env_name("f", name).unwrap_or_else(|err| panic!("{name} should pass: {err}"));
    }
    for name in ["", "a", "1A", "A-B", "A B", "Ab"] {
        let err = validate_env_name("f", name)
            .err()
            .unwrap_or_else(|| panic!("{name:?} must fail"));
        assert_eq!(err.code(), HarCode::E001, "{name:?}");
    }
    let at_limit = "A".repeat(128);
    validate_env_name("f", &at_limit).unwrap_or_else(|err| panic!("should pass: {err}"));
    let over = "A".repeat(129);
    assert!(validate_env_name("f", &over).is_err());
}

#[test]
fn capture_ids_are_closed() {
    for id in ["gh", "git", "a.b-c_d", "X9"] {
        validate_id("f", id).unwrap_or_else(|err| panic!("{id} should pass: {err}"));
    }
    for id in ["", ".", "..", "a/b", "a b", "a\\b"] {
        let err = validate_id("f", id)
            .err()
            .unwrap_or_else(|| panic!("{id:?} must fail"));
        assert_eq!(err.code(), HarCode::E001, "{id:?}");
    }
    validate_id("f", &"a".repeat(64)).unwrap_or_else(|err| panic!("should pass: {err}"));
    assert!(validate_id("f", &"a".repeat(65)).is_err());
}

#[test]
fn secrets_reject_empty_and_over_count() {
    validate_secrets(&[]).unwrap_or_else(|err| panic!("empty list should pass: {err}"));
    let at_limit: Vec<String> = (0..64).map(|i| format!("s{i}")).collect();
    validate_secrets(&at_limit).unwrap_or_else(|err| panic!("at-limit should pass: {err}"));
    let over: Vec<String> = (0..65).map(|i| format!("s{i}")).collect();
    assert_eq!(
        validate_secrets(&over)
            .err()
            .unwrap_or_else(|| panic!("must fail"))
            .code(),
        HarCode::E002
    );
    assert_eq!(
        validate_secrets(&[String::new()])
            .err()
            .unwrap_or_else(|| panic!("must fail"))
            .code(),
        HarCode::E001
    );
}

#[test]
fn base64_round_trips_known_vectors() {
    let cases: &[(&str, &[u8])] = &[
        ("", b""),
        ("QQ==", b"A"),
        ("QUI=", b"AB"),
        ("QUJD", b"ABC"),
        ("aGVsbG8gd29ybGQ=", b"hello world"),
        ("AAECAwT/", &[0, 1, 2, 3, 4, 255]),
    ];
    for (encoded, expected) in cases {
        let decoded = decode_base64("f", encoded)
            .unwrap_or_else(|err| panic!("{encoded} should decode: {err}"));
        assert_eq!(&decoded, expected, "{encoded}");
    }
}

#[test]
fn base64_rejects_malformed_input() {
    for bad in [
        "A",
        "AB",
        "ABC",
        "A===",
        "=AAA",
        "QQ=X",
        "QQ==QQ==x",
        "a b=",
        "😀😀😀😀",
        "QR==",
        "QUJ=",
    ] {
        let err = decode_base64("f", bad)
            .err()
            .unwrap_or_else(|| panic!("{bad:?} must fail"));
        assert_eq!(err.code(), HarCode::E001, "{bad:?}");
    }
}

#[test]
fn base64_interior_padding_rejected() {
    let err = decode_base64("f", "QQ==QUJD")
        .err()
        .unwrap_or_else(|| panic!("must fail"));
    assert_eq!(err.code(), HarCode::E001);
}
