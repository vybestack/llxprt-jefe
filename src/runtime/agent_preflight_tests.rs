//! Tests for the ordered execution preparation boundary (issue #382 S10).
//!
//! Authority: issue #382 CW02-09 — sandbox preflight must succeed before any
//! clone/reset/prompt write/SSH/tmux/spawn, and must run only after S8
//! `authorize_execution` succeeds. Missing/changed engine fingerprint,
//! unavailable image, or missing required environment names return typed
//! `Unavailable { reason }` and a zero-effect outcome.
//!
//! These tests use a **recording inspector** to assert ordered typed effects
//! rather than mock call counts. Each failure case proves no later
//! preparation effect ran (the recording is truncated at the failing step).

use std::cell::RefCell;
use std::path::PathBuf;

use super::*;
use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::domain::agent_definition::sha256::DefinitionSha256;
use crate::domain::agent_definition::types::{AgentLaunchPlan, Preflight};
use crate::domain::agent_definition::{AgentTypeId, LaunchSignatureV1, Operation, Target};
use crate::runtime::agent_execution_guard::{
    AuthorizedExecution, ExecutionEvidence, authorize_execution,
};

// ---------------------------------------------------------------------------
// Recording inspector
// ---------------------------------------------------------------------------

/// A recording inspector that captures the ordered sequence of inspection
/// calls and returns programmed outcomes. This is the boundary recorder: the
/// tests assert ordered typed effects, not mock call counts.
#[derive(Default)]
struct RecordingInspector {
    calls: RefCell<Vec<String>>,
    engine_available: bool,
    engine_fingerprint: String,
    image_available: bool,
    env_present_names: Vec<String>,
}

impl RecordingInspector {
    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl SandboxInspector for RecordingInspector {
    fn inspect_engine(&self, engine: &str) -> InspectOutcome {
        self.calls
            .borrow_mut()
            .push(format!("inspect_engine({engine})"));
        if self.engine_available {
            InspectOutcome::available(self.engine_fingerprint.clone())
        } else {
            InspectOutcome::unavailable()
        }
    }

    fn inspect_image(&self, engine: &str, image: &str) -> InspectOutcome {
        self.calls
            .borrow_mut()
            .push(format!("inspect_image({engine}, {image})"));
        if self.image_available {
            InspectOutcome::available(String::new())
        } else {
            InspectOutcome::unavailable()
        }
    }

    fn env_present(&self, name: &str) -> bool {
        self.calls.borrow_mut().push(format!("env_present({name})"));
        self.env_present_names.iter().any(|n| n == name)
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn zero_hash() -> DefinitionSha256 {
    DefinitionSha256::default()
}

fn fingerprint(path: &str) -> CandidateFingerprint {
    CandidateFingerprint::new(PathBuf::from(path), None, None, 1024, 1_000)
}

fn base_evidence() -> ExecutionEvidence {
    ExecutionEvidence::new(zero_hash(), fingerprint("/opt/bin/agent"), 1, 1, 1)
}

fn configured_preflight() -> Preflight {
    Preflight {
        engine: Some("podman".to_owned()),
        image: Some("registry.example/agent:1".to_owned()),
        required_env: vec!["API_TOKEN".to_owned()],
        required: true,
    }
}

fn base_plan_with(preflight: Preflight) -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: AgentTypeId::from_validated("core.test"),
        operation: Operation::Normal,
        definition_sha256: zero_hash(),
        executable: PathBuf::from("/opt/bin/agent"),
        executable_fingerprint: fingerprint("/opt/bin/agent"),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv: Vec::new(),
        env: Vec::new(),
        cwd: PathBuf::from("/srv/project"),
        target: Target::Local {
            canonical_cwd: PathBuf::from("/srv/project"),
        },
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        preflight,
        signature: LaunchSignatureV1::default(),
    }
}

/// Authorize the plan so the preflight boundary can run.
fn authorize(plan: &AgentLaunchPlan) -> AuthorizedExecution<'_> {
    match authorize_execution(plan, &base_evidence()) {
        crate::runtime::agent_execution_guard::AuthorizationResult::Authorized(a) => a,
        crate::runtime::agent_execution_guard::AuthorizationResult::Rejected(r) => {
            panic!("fixture plan must authorize: {r}")
        }
    }
}

fn fully_available_inspector() -> RecordingInspector {
    RecordingInspector {
        engine_available: true,
        engine_fingerprint: "podman 5.0.0".to_owned(),
        image_available: true,
        env_present_names: vec!["API_TOKEN".to_owned()],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Success path
// ---------------------------------------------------------------------------

#[test]
fn fully_configured_and_available_clears_after_all_checks() {
    let plan = base_plan_with(configured_preflight());
    let authorized = authorize(&plan);
    let inspector = fully_available_inspector();
    let outcome = prepare_execution(authorized, Some("podman 5.0.0"), &inspector);
    let cleared = match outcome {
        PreparationOutcome::Cleared(c) => c,
        PreparationOutcome::Unavailable(r) => panic!("must clear: {r}"),
    };
    assert_eq!(cleared.plan().type_id, plan.type_id);
    // All four checks ran in order: engine, (fingerprint compare is pure),
    // image, env.
    assert_eq!(
        inspector.calls(),
        vec![
            "inspect_engine(podman)",
            "inspect_image(podman, registry.example/agent:1)",
            "env_present(API_TOKEN)",
        ],
        "checks must run in fixed order on success"
    );
}

#[test]
fn preflight_not_required_clears_without_any_inspection() {
    let plan = base_plan_with(Preflight {
        engine: Some("podman".to_owned()),
        image: Some("img".to_owned()),
        required_env: Vec::new(),
        required: false,
    });
    let authorized = authorize(&plan);
    let inspector = fully_available_inspector();
    let outcome = prepare_execution(authorized, None, &inspector);
    assert!(matches!(outcome, PreparationOutcome::Cleared(_)));
    assert!(
        inspector.calls().is_empty(),
        "no inspection runs when preflight is not required"
    );
}

// ---------------------------------------------------------------------------
// Failure ordering: each case proves no later effect ran
// ---------------------------------------------------------------------------

#[test]
fn required_but_unconfigured_returns_contract_unconfigured_zero_effects() {
    let plan = base_plan_with(Preflight::default());
    let authorized = authorize(&plan);
    let inspector = fully_available_inspector();
    let outcome = prepare_execution(authorized, None, &inspector);
    match outcome {
        PreparationOutcome::Unavailable(UnavailableReason::ContractUnconfigured) => {}
        other => panic!("expected ContractUnconfigured, got {other:?}"),
    }
    assert!(
        inspector.calls().is_empty(),
        "no inspection may run when the contract is unconfigured"
    );
}

#[test]
fn engine_missing_returns_engine_missing_before_image_or_env() {
    let plan = base_plan_with(configured_preflight());
    let authorized = authorize(&plan);
    let inspector = RecordingInspector {
        engine_available: false,
        ..fully_available_inspector()
    };
    let outcome = prepare_execution(authorized, None, &inspector);
    match outcome {
        PreparationOutcome::Unavailable(UnavailableReason::EngineMissing { engine }) => {
            assert_eq!(engine, "podman");
        }
        other => panic!("expected EngineMissing, got {other:?}"),
    }
    assert_eq!(
        inspector.calls(),
        vec!["inspect_engine(podman)"],
        "only engine inspection runs before failing"
    );
}

#[test]
fn engine_fingerprint_changed_returns_fingerprint_changed_before_image_or_env() {
    let plan = base_plan_with(configured_preflight());
    let authorized = authorize(&plan);
    let inspector = RecordingInspector {
        engine_available: true,
        engine_fingerprint: "podman 9.9.9".to_owned(),
        ..fully_available_inspector()
    };
    let outcome = prepare_execution(authorized, Some("podman 5.0.0"), &inspector);
    match outcome {
        PreparationOutcome::Unavailable(UnavailableReason::EngineFingerprintChanged {
            engine,
            expected,
            actual,
        }) => {
            assert_eq!(engine, "podman");
            assert_eq!(expected, "podman 5.0.0");
            assert_eq!(actual, "podman 9.9.9");
        }
        other => panic!("expected EngineFingerprintChanged, got {other:?}"),
    }
    assert_eq!(
        inspector.calls(),
        vec!["inspect_engine(podman)"],
        "image/env must not run after fingerprint mismatch"
    );
}

#[test]
fn image_missing_returns_image_missing_before_env() {
    let plan = base_plan_with(configured_preflight());
    let authorized = authorize(&plan);
    let inspector = RecordingInspector {
        image_available: false,
        ..fully_available_inspector()
    };
    let outcome = prepare_execution(authorized, Some("podman 5.0.0"), &inspector);
    match outcome {
        PreparationOutcome::Unavailable(UnavailableReason::ImageMissing { engine, image }) => {
            assert_eq!(engine, "podman");
            assert_eq!(image, "registry.example/agent:1");
        }
        other => panic!("expected ImageMissing, got {other:?}"),
    }
    assert_eq!(
        inspector.calls(),
        vec![
            "inspect_engine(podman)",
            "inspect_image(podman, registry.example/agent:1)",
        ],
        "env must not run after image missing"
    );
}

#[test]
fn missing_env_returns_missing_required_env_names_only() {
    let plan = base_plan_with(Preflight {
        engine: Some("podman".to_owned()),
        image: Some("registry.example/agent:1".to_owned()),
        required_env: vec!["API_TOKEN".to_owned(), "SECRET_KEY".to_owned()],
        required: true,
    });
    let authorized = authorize(&plan);
    let inspector = RecordingInspector {
        env_present_names: vec!["API_TOKEN".to_owned()],
        ..fully_available_inspector()
    };
    let outcome = prepare_execution(authorized, Some("podman 5.0.0"), &inspector);
    match outcome {
        PreparationOutcome::Unavailable(UnavailableReason::MissingRequiredEnv { names }) => {
            assert_eq!(names, vec!["SECRET_KEY"]);
        }
        other => panic!("expected MissingRequiredEnv, got {other:?}"),
    }
    assert!(
        !format!("{:?}", inspector.calls()).contains("=value"),
        "no environment value may ever appear in diagnostics"
    );
}

// ---------------------------------------------------------------------------
// Structural ordering: authorization precedes preflight
// ---------------------------------------------------------------------------

#[test]
fn authorized_execution_is_required_by_type() {
    // The boundary takes AuthorizedExecution, which can only be produced by
    // authorize_execution. This test documents the structural ordering:
    // there is no prepare_execution overload that accepts a bare plan.
    let plan = base_plan_with(configured_preflight());
    let authorized = authorize(&plan);
    // If authorization had failed, we would have panicked above — proving
    // the ordering is enforced before preflight.
    let inspector = fully_available_inspector();
    let outcome = prepare_execution(authorized, Some("podman 5.0.0"), &inspector);
    assert!(matches!(outcome, PreparationOutcome::Cleared(_)));
}

#[test]
fn cleared_wrapper_provides_authorized_plan_and_fingerprint() {
    let plan = base_plan_with(configured_preflight());
    let authorized = authorize(&plan);
    let inspector = fully_available_inspector();
    let cleared = match prepare_execution(authorized, Some("podman 5.0.0"), &inspector) {
        PreparationOutcome::Cleared(c) => c,
        PreparationOutcome::Unavailable(r) => panic!("must clear: {r}"),
    };
    assert_eq!(cleared.plan().operation, Operation::Normal);
    assert_eq!(cleared.authorized().plan().operation, Operation::Normal);
    assert_eq!(cleared.engine_fingerprint(), Some("podman 5.0.0"));
}

// ---------------------------------------------------------------------------
// Display and error trait
// ---------------------------------------------------------------------------

#[test]
fn unavailable_reason_display_is_descriptive_without_values() {
    let reason = UnavailableReason::MissingRequiredEnv {
        names: vec!["API_TOKEN".to_owned(), "SECRET_KEY".to_owned()],
    };
    let msg = reason.to_string();
    assert!(msg.contains("API_TOKEN"), "names appear: {msg}");
    assert!(msg.contains("SECRET_KEY"), "names appear: {msg}");
    let _: &dyn std::error::Error = &reason;
}

#[test]
fn engine_unavailable_predicate_distinguishes_engine_failures() {
    assert!(
        UnavailableReason::EngineMissing {
            engine: "podman".to_owned()
        }
        .is_engine_unavailable()
    );
    assert!(
        UnavailableReason::EngineFingerprintChanged {
            engine: "podman".to_owned(),
            expected: String::new(),
            actual: String::new(),
        }
        .is_engine_unavailable()
    );
    assert!(
        !UnavailableReason::ImageMissing {
            engine: "podman".to_owned(),
            image: "img".to_owned(),
        }
        .is_engine_unavailable()
    );
}
