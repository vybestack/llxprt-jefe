//! core.code-puppy shipped definition (issue #382 CW-02).
//!
//! Mappings are fixture-proven: see
//! `tests/fixtures/agent-definitions/code-puppy/0.0.634/`.

use super::super::definition::AgentDefinition;
use super::super::fields::Emitter;
use super::super::types::{
    OperationMatrix, OperationSupport, PromptShape, Support, TargetMatrix, TargetSupport,
};
use super::common::{
    DefinitionParts, assemble, bool_field, line_suffix_probe, optional_bool_field, path_candidate,
    sig_string_field, uvx_candidate,
};

/// Build the core.code-puppy shipped definition.
pub fn build() -> AgentDefinition {
    assemble(DefinitionParts {
        id: "core.code-puppy",
        display_name: "Code Puppy",
        candidates: vec![
            path_candidate("code-puppy"),
            uvx_candidate("code-puppy", "code-puppy"),
        ],
        probe: line_suffix_probe("0.0.634", &["interactive"]),
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
                prompt: PromptShape::InteractiveOption,
            },
            fresh_pull_request: OperationSupport {
                supported: Support::supported(),
                prompt: PromptShape::InteractiveOption,
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
        agent_fields: vec![bool_field("interactive")],
        emitters: vec![
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
                field: "interactive".to_string(),
            },
        ],
    })
}
