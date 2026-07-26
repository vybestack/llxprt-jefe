//! Unit tests for the closed probe specification and recognizer set.

use super::*;

#[test]
fn version_token_recognizer() {
    assert!(matches_version_token("0.142.0"));
    assert!(matches_version_token("2.1.212"));
    assert!(matches_version_token("0.10.0-nightly.260720.d69bda66a"));
    assert!(!matches_version_token("not-a-version"));
    assert!(!matches_version_token("1.2"));
}

#[test]
fn anchored_pattern_variants() {
    assert!(AnchoredPattern::Exact { value: "x".into() }.matches("x"));
    assert!(
        AnchoredPattern::Prefix {
            prefix: "codex-cli ".into()
        }
        .matches("codex-cli 0.142.0")
    );
    assert!(
        AnchoredPattern::Suffix {
            suffix: "0.0.634".into()
        }
        .matches("version 0.0.634")
    );
    assert!(AnchoredPattern::VersionToken.matches("0.142.0"));
}

#[test]
fn parse_text_identity_line() {
    let recognizer = IdentityRecognizer::Line {
        prefix: String::new(),
        anchored_pattern: AnchoredPattern::Prefix {
            prefix: "codex-cli ".into(),
        },
    };
    assert_eq!(
        parse_text_identity("codex-cli 0.142.0\n", &recognizer),
        Some("codex-cli 0.142.0".to_string())
    );
}

#[test]
fn parse_prefixed_capabilities_dedup_sort() {
    let caps =
        parse_prefixed_capabilities("capability:b\ncapability:a\ncapability:a\n", "capability:");
    assert_eq!(caps, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn validate_accepts_minimal_probe() {
    let spec = ProbeSpec {
        argv: vec!["--version".to_string()],
        stream: ProbeStream::Stdout,
        framing: ProbeFraming::Utf8Text,
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern: AnchoredPattern::VersionToken,
        },
        capabilities: None,
        required: vec!["interactive".to_string()],
        timeout_ms: LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: PROBE_STREAM_LIMIT,
    };
    assert!(validate(&spec).is_ok(), "minimal probe must validate");
}

#[test]
fn validate_rejects_empty_argv() {
    let spec = ProbeSpec {
        argv: vec![],
        ..valid_probe()
    };
    let Err(err) = validate(&spec) else {
        panic!("empty argv rejected");
    };
    assert!(matches!(err, ProbeValidateError::ArgvBounds { len: 0 }));
}

#[test]
fn validate_rejects_argv_over_n() {
    let too_many = vec!["a".to_string(); PROBE_ARGV_LIMIT + 1];
    let spec = ProbeSpec {
        argv: too_many,
        ..valid_probe()
    };
    let Err(err) = validate(&spec) else {
        panic!("argv over N rejected");
    };
    assert!(matches!(err, ProbeValidateError::ArgvBounds { len } if len == PROBE_ARGV_LIMIT + 1));
}

#[test]
fn validate_rejects_capabilities_over_n() {
    let too_many = vec!["cap".to_string(); CAPABILITY_LIMIT + 1];
    let spec = ProbeSpec {
        required: too_many,
        ..valid_probe()
    };
    let Err(err) = validate(&spec) else {
        panic!("capabilities over N rejected");
    };
    assert!(
        matches!(err, ProbeValidateError::CapabilityBounds { len } if len == CAPABILITY_LIMIT + 1)
    );
}

#[test]
fn validate_rejects_duplicate_capability() {
    let spec = ProbeSpec {
        required: vec!["interactive".to_string(), "interactive".to_string()],
        ..valid_probe()
    };
    let Err(err) = validate(&spec) else {
        panic!("duplicate capability rejected");
    };
    assert!(matches!(
        err,
        ProbeValidateError::DuplicateCapability { index: 1, .. }
    ));
}

#[test]
fn validate_rejects_invalid_capability_id() {
    let spec = ProbeSpec {
        required: vec!["UPPER".to_string()],
        ..valid_probe()
    };
    let Err(err) = validate(&spec) else {
        panic!("invalid capability id rejected");
    };
    assert!(matches!(
        err,
        ProbeValidateError::InvalidCapabilityId { index: 0, .. }
    ));
}

#[test]
fn validate_rejects_timeout_zero_and_over_remote_ceiling() {
    let zero = ProbeSpec {
        timeout_ms: 0,
        ..valid_probe()
    };
    assert!(validate(&zero).is_err(), "timeout 0 rejected");
    let over = ProbeSpec {
        timeout_ms: REMOTE_PROBE_TIMEOUT_MS + 1,
        ..valid_probe()
    };
    assert!(
        validate(&over).is_err(),
        "timeout over remote ceiling rejected"
    );
}

#[test]
fn validate_rejects_max_bytes_zero_and_over_limit() {
    let zero = ProbeSpec {
        max_bytes: 0,
        ..valid_probe()
    };
    assert!(validate(&zero).is_err(), "max_bytes 0 rejected");
    let over = ProbeSpec {
        max_bytes: PROBE_STREAM_LIMIT + 1,
        ..valid_probe()
    };
    assert!(validate(&over).is_err(), "max_bytes over limit rejected");
}

#[test]
fn validate_rejects_invalid_json_pointer() {
    let spec = ProbeSpec {
        identity: IdentityRecognizer::JsonPointer {
            pointer: "identity".to_string(),
            anchored_pattern: AnchoredPattern::VersionToken,
        },
        ..valid_probe()
    };
    assert!(validate(&spec).is_err(), "invalid JSON pointer rejected");
}

#[test]
fn validate_rejects_empty_anchored_pattern_value() {
    let spec = ProbeSpec {
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern: AnchoredPattern::Prefix {
                prefix: String::new(),
            },
        },
        ..valid_probe()
    };
    assert!(validate(&spec).is_err(), "empty anchored pattern rejected");
}

fn valid_probe() -> ProbeSpec {
    ProbeSpec {
        argv: vec!["--version".to_string()],
        stream: ProbeStream::Stdout,
        framing: ProbeFraming::Utf8Text,
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern: AnchoredPattern::VersionToken,
        },
        capabilities: None,
        required: vec!["interactive".to_string()],
        timeout_ms: LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: PROBE_STREAM_LIMIT,
    }
}
