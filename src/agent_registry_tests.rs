//! Unit tests for the immutable [`AgentTypeRegistry`] (issue #382 CW-02 S2).

use crate::agent_registry::{AgentTypeRegistry, RegistryPublishError};
use crate::domain::agent_definition::diagnostics::DefinitionError;
use crate::domain::agent_definition::probe::{
    AnchoredPattern, IdentityRecognizer, ProbeFraming, ProbeSpec, ProbeStream,
};
use crate::domain::agent_definition::type_id::{AgentTypeId, CandidateKind, ExecutableCandidate};
use crate::domain::agent_definition::types::{OperationMatrix, TargetMatrix};
use crate::domain::agent_definition::{AgentDefinition, DEFINITION_SCHEMA};

use std::path::PathBuf;

fn path_name(name: &str) -> ExecutableCandidate {
    ExecutableCandidate {
        kind: CandidateKind::PathName {
            name: name.to_string(),
        },
        value: PathBuf::from(name),
    }
}

fn definition(id: &str, candidate_name: &str) -> AgentDefinition {
    let Ok(parsed_id) = AgentTypeId::parse(id) else {
        panic!("valid test id must parse");
    };
    AgentDefinition {
        schema: DEFINITION_SCHEMA,
        id: parsed_id,
        display_name: id.to_string(),
        candidates: vec![path_name(candidate_name)],
        probe: ProbeSpec {
            argv: vec!["--version".to_string()],
            stream: ProbeStream::Stdout,
            framing: ProbeFraming::Utf8Text,
            identity: IdentityRecognizer::Line {
                prefix: String::new(),
                anchored_pattern: AnchoredPattern::VersionToken,
            },
            capabilities: None,
            required: vec!["x".to_string()],
            timeout_ms: 5_000,
            max_bytes: 65_536,
        },
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![],
        agent_fields: vec![],
        emitters: vec![],
    }
}

#[test]
fn publish_validates_and_stores_in_canonical_id_order() {
    let defs = vec![
        definition("core.zebra", "z"),
        definition("core.alpha", "a"),
        definition("core.middle", "m"),
    ];
    let registry =
        AgentTypeRegistry::publish(defs).unwrap_or_else(|error| panic!("publish valid: {error}"));
    let ids: Vec<&str> = registry
        .definitions()
        .iter()
        .map(|d| d.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["core.alpha", "core.middle", "core.zebra"],
        "definitions stored in canonical ID order"
    );
    assert_eq!(registry.len(), 3);
    assert!(!registry.is_empty());
}

#[test]
fn publish_rejects_duplicate_type_id() {
    let defs = vec![definition("core.dup", "a"), definition("core.dup", "b")];
    let Err(err) = AgentTypeRegistry::publish(defs) else {
        panic!("duplicate rejected");
    };
    assert!(
        matches!(err, RegistryPublishError::DuplicateTypeId { ref id } if id == "core.dup"),
        "{err:?}"
    );
}

#[test]
fn publish_rejects_invalid_definition() {
    // Empty candidate list violates the closed bound (1..=8).
    let mut def = definition("core.bad", "a");
    def.candidates.clear();
    let Err(err) = AgentTypeRegistry::publish(vec![def]) else {
        panic!("invalid rejected");
    };
    assert!(
        matches!(err, RegistryPublishError::Definition(_)),
        "{err:?}"
    );
}

#[test]
fn publish_rejects_wrong_schema_version() {
    let mut def = definition("core.wrong-schema", "a");
    def.schema = 999;
    let Err(err) = AgentTypeRegistry::publish(vec![def]) else {
        panic!("wrong schema rejected");
    };
    assert!(matches!(
        err,
        RegistryPublishError::Definition(DefinitionError::SchemaVersion { found: 999 })
    ));
}

#[test]
fn get_returns_definition_by_id() {
    let defs = vec![definition("core.alpha", "a"), definition("core.beta", "b")];
    let registry =
        AgentTypeRegistry::publish(defs).unwrap_or_else(|error| panic!("publish: {error}"));
    let id = AgentTypeId::parse("core.beta").unwrap_or_else(|error| panic!("valid id: {error}"));
    let Some(found) = registry.get(&id) else {
        panic!("found by id");
    };
    assert_eq!(found.id.as_str(), "core.beta");
    let missing =
        AgentTypeId::parse("core.gamma").unwrap_or_else(|error| panic!("valid id: {error}"));
    assert!(registry.get(&missing).is_none());
}

#[test]
fn at_returns_definition_by_canonical_index() {
    let defs = vec![definition("core.alpha", "a"), definition("core.beta", "b")];
    let registry =
        AgentTypeRegistry::publish(defs).unwrap_or_else(|error| panic!("publish: {error}"));
    let Some(zero) = registry.at(0) else {
        panic!("index 0");
    };
    assert_eq!(zero.id.as_str(), "core.alpha");
    let Some(one) = registry.at(1) else {
        panic!("index 1");
    };
    assert_eq!(one.id.as_str(), "core.beta");
    assert!(registry.at(2).is_none());
}

#[test]
fn shipped_publishes_four_definitions() {
    let registry =
        AgentTypeRegistry::shipped().unwrap_or_else(|error| panic!("shipped publishes: {error}"));
    assert_eq!(registry.len(), 4, "exactly four shipped definitions");
    // Canonical order is bytewise-stable.
    let ids: Vec<&str> = registry
        .definitions()
        .iter()
        .map(|d| d.id.as_str())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "shipped registry in canonical order");
}

#[test]
fn empty_registry_is_constructible() {
    let registry = AgentTypeRegistry::publish(vec![])
        .unwrap_or_else(|error| panic!("empty publishes: {error}"));
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn definition_error_converts_into_registry_error() {
    let err: RegistryPublishError = DefinitionError::DisplayNameLength { bytes: 0 }.into();
    assert!(matches!(err, RegistryPublishError::Definition(_)));
}

#[test]
fn registry_error_display_is_informative() {
    let dup = RegistryPublishError::DuplicateTypeId {
        id: "core.x".to_string(),
    };
    assert!(dup.to_string().contains("core.x"));
}
