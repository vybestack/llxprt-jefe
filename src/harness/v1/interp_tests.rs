//! Behavioral tests for `${workspace}` interpolation (issue #380, CW00-04).

use super::super::error::HarCode;
use super::{apply, validate_value};

#[test]
fn complete_prefix_expands() {
    let out = apply("f", "${workspace}/bin", "/tmp/ws")
        .unwrap_or_else(|err| panic!("should apply: {err}"));
    assert_eq!(out, "/tmp/ws/bin");
}

#[test]
fn bare_reference_expands_to_root() {
    let out =
        apply("f", "${workspace}", "/tmp/ws").unwrap_or_else(|err| panic!("should apply: {err}"));
    assert_eq!(out, "/tmp/ws");
}

#[test]
fn double_dollar_is_literal_dollar() {
    let out =
        apply("f", "cost $$5 and $$$$", "/ws").unwrap_or_else(|err| panic!("should apply: {err}"));
    assert_eq!(out, "cost $5 and $$");
}

#[test]
fn escaped_workspace_reference_stays_literal() {
    let out =
        apply("f", "$${workspace}", "/ws").unwrap_or_else(|err| panic!("should apply: {err}"));
    assert_eq!(out, "${workspace}");
}

#[test]
fn embedded_workspace_reference_is_e003() {
    for value in [
        "a${workspace}",
        "${workspace}${workspace}",
        "x/${workspace}/y",
    ] {
        let err = validate_value("f", value)
            .err()
            .unwrap_or_else(|| panic!("{value:?} must fail"));
        assert_eq!(err.code, HarCode::E003, "{value:?}");
        assert_eq!(err.exit_code(), 2, "{value:?}");
    }
}

#[test]
fn unknown_name_is_e003() {
    for value in ["${home}", "${WORKSPACE}", "${workspaces}", "a${x}b"] {
        let err = validate_value("f", value)
            .err()
            .unwrap_or_else(|| panic!("{value:?} must fail"));
        assert_eq!(err.code, HarCode::E003, "{value:?}");
    }
}

#[test]
fn bare_and_unterminated_dollar_are_e003() {
    for value in ["$", "$x", "a$b", "${", "${workspace", "a${"] {
        let err = validate_value("f", value)
            .err()
            .unwrap_or_else(|| panic!("{value:?} must fail"));
        assert_eq!(err.code, HarCode::E003, "{value:?}");
    }
}

#[test]
fn plain_values_pass_unchanged() {
    for value in ["", "plain", "a/b/c", "no dollars here"] {
        validate_value("f", value).unwrap_or_else(|err| panic!("{value:?} should pass: {err}"));
        let out = apply("f", value, "/ws").unwrap_or_else(|err| panic!("should apply: {err}"));
        assert_eq!(out, *value);
    }
}
