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
            capabilities: vec![
                "prompt-interactive".to_owned(),
                "profile".to_owned(),
                "yolo".to_owned(),
                "continue".to_owned(),
            ],
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
            capabilities: vec![
                "prompt-interactive".to_owned(),
                "profile".to_owned(),
                "yolo".to_owned(),
                "continue".to_owned(),
            ],
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
