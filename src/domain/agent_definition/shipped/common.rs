//! Shipped-definition builder helpers (issue #382 CW-02).
//!
//! Small pure functions that assemble the repeated pieces of an
//! [`AgentDefinition`] so each agent module stays under the function-line and
//! complexity limits. No product tokens live here.

use std::path::PathBuf;

use super::super::definition::{AgentDefinition, DEFINITION_SCHEMA};
use super::super::fields::{Emitter, Field, FieldKind, FieldValue};
use super::super::limits::{LOCAL_PROBE_TIMEOUT_MS, PROBE_STREAM_LIMIT};
use super::super::normalize::Normalize;
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
    bool_field_with_default(id, None)
}

/// Build a boolean field with an explicit default value.
pub fn bool_field_with_default(id: &str, default: Option<bool>) -> Field {
    Field {
        id: id.to_string(),
        kind: FieldKind::Boolean,
        required: false,
        default: default.map(FieldValue::Boolean),
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
pub fn line_prefix_probe(prefix: &str) -> ProbeSpec {
    identity_probe(AnchoredPattern::Prefix {
        prefix: prefix.to_string(),
    })
}

/// Build a line-suffix identity probe spec.
pub fn line_suffix_probe(suffix: &str) -> ProbeSpec {
    identity_probe(AnchoredPattern::Suffix {
        suffix: suffix.to_string(),
    })
}

/// Build a version-token identity probe spec.
pub fn line_version_probe(normalize: Normalize) -> ProbeSpec {
    ProbeSpec {
        normalize,
        ..identity_probe(AnchoredPattern::VersionToken)
    }
}

/// Build the shared `--version` identity probe for a recognizer.
fn identity_probe(anchored_pattern: AnchoredPattern) -> ProbeSpec {
    ProbeSpec {
        argv: vec!["--version".to_string()],
        stream: ProbeStream::Stdout,
        framing: ProbeFraming::Utf8Text,
        normalize: Normalize::None,
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern,
        },
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

/// Build an operation matrix for an agent whose help declares a single optional
/// positional prompt that starts an interactive session.
///
/// Every prompt-bearing operation therefore shares one shape; only resume takes
/// no prompt.
pub fn positional_prompt_operations() -> OperationMatrix {
    use super::super::types::{OperationSupport, PromptShape};
    let initial_positional = || OperationSupport {
        supported: Support::supported(),
        prompt: PromptShape::InitialPositional,
    };
    OperationMatrix {
        normal: initial_positional(),
        resume: OperationSupport {
            supported: Support::supported(),
            prompt: PromptShape::None,
        },
        fresh_issue: initial_positional(),
        fresh_pull_request: initial_positional(),
    }
}

/// Input bundle for [`assemble`], grouping the per-agent pieces.
pub struct DefinitionParts {
    /// Stable agent type id (validated).
    pub id: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Release the argv mappings were authored against (documentation only).
    pub minimum_version: &'static str,
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
        minimum_version: parts.minimum_version.to_string(),
        candidates: parts.candidates,
        probe: parts.probe,
        operations: parts.operations,
        targets: parts.targets,
        repository_fields: parts.repository_fields,
        agent_fields: parts.agent_fields,
        emitters: parts.emitters,
    }
}
