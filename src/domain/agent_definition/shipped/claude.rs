//! core.claude-code shipped definition (issue #382 CW-02).
//!
//! Mappings are fixture-proven: see
//! `tests/fixtures/agent-definitions/claude/2.1.212/`. The fixture-authoring
//! release was captured from a real installation; runtime support is
//! probe-decided per installation.

use super::super::definition::AgentDefinition;
use super::super::fields::Emitter;
use super::super::normalize::Normalize;
use super::common::{
    DefinitionParts, assemble, capability_probe, enum_field, line_suffix_probe, local_only_targets,
    npm_candidate, path_candidate, sig_string_field,
};

/// Build the core.claude-code shipped definition.
pub fn build() -> AgentDefinition {
    let probe = line_suffix_probe(
        "(Claude Code)",
        capability_probe(
            Normalize::None,
            &[
                ("continue", "--continue"),
                ("resume", "--resume"),
                ("model", "--model"),
                ("permission-mode", "--permission-mode"),
                ("bypass-permissions", "--dangerously-skip-permissions"),
            ],
        ),
        &[],
    );
    let operations = super::common::unsupported_only_operations(
        "Claude fresh-issue prompt is not fixture-verified",
        "Claude fresh-PR prompt is not fixture-verified",
    );
    assemble(DefinitionParts {
        id: "core.claude-code",
        display_name: "Claude Code",
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
        agent_fields: vec![sig_string_field("prompt")],
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
