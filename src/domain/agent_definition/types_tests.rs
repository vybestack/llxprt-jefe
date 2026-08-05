//! Unit tests for the core closed agent-definition contract types.

use super::*;

#[test]
fn support_supported_and_unsupported() {
    assert!(!Support::supported().is_unsupported());
    let u = Support::unsupported("missing");
    assert!(u.is_unsupported());
    assert_eq!(u.reason(), Some("missing"));
}

#[test]
fn operation_is_fresh_and_resume() {
    assert!(!Operation::Normal.is_fresh());
    assert!(Operation::FreshIssue.is_fresh());
    assert!(Operation::FreshPullRequest.is_fresh());
    assert!(Operation::Resume.is_resume());
}

#[test]
fn operation_matrix_support_for() {
    let matrix = OperationMatrix::default();
    assert!(matrix.has_any_unsupported(), "default matrix has gaps");
    assert!(
        matrix
            .support_for(Operation::Normal)
            .supported
            .is_unsupported(),
        "default normal is unsupported"
    );
}

#[test]
fn target_local_and_remote() {
    let local = Target::Local {
        canonical_cwd: std::path::PathBuf::from("/srv"),
    };
    assert!(local.is_local());
    let remote = Target::Remote(RemoteTarget::default());
    assert!(!remote.is_local());
}

#[test]
fn probe_error_code_strings() {
    assert_eq!(ProbeErrorCode::Agte201.as_str(), "AGT-E201");
    assert_eq!(ProbeErrorCode::Agte202.as_str(), "AGT-E202");
    assert_eq!(ProbeErrorCode::Agte203.as_str(), "AGT-E203");
    assert!(ProbeErrorCode::Agte202.is_probe_error());
    assert!(ProbeErrorCode::Agte203.is_generation_mismatch());
}

#[test]
fn availability_variants() {
    let not_found = Availability::NotFound;
    assert!(not_found.is_not_found());
    assert!(!not_found.is_installed());
    assert_eq!(not_found.generation(), None);

    let compatible = Availability::InstalledCompatible {
        identity: "x".to_string(),
        generation: 1,
    };
    assert!(compatible.is_installed());
    assert_eq!(compatible.generation(), Some(1));

    let incompatible = Availability::InstalledIncompatible {
        reason: "missing".to_string(),
        generation: 2,
    };
    assert!(incompatible.is_installed());

    let err = Availability::ProbeError {
        code: ProbeErrorCode::Agte202,
        reason: "boom".to_string(),
        generation: 3,
    };
    assert_eq!(err.generation(), Some(3));
}

#[test]
fn preflight_default_is_unsandboxed() {
    let preflight = Preflight::default();
    assert!(!preflight.is_unavailable(), "default preflight is optional");
    assert!(!preflight.is_required());
}

#[test]
fn preflight_configured_is_available() {
    let preflight = Preflight {
        engine: Some("docker".to_string()),
        image: Some("img".to_string()),
        required_env: vec![],
        required: true,
    };
    assert!(
        !preflight.is_unavailable(),
        "configured preflight available"
    );
}

#[test]
fn agent_launch_plan_signature_excludes_secrets() {
    let plan = AgentLaunchPlan::default();
    assert!(plan.signature_excludes_secrets());
}

#[test]
fn prompt_shape_serde_snake_case() {
    let json = serde_json::to_string(&PromptShape::InitialPositional)
        .unwrap_or_else(|error| panic!("serialize: {error}"));
    assert_eq!(json, "\"initial_positional\"");
    let json = serde_json::to_string(&PromptShape::InteractiveOption)
        .unwrap_or_else(|error| panic!("serialize: {error}"));
    assert_eq!(json, "\"interactive_option\"");
}
