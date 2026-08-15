use jefe::agent_candidate_fingerprint::CandidateFingerprint;
use jefe::agent_candidate_path::AgentWrapperKind;
use jefe::domain::agent_definition::{
    AgentLaunchPlan, AgentTypeId, DefinitionSha256, LaunchSignatureV1, Operation, Preflight, Target,
};
use jefe::runtime::{
    AuthorizationResult, ExecutionEvidence, PreparationOutcome, ProcessSandboxInspector,
    UnavailableReason, authorize_execution, prepare_execution,
};

pub fn stale_generation_diagnostic() -> Result<String, String> {
    let plan = plan(Preflight::default());
    let evidence = ExecutionEvidence::new(
        plan.definition_sha256,
        plan.executable_fingerprint.clone(),
        plan.probe_generation + 1,
        plan.target_generation,
        plan.activation_generation,
    );
    match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Rejected(rejection) => Ok(rejection.to_string()),
        AuthorizationResult::Authorized(_) => {
            Err("stale generation unexpectedly authorized".to_owned())
        }
    }
}

pub fn preflight_diagnostic() -> Result<String, String> {
    let engine = "issue382-missing-engine";
    let plan = plan(Preflight {
        engine: Some(engine.to_owned()),
        image: Some("issue382/fixture:1".to_owned()),
        required_env: Vec::new(),
        required: true,
    });
    let evidence = ExecutionEvidence::new(
        plan.definition_sha256,
        plan.executable_fingerprint.clone(),
        plan.probe_generation,
        plan.target_generation,
        plan.activation_generation,
    );
    let authorized = match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(rejection) => {
            return Err(format!(
                "preflight fixture authorization failed: {rejection}"
            ));
        }
    };
    match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
        PreparationOutcome::Unavailable(UnavailableReason::EngineMissing { engine: missing })
            if missing == engine =>
        {
            Ok(format!("PREFLIGHT REFUSED: engine missing: {missing}"))
        }
        other => Err(format!("unexpected preflight outcome: {other:?}")),
    }
}

fn plan(preflight: Preflight) -> AgentLaunchPlan {
    let executable = std::path::PathBuf::from("/issue382/fixture-agent");
    let fingerprint = CandidateFingerprint::new(executable.clone(), None, None, 1, 1);
    AgentLaunchPlan {
        type_id: AgentTypeId::parse("core.test")
            .unwrap_or_else(|_| panic!("core.test is a valid fixture type id")),
        operation: Operation::Normal,
        definition_sha256: DefinitionSha256::default(),
        executable,
        executable_fingerprint: fingerprint,
        executable_wrapper: AgentWrapperKind::Direct,
        argv: Vec::new(),
        env: Vec::new(),
        cwd: std::path::PathBuf::from("/issue382"),
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/issue382"),
        },
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        preflight,
        signature: LaunchSignatureV1::default(),
    }
}
