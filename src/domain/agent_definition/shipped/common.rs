//! Shipped-definition builder helpers (issue #382 CW-02).
//!
//! Small pure functions that assemble the repeated pieces of an
//! [`AgentDefinition`] so each agent module stays under the function-line and
//! complexity limits. No product tokens live here.

use std::path::PathBuf;

use super::super::definition::{AgentDefinition, DEFINITION_SCHEMA};
use super::super::fields::{Emitter, Field, FieldKind, FieldValue};
use super::super::limits::{LOCAL_PROBE_TIMEOUT_MS, PROBE_STREAM_LIMIT};
use super::super::probe::{
    AnchoredPattern, IdentityRecognizer, ProbeFraming, ProbeSpec, ProbeStream,
};
use super::super::type_id::{CandidateKind, ExecutableCandidate};
use super::super::types::{OperationMatrix, Support, TargetMatrix, TargetSupport};

/// Build a string field with `launch_signature` participation.
pub fn sig_string_field(id: &str) -> Field {
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

/// Build an enum field with the given choices.
pub fn enum_field(id: &str, choices: &[&str]) -> Field {
    Field {
        id: id.to_string(),
        kind: FieldKind::Enum,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: choices.iter().map(|c| (*c).to_string()).collect(),
        visible_when: None,
        launch_signature: true,
    }
}

/// Build a boolean field.
pub fn bool_field(id: &str) -> Field {
    Field {
        id: id.to_string(),
        kind: FieldKind::Boolean,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: None,
        launch_signature: true,
    }
}

/// Build an optional-boolean field with a default value.
pub fn optional_bool_field(id: &str, default: Option<bool>) -> Field {
    Field {
        id: id.to_string(),
        kind: FieldKind::OptionalBoolean,
        required: false,
        default: default.map(|b| FieldValue::OptionalBoolean(Some(b))),
        minimum: None,
        maximum: None,
        choices: vec![],
        visible_when: None,
        launch_signature: true,
    }
}

/// Build a path-name candidate.
pub fn path_candidate(name: &str) -> ExecutableCandidate {
    ExecutableCandidate {
        kind: CandidateKind::PathName {
            name: name.to_string(),
        },
        value: PathBuf::from(name),
    }
}

/// Build an npm-package candidate.
pub fn npm_candidate(package: &str, binary: &str) -> ExecutableCandidate {
    ExecutableCandidate {
        kind: CandidateKind::NpmPackage {
            package: package.to_string(),
            binary: binary.to_string(),
        },
        value: PathBuf::from(binary),
    }
}

/// Build a uvx-package candidate.
pub fn uvx_candidate(package: &str, binary: &str) -> ExecutableCandidate {
    ExecutableCandidate {
        kind: CandidateKind::UvxPackage {
            package: package.to_string(),
            binary: binary.to_string(),
        },
        value: PathBuf::from(binary),
    }
}

/// Build a line-prefix identity probe spec.
pub fn line_prefix_probe(prefix: &str, required: &[&str]) -> ProbeSpec {
    ProbeSpec {
        argv: vec!["--version".to_string()],
        stream: ProbeStream::Stdout,
        framing: ProbeFraming::Utf8Text,
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern: AnchoredPattern::Prefix {
                prefix: prefix.to_string(),
            },
        },
        capabilities: None,
        required: required.iter().map(|s| (*s).to_string()).collect(),
        timeout_ms: LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: PROBE_STREAM_LIMIT,
    }
}

/// Build a line-suffix identity probe spec.
pub fn line_suffix_probe(suffix: &str, required: &[&str]) -> ProbeSpec {
    ProbeSpec {
        argv: vec!["--version".to_string()],
        stream: ProbeStream::Stdout,
        framing: ProbeFraming::Utf8Text,
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern: AnchoredPattern::Suffix {
                suffix: suffix.to_string(),
            },
        },
        capabilities: None,
        required: required.iter().map(|s| (*s).to_string()).collect(),
        timeout_ms: LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: PROBE_STREAM_LIMIT,
    }
}

/// Build a local-supported, remote-unsupported target matrix.
pub fn local_only_targets(remote_reason: &str) -> TargetMatrix {
    TargetMatrix {
        local: TargetSupport {
            supported: Support::supported(),
        },
        remote: TargetSupport {
            supported: Support::unsupported(remote_reason),
        },
    }
}

/// Build an operation matrix where normal and resume are supported but
/// fresh-issue and fresh-PR are unsupported with the given reasons.
pub fn unsupported_only_operations(
    fresh_issue_reason: &str,
    fresh_pull_request_reason: &str,
) -> OperationMatrix {
    use super::super::types::{OperationSupport, PromptShape};
    OperationMatrix {
        normal: OperationSupport {
            supported: Support::supported(),
            prompt: PromptShape::InitialPositional,
        },
        resume: OperationSupport {
            supported: Support::supported(),
            prompt: PromptShape::None,
        },
        fresh_issue: OperationSupport {
            supported: Support::unsupported(fresh_issue_reason),
            prompt: PromptShape::None,
        },
        fresh_pull_request: OperationSupport {
            supported: Support::unsupported(fresh_pull_request_reason),
            prompt: PromptShape::None,
        },
    }
}

/// Input bundle for [`assemble`], grouping the per-agent pieces.
pub struct DefinitionParts {
    /// Stable agent type id (validated).
    pub id: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Ordered executable candidates.
    pub candidates: Vec<ExecutableCandidate>,
    /// Closed probe specification.
    pub probe: ProbeSpec,
    /// Per-operation support matrix.
    pub operations: OperationMatrix,
    /// Per-target support matrix.
    pub targets: TargetMatrix,
    /// Repository-scope form fields.
    pub repository_fields: Vec<Field>,
    /// Agent-scope form fields.
    pub agent_fields: Vec<Field>,
    /// Ordered argv/env emitters.
    pub emitters: Vec<Emitter>,
}

/// Assemble a definition from its parts.
pub fn assemble(parts: DefinitionParts) -> AgentDefinition {
    AgentDefinition {
        schema: DEFINITION_SCHEMA,
        id: super::super::type_id::AgentTypeId::from_validated(parts.id),
        display_name: parts.display_name.to_string(),
        candidates: parts.candidates,
        probe: parts.probe,
        operations: parts.operations,
        targets: parts.targets,
        repository_fields: parts.repository_fields,
        agent_fields: parts.agent_fields,
        emitters: parts.emitters,
    }
}
