//! Shipped four-agent definitions (issue #382 CW-02).
//!
//! THIS IS THE ONLY ALLOWLISTED LOCATION FOR PRODUCT TOKENS IN SOURCE.
//! The four definitions encode candidates, probe spec, operation/target
//! support, fields, and emitters strictly from the fixture-proven mappings
//! recorded under `tests/fixtures/agent-definitions/`. Mappings are never
//! silently broadened beyond fixture-verified evidence.
//!
//! Fixture bytes are deterministic provenance of a real captured release;
//! they are not a runtime version allow-list. Runtime support is decided per
//! installation by the definition's probe: identity plus required
//! capabilities present means compatible regardless of version.

mod claude;
mod code_puppy;
mod codex;
mod common;
mod llxprt;

use super::definition::AgentDefinition;
use super::type_id::AgentTypeId;

/// The four shipped definitions in canonical (bytewise-stable) ID order.
///
/// Order is deterministic so resolution is stable across builds.
#[must_use]
pub fn shipped_definitions() -> Vec<AgentDefinition> {
    let mut defs = vec![
        claude::build(),
        code_puppy::build(),
        codex::build(),
        llxprt::build(),
    ];
    defs.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    defs
}

/// The four shipped ids in product default-preference order.
///
/// LLxprt is the product default agent type, followed by Code Puppy, Claude
/// Code, and Codex. Default-selection surfaces (form seeds, space-cycling,
/// availability snapshots) present types in this order. The canonical
/// bytewise ID order in [`shipped_definitions`] remains the registry order
/// and the `shipped_agent_type` index contract.
#[must_use]
pub fn shipped_preference_order() -> Vec<AgentTypeId> {
    vec![
        llxprt::build().id,
        code_puppy::build().id,
        claude::build().id,
        codex::build().id,
    ]
}
