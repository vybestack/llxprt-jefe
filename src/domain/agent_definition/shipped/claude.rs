//! core.claude-code shipped definition (issue #382 CW-02).
//!
//! Mappings are fixture-proven: see
//! `tests/fixtures/agent-definitions/claude/2.1.212/`. The fixture-authoring
//! release was captured from a real installation; runtime support is
//! probe-decided per installation.

use super::super::definition::AgentDefinition;
use super::super::fields::Emitter;
use super::common::{
    DefinitionParts, assemble, enum_field, line_suffix_probe, local_only_targets, npm_candidate,
    path_candidate, sig_string_field,
};

/// Build the core.claude-code shipped definition.
pub fn build() -> AgentDefinition {
    let probe = line_suffix_probe("(Claude Code)");
    // `help.stdout` in the fixture declares `claude [options] [command] [prompt]`
    // with `prompt  Your prompt`, and an interactive session by default, so the
    // prompt shape below is fixture-proven for every prompt-bearing operation.
    let operations = super::common::positional_prompt_operations();
    assemble(DefinitionParts {
        id: "core.claude-code",
        display_name: "Claude Code",
        minimum_version: "2.1.212",
        candidates: vec![
            path_candidate("claude"),
            npm_candidate("@anthropic-ai/claude-code", "claude"),
        ],
        probe,
        operations,
        targets: local_only_targets("Claude remote/setup is not fixture-verified"),
        repository_fields: vec![
            sig_string_field("model"),
            enum_field(
                "permission_mode",
                &[
                    "acceptEdits",
                    "auto",
                    "bypassPermissions",
                    "manual",
                    "dontAsk",
                    "plan",
                ],
            ),
        ],
        agent_fields: vec![
            sig_string_field("version_selector"),
            sig_string_field("prompt"),
        ],
        emitters: claude_emitters(),
    })
}

fn claude_emitters() -> Vec<Emitter> {
    vec![
        Emitter::Option {
            name: "--model".to_string(),
            field: "model".to_string(),
        },
        Emitter::Option {
            name: "--permission-mode".to_string(),
            field: "permission_mode".to_string(),
        },
        Emitter::Positional {
            field: "prompt".to_string(),
        },
    ]
}
