//! Production-boundary helpers for CW02-10/CW02-11 fresh sends.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use jefe::agent_candidate_fingerprint::CandidateFingerprint;
use jefe::domain::agent_definition::shipped::shipped_definitions;
use jefe::domain::agent_definition::{
    AgentDefinition, AgentLaunchPlan, LaunchSignature, Operation, Preflight, PromptShape, Target,
};
use jefe::runtime::{
    AuthorizationResult, ExecutionEvidence, FreshSendRejection, PreparationOutcome,
    ProcessSandboxInspector, authorize_execution, fresh_send_support, prepare_execution,
    prepare_fresh_send,
};

const ISSUE_PROMPT: &str = "issue prompt\nexact bytes";
const PR_PROMPT: &str = "pull request prompt\nexact bytes";

fn definition(id: &str) -> AgentDefinition {
    shipped_definitions()
        .into_iter()
        .find(|definition| definition.id.as_str() == id)
        .unwrap_or_else(|| panic!("missing shipped definition {id}"))
}

fn plan(definition: &AgentDefinition, operation: Operation) -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: definition.id.clone(),
        operation,
        definition_sha256: definition.sha256(),
        executable: PathBuf::from("/opt/bin/agent"),
        argv: vec![OsString::from("--existing")],
        env: Vec::new(),
        cwd: PathBuf::from("/srv/project"),
        target: Target::Local {
            canonical_cwd: PathBuf::from("/srv/project"),
        },
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        preflight: Preflight {
            required: false,
            ..Preflight::default()
        },
        signature: LaunchSignature::default(),
    }
}

fn assert_supported(definition: &AgentDefinition, operation: Operation, prompt: &str) {
    let plan = plan(definition, operation);
    let evidence = ExecutionEvidence::new(
        plan.definition_sha256,
        CandidateFingerprint::new(plan.executable.clone(), None, None, 1, 1),
        1,
        1,
        1,
    );
    let authorized = match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(rejection) => panic!("must authorize: {rejection}"),
    };
    let cleared = match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
        PreparationOutcome::Cleared(cleared) => cleared,
        PreparationOutcome::Unavailable(reason) => panic!("must clear: {reason}"),
    };
    let prepared = prepare_fresh_send(definition, cleared, prompt)
        .unwrap_or_else(|error| panic!("must prepare: {error}"));
    assert_eq!(
        prepared.plan().argv[prepared.prompt_index()],
        OsString::from(prompt)
    );
    assert_eq!(
        prepared
            .plan()
            .argv
            .iter()
            .filter(|argument| argument.as_os_str() == OsStr::new(prompt))
            .count(),
        1
    );

    match definition.operations.support_for(operation).prompt {
        PromptShape::InteractiveOption => assert_eq!(
            prepared.plan().argv[prepared.prompt_index() - 1],
            OsString::from("-i")
        ),
        PromptShape::InitialPositional => {
            assert_eq!(prepared.prompt_index(), prepared.plan().argv.len() - 1);
        }
        PromptShape::None | PromptShape::NoneDefault => {
            panic!("supported fresh definition must declare prompt shape")
        }
    }
}

fn assert_unsupported(definition: &AgentDefinition, operation: Operation) {
    let target = Target::Local {
        canonical_cwd: PathBuf::from("/srv/project"),
    };
    let expected = definition
        .operations
        .support_for(operation)
        .supported
        .reason()
        .unwrap_or_else(|| panic!("fixture must declare unsupported"));
    assert_eq!(
        fresh_send_support(definition, operation, &target),
        Err(FreshSendRejection::Unsupported {
            reason: expected.to_owned()
        })
    );
}

pub fn assert_operation(operation: Operation) {
    let prompt = match operation {
        Operation::FreshIssue => ISSUE_PROMPT,
        Operation::FreshPullRequest => PR_PROMPT,
        Operation::Normal | Operation::Resume => {
            panic!("acceptance helper requires fresh operation")
        }
    };

    for id in ["core.llxprt", "core.code-puppy"] {
        assert_supported(&definition(id), operation, prompt);
    }
    for id in ["core.codex", "core.claude-code"] {
        assert_unsupported(&definition(id), operation);
    }
}
