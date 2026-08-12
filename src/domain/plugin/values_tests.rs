//! Boundary tables for the manifest value types
//! (issue #389 CW-09, acceptance rows D6 and D8).

use super::*;
use crate::domain::plugin::limits::{
    PACKAGE_PATH_BYTE_LIMIT, PACKAGE_PATH_DEPTH_LIMIT, SECRET_ENV_BYTE_LIMIT,
};

fn path_reason(value: &str) -> RelativePathErrorReason {
    RelativePath::parse(value).err().map_or_else(
        || panic!("{value:?} must be rejected"),
        |error| error.reason,
    )
}

#[test]
fn a_relative_path_keeps_its_exact_declared_text() {
    for value in ["plugin.json", "bin/provider", "a/b/c/d.txt", "x-1_2.3"] {
        let path = RelativePath::parse(value)
            .unwrap_or_else(|error| panic!("{value} must parse: {error}"));
        assert_eq!(path.as_str(), value);
    }
}

#[test]
fn a_relative_path_exposes_its_components() {
    let path = RelativePath::parse("bin/tools/provider")
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(path.components(), ["bin", "tools", "provider"]);
    assert_eq!(path.depth(), 3);
}

#[test]
fn an_absolute_or_rooted_path_is_rejected() {
    for value in ["/etc/passwd", "/", "//a"] {
        assert_eq!(path_reason(value), RelativePathErrorReason::Absolute);
    }
}

#[test]
fn a_backslash_is_never_a_separator() {
    // Accepting a backslash would let one archive entry name two different
    // files depending on the host, which is the divergence the contract bans.
    for value in ["bin\\provider", "a\\b", "\\a"] {
        assert_eq!(path_reason(value), RelativePathErrorReason::Backslash);
    }
}

#[test]
fn a_nul_byte_is_rejected() {
    assert_eq!(path_reason("a\u{0}b"), RelativePathErrorReason::Nul);
}

#[test]
fn empty_dot_and_dotdot_components_are_rejected() {
    for value in [
        "", "a//b", "a/", "./a", "a/./b", "../a", "a/../b", "..", ".",
    ] {
        assert_eq!(
            path_reason(value),
            RelativePathErrorReason::Component,
            "{value:?} must be rejected"
        );
    }
}

#[test]
fn depth_accepts_its_limit_and_rejects_one_more() {
    let at_limit = vec!["a"; PACKAGE_PATH_DEPTH_LIMIT].join("/");
    assert!(
        RelativePath::parse(&at_limit).is_ok(),
        "a path of exactly {PACKAGE_PATH_DEPTH_LIMIT} components must be accepted"
    );
    let over_limit = vec!["a"; PACKAGE_PATH_DEPTH_LIMIT + 1].join("/");
    assert_eq!(path_reason(&over_limit), RelativePathErrorReason::Depth);
}

#[test]
fn byte_length_accepts_its_limit_and_rejects_one_more() {
    let at_limit = "a".repeat(PACKAGE_PATH_BYTE_LIMIT);
    assert!(RelativePath::parse(&at_limit).is_ok());
    let over_limit = "a".repeat(PACKAGE_PATH_BYTE_LIMIT + 1);
    assert_eq!(path_reason(&over_limit), RelativePathErrorReason::Length);
}

#[test]
fn a_host_triple_accepts_the_shipped_target_forms() {
    for value in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl",
        "x86_64-pc-windows-msvc",
    ] {
        let triple =
            HostTriple::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"));
        assert_eq!(triple.as_str(), value);
    }
}

#[test]
fn a_host_triple_rejects_anything_that_is_not_an_exact_triple() {
    for value in [
        "",
        "x86_64",
        "x86_64-apple",
        "x86_64-unknown-linux-gnu-extra-part",
        "X86_64-apple-darwin",
        "x86_64-apple-",
        "-apple-darwin",
        "x86_64--darwin",
        "x86_64 apple darwin",
        "x86_64/apple/darwin",
    ] {
        assert!(
            HostTriple::parse(value).is_err(),
            "{value:?} is not an exact host triple"
        );
    }
}

#[test]
fn the_running_host_triple_parses_as_a_host_triple() {
    let current = HostTriple::current();
    assert!(
        HostTriple::parse(current.as_str()).is_ok(),
        "the build host triple {current} must satisfy the grammar"
    );
}

#[test]
fn a_secret_reference_accepts_the_documented_grammar() {
    for value in ["A", "_", "A_B", "GITHUB_TOKEN", "_X9"] {
        let secret = SecretReference::parse(value)
            .unwrap_or_else(|error| panic!("{value} must parse: {error}"));
        assert_eq!(secret.env(), value);
    }
}

#[test]
fn a_secret_reference_accepts_its_byte_limit_and_rejects_one_more() {
    let at_limit = format!("A{}", "B".repeat(SECRET_ENV_BYTE_LIMIT - 1));
    assert_eq!(at_limit.len(), SECRET_ENV_BYTE_LIMIT);
    assert!(SecretReference::parse(&at_limit).is_ok());

    let over_limit = format!("A{}", "B".repeat(SECRET_ENV_BYTE_LIMIT));
    assert!(SecretReference::parse(&over_limit).is_err());
}

#[test]
fn a_secret_reference_rejects_anything_outside_the_grammar() {
    for value in ["", "9A", "a", "aB", "A-B", "A B", "A.B", "A\u{c9}"] {
        assert!(
            SecretReference::parse(value).is_err(),
            "{value:?} must be rejected"
        );
    }
}

#[test]
fn a_secret_reference_never_renders_a_value() {
    // A secret reference names an environment variable; it must never carry or
    // display the secret itself.
    let secret = SecretReference::parse("GITHUB_TOKEN")
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(secret.to_string(), "GITHUB_TOKEN");
}

#[test]
fn secret_reference_deserialization_preserves_validation() {
    let valid: Result<SecretReference, _> = serde_json::from_str("\"GITHUB_TOKEN\"");
    let Ok(valid) = valid else {
        panic!("valid environment reference must deserialize");
    };
    assert_eq!(valid.env(), "GITHUB_TOKEN");

    let invalid: Result<SecretReference, _> = serde_json::from_str("\"lowercase-token\"");
    assert!(
        invalid.is_err(),
        "deserialization must not bypass validation"
    );
}
