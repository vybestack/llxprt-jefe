//! core.llxprt shipped definition (issue #382 CW-02).
//!
//! Mappings are fixture-proven: see
//! `tests/fixtures/agent-definitions/llxprt/0.10.0-nightly.260720.d69bda66a/`.

use std::path::PathBuf;

use super::super::definition::AgentDefinition;
use super::super::fields::Emitter;
use super::super::type_id::{CandidateKind, ExecutableCandidate};
use super::super::types::{
    OperationMatrix, OperationSupport, PromptShape, Support, TargetMatrix, TargetSupport,
};
use super::common::{
    DefinitionParts, assemble, bool_field, line_prefix_probe, npm_candidate, path_candidate,
    sig_string_field,
};

/// Build the core.llxprt shipped definition.
pub fn build() -> AgentDefinition {
    assemble(DefinitionParts {
        id: "core.llxprt",
        display_name: "LLxprt",
        candidates: vec![
            ExecutableCandidate {
                kind: CandidateKind::RepositoryLlxprt,
                value: PathBuf::from(".llxprt/bin/llxprt"),
            },
            path_candidate("llxprt"),
            npm_candidate("@vybestack/llxprt-code", "llxprt"),
        ],
        probe: line_prefix_probe("0.", &["interactive"]),
        operations: OperationMatrix {
            normal: OperationSupport {
                supported: Support::supported(),
                prompt: PromptShape::InteractiveOption,
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
                supported: Support::supported(),
            },
        },
        repository_fields: vec![sig_string_field("profile"), bool_field("yolo")],
        agent_fields: vec![bool_field("interactive")],
        emitters: vec![
            Emitter::Option {
                name: "--profile-load".to_string(),
                field: "profile".to_string(),
            },
            Emitter::Flag {
                field: "yolo".to_string(),
            },
            Emitter::Flag {
                field: "interactive".to_string(),
            },
        ],
    })
}
