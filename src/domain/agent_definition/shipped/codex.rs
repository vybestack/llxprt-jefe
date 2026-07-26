//! core.codex shipped definition (issue #382 CW-02).
//!
//! Mappings are fixture-proven: see
//! `tests/fixtures/agent-definitions/codex/0.142.0/`.

use super::super::definition::AgentDefinition;
use super::super::fields::Emitter;
use super::common::{
    DefinitionParts, assemble, enum_field, local_only_targets, npm_candidate, path_candidate,
    sig_string_field,
};

/// Build the core.codex shipped definition.
pub fn build() -> AgentDefinition {
    let probe = super::common::line_prefix_probe("codex-cli ", &["prompt"]);
    let operations = super::common::unsupported_only_operations(
        "Codex fresh-issue prompt is not fixture-verified",
        "Codex fresh-PR prompt is not fixture-verified",
    );
    assemble(DefinitionParts {
        id: "core.codex",
        display_name: "Codex CLI",
        candidates: vec![
            path_candidate("codex"),
            npm_candidate("@openai/codex", "codex"),
        ],
        probe,
        operations,
        targets: local_only_targets("Codex remote/setup is not fixture-verified"),
        repository_fields: vec![
            sig_string_field("model"),
            sig_string_field("profile"),
            enum_field(
                "sandbox",
                &["read-only", "workspace-write", "danger-full-access"],
            ),
        ],
        agent_fields: vec![sig_string_field("prompt")],
        emitters: codex_emitters(),
    })
}

fn codex_emitters() -> Vec<Emitter> {
    vec![
        Emitter::Option {
            name: "--model".to_string(),
            field: "model".to_string(),
        },
        Emitter::Option {
            name: "--profile".to_string(),
            field: "profile".to_string(),
        },
        Emitter::Option {
            name: "--sandbox".to_string(),
            field: "sandbox".to_string(),
        },
        Emitter::Positional {
            field: "prompt".to_string(),
        },
    ]
}
