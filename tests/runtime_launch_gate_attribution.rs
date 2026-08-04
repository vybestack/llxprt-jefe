//! Every launch refusal names the gate that produced it (issue #544).
//!
//! The launch pipeline runs fifteen gates. Before this, most of them collapsed
//! into `spawn failed: <text>`, so a user reading the error could not tell
//! which of the fifteen had stopped them, and neither could the next person to
//! file the bug. #519 -> #525 -> #529 is one defect class filed three times for
//! exactly that reason.
//!
//! These assert on the message a user actually sees, not on an internal enum,
//! because the attribution is only worth anything if it survives to the surface.

use jefe::domain::agent_definition::{AgentTypeId, Operation};
use jefe::domain::{AgentLaunchRequest, Id, RemoteRepositorySettings, TypedMap, TypedValue};
use jefe::runtime::LaunchGate;
use jefe::runtime::launch_compose::validate_launch;

fn work_dir() -> std::path::PathBuf {
    let Ok(dir) = std::env::current_dir() else {
        panic!("the test process must have a working directory");
    };
    dir
}

fn typed(pairs: &[(&str, TypedValue)]) -> TypedMap {
    let mut values = TypedMap::new();
    for (field, value) in pairs {
        // Field ids are kebab-case on the wire; the launch code spells them with
        // underscores and `typed_field` converts, so the test converts too.
        let Ok(key) = Id::parse(&field.replace('_', "-")) else {
            panic!("{field} must be a valid field id");
        };
        values.insert(key, value.clone());
    }
    values
}

fn request(type_id: &str, operation: Operation, values: TypedMap) -> AgentLaunchRequest {
    let Ok(id) = AgentTypeId::parse(type_id) else {
        panic!("{type_id} must be a valid agent type id");
    };
    AgentLaunchRequest {
        type_id: id,
        values,
        work_dir: work_dir(),
        remote: RemoteRepositorySettings::default(),
        operation,
    }
}

/// The message must carry the gate id, and the remediation the gate declares,
/// so the user has both the location and the next action.
fn assert_names_gate(message: &str, gate: LaunchGate) {
    assert!(
        message.contains(gate.id()),
        "refusal must name the {} gate, got: {message}",
        gate.id()
    );
    assert!(
        message.contains("remediation:"),
        "refusal must carry a remediation, got: {message}"
    );
    assert!(
        message.contains(gate.remediation()),
        "refusal must carry the {} remediation, got: {message}",
        gate.id()
    );
}

#[test]
fn an_unknown_agent_type_names_the_launch_composition_gate() {
    let outcome = validate_launch(&request(
        "core.does-not-exist",
        Operation::Normal,
        typed(&[]),
    ));

    let Err(error) = outcome else {
        panic!("an unknown agent type must not validate");
    };
    assert_names_gate(&error.to_string(), LaunchGate::LaunchComposition);
}

/// A malformed version selector is rejected before anything is probed or
/// installed, and the refusal still names the composition gate rather than
/// arriving as an unattributed `spawn failed`.
#[test]
fn a_malformed_version_selector_names_its_gate_without_probing() {
    let outcome = validate_launch(&request(
        "core.llxprt",
        Operation::Normal,
        typed(&[("version_selector", TypedValue::Bool(true))]),
    ));

    let Err(error) = outcome else {
        panic!("a non-string version selector must not validate");
    };
    assert_names_gate(&error.to_string(), LaunchGate::LaunchComposition);
}

/// A gate that already named itself keeps its own diagnostic. Attribution must
/// not overwrite the more specific answer with the coarser one.
#[test]
fn an_already_attributed_refusal_is_not_relabelled() {
    let outcome = validate_launch(&request(
        "core.does-not-exist",
        Operation::Normal,
        typed(&[]),
    ));

    let Err(error) = outcome else {
        panic!("an unknown agent type must not validate");
    };
    let message = error.to_string();
    let occurrences = message.matches("remediation:").count();
    assert_eq!(
        occurrences, 1,
        "a refusal must carry exactly one remediation, got: {message}"
    );
}
