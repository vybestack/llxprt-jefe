use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use super::*;
use crate::domain::Id;
use crate::domain::RemoteRepositorySettings;
use crate::domain::agent_definition::{
    AgentLaunchPlan, Availability, FieldValue, Operation, Preflight, RemoteTarget, Target,
};
use crate::runtime::agent_plan::{LaunchFieldValues, PlanOutcome, PlanRequest, plan_local_launch};
use crate::runtime::agent_remote_plan::{RemotePlanOutcome, RemotePlanRequest, plan_remote_launch};
use crate::runtime::{
    AuthorizationResult, ExecutionEvidence, PreparationOutcome, ProcessSandboxInspector,
    authorize_execution, prepare_execution, prepare_fresh_send,
};

const PROMPT: &str = "exact prompt bytes\nwith a second line";

fn llxprt() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.llxprt")
        .unwrap_or_else(|| panic!("LLxprt definition must be shipped"))
}

fn typed_values(continuation: bool) -> TypedMap {
    let mut values = TypedMap::new();
    for (field, value) in [
        ("profile", TypedValue::String("glm".to_owned())),
        ("yolo", TypedValue::Bool(true)),
        ("continue", TypedValue::Bool(continuation)),
        ("prompt-interactive", TypedValue::Bool(true)),
    ] {
        let key = Id::parse(field)
            .unwrap_or_else(|error| panic!("{field} must be a valid typed key: {error}"));
        values.insert(key, value);
    }
    values
}

fn local_target() -> Target {
    Target::Local {
        canonical_cwd: PathBuf::from("/srv/project"),
    }
}

fn local_plan(
    definition: &AgentDefinition,
    values: &LaunchFieldValues,
    operation: Operation,
) -> AgentLaunchPlan {
    let request = PlanRequest {
        definition,
        operation,
        target: local_target(),
        executable: PathBuf::from("/opt/bin/llxprt"),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/opt/bin/llxprt"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: Availability::InstalledCompatible {
            identity: "0.10.0".to_owned(),
            generation: 1,
        },
        probe_generation: 1,
        target_generation: 1,
        values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => *plan,
        other => panic!("LLxprt operation must plan: {other:?}"),
    }
}

fn prepare_fresh_plan(operation: Operation) -> AgentLaunchPlan {
    let definition = llxprt();
    let projected = launch_values(&definition, &typed_values(true), operation)
        .unwrap_or_else(|error| panic!("fresh values must project: {error}"));
    let plan = local_plan(&definition, &projected, operation);
    let evidence = ExecutionEvidence::new(
        plan.definition_sha256,
        plan.executable_fingerprint.clone(),
        1,
        1,
        1,
    );
    let authorized = match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(rejection) => panic!("plan must authorize: {rejection}"),
    };
    let cleared = match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
        PreparationOutcome::Cleared(cleared) => cleared,
        PreparationOutcome::Unavailable(reason) => panic!("plan must clear: {reason}"),
    };
    prepare_fresh_send(&definition, cleared, PROMPT)
        .unwrap_or_else(|error| panic!("fresh send must prepare: {error}"))
        .plan()
        .clone()
}

#[test]
fn llxprt_sandbox_does_not_require_image_preflight() {
    let definition = llxprt();
    let mut values = typed_values(true);
    values.insert(
        Id::parse("sandbox-enabled").unwrap_or_else(|error| panic!("valid key: {error}")),
        TypedValue::Bool(true),
    );
    values.insert(
        Id::parse("sandbox-engine").unwrap_or_else(|error| panic!("valid key: {error}")),
        TypedValue::String("podman".to_owned()),
    );

    let preflight = preflight_contract(&definition, &values)
        .unwrap_or_else(|error| panic!("sandbox values must project: {error}"));

    assert!(!preflight.required);
    assert!(!preflight.is_unavailable());
}

#[test]
fn launch_values_reject_unknown_enum_member() {
    let definition = llxprt();
    let mut values = typed_values(true);
    values.insert(
        Id::parse("sandbox-engine").unwrap_or_else(|error| panic!("valid key: {error}")),
        TypedValue::String("unknown".to_owned()),
    );

    let error = launch_values(&definition, &values, Operation::Normal)
        .err()
        .unwrap_or_else(|| panic!("unknown enum member must fail"));

    assert!(
        error
            .to_string()
            .contains("not a valid value for sandbox_engine")
    );
}

#[test]
fn fresh_llxprt_plan_emits_one_prompt_without_continuation() {
    for operation in [Operation::FreshIssue, Operation::FreshPullRequest] {
        let plan = prepare_fresh_plan(operation);
        assert_eq!(
            plan.argv,
            ["--profile-load", "glm", "--yolo", "-i", PROMPT].map(OsString::from)
        );
        assert!(plan.env.is_empty());
        assert_eq!(plan.cwd, PathBuf::from("/srv/project"));
        assert_eq!(
            plan.argv
                .iter()
                .filter(|argument| {
                    matches!(
                        argument.as_os_str().to_str(),
                        Some("-i" | "--prompt-interactive")
                    )
                })
                .count(),
            1
        );
        assert!(
            !plan
                .argv
                .iter()
                .any(|argument| argument == OsStr::new("--continue"))
        );
    }
}

#[test]
fn normal_and_resume_emit_only_selected_continuation() {
    let definition = llxprt();
    for operation in [Operation::Normal, Operation::Resume] {
        for continuation in [false, true] {
            let projected = launch_values(&definition, &typed_values(continuation), operation)
                .unwrap_or_else(|error| panic!("values must project: {error}"));
            assert_eq!(
                projected.agent("continue"),
                Some(&FieldValue::Boolean(continuation))
            );
            let expected = if continuation {
                vec![
                    "--profile-load",
                    "glm",
                    "--yolo",
                    "--prompt-interactive",
                    "--continue",
                ]
            } else {
                vec!["--profile-load", "glm", "--yolo", "--prompt-interactive"]
            };
            assert_eq!(
                local_plan(&definition, &projected, operation).argv,
                expected.into_iter().map(OsString::from).collect::<Vec<_>>()
            );
        }
    }
}

fn remote_target() -> Target {
    Target::Remote(RemoteTarget {
        user: "dev".to_owned(),
        host: "example.com".to_owned(),
        port: Some(22),
        run_as_user: String::new(),
        canonical_cwd: PathBuf::from("/srv/project"),
    })
}

fn remote_settings() -> RemoteRepositorySettings {
    RemoteRepositorySettings {
        enabled: true,
        login_user: "dev".to_owned(),
        host: "example.com".to_owned(),
        port: Some(22),
        ..RemoteRepositorySettings::default()
    }
}

fn remote_plan(
    definition: &AgentDefinition,
    values: &LaunchFieldValues,
    operation: Operation,
) -> AgentLaunchPlan {
    let settings = remote_settings();
    let request = RemotePlanRequest {
        definition,
        operation,
        target: remote_target(),
        executable: PathBuf::from("/opt/bin/llxprt"),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/opt/bin/llxprt"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: Availability::InstalledCompatible {
            identity: "0.10.0".to_owned(),
            generation: 1,
        },
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values,
        preflight: Preflight::default(),
        ssh_settings: &settings,
    };
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(transcript) => transcript.plan().clone(),
        other => panic!("remote LLxprt operation must plan: {other:?}"),
    }
}

#[test]
fn remote_normal_and_resume_emit_only_selected_continuation() {
    let definition = llxprt();
    for operation in [Operation::Normal, Operation::Resume] {
        for continuation in [false, true] {
            let projected = launch_values(&definition, &typed_values(continuation), operation)
                .unwrap_or_else(|error| panic!("values must project: {error}"));
            let plan = remote_plan(&definition, &projected, operation);
            assert_eq!(
                plan.argv
                    .iter()
                    .any(|argument| argument == OsStr::new("--continue")),
                continuation
            );
            assert!(
                plan.argv
                    .iter()
                    .any(|argument| argument == OsStr::new("--prompt-interactive")),
                "prompt_interactive flag should be emitted for normal/resume"
            );
        }
    }
}

#[cfg(unix)]
fn write_direct_fixture(path: &std::path::Path, version: &str, exit_code: i32) {
    use std::os::unix::fs::PermissionsExt;

    let body = format!("#!/bin/sh\nprintf '{version}\\n'\nexit {exit_code}\n");
    std::fs::write(path, body).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    let mut permissions = std::fs::metadata(path)
        .unwrap_or_else(|error| panic!("metadata {}: {error}", path.display()))
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
        .unwrap_or_else(|error| panic!("chmod {}: {error}", path.display()));
}

#[cfg(unix)]
fn direct_fixture(
    generation: u64,
) -> (
    tempfile::TempDir,
    AgentDefinition,
    AgentLaunchRequest,
    LaunchStateEvidence,
) {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap_or_else(|error| panic!("create bin: {error}"));
    let executable = bin.join("llxprt");
    write_direct_fixture(&executable, "0.10.0", 0);
    let definition = llxprt();
    let snapshot = PathSnapshot::for_platform(
        crate::agent_candidate_path::AgentExecutablePlatform::Unix,
        vec![bin],
        None,
    );
    let resolution =
        AgentCandidateResolver::new(&snapshot, root.path().to_path_buf()).resolve(&definition);
    let mut observation =
        AgentAvailabilityObservation::pending(&definition, true, generation, resolution.clone());
    let probe = run_local_agent_probe(&definition, &resolution, generation);
    assert!(observation.apply_probe_result(generation, probe.availability().clone()));
    let request = AgentLaunchRequest {
        type_id: definition.id.clone(),
        values: typed_values(false),
        work_dir: root.path().to_path_buf(),
        remote: RemoteRepositorySettings::default(),
        operation: Operation::Normal,
    };
    let evidence = LaunchStateEvidence::from_observation(&observation, 0, 0);
    (root, definition, request, evidence)
}

#[cfg(unix)]
fn direct_snapshot(root: &std::path::Path) -> PathSnapshot {
    PathSnapshot::for_platform(
        crate::agent_candidate_path::AgentExecutablePlatform::Unix,
        vec![root.join("bin")],
        None,
    )
}

#[cfg(unix)]
#[test]
fn unchanged_direct_candidate_retains_probe_generation() {
    let (root, _definition, request, evidence) = direct_fixture(7);
    let prepared = prepare_launch_with_snapshot(&request, &evidence, &direct_snapshot(root.path()))
        .unwrap_or_else(|error| panic!("unchanged direct candidate must prepare: {error}"));

    assert_eq!(prepared.plan().probe_generation, 7);
}

#[cfg(unix)]
#[test]
fn stable_direct_replacement_advances_generation_and_reprobes_current_file() {
    let (root, _definition, request, evidence) = direct_fixture(7);
    let executable = root.path().join("bin/llxprt");
    let replacement = root.path().join("bin/llxprt.next");
    write_direct_fixture(&replacement, "0.11.0", 0);
    std::fs::rename(&replacement, &executable)
        .unwrap_or_else(|error| panic!("replace {}: {error}", executable.display()));

    let prepared = prepare_launch_with_snapshot(&request, &evidence, &direct_snapshot(root.path()))
        .unwrap_or_else(|error| panic!("stable replacement must prepare: {error}"));

    assert_eq!(prepared.plan().probe_generation, 8);
}

#[cfg(unix)]
#[test]
fn removed_direct_candidate_reports_current_not_found() {
    let (root, _definition, request, evidence) = direct_fixture(7);
    let executable = root.path().join("bin/llxprt");
    std::fs::remove_file(&executable)
        .unwrap_or_else(|error| panic!("remove {}: {error}", executable.display()));

    let error = prepare_launch_with_snapshot(&request, &evidence, &direct_snapshot(root.path()))
        .err()
        .unwrap_or_else(|| panic!("removed direct candidate must fail"));

    assert!(
        error
            .to_string()
            .contains("configured agent executable was not found")
    );
}

#[cfg(unix)]
#[test]
fn failing_direct_replacement_reports_current_probe_error() {
    let (root, _definition, request, evidence) = direct_fixture(7);
    let executable = root.path().join("bin/llxprt");
    let replacement = root.path().join("bin/llxprt.next");
    write_direct_fixture(&replacement, "0.11.0", 9);
    std::fs::rename(&replacement, &executable)
        .unwrap_or_else(|error| panic!("replace {}: {error}", executable.display()));

    let error = prepare_launch_with_snapshot(&request, &evidence, &direct_snapshot(root.path()))
        .err()
        .unwrap_or_else(|| panic!("failing replacement must reject launch"));

    assert!(error.to_string().contains("AGT-E202"));
}

#[cfg(unix)]
#[test]
fn post_evidence_replacement_is_rejected_before_stub_session_effects() {
    use crate::domain::AgentId;
    use crate::runtime::{RuntimeManager, StubRuntimeManager};

    let (root, _definition, request, evidence) = direct_fixture(7);
    let prepared = prepare_launch_with_snapshot(&request, &evidence, &direct_snapshot(root.path()))
        .unwrap_or_else(|error| panic!("initial launch must prepare: {error}"));
    let executable = root.path().join("bin/llxprt");
    let replacement = root.path().join("bin/llxprt.next");
    write_direct_fixture(&replacement, "0.12.0", 0);
    std::fs::rename(&replacement, &executable)
        .unwrap_or_else(|error| panic!("replace {}: {error}", executable.display()));

    let agent_id = AgentId("issue575-race".to_owned());
    let mut manager = StubRuntimeManager::default();
    let error = manager
        .spawn_session(&agent_id, prepared.authorized(), None)
        .err()
        .unwrap_or_else(|| panic!("post-evidence replacement must fail closed"));

    let crate::runtime::RuntimeError::SpawnFailed(reason) = error else {
        panic!("fingerprint mismatch must be a spawn failure");
    };
    assert!(reason.contains("AGT-E203"), "{reason}");
    assert!(!manager.has_session_record(&agent_id));
}
