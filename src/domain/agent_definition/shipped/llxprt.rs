//! core.llxprt shipped definition (issue #382 CW-02).
//!
//! Mappings are fixture-proven: see
//! `tests/fixtures/agent-definitions/llxprt/0.10.0-nightly.260720.d69bda66a/`.

use std::path::PathBuf;

use super::super::definition::AgentDefinition;
use super::super::fields::Emitter;
use super::super::normalize::Normalize;
use super::super::type_id::{CandidateKind, ExecutableCandidate};
use super::super::types::{
    OperationMatrix, OperationSupport, PromptShape, Support, TargetMatrix, TargetSupport,
};
use super::common::{
    DefinitionParts, assemble, bool_field, capability_probe, line_version_probe, npm_candidate,
    path_candidate, sig_string_field,
};

fn emitters() -> Vec<Emitter> {
    vec![
        Emitter::Option {
            name: "--prompt".to_string(),
            field: "prompt".to_string(),
        },
        Emitter::Option {
            name: "--profile-load".to_string(),
            field: "profile".to_string(),
        },
        Emitter::Flag {
            field: "yolo".to_string(),
        },
        Emitter::Flag {
            field: "prompt_interactive".to_string(),
        },
    ]
}

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
        probe: llxprt_probe(),
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
        agent_fields: vec![
            sig_string_field("version_selector"),
            sig_string_field("prompt"),
            bool_field("prompt_interactive"),
        ],
        emitters: emitters(),
    })
}
fn llxprt_probe() -> super::super::probe::ProbeSpec {
    line_version_probe(
        Normalize::None,
        capability_probe(
            Normalize::None,
            &[
                ("prompt-interactive", "--prompt-interactive"),
                ("profile", "--profile-load"),
                ("sandbox", "--sandbox"),
                ("sandbox-engine", "--sandbox-engine"),
                ("yolo", "--yolo"),
                ("approval", "--approval-mode"),
                ("continue", "--continue"),
            ],
        ),
        &["prompt-interactive"],
    )
}
