//! Issue #382 CW02-07 remote-plan golden helpers.
//!
//! Extracted from `tests/issue382_behavior.rs` so that file stays inside the
//! source-file-length policy. These are fixture helpers only: the `#[test]`
//! entry points remain in the acceptance-test file.

use jefe::agent_registry::AgentTypeRegistry;
use jefe::domain::agent_definition::{
    AgentDefinition, Availability, Operation, Preflight, RemoteTarget, Target,
};
use jefe::runtime::agent_remote_plan::{
    self, RemotePlanOutcome, RemoteSerializeError, plan_remote_launch, posix_single_quote,
};

/// Build a compatible probe availability for a definition.
pub fn probe_compatible(definition: &AgentDefinition, generation: u64) -> Availability {
    let capabilities: Vec<String> = definition
        .probe
        .capabilities
        .as_ref()
        .map(|probe| probe.tokens.iter().map(|token| token.id.clone()).collect())
        .unwrap_or_default();
    Availability::InstalledCompatible {
        identity: "fixture-identity".to_string(),
        capabilities,
        generation,
    }
}

/// Build a `RemotePlanRequest` for the given definition and remote target.
pub fn remote_request<'a>(
    definition: &'a AgentDefinition,
    executable: &str,
    remote_target: &RemoteTarget,
    ssh_settings: &'a jefe::domain::RemoteRepositorySettings,
    values: &'a jefe::runtime::agent_plan::LaunchFieldValues,
) -> agent_remote_plan::RemotePlanRequest<'a> {
    agent_remote_plan::RemotePlanRequest {
        definition,
        operation: Operation::Normal,
        target: Target::Remote(remote_target.clone()),
        executable: std::path::PathBuf::from(executable),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            std::path::PathBuf::from(executable),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: probe_compatible(definition, 1),
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values,
        preflight: Preflight::default(),
        ssh_settings,
    }
}

/// The authorized SSH settings and matching remote target used by the
/// golden remote-plan assertions.
pub fn golden_remote_identity() -> (jefe::domain::RemoteRepositorySettings, RemoteTarget) {
    let ssh_settings = jefe::domain::RemoteRepositorySettings {
        enabled: true,
        login_user: "dev".to_string(),
        host: "example.com".to_string(),
        port: Some(22),
        ..jefe::domain::RemoteRepositorySettings::default()
    };
    let remote_target = RemoteTarget {
        user: "dev".to_string(),
        host: "example.com".to_string(),
        port: Some(22),
        run_as_user: String::new(),
        canonical_cwd: std::path::PathBuf::from("/srv/project"),
    };
    (ssh_settings, remote_target)
}

/// POSIX-quote one input, panicking on error with context.
fn expect_quoted(input: &str) -> String {
    match posix_single_quote(input) {
        Ok(quoted) => quoted,
        Err(error) => panic!("posix_single_quote({input:?}) must succeed: {error}"),
    }
}

/// Assert the POSIX single-quote serializer contract (golden quoting).
pub fn assert_serializer_contract() {
    // Each string enclosed in single quotes.
    assert_eq!(expect_quoted("hello"), "'hello'");
    // Empty string preserved as two single quotes.
    assert_eq!(expect_quoted(""), "''");
    // Embedded apostrophe: '"'"' sequence between quoted portions.
    assert_eq!(expect_quoted("it's"), "'it'\"'\"'s'");
    assert_eq!(expect_quoted("a'b'c"), "'a'\"'\"'b'\"'\"'c'");
    // Apostrophe-only string.
    assert_eq!(expect_quoted("'"), "''\"'\"''");
    // NUL byte rejected with typed error.
    assert_eq!(
        posix_single_quote("a\0b"),
        Err(RemoteSerializeError::NulByte)
    );
}

/// Assert one unsupported remote plan returns the exact reason with zero SSH.
pub fn assert_remote_unsupported(
    definition: &AgentDefinition,
    executable: &str,
    remote_target: &RemoteTarget,
    ssh_settings: &jefe::domain::RemoteRepositorySettings,
) {
    let values = jefe::runtime::agent_plan::LaunchFieldValues::new();
    let request = remote_request(definition, executable, remote_target, ssh_settings, &values);
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Unsupported { reason } => {
            assert!(
                reason.contains("not fixture-verified"),
                "{} remote reason must be exact: {reason}",
                definition.display_name
            );
        }
        other => panic!(
            "{} remote must be unsupported, got {other:?}",
            definition.display_name
        ),
    }
}

/// Find a shipped definition by stable id, panicking if absent.
pub fn find_shipped<'a>(registry: &'a AgentTypeRegistry, id: &str) -> &'a AgentDefinition {
    registry
        .definitions()
        .iter()
        .find(|d| d.id.as_str() == id)
        .unwrap_or_else(|| panic!("{id} shipped"))
}

/// Assert the LLxprt remote Normal plan produces the fixture-golden
/// transcript (remote command, agent argv, and SSH arguments).
pub fn assert_llxprt_remote_golden() {
    let registry =
        AgentTypeRegistry::shipped().unwrap_or_else(|error| panic!("shipped registry: {error}"));
    let llxprt = find_shipped(&registry, "core.llxprt");
    let (ssh_settings, remote_target) = golden_remote_identity();
    let mut values = jefe::runtime::agent_plan::LaunchFieldValues::new();
    values.set_repository(
        "profile",
        jefe::domain::agent_definition::FieldValue::String("dev".to_string()),
    );
    values.set_agent(
        "continue",
        jefe::domain::agent_definition::FieldValue::Boolean(true),
    );
    let request = remote_request(
        llxprt,
        "/opt/bin/llxprt",
        &remote_target,
        &ssh_settings,
        &values,
    );
    let transcript = match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(t) => t,
        other => panic!("LLxprt remote Normal must be supported, got {other:?}"),
    };
    assert_eq!(
        transcript.remote_command(),
        "cd '/srv/project' && exec '/opt/bin/llxprt' '--profile-load' 'dev' '--yolo' '--prompt-interactive' '--continue'"
    );
    let agent_argv: Vec<String> = transcript
        .agent_argv()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        agent_argv,
        &[
            "--profile-load".to_string(),
            "dev".to_string(),
            "--yolo".to_string(),
            "--prompt-interactive".to_string(),
            "--continue".to_string(),
        ]
    );
    assert_golden_ssh_arguments(&transcript);
}

/// Assert the SSH arguments carry the audited boundary's non-interactive
/// mode, target identity, port, and trailing remote command.
fn assert_golden_ssh_arguments(transcript: &agent_remote_plan::RemoteTranscript) {
    let ssh_args: Vec<String> = transcript
        .ssh_arguments()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(ssh_args.contains(&"-T".to_string()), "non-interactive mode");
    assert!(ssh_args.contains(&"--".to_string()), "argument separator");
    assert!(
        ssh_args.contains(&"dev@example.com".to_string()),
        "remote target user@host"
    );
    assert!(
        ssh_args.contains(&"-p".to_string()) && ssh_args.contains(&"22".to_string()),
        "port option"
    );
    assert_eq!(
        ssh_args.last(),
        Some(&transcript.remote_command().to_string()),
        "remote command is the last SSH argument"
    );
}
