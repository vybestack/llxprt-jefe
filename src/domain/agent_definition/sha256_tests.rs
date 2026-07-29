//! Unit tests for the definition SHA-256 digest type.

use super::DefinitionSha256;

#[test]
fn digest_of_empty_is_known_constant() {
    let d = DefinitionSha256::digest(b"");
    // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    assert_eq!(
        d.to_hex(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn digest_is_deterministic() {
    let a = DefinitionSha256::digest(b"core.llxprt");
    let b = DefinitionSha256::digest(b"core.llxprt");
    assert_eq!(a, b);
}

#[test]
fn distinct_inputs_produce_distinct_digests() {
    let a = DefinitionSha256::digest(b"core.llxprt");
    let b = DefinitionSha256::digest(b"core.codex");
    assert_ne!(a, b);
}

#[test]
fn serde_round_trips_canonical_hex() {
    let d = DefinitionSha256::digest(b"core.codex");
    let json = serde_json::to_string(&d).unwrap_or_else(|error| panic!("serialize: {error}"));
    assert!(json.starts_with('"') && json.ends_with('"'));
    let back: DefinitionSha256 =
        serde_json::from_str(&json).unwrap_or_else(|error| panic!("deserialize: {error}"));
    assert_eq!(back, d);
}

#[test]
fn serde_rejects_short_hex() {
    let bad: Result<DefinitionSha256, _> = serde_json::from_str("\"abc\"");
    assert!(bad.is_err(), "short hex rejected");
}

#[test]
fn serde_rejects_non_hex() {
    let bad: Result<DefinitionSha256, _> = serde_json::from_str(
        "\"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz\"",
    );
    assert!(bad.is_err(), "non-hex rejected");
}
