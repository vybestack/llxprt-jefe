//! core.code-puppy shipped definition (issue #382 CW-02).
//!
//! Mappings are fixture-proven: see
//! `tests/fixtures/agent-definitions/code-puppy/0.0.634/`.

use super::super::definition::AgentDefinition;
use super::super::fields::Emitter;
use super::super::normalize::Normalize;
use super::super::types::{
    OperationMatrix, OperationSupport, PromptShape, Support, TargetMatrix, TargetSupport,
};
use super::common::{
    DefinitionParts, assemble, bool_field, line_version_probe, optional_bool_field, path_candidate,
    sig_string_field, uvx_candidate,
};

/// Build the core.code-puppy shipped definition.
pub fn build() -> AgentDefinition {
    assemble(DefinitionParts {
        id: "core.code-puppy",
        display_name: "Code Puppy",
        minimum_version: "0.0.634",
        candidates: vec![
            path_candidate("code-puppy"),
            uvx_candidate("code-puppy", "code-puppy"),
        ],
        probe: code_puppy_probe(),
        operations: OperationMatrix {
            normal: OperationSupport {
                supported: Support::supported(),
                prompt: PromptShape::InitialPositional,
            },
            resume: OperationSupport {
                supported: Support::supported(),
                prompt: PromptShape::None,
            },
            fresh_issue: OperationSupport {
                supported: Support::supported(),
                prompt: PromptShape::InitialPositional,
            },
            fresh_pull_request: OperationSupport {
                supported: Support::supported(),
                prompt: PromptShape::InitialPositional,
            },
        },
        targets: TargetMatrix {
            local: TargetSupport {
                supported: Support::supported(),
            },
            remote: TargetSupport {
                supported: Support::unsupported("Code Puppy remote/setup is not fixture-verified"),
            },
        },
        repository_fields: vec![sig_string_field("model"), optional_bool_field("yolo", None)],
        agent_fields: vec![
            sig_string_field("version_selector"),
            sig_string_field("prompt"),
            bool_field("interactive"),
        ],
        emitters: vec![
            Emitter::Option {
                name: "--prompt".to_string(),
                field: "prompt".to_string(),
            },
            Emitter::Option {
                name: "--model".to_string(),
                field: "model".to_string(),
            },
            Emitter::BooleanOption {
                name: "--yolo".to_string(),
                field: "yolo".to_string(),
                true_value: "true".to_string(),
                false_value: Some("false".to_string()),
            },
            Emitter::Flag {
                name: "--interactive".to_string(),
                field: "interactive".to_string(),
            },
        ],
    })
}

fn code_puppy_probe() -> super::super::probe::ProbeSpec {
    // `code-puppy --version` wraps its version in OSC colour sequences, so the
    // identity stream must be stripped before the version token is recognized.
    line_version_probe(Normalize::StripAnsi)
}
