//! Unit tests for `AgentTypeId`, `CandidateKind`, and capability-id grammar.

use super::*;

#[test]
fn type_id_grammar_accepts_stable_literals() {
    for valid in [
        "core.llxprt",
        "core.code-puppy",
        "core.codex",
        "core.claude-code",
        "a",
        "a.b",
        "a.b.c",
        "core.llxprt.0",
        "core.0",
    ] {
        let parsed = AgentTypeId::parse(valid);
        assert!(parsed.is_ok(), "{valid:?} must parse, got {parsed:?}");
    }
}

#[test]
fn type_id_grammar_rejects_invalid() {
    for invalid in [
        "",
        "A",
        "0abc",
        "core..llxprt",
        "core.llxprt.",
        "core.-llxprt",
        "core.llxprt-",
        "core.llvm_",
        "with space",
        "with/slash",
    ] {
        let parsed = AgentTypeId::parse(invalid);
        assert!(parsed.is_err(), "{invalid:?} must be rejected");
    }
}

#[test]
fn type_id_length_bounds_n_and_n_plus_one() {
    // N = 128 accepted, N+1 = 129 rejected.
    let exactly = str::repeat("a", super::super::limits::ID_BYTE_LIMIT);
    let too_long = str::repeat("a", super::super::limits::ID_BYTE_LIMIT + 1);
    assert!(
        AgentTypeId::parse(&exactly).is_ok(),
        "128 bytes accepted at N"
    );
    assert!(
        AgentTypeId::parse(&too_long).is_err(),
        "129 bytes rejected at N+1"
    );
}

#[test]
fn type_id_round_trips_through_serde() {
    let id = AgentTypeId::parse("core.llxprt").unwrap_or_else(|error| panic!("valid: {error}"));
    let json = serde_json::to_string(&id).unwrap_or_else(|error| panic!("serialize: {error}"));
    assert_eq!(json, "\"core.llxprt\"");
    let back: AgentTypeId =
        serde_json::from_str(&json).unwrap_or_else(|error| panic!("deserialize: {error}"));
    assert_eq!(back, id);
}

#[test]
fn type_id_serde_rejects_invalid() {
    let bad = serde_json::from_str::<AgentTypeId>("\"Core.Bad\"");
    assert!(bad.is_err(), "serde must reject invalid id");
}

#[test]
fn from_validated_constructs_stable_ids() {
    let id = AgentTypeId::from_validated("core.codex");
    assert_eq!(id.as_str(), "core.codex");
}

#[test]
fn capability_id_grammar_accepts_lowercase_tokens() {
    for valid in ["interactive", "resume", "fresh.issue", "remote.setup"] {
        assert!(
            validate_capability_id(valid).is_ok(),
            "{valid:?} must be a valid capability id"
        );
    }
}

#[test]
fn capability_id_grammar_rejects_uppercase_or_invalid() {
    for invalid in ["", "UPPER", "with space", "core..bad"] {
        assert!(
            validate_capability_id(invalid).is_err(),
            "{invalid:?} must be rejected"
        );
    }
}

#[test]
fn candidate_kind_path_name_rejects_slash() {
    let candidate = ExecutableCandidate {
        kind: CandidateKind::PathName {
            name: "bin/llxprt".to_string(),
        },
        value: std::path::PathBuf::from("bin/llxprt"),
    };
    assert!(
        candidate.validate().is_err(),
        "path-name with slash rejected"
    );
}

#[test]
fn candidate_kind_repository_llxprt_accepts_safe_relative() {
    let candidate = ExecutableCandidate {
        kind: CandidateKind::RepositoryLlxprt,
        value: std::path::PathBuf::from(".llxprt/bin/llxprt"),
    };
    assert!(candidate.validate().is_ok(), "safe relative path accepted");
}

#[test]
fn candidate_kind_repository_llxprt_rejects_traversal() {
    let candidate = ExecutableCandidate {
        kind: CandidateKind::RepositoryLlxprt,
        value: std::path::PathBuf::from("../escape"),
    };
    assert!(candidate.validate().is_err(), "traversal rejected");
}

#[test]
fn candidate_kind_package_runner_validates_lengths() {
    let candidate = ExecutableCandidate {
        kind: CandidateKind::NpmPackage {
            package: "@vybestack/llxprt-code".to_string(),
            binary: "llxprt".to_string(),
        },
        value: std::path::PathBuf::from("llxprt"),
    };
    assert!(candidate.validate().is_ok(), "valid npm candidate accepted");
}

#[test]
fn candidate_kind_package_runner_rejects_empty_package() {
    let candidate = ExecutableCandidate {
        kind: CandidateKind::NpmPackage {
            package: String::new(),
            binary: "llxprt".to_string(),
        },
        value: std::path::PathBuf::from("llxprt"),
    };
    assert!(candidate.validate().is_err(), "empty package rejected");
}

#[test]
fn candidate_kind_package_runner_rejects_unsafe_package_and_binary() {
    let unsafe_package = ExecutableCandidate {
        kind: CandidateKind::NpmPackage {
            package: "package name".to_string(),
            binary: "agent".to_string(),
        },
        value: std::path::PathBuf::from("agent"),
    };
    assert_eq!(
        unsafe_package.validate(),
        Err(CandidateValidateError::PackageUnsafe)
    );

    for binary in ["../agent", "dir/agent", "dir\\agent", "agent name", "."] {
        let unsafe_binary = ExecutableCandidate {
            kind: CandidateKind::UvxPackage {
                package: "agent-package".to_string(),
                binary: binary.to_string(),
            },
            value: std::path::PathBuf::from(binary),
        };
        assert_eq!(
            unsafe_binary.validate(),
            Err(CandidateValidateError::BinaryUnsafe),
            "binary {binary:?} must be rejected"
        );
    }
}
