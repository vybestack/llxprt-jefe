//! Unit tests for the closed probe specification and recognizer set.

use super::*;
use crate::domain::agent_definition::normalize::Normalize;

#[test]
fn version_token_recognizer_is_dynamic_but_closed() {
    assert!(matches_version_token("0.142.0"));
    assert!(matches_version_token("2.1.212"));
    assert!(matches_version_token("17.23.901-nightly.260720.d69bda66a"));
    assert!(!matches_version_token("not-a-version"));
    assert!(!matches_version_token("1.2"));
    assert!(!matches_version_token("1.2.3.4"));
    assert!(!matches_version_token("1.2.3-"));
}

#[test]
fn anchored_pattern_variants() {
    assert!(AnchoredPattern::Exact { value: "x".into() }.matches("x"));
    assert!(
        AnchoredPattern::Prefix {
            prefix: "codex-cli ".into()
        }
        .matches("codex-cli 17.23.901")
    );
    assert!(
        AnchoredPattern::Suffix {
            suffix: "(Claude Code)".into()
        }
        .matches("17.23.901 (Claude Code)")
    );
    assert!(AnchoredPattern::VersionToken.matches("17.23.901"));
}

#[test]
fn parse_text_identity_scans_past_nonmatching_lines() {
    let recognizer = IdentityRecognizer::Line {
        prefix: String::new(),
        anchored_pattern: AnchoredPattern::Prefix {
            prefix: "codex-cli ".into(),
        },
    };
    assert_eq!(
        parse_text_identity("noise\ncodex-cli 17.23.901\n", &recognizer),
        Some("codex-cli 17.23.901".to_string())
    );
}

#[test]
fn evaluate_identity_strips_ansi_within_the_stream_bound() {
    let mut spec = valid_probe();
    spec.max_bytes = 64;
    if let Some(probe) = &mut spec.capabilities {
        probe.normalize = Normalize::StripAnsi;
    }
    let identity = evaluate_identity(b"\x1b]11;#000000\x0717.23.901\n\x1b]104\x07", &spec);
    assert_eq!(identity, Ok(Some("17.23.901".to_string())));
}

#[test]
fn evaluate_identity_reports_exact_input_errors() {
    let spec = ProbeSpec {
        max_bytes: 3,
        ..valid_probe()
    };
    assert_eq!(
        evaluate_identity(b"1234", &spec),
        Err(ProbeParseError::StreamTooLong {
            bytes: 4,
            max_bytes: 3,
        })
    );
    assert_eq!(
        ProbeParseError::StreamTooLong {
            bytes: 4,
            max_bytes: 3,
        }
        .to_string(),
        "probe stream is 4 bytes; maximum is 3"
    );
    let utf8_spec = ProbeSpec {
        max_bytes: 8,
        ..valid_probe()
    };
    assert_eq!(
        evaluate_identity(&[0xff], &utf8_spec),
        Err(ProbeParseError::InvalidUtf8)
    );
    assert_eq!(
        ProbeParseError::InvalidUtf8.to_string(),
        "probe stream is not valid UTF-8"
    );
}

#[test]
fn capability_evaluation_sorts_and_reports_required_tokens() {
    let probe = capability_probe(&[
        ("resume", "resume"),
        ("model", "--model"),
        ("interactive", "--interactive"),
    ]);
    let required = vec!["interactive".to_string()];
    let evaluated = evaluate_capabilities("resume --model value", &probe, &required);
    assert_eq!(
        evaluated.present,
        vec!["model".to_string(), "resume".to_string()]
    );
    assert_eq!(evaluated.missing_required, required);
    assert!(!evaluated.all_required_present());
}

#[test]
fn capability_evaluation_strips_ansi() {
    let mut probe = capability_probe(&[("interactive", "--interactive")]);
    probe.normalize = Normalize::StripAnsi;
    let evaluated = evaluate_capabilities(
        "\x1b]11;#000000\x07--interactive\x1b[0m",
        &probe,
        &["interactive".to_string()],
    );
    assert_eq!(evaluated.present, vec!["interactive".to_string()]);
    assert!(evaluated.all_required_present());
}

#[test]
fn token_matching_is_boundary_safe_for_flags_and_commands() {
    assert!(token_present("Options: --model <MODEL>", "--model"));
    assert!(!token_present("Options: --model-name <MODEL>", "--model"));
    assert!(!token_present("Options: ---model <MODEL>", "--model"));
    assert!(token_present("Commands:\n  resume  Continue", "resume"));
    assert!(!token_present("Commands:\n  presume  Continue", "resume"));
    assert!(!token_present(
        "Commands:\n  resume-last  Continue",
        "resume"
    ));
}

#[test]
fn validate_accepts_authored_required_token() {
    assert!(validate(&valid_probe()).is_ok());
}

#[test]
fn validate_rejects_empty_and_overlong_argv() {
    let empty = ProbeSpec {
        argv: vec![],
        ..valid_probe()
    };
    assert_eq!(
        validate(&empty),
        Err(ProbeValidateError::ArgvBounds { len: 0 })
    );
    let over = ProbeSpec {
        argv: vec!["a".to_string(); PROBE_ARGV_LIMIT + 1],
        ..valid_probe()
    };
    assert_eq!(
        validate(&over),
        Err(ProbeValidateError::ArgvBounds {
            len: PROBE_ARGV_LIMIT + 1,
        })
    );
}

#[test]
fn validate_rejects_capability_bounds_duplicates_and_missing_token() {
    let over = ProbeSpec {
        required: vec!["cap".to_string(); CAPABILITY_LIMIT + 1],
        ..valid_probe()
    };
    assert!(matches!(
        validate(&over),
        Err(ProbeValidateError::CapabilityBounds { len }) if len == CAPABILITY_LIMIT + 1
    ));

    let duplicate = ProbeSpec {
        required: vec!["interactive".to_string(), "interactive".to_string()],
        ..valid_probe()
    };
    assert!(matches!(
        validate(&duplicate),
        Err(ProbeValidateError::DuplicateCapability { index: 1, .. })
    ));

    let missing = ProbeSpec {
        required: vec!["missing".to_string()],
        ..valid_probe()
    };
    assert_eq!(
        validate(&missing),
        Err(ProbeValidateError::RequiredCapabilityHasNoToken {
            index: 0,
            id: "missing".to_string(),
        })
    );
}

#[test]
fn validate_reports_actual_capability_token_index() {
    let mut spec = valid_probe();
    if let Some(probe) = &mut spec.capabilities {
        probe.tokens.push(CapabilityToken {
            id: "UPPER".to_string(),
            token: "--upper".to_string(),
        });
    }
    assert!(matches!(
        validate(&spec),
        Err(ProbeValidateError::InvalidCapabilityId { index: 1, .. })
    ));
}

#[test]
fn validate_rejects_duplicate_literal_tokens() {
    let mut spec = valid_probe();
    if let Some(probe) = &mut spec.capabilities {
        probe.tokens.push(CapabilityToken {
            id: "second".to_string(),
            token: "--interactive".to_string(),
        });
    }
    assert_eq!(
        validate(&spec),
        Err(ProbeValidateError::DuplicateToken {
            index: 1,
            token: "--interactive".to_string(),
        })
    );
}

#[test]
fn validate_rejects_timeout_and_stream_bounds() {
    let zero_timeout = ProbeSpec {
        timeout_ms: 0,
        ..valid_probe()
    };
    assert!(validate(&zero_timeout).is_err());
    let over_timeout = ProbeSpec {
        timeout_ms: REMOTE_PROBE_TIMEOUT_MS + 1,
        ..valid_probe()
    };
    assert!(validate(&over_timeout).is_err());
    let zero_stream = ProbeSpec {
        max_bytes: 0,
        ..valid_probe()
    };
    assert!(validate(&zero_stream).is_err());
    let over_stream = ProbeSpec {
        max_bytes: PROBE_STREAM_LIMIT + 1,
        ..valid_probe()
    };
    assert!(validate(&over_stream).is_err());
}

#[test]
fn validate_rejects_invalid_recognizer() {
    let pointer = ProbeSpec {
        identity: IdentityRecognizer::JsonPointer {
            pointer: "identity".to_string(),
            anchored_pattern: AnchoredPattern::VersionToken,
        },
        ..valid_probe()
    };
    assert!(validate(&pointer).is_err());
    let empty_pattern = ProbeSpec {
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern: AnchoredPattern::Prefix {
                prefix: String::new(),
            },
        },
        ..valid_probe()
    };
    assert!(validate(&empty_pattern).is_err());
}

fn capability_probe(tokens: &[(&str, &str)]) -> CapabilityProbe {
    CapabilityProbe {
        argv: vec!["--help".to_string()],
        stream: ProbeStream::Stdout,
        normalize: Normalize::None,
        tokens: tokens
            .iter()
            .map(|(id, token)| CapabilityToken {
                id: (*id).to_string(),
                token: (*token).to_string(),
            })
            .collect(),
    }
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
        capabilities: Some(capability_probe(&[("interactive", "--interactive")])),
        required: vec!["interactive".to_string()],
        timeout_ms: LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: PROBE_STREAM_LIMIT,
    }
}
