//! Definition-driven composition of an immutable runtime launch plan.
//!
//! This boundary resolves the configured type and typed values through the
//! shipped definition registry, candidate resolver, probe, package finalizer,
//! and pure local/remote planner. Runtime managers receive only the resulting
//! immutable plan.

use std::path::{Path, PathBuf};

use crate::agent_candidate::{AgentCandidateResolver, CandidateResolution, VersionSelector};
use crate::agent_candidate_path::PathSnapshot;
use crate::domain::agent_definition::{
    AgentDefinition, AgentLaunchPlan, FieldKind, FieldValue, Preflight, RemoteTarget, Target,
};
use crate::domain::canonical_values::{
    canonical_local_target, launch_value_fingerprint, normalize_remote_path,
};
use crate::domain::{AgentLaunchRequest, RemoteRepositorySettings, TypedMap, TypedValue};

use super::RuntimeError;
use super::agent_plan::{LaunchFieldValues, PlanOutcome, PlanRequest, plan_local_launch};
use super::agent_probe::run_local_agent_probe;
use super::agent_remote_plan::{RemotePlanOutcome, RemotePlanRequest, plan_remote_launch};
use super::package_runtime::{
    PackageExecutionTarget, finalize_local_invocation, managed_package_cache_root,
    package_invocation,
};

struct ComposeContext<'a> {
    configuration: &'a AgentLaunchRequest,
    definition: &'a AgentDefinition,
    values: &'a LaunchFieldValues,
    candidate: &'a crate::agent_candidate::ResolvedCandidate,
    availability: crate::domain::agent_definition::Availability,
}

/// Compose one immutable plan from generic type/value authority.
pub fn plan_from_request(
    configuration: &AgentLaunchRequest,
) -> Result<(AgentLaunchPlan, Option<RemoteRepositorySettings>), RuntimeError> {
    let definition = definition_for(configuration)?;
    let values = launch_values(&definition, &configuration.values)?;
    let selector = version_selector(&configuration.values)?;
    let snapshot = PathSnapshot::current();
    let resolution = AgentCandidateResolver::new(&snapshot, configuration.work_dir.clone())
        .with_version_selector(selector)
        .resolve(&definition);
    let candidate = resolved_candidate(&resolution)?;
    let probe = run_local_agent_probe(&definition, &resolution, 1);
    let availability = probe.availability().clone();
    let remote = configuration
        .remote
        .enabled
        .then(|| configuration.remote.clone());
    let context = ComposeContext {
        configuration,
        definition: &definition,
        values: &values,
        candidate,
        availability,
    };
    if let Some(settings) = remote.as_ref() {
        return plan_remote(&context, settings, Some(settings.clone()));
    }
    plan_local(&context)
}
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

/// Reconstruct a plan for an already-live session without probing or package effects.
///
/// Startup uses this only after durable signature validation and independent
/// tmux/process liveness evidence. The retained plan is sufficient for manager
/// registration; any future fresh relaunch is replanned and reauthorized.
pub fn restore_plan_from_request(
    configuration: &AgentLaunchRequest,
) -> Result<AgentLaunchPlan, RuntimeError> {
    let definition = definition_for(configuration)?;
    let values = launch_values(&definition, &configuration.values)?;
    let operation = configuration.operation;
    let target = launch_target(configuration)?;
    let executable = restore_executable(&definition)?;
    let request = PlanRequest {
        definition: &definition,
        operation,
        target,
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            executable.clone(),
            None,
            None,
            0,
            0,
        ),
        executable,
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: crate::domain::agent_definition::Availability::InstalledCompatible {
            identity: "restored-live-session".to_owned(),
            capabilities: definition.probe.required.clone(),
            generation: 0,
        },
        probe_generation: 0,
        target_generation: 0,
        activation_generation: 0,
        values: &values,
        preflight: Preflight {
            required: false,
            ..Preflight::default()
        },
    };
    match plan_local_launch(&request) {
        PlanOutcome::Supported(mut plan) => {
            plan.signature = launch_signature_from_request(configuration)?;
            Ok(*plan)
        }
        PlanOutcome::Unsupported { reason } => Err(RuntimeError::SpawnFailed(reason)),
        PlanOutcome::Error(error) => Err(RuntimeError::SpawnFailed(error.to_string())),
    }
}

fn restore_executable(definition: &AgentDefinition) -> Result<PathBuf, RuntimeError> {
    definition
        .candidates
        .iter()
        .map(|candidate| match &candidate.kind {
            crate::domain::agent_definition::CandidateKind::PathName { name } => {
                PathBuf::from(name)
            }
            crate::domain::agent_definition::CandidateKind::RepositoryLlxprt => {
                candidate.value.clone()
            }
            crate::domain::agent_definition::CandidateKind::NpmPackage { binary, .. }
            | crate::domain::agent_definition::CandidateKind::UvxPackage { binary, .. } => {
                PathBuf::from(binary)
            }
        })
        .next()
        .ok_or_else(|| RuntimeError::SpawnFailed("agent definition has no candidate".to_owned()))
}

fn launch_target(configuration: &AgentLaunchRequest) -> Result<Target, RuntimeError> {
    if configuration.remote.enabled {
        return Ok(Target::Remote(RemoteTarget {
            user: configuration.remote.login_user.trim().to_owned(),
            host: configuration.remote.host.trim().to_owned(),
            port: configuration.remote.port,
            run_as_user: configuration.remote.run_as_user.trim().to_owned(),
            canonical_cwd: PathBuf::from(normalize_remote_path(
                &configuration.work_dir.to_string_lossy(),
            )),
        }));
    }
    canonical_local_target(&configuration.work_dir)
        .map(PathBuf::from)
        .map(|canonical_cwd| Target::Local { canonical_cwd })
        .map_err(RuntimeError::SpawnFailed)
}

fn plan_remote(
    context: &ComposeContext<'_>,
    settings: &RemoteRepositorySettings,
    remote: Option<RemoteRepositorySettings>,
) -> Result<(AgentLaunchPlan, Option<RemoteRepositorySettings>), RuntimeError> {
    let invocation = package_invocation(
        context.candidate,
        PackageExecutionTarget::Remote,
        &managed_package_cache_root(),
    )
    .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    let executable = invocation.as_ref().map_or_else(
        || remote_executable(context.candidate.executable()),
        |value| value.executable().to_path_buf(),
    );
    let executable_fingerprint = invocation
        .as_ref()
        .and_then(|value| value.fingerprint().cloned())
        .unwrap_or_else(|| context.candidate.fingerprint().clone());
    let executable_wrapper = invocation.as_ref().map_or_else(
        || context.candidate.wrapper_kind(),
        super::package_runtime::PackageInvocation::wrapper_kind,
    );
    let argv_prefix = invocation
        .as_ref()
        .map_or_else(Vec::new, |value| value.prefix().to_vec());
    let request = RemotePlanRequest {
        definition: context.definition,
        operation: context.configuration.operation,
        target: Target::Remote(RemoteTarget {
            user: settings.login_user.trim().to_owned(),
            host: settings.host.trim().to_owned(),
            port: settings.port,
            run_as_user: settings.run_as_user.trim().to_owned(),
            canonical_cwd: PathBuf::from(normalize_remote_path(
                &context.configuration.work_dir.to_string_lossy(),
            )),
        }),
        executable,
        executable_fingerprint,
        executable_wrapper,
        argv_prefix,
        probe: context.availability.clone(),
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values: context.values,
        preflight: Preflight::default(),
        ssh_settings: settings,
    };
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(transcript) => Ok((transcript.plan().clone(), remote)),
        RemotePlanOutcome::Unsupported { reason } => Err(RuntimeError::SpawnFailed(reason)),
        RemotePlanOutcome::Error(error) => Err(RuntimeError::SpawnFailed(error.to_string())),
    }
}

fn plan_local(
    context: &ComposeContext<'_>,
) -> Result<(AgentLaunchPlan, Option<RemoteRepositorySettings>), RuntimeError> {
    let invocation = finalize_local_invocation(context.candidate, &managed_package_cache_root())
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    let canonical_cwd = canonical_local_target(&context.configuration.work_dir)
        .map(PathBuf::from)
        .map_err(RuntimeError::SpawnFailed)?;
    let request = PlanRequest {
        definition: context.definition,
        operation: context.configuration.operation,
        target: Target::Local { canonical_cwd },
        executable: invocation.executable().to_path_buf(),
        executable_fingerprint: invocation
            .fingerprint()
            .cloned()
            .unwrap_or_else(|| context.candidate.fingerprint().clone()),
        executable_wrapper: invocation.wrapper_kind(),
        argv_prefix: invocation.prefix().to_vec(),
        probe: context.availability.clone(),
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values: context.values,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => Ok((*plan, None)),
        PlanOutcome::Unsupported { reason } => Err(RuntimeError::SpawnFailed(reason)),
        PlanOutcome::Error(error) => Err(RuntimeError::SpawnFailed(error.to_string())),
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
    let value = match crate::domain::canonical_values::typed_field(values, "version_selector") {
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
) -> Result<LaunchFieldValues, RuntimeError> {
    let mut launch = LaunchFieldValues::new();
    for field in &definition.repository_fields {
        if let Some(value) = typed_field(values, field)? {
            launch.set_repository(field.id.clone(), value);
        }
    }
    for field in &definition.agent_fields {
        if let Some(value) = typed_field(values, field)? {
            launch.set_agent(field.id.clone(), value);
        }
    }
    Ok(launch)
}

fn typed_field(
    values: &TypedMap,
    field: &crate::domain::agent_definition::Field,
) -> Result<Option<FieldValue>, RuntimeError> {
    crate::domain::canonical_values::typed_field(values, &field.id)
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

fn remote_executable(path: &Path) -> PathBuf {
    path.file_name()
        .map_or_else(|| path.to_path_buf(), PathBuf::from)
}
