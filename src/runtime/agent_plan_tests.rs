//! Unit tests for the local launch planner (issue #382 S7).

use super::*;
use crate::domain::agent_definition::fields::FieldValue;
use crate::domain::agent_definition::{Operation, Preflight, Target};

fn llxprt() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|d| d.display_name == "LLxprt")
        .unwrap_or_else(|| panic!("LLxprt shipped"))
}

fn compatible(generation: u64) -> Availability {
    Availability::InstalledCompatible {
        identity: "id".to_string(),
        capabilities: Vec::new(),
        generation,
    }
}

#[test]
fn unsupported_operation_returns_declared_reason_and_zero_effects() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    // LLxprt supports all operations, so mutate to make Resume unsupported.
    let mut definition = definition.clone();
    definition.operations.resume.supported =
        crate::domain::agent_definition::Support::unsupported("resume not available");
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Resume,
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/r"),
        },
        executable: std::path::PathBuf::from("/x"),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from("/x"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Unsupported { reason } => {
            assert_eq!(reason, "resume not available");
        }
        other => panic!("expected unsupported, got {other:?}"),
    }
}

#[test]
fn default_field_values_are_used_when_not_provided() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/r"),
        },
        executable: std::path::PathBuf::from("/x"),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from("/x"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => *plan,
        other => panic!("expected supported, got {other:?}"),
    };
    // LLxprt's profile has no default, while yolo defaults on.
    assert_eq!(plan.argv, ["--yolo"]);
}

#[test]
fn flag_resolves_token_from_capability_probe() {
    let definition = llxprt();
    let mut values = LaunchFieldValues::new();
    values.set_agent("continue", FieldValue::Boolean(true));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/r"),
        },
        executable: std::path::PathBuf::from("/x"),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from("/x"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => *plan,
        other => panic!("expected supported, got {other:?}"),
    };
    let argv: Vec<String> = plan
        .argv
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, vec!["--yolo".to_string(), "--continue".to_string()]);
}

#[test]
fn empty_string_value_skips_option_emitter() {
    let definition = llxprt();
    let mut values = LaunchFieldValues::new();
    values.set_repository("profile", FieldValue::String("  ".to_string()));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/r"),
        },
        executable: std::path::PathBuf::from("/x"),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from("/x"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => *plan,
        other => panic!("expected supported, got {other:?}"),
    };
    // Whitespace-only profile skips its Option emitter; default yolo remains.
    assert_eq!(plan.argv, ["--yolo"]);
}

#[test]
fn stamping_carries_generations_and_signature() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/srv/proj"),
        },
        executable: std::path::PathBuf::from("/bin/llxprt"),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from("/x"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(42),
        probe_generation: 42,
        target_generation: 7,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => *plan,
        other => panic!("expected supported, got {other:?}"),
    };
    assert_eq!(plan.probe_generation, 42);
    assert_eq!(plan.target_generation, 7);
    assert_eq!(plan.signature.version, 1);
    assert_eq!(
        plan.signature.definition_hash.as_str(),
        definition.sha256().to_hex()
    );
    assert!(!plan.signature.target_fingerprint.to_hex().is_empty());
}
