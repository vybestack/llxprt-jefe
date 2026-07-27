//! Unit tests for the validation helpers (graph invariants and bounds).

use super::super::definition::AgentDefinition;
use super::super::fields::{Emitter, Field, FieldKind};
use super::super::probe::{
    AnchoredPattern, CapabilityProbe, CapabilityToken, IdentityRecognizer, ProbeFraming, ProbeSpec,
    ProbeStream,
};
use super::super::type_id::{AgentTypeId, CandidateKind, ExecutableCandidate};
use super::super::types::{OperationMatrix, TargetMatrix};
use super::*;

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

fn string_field(id: &str) -> Field {
    Field {
        id: id.to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: None,
        launch_signature: true,
    }
}

fn base_def() -> AgentDefinition {
    let Ok(id) = AgentTypeId::parse("core.test") else {
        panic!("valid test id must parse");
    };
    AgentDefinition {
        schema: 1,
        id,
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
    }
}

#[test]
fn valid_definition_passes() {
    assert!(validate_definition(&base_def()).is_ok());
}

#[test]
fn duplicate_candidate_rejected() {
    let mut def = base_def();
    def.candidates.push(ExecutableCandidate {
        kind: CandidateKind::PathName {
            name: "a".to_string(),
        },
        value: std::path::PathBuf::from("a"),
    });
    let Err(err) = validate_definition(&def) else {
        panic!("duplicate candidate rejected");
    };
    assert!(matches!(
        err,
        DefinitionError::DuplicateCandidate { index: 1 }
    ));
}

#[test]
fn duplicate_field_id_rejected() {
    let mut def = base_def();
    let field = string_field("model");
    def.repository_fields = vec![field.clone(), field];
    let Err(err) = validate_definition(&def) else {
        panic!("duplicate field id rejected");
    };
    assert!(matches!(err, DefinitionError::DuplicateFieldId { .. }));
}

#[test]
fn unknown_visible_when_rejected() {
    let mut def = base_def();
    let mut field = string_field("model");
    field.visible_when = Some("nonexistent".to_string());
    def.repository_fields = vec![field];
    let Err(err) = validate_definition(&def) else {
        panic!("unknown visible_when rejected");
    };
    assert!(matches!(err, DefinitionError::UnknownVisibleWhen { .. }));
}

#[test]
fn visibility_cycle_rejected() {
    let mut def = base_def();
    let mut a = string_field("a");
    a.visible_when = Some("b".to_string());
    let mut b = string_field("b");
    b.visible_when = Some("a".to_string());
    def.repository_fields = vec![a, b];
    let Err(err) = validate_definition(&def) else {
        panic!("cycle rejected");
    };
    assert!(matches!(err, DefinitionError::VisibilityCycle { .. }));
}

#[test]
fn unknown_emitter_field_rejected() {
    let mut def = base_def();
    def.emitters = vec![Emitter::Flag {
        field: "nonexistent".to_string(),
    }];
    let Err(err) = validate_definition(&def) else {
        panic!("unknown emitter field rejected");
    };
    assert!(matches!(err, DefinitionError::UnknownEmitterField { .. }));
}

#[test]
fn duplicate_emitter_field_rejected() {
    let mut def = base_def();
    def.repository_fields = vec![string_field("model")];
    def.emitters = vec![
        Emitter::Flag {
            field: "model".to_string(),
        },
        Emitter::Flag {
            field: "model".to_string(),
        },
    ];
    let Err(err) = validate_definition(&def) else {
        panic!("duplicate emitter field rejected");
    };
    assert!(matches!(err, DefinitionError::DuplicateEmitterField { .. }));
}

#[test]
fn emitter_bounds_over_n_rejected() {
    let mut def = base_def();
    def.emitters = (0..=super::super::limits::EMITTER_LIMIT)
        .map(|i| Emitter::Fixed {
            value: format!("--e{i}"),
        })
        .collect();
    let Err(err) = validate_definition(&def) else {
        panic!("emitter bounds rejected");
    };
    assert!(matches!(err, DefinitionError::EmitterBounds { len: 129 }));
}

#[test]
fn repository_field_bounds_over_n_rejected() {
    let mut def = base_def();
    def.repository_fields = (0..=super::super::limits::FIELD_SCOPE_LIMIT)
        .map(|i| string_field(&format!("f{i}")))
        .collect();
    let Err(err) = validate_definition(&def) else {
        panic!("repository field bounds rejected");
    };
    assert!(matches!(
        err,
        DefinitionError::RepositoryFieldBounds { len: 65 }
    ));
}

#[test]
fn total_field_bounds_equals_two_per_scope_bounds() {
    // The issue mandates "fields 64 per scope/128 total form". Since
    // 2*FIELD_SCOPE_LIMIT == FORM_FIELD_LIMIT, the per-scope bounds are
    // always tighter and TotalFieldBounds is unreachable in practice. The
    // relationship is verified here so the contract is documented.
    assert_eq!(
        super::super::limits::FORM_FIELD_LIMIT,
        2 * super::super::limits::FIELD_SCOPE_LIMIT,
        "total form field bound must equal twice the per-scope bound"
    );
}

fn package_candidate() -> ExecutableCandidate {
    ExecutableCandidate {
        kind: CandidateKind::NpmPackage {
            package: "agent-package".to_string(),
            binary: "agent".to_string(),
        },
        value: std::path::PathBuf::from("npm"),
    }
}

#[test]
fn package_candidate_requires_generic_selector_contract() {
    let mut def = base_def();
    def.candidates = vec![package_candidate()];
    assert!(validate_definition(&def).is_err());

    def.agent_fields = vec![string_field("version_selector")];
    assert!(validate_definition(&def).is_ok());

    def.agent_fields[0].launch_signature = false;
    assert!(validate_definition(&def).is_err());
}

#[test]
fn selector_without_package_or_with_emitter_is_rejected() {
    let mut def = base_def();
    def.agent_fields = vec![string_field("version_selector")];
    assert!(validate_definition(&def).is_err());

    def.candidates = vec![package_candidate()];
    def.emitters = vec![Emitter::Positional {
        field: "version_selector".to_string(),
    }];
    assert!(validate_definition(&def).is_err());
}
