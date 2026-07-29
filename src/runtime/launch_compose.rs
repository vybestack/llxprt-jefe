//! Definition-driven launch clearance rooted in state-owned availability.

use std::path::PathBuf;

use crate::agent_candidate::{
    AgentCandidateResolver, CandidateGenerationKey, CandidateResolution, VersionSelector,
    next_probe_generation,
};
use crate::agent_candidate_path::PathSnapshot;
use crate::agent_status_view::AgentAvailabilityObservation;
use crate::domain::agent_definition::{
    AgentDefinition, AgentLaunchPlan, FieldKind, FieldValue, Preflight, RemoteTarget, Support,
    Target,
};
use crate::domain::canonical_values::{
    canonical_local_target, launch_value_fingerprint, normalize_remote_path, typed_field,
};
use crate::domain::{AgentLaunchRequest, RemoteRepositorySettings, TypedMap, TypedValue};

use super::RuntimeError;
use super::agent_execution_guard::{AuthorizationResult, ExecutionEvidence, authorize_execution};
use super::agent_fresh_send::{fresh_send_support, prepare_fresh_send};
use super::agent_plan::{LaunchFieldValues, PlanOutcome, PlanRequest, plan_local_launch};
use super::agent_preflight::{
    AuthorizedLaunchPlan, PreflightCleared, PreparationOutcome, ProcessSandboxInspector,
    prepare_execution,
};
use super::agent_probe::run_local_agent_probe;
use super::agent_remote_plan::{RemotePlanOutcome, RemotePlanRequest, plan_remote_launch};
use super::agent_remote_probe::run_remote_agent_probe;
use super::package_runtime::{finalize_local_invocation, managed_package_cache_root};

/// Current state-owned evidence from which one launch attempt is derived.
#[derive(Debug, Clone)]
pub struct LaunchStateEvidence {
    availability: crate::domain::agent_definition::Availability,
    resolution: Option<CandidateResolution>,
    candidate_generation_key: Option<CandidateGenerationKey>,
    probe_generation: u64,
    target_generation: u64,
    activation_generation: u64,
    expected_engine_fingerprint: Option<String>,
}

impl LaunchStateEvidence {
    /// Capture launch authority from one application-state observation.
    #[must_use]
    pub fn from_observation(
        observation: &AgentAvailabilityObservation,
        target_generation: u64,
        activation_generation: u64,
    ) -> Self {
        Self {
            availability: observation.availability().clone(),
            resolution: observation.candidate_resolution().cloned(),
            candidate_generation_key: observation.candidate_generation_key().cloned(),
            probe_generation: observation.generation(),
            target_generation,
            activation_generation,
            expected_engine_fingerprint: None,
        }
    }

    /// Attach the last sandbox-engine fingerprint observed by state.
    #[must_use]
    pub fn with_engine_fingerprint(mut self, fingerprint: Option<String>) -> Self {
        self.expected_engine_fingerprint = fingerprint;
        self
    }
}

/// Fully authorized/preflight-cleared launch plus its audited remote transport.
#[derive(Debug, Clone)]
pub struct PreparedLaunch {
    authorized: AuthorizedLaunchPlan,
    remote: Option<RemoteRepositorySettings>,
}

impl PreparedLaunch {
    /// Proof consumed by runtime session creation.
    #[must_use]
    pub const fn authorized(&self) -> &AuthorizedLaunchPlan {
        &self.authorized
    }

    /// Remote settings matching the plan target, when remote.
    #[must_use]
    pub const fn remote(&self) -> Option<&RemoteRepositorySettings> {
        self.remote.as_ref()
    }

    /// Immutable cleared plan for signature projection.
    #[must_use]
    pub const fn plan(&self) -> &AgentLaunchPlan {
        self.authorized.plan()
    }
}

struct CandidateEvidence {
    executable: PathBuf,
    fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint,
    wrapper: crate::agent_candidate_path::AgentWrapperKind,
    argv_prefix: Vec<std::ffi::OsString>,
    availability: crate::domain::agent_definition::Availability,
    generation: u64,
}

struct ImmutablePlanInputs<'a> {
    definition: &'a AgentDefinition,
    configuration: &'a AgentLaunchRequest,
    values: &'a LaunchFieldValues,
    target: Target,
    candidate: &'a CandidateEvidence,
    state_evidence: &'a LaunchStateEvidence,
    preflight: Preflight,
}

/// Validate support, capture target-specific probe evidence, build one immutable
/// plan, authorize it, run preflight, and seal the runtime proof.
pub fn prepare_launch(
    configuration: &AgentLaunchRequest,
    state_evidence: &LaunchStateEvidence,
) -> Result<PreparedLaunch, RuntimeError> {
    let definition = definition_for(configuration)?;
    validate_support_before_effects(&definition, configuration)?;
    let selector = version_selector(&configuration.values)?;
    let target = launch_target(configuration)?;
    let values = launch_values(
        &definition,
        &configuration.values,
        configuration.operation.is_fresh(),
    )?;
    let candidate = if configuration.remote.enabled {
        remote_candidate(
            &definition,
            configuration,
            &selector,
            state_evidence.probe_generation,
        )?
    } else {
        local_candidate(&definition, configuration, &selector, state_evidence)?
    };
    let preflight = preflight_contract(&definition, &configuration.values)?;
    let plan = immutable_plan(ImmutablePlanInputs {
        definition: &definition,
        configuration,
        values: &values,
        target,
        candidate: &candidate,
        state_evidence,
        preflight,
    })?;
    let evidence = execution_evidence(&definition, &candidate, state_evidence);
    let final_plan = if configuration.operation.is_fresh() {
        let prompt = fresh_prompt(&configuration.values)?;
        authorize_and_preflight(&plan, &evidence, state_evidence, |cleared| {
            prepare_fresh_send(&definition, cleared.clone(), prompt)
                .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))
                .map(|send| send.plan().clone())
        })?
    } else {
        authorize_and_preflight(&plan, &evidence, state_evidence, |_| Ok(plan.clone()))?
    };
    Ok(PreparedLaunch {
        authorized: final_plan,
        remote: configuration
            .remote
            .enabled
            .then(|| configuration.remote.clone()),
    })
}

/// Build the execution evidence derived from the definition, resolved candidate,
/// and caller-captured state generations.
fn execution_evidence(
    definition: &AgentDefinition,
    candidate: &CandidateEvidence,
    state_evidence: &LaunchStateEvidence,
) -> ExecutionEvidence {
    ExecutionEvidence::new(
        definition.sha256(),
        candidate.fingerprint.clone(),
        candidate.generation,
        state_evidence.target_generation,
        state_evidence.activation_generation,
    )
}

/// Authorize the plan, run preflight, and seal an [`AuthorizedLaunchPlan`].
///
/// `assemble_final_plan` receives the cleared preflight wrapper and returns the
/// final plan that may differ from `plan` only in argv (fresh-send prompt
/// assembly). The sealed plan re-authorizes the final plan.
fn authorize_and_preflight(
    plan: &AgentLaunchPlan,
    evidence: &ExecutionEvidence,
    state_evidence: &LaunchStateEvidence,
    assemble_final_plan: impl Fn(&PreflightCleared<'_>) -> Result<AgentLaunchPlan, RuntimeError>,
) -> Result<AuthorizedLaunchPlan, RuntimeError> {
    let authorized = match authorize_execution(plan, evidence) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(error) => {
            return Err(RuntimeError::SpawnFailed(error.to_string()));
        }
    };
    let cleared = match prepare_execution(
        authorized,
        state_evidence.expected_engine_fingerprint.as_deref(),
        &ProcessSandboxInspector::new(),
    ) {
        PreparationOutcome::Cleared(cleared) => cleared,
        PreparationOutcome::Unavailable(reason) => {
            return Err(RuntimeError::SpawnFailed(reason.to_string()));
        }
    };
    let final_plan = assemble_final_plan(&cleared)?;
    AuthorizedLaunchPlan::from_cleared(cleared, final_plan, evidence.clone())
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))
}
/// Establish an isolated launch-state root for production routes that do not
/// own `AppState` (currently the non-interactive CLI boundary).
pub fn observe_launch_state(
    configuration: &AgentLaunchRequest,
) -> Result<LaunchStateEvidence, RuntimeError> {
    let definition = definition_for(configuration)?;
    validate_support_before_effects(&definition, configuration)?;
    if configuration.remote.enabled {
        return Err(RuntimeError::SpawnFailed(
            "remote launch state must be supplied by its owning application state".into(),
        ));
    }
    let selector = version_selector(&configuration.values)?;
    let snapshot = PathSnapshot::current();
    let resolution = AgentCandidateResolver::new(&snapshot, configuration.work_dir.clone())
        .with_version_selector(selector)
        .resolve(&definition);
    let candidate = resolved_candidate(&resolution)?;
    let key = candidate.generation_key(&definition);
    let generation = next_probe_generation(None, &key, u64::default())
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    let probe = run_local_agent_probe(&definition, &resolution, generation);
    Ok(LaunchStateEvidence {
        availability: probe.availability().clone(),
        resolution: Some(resolution),
        candidate_generation_key: Some(key),
        probe_generation: generation,
        target_generation: u64::default(),
        activation_generation: u64::default(),
        expected_engine_fingerprint: None,
    })
}

/// Compute a durable launch signature without candidate, package, probe, or
/// process effects.
pub fn launch_signature_from_request(
    configuration: &AgentLaunchRequest,
) -> Result<crate::domain::LaunchSignatureV1, RuntimeError> {
    let definition = definition_for(configuration)?;
    let typed_value_hash = launch_value_fingerprint(&definition, &configuration.values)
        .map_err(RuntimeError::SpawnFailed)?;
    let target = launch_target(configuration)?;
    Ok(crate::domain::LaunchSignatureV1::v1(
        definition.sha256(),
        typed_value_hash,
        crate::domain::canonical_values::launch_target_fingerprint(&target),
    ))
}

fn validate_support_before_effects(
    definition: &AgentDefinition,
    configuration: &AgentLaunchRequest,
) -> Result<(), RuntimeError> {
    if configuration.remote.enabled
        && !crate::domain::target::is_valid_remote(&configuration.remote)
    {
        return Err(RuntimeError::SpawnFailed(
            crate::domain::target::invalid_remote_message(),
        ));
    }
    let target = support_target(configuration);
    if configuration.operation.is_fresh() {
        return fresh_send_support(definition, configuration.operation, &target)
            .map_err(|error| RuntimeError::SpawnFailed(error.to_string()));
    }
    if let Support::Unsupported { reason } = &definition
        .operations
        .support_for(configuration.operation)
        .supported
    {
        return Err(RuntimeError::SpawnFailed(reason.clone()));
    }
    let target_support = if configuration.remote.enabled {
        &definition.targets.remote.supported
    } else {
        &definition.targets.local.supported
    };
    if let Support::Unsupported { reason } = target_support {
        return Err(RuntimeError::SpawnFailed(reason.clone()));
    }
    Ok(())
}

fn support_target(configuration: &AgentLaunchRequest) -> Target {
    if configuration.remote.enabled {
        Target::Remote(remote_target(configuration))
    } else {
        Target::Local {
            canonical_cwd: configuration.work_dir.clone(),
        }
    }
}

fn local_candidate(
    definition: &AgentDefinition,
    configuration: &AgentLaunchRequest,
    selector: &VersionSelector,
    state_evidence: &LaunchStateEvidence,
) -> Result<CandidateEvidence, RuntimeError> {
    let resolution = if selector.is_direct() {
        state_evidence.resolution.clone().ok_or_else(|| {
            RuntimeError::SpawnFailed("state has no resolved local executable evidence".into())
        })?
    } else {
        let snapshot = PathSnapshot::current();
        AgentCandidateResolver::new(&snapshot, configuration.work_dir.clone())
            .with_version_selector(selector.clone())
            .resolve(definition)
    };
    let candidate = resolved_candidate(&resolution)?;
    let current_key = candidate.generation_key(definition);
    let generation = next_probe_generation(
        state_evidence.candidate_generation_key.as_ref(),
        &current_key,
        state_evidence.probe_generation,
    )
    .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    if selector.is_direct()
        && !matches!(
            state_evidence.availability,
            crate::domain::agent_definition::Availability::InstalledCompatible { .. }
                | crate::domain::agent_definition::Availability::NotFound
        )
    {
        return Err(RuntimeError::SpawnFailed(
            "state-owned local availability is not compatible".into(),
        ));
    }
    let probe = run_local_agent_probe(definition, &resolution, generation);
    let invocation = finalize_local_invocation(candidate, &managed_package_cache_root())
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    Ok(CandidateEvidence {
        executable: invocation.executable().to_path_buf(),
        fingerprint: invocation
            .fingerprint()
            .cloned()
            .unwrap_or_else(|| candidate.fingerprint().clone()),
        wrapper: invocation.wrapper_kind(),
        argv_prefix: invocation.prefix().to_vec(),
        availability: probe.availability().clone(),
        generation,
    })
}

fn remote_candidate(
    definition: &AgentDefinition,
    configuration: &AgentLaunchRequest,
    selector: &VersionSelector,
    state_generation: u64,
) -> Result<CandidateEvidence, RuntimeError> {
    let generation = state_generation
        .checked_add(1)
        .ok_or_else(|| RuntimeError::SpawnFailed("probe generation exhausted".into()))?;
    let evidence = run_remote_agent_probe(
        definition,
        selector,
        &configuration.remote,
        &configuration.work_dir,
        generation,
    )?;
    Ok(CandidateEvidence {
        executable: evidence.executable,
        fingerprint: evidence.executable_fingerprint,
        wrapper: evidence.executable_wrapper,
        argv_prefix: evidence.argv_prefix,
        availability: evidence.availability,
        generation,
    })
}

fn immutable_plan(inputs: ImmutablePlanInputs<'_>) -> Result<AgentLaunchPlan, RuntimeError> {
    let ImmutablePlanInputs {
        definition,
        configuration,
        values,
        target,
        candidate,
        state_evidence,
        preflight,
    } = inputs;
    if configuration.remote.enabled {
        let request = RemotePlanRequest {
            definition,
            operation: configuration.operation,
            target,
            executable: candidate.executable.clone(),
            executable_fingerprint: candidate.fingerprint.clone(),
            executable_wrapper: candidate.wrapper,
            argv_prefix: candidate.argv_prefix.clone(),
            probe: candidate.availability.clone(),
            probe_generation: candidate.generation,
            target_generation: state_evidence.target_generation,
            activation_generation: state_evidence.activation_generation,
            values,
            preflight,
            ssh_settings: &configuration.remote,
        };
        return match plan_remote_launch(&request) {
            RemotePlanOutcome::Transcript(transcript) => Ok(transcript.plan().clone()),
            RemotePlanOutcome::Unsupported { reason } => Err(RuntimeError::SpawnFailed(reason)),
            RemotePlanOutcome::Error(error) => Err(RuntimeError::SpawnFailed(error.to_string())),
        };
    }
    let request = PlanRequest {
        definition,
        operation: configuration.operation,
        target,
        executable: candidate.executable.clone(),
        executable_fingerprint: candidate.fingerprint.clone(),
        executable_wrapper: candidate.wrapper,
        argv_prefix: candidate.argv_prefix.clone(),
        probe: candidate.availability.clone(),
        probe_generation: candidate.generation,
        target_generation: state_evidence.target_generation,
        activation_generation: state_evidence.activation_generation,
        values,
        preflight,
    };
    match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => Ok(*plan),
        PlanOutcome::Unsupported { reason } => Err(RuntimeError::SpawnFailed(reason)),
        PlanOutcome::Error(error) => Err(RuntimeError::SpawnFailed(error.to_string())),
    }
}

fn preflight_contract(
    definition: &AgentDefinition,
    values: &TypedMap,
) -> Result<Preflight, RuntimeError> {
    let sandbox_capable = definition
        .probe
        .capabilities
        .as_ref()
        .and_then(|probe| probe.token_for("sandbox"))
        .is_some();
    let required = sandbox_capable && bool_value(values, "sandbox_enabled")?.unwrap_or(false);
    if !required {
        return Ok(Preflight::default());
    }
    Ok(Preflight {
        engine: string_value(values, "sandbox_engine")?.map(str::to_owned),
        image: string_value(values, "sandbox_image")?.map(str::to_owned),
        required_env: string_list_value(values, "sandbox_required_env")?,
        required: true,
    })
}

fn bool_value(values: &TypedMap, name: &str) -> Result<Option<bool>, RuntimeError> {
    match typed_field(values, name) {
        None => Ok(None),
        Some(TypedValue::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(RuntimeError::SpawnFailed(format!(
            "{name} must be a boolean"
        ))),
    }
}

fn string_value<'a>(values: &'a TypedMap, name: &str) -> Result<Option<&'a str>, RuntimeError> {
    match typed_field(values, name) {
        Some(TypedValue::String(value)) if !value.trim().is_empty() => Ok(Some(value.trim())),
        None | Some(TypedValue::String(_)) => Ok(None),
        Some(_) => Err(RuntimeError::SpawnFailed(format!(
            "{name} must be a string"
        ))),
    }
}

fn string_list_value(values: &TypedMap, name: &str) -> Result<Vec<String>, RuntimeError> {
    match typed_field(values, name) {
        None => Ok(Vec::new()),
        Some(TypedValue::List(values)) => values
            .iter()
            .map(|value| match value {
                TypedValue::String(value) if !value.is_empty() => Ok(value.clone()),
                _ => Err(RuntimeError::SpawnFailed(format!(
                    "{name} must contain non-empty strings"
                ))),
            })
            .collect(),
        Some(_) => Err(RuntimeError::SpawnFailed(format!("{name} must be a list"))),
    }
}

fn fresh_prompt(values: &TypedMap) -> Result<&str, RuntimeError> {
    string_value(values, "prompt")?.ok_or_else(|| {
        RuntimeError::SpawnFailed("fresh operation requires one typed prompt".into())
    })
}

fn launch_target(configuration: &AgentLaunchRequest) -> Result<Target, RuntimeError> {
    if configuration.remote.enabled {
        return Ok(Target::Remote(remote_target(configuration)));
    }
    canonical_local_target(&configuration.work_dir)
        .map(PathBuf::from)
        .map(|canonical_cwd| Target::Local { canonical_cwd })
        .map_err(RuntimeError::SpawnFailed)
}

fn remote_target(configuration: &AgentLaunchRequest) -> RemoteTarget {
    RemoteTarget {
        user: configuration.remote.login_user.trim().to_owned(),
        host: configuration.remote.host.trim().to_owned(),
        port: configuration.remote.port,
        run_as_user: configuration.remote.run_as_user.trim().to_owned(),
        canonical_cwd: PathBuf::from(normalize_remote_path(
            &configuration.work_dir.to_string_lossy(),
        )),
    }
}

fn definition_for(configuration: &AgentLaunchRequest) -> Result<AgentDefinition, RuntimeError> {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id == configuration.type_id)
        .ok_or_else(|| {
            RuntimeError::SpawnFailed(format!(
                "unknown active agent type {}",
                configuration.type_id
            ))
        })
}

fn version_selector(values: &TypedMap) -> Result<VersionSelector, RuntimeError> {
    let value = match typed_field(values, "version_selector") {
        None => "",
        Some(TypedValue::String(value)) => value,
        Some(_) => {
            return Err(RuntimeError::SpawnFailed(
                "version_selector must be a string".to_owned(),
            ));
        }
    };
    VersionSelector::normalize(value).map_err(|error| RuntimeError::SpawnFailed(error.to_string()))
}

fn resolved_candidate(
    resolution: &CandidateResolution,
) -> Result<&crate::agent_candidate::ResolvedCandidate, RuntimeError> {
    resolution.resolved().ok_or_else(|| {
        RuntimeError::SpawnFailed("configured agent executable was not found".to_owned())
    })
}

fn launch_values(
    definition: &AgentDefinition,
    values: &TypedMap,
    omit_prompt: bool,
) -> Result<LaunchFieldValues, RuntimeError> {
    let mut launch = LaunchFieldValues::new();
    for field in &definition.repository_fields {
        if let Some(value) = launch_field(values, field, omit_prompt)? {
            launch.set_repository(field.id.clone(), value);
        }
    }
    for field in &definition.agent_fields {
        if let Some(value) = launch_field(values, field, omit_prompt)? {
            launch.set_agent(field.id.clone(), value);
        }
    }
    Ok(launch)
}

fn launch_field(
    values: &TypedMap,
    field: &crate::domain::agent_definition::Field,
    omit_prompt: bool,
) -> Result<Option<FieldValue>, RuntimeError> {
    if omit_prompt && field.id == "prompt" {
        return Ok(None);
    }
    typed_field(values, &field.id)
        .map(|value| to_field_value(field.kind, value))
        .transpose()
}

fn to_field_value(kind: FieldKind, value: &TypedValue) -> Result<FieldValue, RuntimeError> {
    let converted = match (kind, value) {
        (FieldKind::Boolean, TypedValue::Bool(value)) => FieldValue::Boolean(*value),
        (FieldKind::OptionalBoolean, TypedValue::Bool(value)) => {
            FieldValue::OptionalBoolean(Some(*value))
        }
        (FieldKind::Integer, TypedValue::Integer(value)) => FieldValue::Integer(*value),
        (FieldKind::StringList, TypedValue::List(values)) => FieldValue::StringList(
            values
                .iter()
                .map(|value| match value {
                    TypedValue::String(value) => Ok(value.clone()),
                    _ => Err(RuntimeError::SpawnFailed(
                        "string-list field contains a non-string value".to_owned(),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        (FieldKind::Path, TypedValue::String(value)) => FieldValue::Path(value.clone()),
        (FieldKind::String | FieldKind::Enum, TypedValue::String(value)) => {
            FieldValue::String(value.clone())
        }
        _ => {
            return Err(RuntimeError::SpawnFailed(
                "typed launch value does not match its definition field".to_owned(),
            ));
        }
    };
    Ok(converted)
}
