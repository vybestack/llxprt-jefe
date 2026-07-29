//! Helpers for the CW02-09 preflight ordering acceptance test.
//!
//! These helpers keep `tests/issue382_behavior.rs` under the source-size hard
//! limit by extracting the recording inspector, plan fixtures, and the
//! authorize-then-prepare driver used by the `preflight_order` test.

use std::cell::RefCell;

use jefe::agent_candidate_fingerprint::CandidateFingerprint;
use jefe::domain::agent_definition::DefinitionSha256;
use jefe::domain::agent_definition::{
    AgentLaunchPlan, AgentTypeId, LaunchSignatureV1, Operation, Preflight, Target,
};
use jefe::runtime::{
    AuthorizationResult, ExecutionEvidence, InspectOutcome, PreparationOutcome, SandboxInspector,
    authorize_execution, prepare_execution,
};

/// Recording inspector: asserts ordered typed effects rather than mock call
/// counts. Each inspection method records its name so the test can verify
/// that failure short-circuits prevent later inspections.
#[derive(Default)]
pub struct RecordingInspector {
    /// Ordered record of which inspection methods were called.
    pub calls: RefCell<Vec<&'static str>>,
    pub engine_available: bool,
    pub image_available: bool,
    pub env_names: Vec<&'static str>,
}

impl SandboxInspector for RecordingInspector {
    fn inspect_engine(&self, _engine: &str) -> InspectOutcome {
        self.calls.borrow_mut().push("inspect_engine");
        if self.engine_available {
            InspectOutcome::available("podman 5.0.0")
        } else {
            InspectOutcome::unavailable()
        }
    }
    fn inspect_image(&self, _engine: &str, _image: &str) -> InspectOutcome {
        self.calls.borrow_mut().push("inspect_image");
        if self.image_available {
            InspectOutcome::available(String::new())
        } else {
            InspectOutcome::unavailable()
        }
    }
    fn env_present(&self, name: &str) -> bool {
        self.calls.borrow_mut().push("env_present");
        self.env_names.contains(&name)
    }
}

/// Shared execution evidence matching the base plan fixture.
fn evidence() -> ExecutionEvidence {
    let fp =
        CandidateFingerprint::new(std::path::PathBuf::from("/opt/bin/agent"), None, None, 0, 0);
    ExecutionEvidence::new(DefinitionSha256::default(), fp, 1, 1, 1)
}

/// A configured sandbox preflight contract for the ordering test.
fn configured_preflight(env: Vec<&str>) -> Preflight {
    Preflight {
        engine: Some("podman".to_owned()),
        image: Some("registry.example/agent:1".to_owned()),
        required_env: env.iter().map(|n| (*n).to_owned()).collect(),
        required: true,
    }
}

/// Build a plan with the given preflight contract.
fn plan(preflight: Preflight) -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: AgentTypeId::parse("core.test")
            .unwrap_or_else(|_| panic!("core.test is a valid agent type id")),
        operation: Operation::Normal,
        definition_sha256: DefinitionSha256::default(),
        executable: std::path::PathBuf::from("/opt/bin/agent"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from("/opt/bin/agent"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv: Vec::new(),
        env: Vec::new(),
        cwd: std::path::PathBuf::from("/srv/project"),
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/srv/project"),
        },
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        preflight,
        signature: LaunchSignatureV1::default(),
    }
}

/// Authorize a configured plan and run preparation through the boundary.
fn authorize_and_prepare(
    env: Vec<&str>,
    inspector: &RecordingInspector,
    check: impl FnOnce(PreparationOutcome<'_>),
) {
    let plan = plan(configured_preflight(env));
    let authorized = match authorize_execution(&plan, &evidence()) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(rejection) => panic!("must authorize: {rejection}"),
    };
    check(prepare_execution(
        authorized,
        Some("podman 5.0.0"),
        inspector,
    ));
}

// ---- CW02-09 sub-cases ----

/// Missing engine: the first inspection short-circuits with zero later effects.
pub fn assert_engine_missing() {
    use jefe::runtime::{PreparationOutcome, UnavailableReason};
    let inspector = RecordingInspector {
        engine_available: false,
        ..Default::default()
    };
    authorize_and_prepare(vec!["API_TOKEN"], &inspector, |outcome| match outcome {
        PreparationOutcome::Unavailable(UnavailableReason::EngineMissing { engine }) => {
            assert_eq!(engine, "podman");
        }
        other => panic!("expected EngineMissing, got {other:?}"),
    });
    assert_eq!(
        &*inspector.calls.borrow(),
        &["inspect_engine"],
        "no image/env inspection runs after engine failure"
    );
}

/// Missing image: the engine inspects but image failure short-circuits.
pub fn assert_image_missing() {
    use jefe::runtime::{PreparationOutcome, UnavailableReason};
    let inspector = RecordingInspector {
        engine_available: true,
        image_available: false,
        env_names: vec!["API_TOKEN"],
        ..Default::default()
    };
    authorize_and_prepare(vec!["API_TOKEN"], &inspector, |outcome| match outcome {
        PreparationOutcome::Unavailable(UnavailableReason::ImageMissing { image, .. }) => {
            assert_eq!(image, "registry.example/agent:1");
        }
        other => panic!("expected ImageMissing, got {other:?}"),
    });
    assert_eq!(
        &*inspector.calls.borrow(),
        &["inspect_engine", "inspect_image"],
        "no env check runs after image failure"
    );
}

/// Missing required env: the diagnostic names the missing names only.
pub fn assert_env_missing() {
    use jefe::runtime::{PreparationOutcome, UnavailableReason};
    let inspector = RecordingInspector {
        engine_available: true,
        image_available: true,
        env_names: vec!["API_TOKEN"],
        ..Default::default()
    };
    authorize_and_prepare(
        vec!["API_TOKEN", "SECRET_KEY"],
        &inspector,
        |outcome| match outcome {
            PreparationOutcome::Unavailable(UnavailableReason::MissingRequiredEnv { names }) => {
                assert_eq!(names, vec!["SECRET_KEY"]);
            }
            other => panic!("expected MissingRequiredEnv, got {other:?}"),
        },
    );
}

/// All available: the boundary returns Cleared, the only path to preparation.
pub fn assert_cleared() {
    use jefe::runtime::PreparationOutcome;
    let inspector = RecordingInspector {
        engine_available: true,
        image_available: true,
        env_names: vec!["API_TOKEN"],
        ..Default::default()
    };
    authorize_and_prepare(vec!["API_TOKEN"], &inspector, |outcome| match outcome {
        PreparationOutcome::Cleared(cleared) => {
            assert_eq!(cleared.plan().operation, Operation::Normal);
            assert_eq!(cleared.engine_fingerprint(), Some("podman 5.0.0"));
        }
        PreparationOutcome::Unavailable(reason) => panic!("must clear: {reason}"),
    });
}
