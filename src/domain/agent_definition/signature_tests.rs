//! Unit tests for launch signature v1.

use super::super::sha256::DefinitionSha256;
use super::LaunchSignature;

#[test]
fn v1_carries_three_digests() {
    let a = DefinitionSha256::digest(b"a");
    let b = DefinitionSha256::digest(b"b");
    let c = DefinitionSha256::digest(b"c");
    let sig = LaunchSignature::v1(a, b, c);
    assert_eq!(sig.version, LaunchSignature::VERSION);
    assert_eq!(sig.definition_hash, a);
    assert_eq!(sig.typed_value_hash, b);
    assert_eq!(sig.target_fingerprint, c);
}

#[test]
fn default_version_is_zero() {
    let sig = LaunchSignature::default();
    assert_eq!(sig.version, 0, "default signature is unversioned");
}

#[test]
fn v1_serde_round_trips() {
    let a = DefinitionSha256::digest(b"a");
    let b = DefinitionSha256::digest(b"b");
    let c = DefinitionSha256::digest(b"c");
    let sig = LaunchSignature::v1(a, b, c);
    let json = serde_json::to_string(&sig).unwrap_or_else(|error| panic!("serialize: {error}"));
    let back: LaunchSignature =
        serde_json::from_str(&json).unwrap_or_else(|error| panic!("deserialize: {error}"));
    assert_eq!(back, sig);
}
