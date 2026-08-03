use std::path::PathBuf;

use super::*;
use crate::domain::agent_definition::shipped::shipped_definitions;
use crate::domain::agent_definition::{
    DefinitionSha256, LaunchSignatureV1, Preflight, RemoteTarget,
};
use crate::runtime::{
    AuthorizationResult, ExecutionEvidence, PreparationOutcome, ProcessSandboxInspector,
    authorize_execution, prepare_execution,
};

const PROMPT: &str = "exact prompt bytes\nwith a second line";

fn definition(id: &str) -> AgentDefinition {
    shipped_definitions()
        .into_iter()
        .find(|definition| definition.id.as_str() == id)
        .unwrap_or_else(|| panic!("missing shipped definition {id}"))
}

fn local_target() -> Target {
    Target::Local {
        canonical_cwd: PathBuf::from("/srv/project"),
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

fn settings() -> RemoteRepositorySettings {
    RemoteRepositorySettings {
        enabled: true,
        login_user: "dev".to_owned(),
        host: "example.com".to_owned(),
        port: Some(22),
        ..RemoteRepositorySettings::default()
    }
}

fn plan(definition: &AgentDefinition, operation: Operation, target: Target) -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: definition.id.clone(),
        operation,
        definition_sha256: definition.sha256(),
        executable: PathBuf::from("/opt/bin/agent"),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/opt/bin/agent"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv: vec![OsString::from("--existing")],
        env: Vec::new(),
        cwd: target.canonical_cwd().to_path_buf(),
        target,
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        preflight: Preflight {
            required: false,
            ..Preflight::default()
        },
        signature: LaunchSignatureV1::default(),
    }
}

fn with_cleared(
    definition: &AgentDefinition,
    operation: Operation,
    target: Target,
    check: impl FnOnce(PreflightCleared<'_>),
) {
    let plan = plan(definition, operation, target);
    let evidence = ExecutionEvidence::new(
        plan.definition_sha256,
        plan.executable_fingerprint.clone(),
        1,
        1,
        1,
    );
    let authorized = match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(rejection) => panic!("fixture must authorize: {rejection}"),
    };
    let cleared = match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
        PreparationOutcome::Cleared(cleared) => cleared,
        PreparationOutcome::Unavailable(reason) => panic!("fixture must clear: {reason}"),
    };
    check(cleared);
}

#[test]
fn supported_fresh_cells_emit_one_exact_prompt_after_clearance() {
    for (id, targets, prompt_shape) in [
        (
            "core.llxprt",
            vec![local_target(), remote_target()],
            PromptShape::InteractiveOption,
        ),
        (
            "core.code-puppy",
            vec![local_target()],
            PromptShape::InitialPositional,
        ),
    ] {
        let definition = definition(id);
        for operation in [Operation::FreshIssue, Operation::FreshPullRequest] {
            for target in targets.clone() {
                assert_eq!(
                    definition.operations.support_for(operation).prompt,
                    prompt_shape
                );
                with_cleared(&definition, operation, target, |cleared| {
                    let prepared = prepare_fresh_send(&definition, cleared, PROMPT)
                        .unwrap_or_else(|error| panic!("supported cell must prepare: {error}"));
                    assert_eq!(
                        prepared.plan().argv[prepared.prompt_index()],
                        OsString::from(PROMPT)
                    );
                    let emitted = prepared
                        .plan()
                        .argv
                        .iter()
                        .filter(|argument| argument.as_os_str() == std::ffi::OsStr::new(PROMPT))
                        .count();
                    assert_eq!(emitted, 1, "fresh boundary emits the prompt once");
                    match prompt_shape {
                        PromptShape::InteractiveOption => assert_eq!(
                            prepared.plan().argv[prepared.prompt_index() - 1],
                            OsString::from("-i")
                        ),
                        PromptShape::InitialPositional => {
                            assert_eq!(prepared.prompt_index(), prepared.plan().argv.len() - 1);
                        }
                        PromptShape::None | PromptShape::NoneDefault => {
                            panic!("supported fixture declares a prompt")
                        }
                    }
                });
            }
        }
    }
}

#[test]
fn prepared_remote_send_uses_audited_transcript_with_prompt() {
    let definition = definition("core.llxprt");
    with_cleared(
        &definition,
        Operation::FreshIssue,
        remote_target(),
        |cleared| {
            let prepared = prepare_fresh_send(&definition, cleared, PROMPT)
                .unwrap_or_else(|error| panic!("remote fresh cell must prepare: {error}"));
            let transcript = prepared
                .remote_transcript(&settings())
                .unwrap_or_else(|error| panic!("remote transcript must serialize: {error}"));
            assert_eq!(transcript.plan(), prepared.plan());
            assert!(transcript.remote_command().contains("'-i'"));
            assert!(
                transcript
                    .remote_command()
                    .contains("'exact prompt bytes\nwith a second line'")
            );
        },
    );
}

#[test]
fn unsupported_operation_and_target_preserve_declared_reasons() {
    // Shipped agents all declare a fixture-proven prompt shape now (issue #620),
    // so an unsupported operation is declared locally to prove the reason is
    // carried through verbatim rather than replaced by a generic message.
    for id in ["core.codex", "core.claude-code"] {
        for operation in [Operation::FreshIssue, Operation::FreshPullRequest] {
            let mut definition = definition(id);
            let reason = format!("{id} {operation:?} is not fixture-verified");
            let declared = match operation {
                Operation::FreshIssue => &mut definition.operations.fresh_issue,
                _ => &mut definition.operations.fresh_pull_request,
            };
            declared.supported = Support::unsupported(&reason);
            assert_eq!(
                fresh_send_support(&definition, operation, &local_target()),
                Err(FreshSendRejection::Unsupported { reason })
            );
        }
    }

    let definition = definition("core.code-puppy");
    assert_eq!(
        fresh_send_support(&definition, Operation::FreshIssue, &remote_target()),
        Err(FreshSendRejection::Unsupported {
            reason: "Code Puppy remote/setup is not fixture-verified".to_owned()
        })
    );
}

#[test]
fn rejects_nonfresh_and_definition_mismatch_before_prompt_emission() {
    let llxprt = definition("core.llxprt");
    with_cleared(&llxprt, Operation::Normal, local_target(), |cleared| {
        assert_eq!(
            prepare_fresh_send(&llxprt, cleared, PROMPT),
            Err(FreshSendRejection::NotFreshOperation {
                operation: Operation::Normal
            })
        );
    });

    let code_puppy = definition("core.code-puppy");
    with_cleared(&llxprt, Operation::FreshIssue, local_target(), |cleared| {
        assert_eq!(
            prepare_fresh_send(&code_puppy, cleared, PROMPT),
            Err(FreshSendRejection::DefinitionMismatch)
        );
    });
}

#[test]
fn remote_transcript_rejects_local_prepared_plan() {
    let definition = definition("core.llxprt");
    with_cleared(
        &definition,
        Operation::FreshPullRequest,
        local_target(),
        |cleared| {
            let prepared = prepare_fresh_send(&definition, cleared, PROMPT)
                .unwrap_or_else(|error| panic!("local fresh cell must prepare: {error}"));
            assert_eq!(
                prepared.remote_transcript(&settings()),
                Err(RemotePlanError::NotRemoteTarget)
            );
        },
    );
}

#[test]
fn definition_digest_is_part_of_fresh_identity() {
    assert_ne!(
        definition("core.llxprt").sha256(),
        DefinitionSha256::default()
    );
}
