//! Issue #534 acceptance tests for the trusted capability probe.
//!
//! Split out of `issue382_behavior.rs` to keep that file within the
//! source-size gate. Only LLxprt is trusted to skip the `--help` probe; every
//! other shipped definition must remain untrusted.

use jefe::domain::agent_definition::AgentDefinition;

#[test]
fn shipped_llxprt_definition_marks_capability_probe_trusted() {
    let definition = AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.llxprt")
        .unwrap_or_else(|| panic!("LLxprt definition must be shipped"));
    let probe = definition
        .probe
        .capabilities
        .as_ref()
        .unwrap_or_else(|| panic!("LLxprt must have a capability probe"));
    assert!(
        probe.trusted,
        "LLxprt shipped definition must mark its capability probe trusted (issue #534)"
    );
}

#[test]
fn shipped_non_llxprt_definitions_remain_untrusted() {
    for name in ["core.claude-code", "core.code-puppy", "core.codex"] {
        let definition = AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id.as_str() == name)
            .unwrap_or_else(|| panic!("{name} definition must be shipped"));
        if let Some(probe) = &definition.probe.capabilities {
            assert!(
                !probe.trusted,
                "{name} must remain untrusted (only LLxprt is trusted)"
            );
        }
    }
}
