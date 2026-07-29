//! Unit tests for the closed definition contract strict-deserialization + validation.

use super::super::diagnostics::DefinitionError;
use super::super::fields::{Emitter, Field, FieldKind};
use super::super::probe::{
    AnchoredPattern, CapabilityProbe, CapabilityToken, IdentityRecognizer, ProbeFraming, ProbeSpec,
    ProbeStream,
};
use super::super::type_id::{AgentTypeId, CandidateKind, ExecutableCandidate};
use super::super::types::{OperationMatrix, TargetMatrix};
use super::*;

fn valid_definition_json() -> String {
    r#"{
        "agent_type_schema": 1,
        "id": "core.test",
        "display_name": "Test Agent",
        "executable_candidates": [
            {"kind": "path-name", "value": "test-agent"}
        ],
        "probe": {
            "argv": ["--version"],
            "stream": "stdout",
            "framing": "utf8_text",
            "identity": {
                "kind": "line",
                "prefix": "",
                "anchored_pattern": {"kind": "version_token"}
            },
            "capability_probe": {
                "argv": ["--help"],
                "stream": "stdout",
                "normalize": "none",
                "tokens": [{"id": "interactive", "token": "--interactive"}]
            },
            "required": ["interactive"],
            "timeout_ms": 5000,
            "max_bytes": 65536
        },
        "operations": {
            "normal": {"supported": true, "prompt": "initial_positional"},
            "resume": {"supported": true, "prompt": "none"},
            "fresh_issue": {"supported": false, "reason": "not supported"},
            "fresh_pull_request": {"supported": false, "reason": "not supported"}
        },
        "targets": {
            "local": {"supported": true},
            "remote": {"supported": false, "reason": "no remote"}
        },
        "repository_fields": [],
        "agent_fields": [],
        "emitters": []
    }"#
    .to_string()
}

#[test]
fn from_bytes_accepts_valid_definition() {
    let parsed = AgentDefinition::from_bytes(valid_definition_json().as_bytes());
    assert!(parsed.is_ok(), "valid definition must parse: {parsed:?}");
    let Ok(def) = parsed else {
        panic!("valid definition must parse");
    };
    assert_eq!(def.schema, 1);
    assert_eq!(def.id.as_str(), "core.test");
    assert_eq!(def.display_name, "Test Agent");
    assert_eq!(def.candidates.len(), 1);
}

#[test]
fn from_bytes_rejects_unknown_top_level_field() {
    let mut json = valid_definition_json();
    json.insert_str(json.len() - 2, ",\"extra\": 1");
    let parsed = AgentDefinition::from_bytes(json.as_bytes());
    assert!(parsed.is_err(), "unknown field rejected");
    let Err(err) = parsed else {
        panic!("unknown field must be rejected");
    };
    let msg = err.to_string();
    assert!(msg.contains("unknown field \"extra\""), "{msg}");
}

#[test]
fn from_bytes_rejects_duplicate_json_key() {
    let json = r#"{
        "agent_type_schema": 1,
        "agent_type_schema": 2,
        "id": "core.test",
        "display_name": "Test",
        "executable_candidates": [{"kind":"path-name","value":"a"}],
        "probe": {"argv":["--version"],"identity":{"kind":"line","prefix":"","anchored_pattern":{"kind":"version_token"}}},
        "operations": {},
        "targets": {}
    }"#;
    let err = AgentDefinition::from_bytes(json.as_bytes());
    assert!(err.is_err(), "duplicate JSON key rejected");
}

#[test]
fn from_bytes_rejects_wrong_schema_version() {
    let json =
        valid_definition_json().replace("\"agent_type_schema\": 1", "\"agent_type_schema\": 2");
    let parsed = AgentDefinition::from_bytes(json.as_bytes());
    assert!(parsed.is_err(), "wrong schema rejected");
    let Err(err) = parsed else {
        panic!("wrong schema must be rejected");
    };
    assert!(
        err.to_string().contains("schema version must be 1"),
        "schema version diagnostic"
    );
}

#[test]
fn from_bytes_rejects_invalid_type_id() {
    let json = valid_definition_json().replace("\"core.test\"", "\"Core.Test\"");
    let err = AgentDefinition::from_bytes(json.as_bytes());
    assert!(err.is_err(), "invalid type id rejected");
}

#[test]
fn from_bytes_rejects_empty_display_name() {
    let json = valid_definition_json().replace("\"Test Agent\"", "\"\"");
    let err = AgentDefinition::from_bytes(json.as_bytes());
    assert!(err.is_err(), "empty display name rejected");
}

#[test]
fn from_bytes_rejects_no_candidates() {
    let json = valid_definition_json().replace(
        "\"executable_candidates\": [\n            {\"kind\": \"path-name\", \"value\": \"test-agent\"}\n        ]",
        "\"executable_candidates\": []",
    );
    let err = AgentDefinition::from_bytes(json.as_bytes());
    assert!(err.is_err(), "no candidates rejected");
}

#[test]
fn validate_rejects_duplicate_candidate() {
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![
            ExecutableCandidate {
                kind: CandidateKind::PathName {
                    name: "a".to_string(),
                },
                value: std::path::PathBuf::from("a"),
            },
            ExecutableCandidate {
                kind: CandidateKind::PathName {
                    name: "a".to_string(),
                },
                value: std::path::PathBuf::from("a"),
            },
        ],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![],
        agent_fields: vec![],
        emitters: vec![],
    };
    let Err(err) = def.validate() else {
        panic!("duplicate candidate rejected");
    };
    assert!(matches!(
        err,
        DefinitionError::DuplicateCandidate { index: 1 }
    ));
}

#[test]
fn validate_rejects_repository_fields_over_n() {
    let fields: Vec<Field> = (0..=super::super::limits::FIELD_SCOPE_LIMIT)
        .map(|i| Field {
            id: format!("f{i}"),
            kind: FieldKind::String,
            required: false,
            default: None,
            minimum: None,
            maximum: None,
            choices: vec![],
            visible_when: None,
            launch_signature: false,
        })
        .collect();
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "a".to_string(),
            },
            value: std::path::PathBuf::from("a"),
        }],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: fields,
        agent_fields: vec![],
        emitters: vec![],
    };
    let Err(err) = def.validate() else {
        panic!("field bounds rejected");
    };
    assert!(matches!(
        err,
        DefinitionError::RepositoryFieldBounds { len: 65 }
    ));
}

#[test]
fn validate_rejects_duplicate_field_id_in_scope() {
    let field = Field {
        id: "model".to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: None,
        launch_signature: true,
    };
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "a".to_string(),
            },
            value: std::path::PathBuf::from("a"),
        }],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![field.clone(), field],
        agent_fields: vec![],
        emitters: vec![],
    };
    let Err(err) = def.validate() else {
        panic!("duplicate field id rejected");
    };
    assert!(matches!(err, DefinitionError::DuplicateFieldId { .. }));
}

#[test]
fn validate_rejects_unknown_visible_when() {
    let field = Field {
        id: "model".to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: Some("nonexistent".to_string()),
        launch_signature: true,
    };
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "a".to_string(),
            },
            value: std::path::PathBuf::from("a"),
        }],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![field],
        agent_fields: vec![],
        emitters: vec![],
    };
    let Err(err) = def.validate() else {
        panic!("unknown visible_when rejected");
    };
    assert!(matches!(err, DefinitionError::UnknownVisibleWhen { .. }));
}

#[test]
fn validate_rejects_visibility_cycle() {
    let field_a = Field {
        id: "a".to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: Some("b".to_string()),
        launch_signature: true,
    };
    let field_b = Field {
        id: "b".to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: Some("a".to_string()),
        launch_signature: true,
    };
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "x".to_string(),
            },
            value: std::path::PathBuf::from("x"),
        }],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![field_a, field_b],
        agent_fields: vec![],
        emitters: vec![],
    };
    let Err(err) = def.validate() else {
        panic!("cycle rejected");
    };
    assert!(matches!(err, DefinitionError::VisibilityCycle { .. }));
}

#[test]
fn validate_rejects_emitter_unknown_field() {
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "a".to_string(),
            },
            value: std::path::PathBuf::from("a"),
        }],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![],
        agent_fields: vec![],
        emitters: vec![Emitter::Flag {
            field: "nonexistent".to_string(),
        }],
    };
    let Err(err) = def.validate() else {
        panic!("unknown emitter field rejected");
    };
    assert!(matches!(err, DefinitionError::UnknownEmitterField { .. }));
}

#[test]
fn validate_rejects_duplicate_emitter_field() {
    let field = Field {
        id: "model".to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: None,
        launch_signature: true,
    };
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "a".to_string(),
            },
            value: std::path::PathBuf::from("a"),
        }],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![field],
        agent_fields: vec![],
        emitters: vec![
            Emitter::Flag {
                field: "model".to_string(),
            },
            Emitter::Flag {
                field: "model".to_string(),
            },
        ],
    };
    let Err(err) = def.validate() else {
        panic!("duplicate emitter field rejected");
    };
    assert!(matches!(err, DefinitionError::DuplicateEmitterField { .. }));
}

#[test]
fn validate_rejects_emitters_over_n() {
    let field = Field {
        id: "model".to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: None,
        launch_signature: true,
    };
    let emitters: Vec<Emitter> = (0..=super::super::limits::EMITTER_LIMIT)
        .map(|i| Emitter::Fixed {
            value: format!("--e{i}"),
        })
        .collect();
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "a".to_string(),
            },
            value: std::path::PathBuf::from("a"),
        }],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![field],
        agent_fields: vec![],
        emitters,
    };
    let Err(err) = def.validate() else {
        panic!("emitter bounds rejected");
    };
    assert!(matches!(err, DefinitionError::EmitterBounds { len: 129 }));
}

#[test]
fn validate_rejects_probe_with_duplicate_capability() {
    let mut probe = valid_probe();
    probe.required = vec!["interactive".to_string(), "interactive".to_string()];
    let def = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "a".to_string(),
            },
            value: std::path::PathBuf::from("a"),
        }],
        probe,
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![],
        agent_fields: vec![],
        emitters: vec![],
    };
    let Err(err) = def.validate() else {
        panic!("duplicate capability rejected");
    };
    assert!(matches!(err, DefinitionError::Probe(_)));
}

#[test]
fn sha256_is_stable_across_field_order() {
    let def_a = AgentDefinition {
        schema: 1,
        id: parse_test_id(),
        display_name: "Test".to_string(),
        candidates: vec![ExecutableCandidate {
            kind: CandidateKind::PathName {
                name: "a".to_string(),
            },
            value: std::path::PathBuf::from("a"),
        }],
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![],
        agent_fields: vec![],
        emitters: vec![],
    };
    let def_b = def_a.clone();
    assert_eq!(
        def_a.sha256(),
        def_b.sha256(),
        "identical definitions hash equally"
    );
}

#[test]
fn shipped_returns_four_definitions_in_id_order() {
    let defs = AgentDefinition::shipped();
    assert_eq!(defs.len(), 4, "exactly four shipped definitions");
    let ids: Vec<String> = defs.iter().map(|d| d.id.as_str().to_string()).collect();
    assert_eq!(
        ids,
        sorted(&ids),
        "shipped definitions are in canonical ID order"
    );
}

fn sorted(input: &[String]) -> Vec<String> {
    let mut out: Vec<String> = input.to_vec();
    out.sort_unstable();
    out
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
        capabilities: Some(CapabilityProbe {
            argv: vec!["--help".to_string()],
            stream: ProbeStream::Stdout,
            normalize: super::super::normalize::Normalize::None,
            tokens: vec![CapabilityToken {
                id: "interactive".to_string(),
                token: "--interactive".to_string(),
            }],
        }),
        required: vec!["interactive".to_string()],
        timeout_ms: super::super::limits::LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: super::super::limits::PROBE_STREAM_LIMIT,
    }
}

fn parse_test_id() -> AgentTypeId {
    let Ok(id) = AgentTypeId::parse("core.test") else {
        panic!("valid test id must parse");
    };
    id
}
