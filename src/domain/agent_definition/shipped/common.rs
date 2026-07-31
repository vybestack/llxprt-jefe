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
    AnchoredPattern, CapabilityProbe, CapabilityToken, IdentityRecognizer, ProbeFraming, ProbeSpec,
    ProbeStream,
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
pub fn line_prefix_probe(
    prefix: &str,
    capability_probe: CapabilityProbe,
    required: &[&str],
) -> ProbeSpec {
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
        capabilities: Some(capability_probe),
        required: required.iter().map(|s| (*s).to_string()).collect(),
        timeout_ms: LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: PROBE_STREAM_LIMIT,
    }
}

/// Build a line-suffix identity probe spec.
pub fn line_suffix_probe(
    suffix: &str,
    capability_probe: CapabilityProbe,
    required: &[&str],
) -> ProbeSpec {
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
        capabilities: Some(capability_probe),
        required: required.iter().map(|s| (*s).to_string()).collect(),
        timeout_ms: LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: PROBE_STREAM_LIMIT,
    }
}

/// Build a version-token identity probe spec.
pub fn line_version_probe(
    normalize: Normalize,
    mut capability_probe: CapabilityProbe,
    required: &[&str],
) -> ProbeSpec {
    capability_probe.normalize = normalize;
    ProbeSpec {
        argv: vec!["--version".to_string()],
        stream: ProbeStream::Stdout,
        framing: ProbeFraming::Utf8Text,
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern: AnchoredPattern::VersionToken,
        },
        capabilities: Some(capability_probe),
        required: required.iter().map(|s| (*s).to_string()).collect(),
        timeout_ms: LOCAL_PROBE_TIMEOUT_MS,
        max_bytes: PROBE_STREAM_LIMIT,
    }
}

/// Build a capability probe from authored (id, token) pairs.
pub fn capability_probe(normalize: Normalize, tokens: &[(&str, &str)]) -> CapabilityProbe {
    trusted_capability_probe(normalize, tokens, false)
}

/// Build a trusted capability probe from authored (id, token) pairs.
///
/// A trusted probe skips the runtime `--help` verification and reports every
/// authored token as present. Used for agents whose every release supports all
/// authored arguments, where the `--help` gate adds launch fragility.
pub fn trusted_capability_probe(
    normalize: Normalize,
    tokens: &[(&str, &str)],
    trusted: bool,
) -> CapabilityProbe {
    CapabilityProbe {
        argv: vec!["--help".to_string()],
        stream: ProbeStream::Stdout,
        normalize,
        trusted,
        tokens: tokens
            .iter()
            .map(|(id, token)| CapabilityToken {
                id: (*id).to_string(),
                token: (*token).to_string(),
            })
            .collect(),
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
