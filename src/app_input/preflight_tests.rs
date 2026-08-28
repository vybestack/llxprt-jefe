//! Behavioral coverage for the launch-path preflight gate (issue #713).
//!
//! Every launch path crosses `preflight_or_prompt`, which asks
//! [`super::preflight::launch_preflight_issue`] whether an issue must be
//! prompted before any launch effect. These tests pin that decision, including
//! that a sandbox-enabled launch actually consults the host sandbox check,
//! the call whose removal left sandboxed agents starting with an empty
//! forwarded SSH agent.

use std::cell::RefCell;

use jefe::domain::{AgentLaunchRequest, RemoteRepositorySettings, SandboxEngine, TypedMap};
use jefe::runtime::PreflightIssue;

use super::preflight::{launch_preflight_issue, sandbox_preflight_engine};

/// Records every engine the host sandbox check was asked about, so a test can
/// prove consultation happened (or did not) rather than counting mock calls on
/// an opaque double.
struct RecordingHostCheck {
    consulted: RefCell<Vec<SandboxEngine>>,
    issue: Option<PreflightIssue>,
}

impl RecordingHostCheck {
    fn reporting(issue: PreflightIssue) -> Self {
        Self {
            consulted: RefCell::new(Vec::new()),
            issue: Some(issue),
        }
    }

    fn clearing() -> Self {
        Self {
            consulted: RefCell::new(Vec::new()),
            issue: None,
        }
    }

    fn check(&self) -> impl Fn(SandboxEngine) -> Option<PreflightIssue> + '_ {
        move |engine| {
            self.consulted.borrow_mut().push(engine);
            self.issue.clone()
        }
    }

    fn consulted_engines(&self) -> Vec<SandboxEngine> {
        self.consulted.borrow().clone()
    }
}

fn typed_key(field: &str) -> jefe::domain::Id {
    jefe::domain::Id::parse(&field.replace('_', "-"))
        .unwrap_or_else(|error| panic!("fixture field id must parse: {error}"))
}

fn sandbox_values(engine: &str) -> TypedMap {
    let mut values = TypedMap::new();
    values.insert(
        typed_key("sandbox_enabled"),
        jefe::domain::TypedValue::Bool(true),
    );
    values.insert(
        typed_key("sandbox_engine"),
        jefe::domain::TypedValue::String(engine.to_owned()),
    );
    values
}

/// A local LLxprt request: the shipped definition that declares sandbox fields.
fn llxprt_request(values: TypedMap) -> AgentLaunchRequest {
    AgentLaunchRequest {
        type_id: jefe::domain::shipped_agent_type(3),
        values,
        work_dir: std::env::temp_dir(),
        remote: RemoteRepositorySettings::default(),
        operation: jefe::domain::agent_definition::Operation::Normal,
    }
}

#[test]
fn sandbox_enabled_launch_prompts_the_issue_the_host_check_reports() {
    let host = RecordingHostCheck::reporting(PreflightIssue::SshAgentNoIdentities);
    let request = llxprt_request(sandbox_values("podman"));

    let issue = launch_preflight_issue(&request, host.check());

    assert_eq!(
        issue,
        Some(PreflightIssue::SshAgentNoIdentities),
        "a sandbox-enabled launch must surface the host preflight issue so the prompt opens"
    );
    assert_eq!(
        host.consulted_engines(),
        vec![SandboxEngine::Podman],
        "the configured engine must be the one checked"
    );
}

#[test]
fn sandbox_engine_selection_reaches_the_host_check() {
    let host = RecordingHostCheck::clearing();
    let request = llxprt_request(sandbox_values("docker"));

    assert_eq!(launch_preflight_issue(&request, host.check()), None);
    assert_eq!(host.consulted_engines(), vec![SandboxEngine::Docker]);
}

#[test]
fn cleared_host_lets_the_launch_proceed() {
    let host = RecordingHostCheck::clearing();
    let request = llxprt_request(sandbox_values("podman"));

    assert_eq!(
        launch_preflight_issue(&request, host.check()),
        None,
        "a cleared host must not open a prompt"
    );
    assert_eq!(host.consulted_engines(), vec![SandboxEngine::Podman]);
}

#[test]
fn sandbox_disabled_launch_never_consults_the_host() {
    let host = RecordingHostCheck::reporting(PreflightIssue::SshAgentNoIdentities);
    let request = llxprt_request(TypedMap::new());

    assert_eq!(launch_preflight_issue(&request, host.check()), None);
    assert!(
        host.consulted_engines().is_empty(),
        "a launch without a sandbox must not be gated on host sandbox state"
    );
    assert_eq!(sandbox_preflight_engine(&request), None);
}

#[test]
fn stale_sandbox_values_on_a_non_sandbox_definition_never_consult_the_host() {
    let host = RecordingHostCheck::reporting(PreflightIssue::SshAgentNoIdentities);
    let non_sandbox_type = jefe::domain::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .find(|definition| {
            !definition
                .agent_fields
                .iter()
                .chain(definition.repository_fields.iter())
                .any(|field| field.id == "sandbox_enabled")
        })
        .map(|definition| definition.id)
        .unwrap_or_else(|| panic!("a shipped definition without sandbox fields must exist"));

    let request = AgentLaunchRequest {
        type_id: non_sandbox_type,
        ..llxprt_request(sandbox_values("podman"))
    };

    assert!(
        host.consulted_engines().is_empty(),
        "no consultation should have happened yet"
    );
    assert_eq!(sandbox_preflight_engine(&request), None);
    let _ = launch_preflight_issue(&request, host.check());
    assert!(
        host.consulted_engines().is_empty(),
        "a definition that declares no sandbox must not run host sandbox preflight"
    );
}

#[test]
fn an_engine_the_gate_cannot_resolve_is_never_normalized_to_a_default() {
    let host = RecordingHostCheck::clearing();
    let request = llxprt_request(sandbox_values("kubernetes"));

    assert_eq!(
        sandbox_preflight_engine(&request),
        None,
        "inspecting one runtime on behalf of a request naming another is the \
         silent mismatch this gate exists to prevent"
    );
    let _ = launch_preflight_issue(&request, host.check());
    assert!(
        host.consulted_engines().is_empty(),
        "no engine may be guessed for a request whose engine does not resolve"
    );
}

#[test]
fn a_missing_engine_is_not_guessed_either() {
    let host = RecordingHostCheck::clearing();
    let mut values = TypedMap::new();
    values.insert(
        typed_key("sandbox_enabled"),
        jefe::domain::TypedValue::Bool(true),
    );
    let request = llxprt_request(values);

    assert_eq!(sandbox_preflight_engine(&request), None);
    let _ = launch_preflight_issue(&request, host.check());
    assert!(host.consulted_engines().is_empty());
}

#[test]
fn remote_launch_is_not_gated_on_local_host_sandbox_state() {
    let host = RecordingHostCheck::reporting(PreflightIssue::SshAgentNoIdentities);
    let request = AgentLaunchRequest {
        remote: RemoteRepositorySettings {
            enabled: true,
            host: "build-host".to_owned(),
            ..RemoteRepositorySettings::default()
        },
        ..llxprt_request(sandbox_values("podman"))
    };

    assert_eq!(sandbox_preflight_engine(&request), None);
    let _ = launch_preflight_issue(&request, host.check());
    assert!(
        host.consulted_engines().is_empty(),
        "local daemon and local SSH agent state does not describe the remote host"
    );
}

#[test]
fn unsupported_runtime_option_is_prompted_before_any_host_check() {
    let host = RecordingHostCheck::reporting(PreflightIssue::SshAgentNoIdentities);
    let request = AgentLaunchRequest {
        type_id: jefe::domain::agent_definition::AgentTypeId::parse("core.absent")
            .unwrap_or_else(|error| panic!("fixture type id must parse: {error}")),
        ..llxprt_request(sandbox_values("podman"))
    };

    let issue = launch_preflight_issue(&request, host.check());

    match issue {
        Some(PreflightIssue::UnsupportedRuntimeOption { diagnostic }) => assert!(
            diagnostic.contains("core.absent"),
            "diagnostic must name the rejected runtime option, got: {diagnostic}"
        ),
        other => panic!("expected an unsupported runtime option prompt, got {other:?}"),
    }
    assert!(
        host.consulted_engines().is_empty(),
        "an invalid launch request must be rejected before host inspection"
    );
}
