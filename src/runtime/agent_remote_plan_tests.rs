//! Unit tests for the remote launch planner and POSIX serializer
//! (issue #382 CW02-07 / S9).

use super::*;
use crate::domain::RemoteRepositorySettings;
use crate::domain::agent_definition::fields::{Emitter, Field, FieldKind, FieldValue};
use crate::domain::agent_definition::{
    AgentDefinition, Availability, Operation, Preflight, RemoteTarget, Support, Target,
};
use crate::runtime::agent_plan::LaunchFieldValues;

fn llxprt() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|d| d.display_name == "LLxprt")
        .unwrap_or_else(|| panic!("LLxprt shipped"))
}

fn codex() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|d| d.display_name == "Codex CLI")
        .unwrap_or_else(|| panic!("Codex shipped"))
}

fn compatible(generation: u64) -> Availability {
    Availability::InstalledCompatible {
        identity: "id".to_string(),
        capabilities: Vec::new(),
        generation,
    }
}

fn remote_settings() -> RemoteRepositorySettings {
    RemoteRepositorySettings {
        enabled: true,
        login_user: "dev".to_string(),
        host: "example.com".to_string(),
        port: Some(22),
        ..RemoteRepositorySettings::default()
    }
}

fn remote_target() -> RemoteTarget {
    RemoteTarget {
        user: "dev".to_string(),
        host: "example.com".to_string(),
        port: Some(22),
        run_as_user: String::new(),
        canonical_cwd: std::path::PathBuf::from("/srv/project"),
    }
}

fn make_request<'a>(
    definition: &'a AgentDefinition,
    values: &'a LaunchFieldValues,
    settings: &'a RemoteRepositorySettings,
    operation: Operation,
) -> RemotePlanRequest<'a> {
    RemotePlanRequest {
        definition,
        operation,
        target: Target::Remote(remote_target()),
        executable: std::path::PathBuf::from("/opt/bin/llxprt"),
        probe: compatible(1),
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values,
        preflight: Preflight::default(),
        ssh_settings: settings,
    }
}

/// Build a `RemotePlanRequest` with an explicit target and settings, for
/// tests that vary the remote identity.
fn make_remote_request<'a>(
    definition: &'a AgentDefinition,
    values: &'a LaunchFieldValues,
    settings: &'a RemoteRepositorySettings,
    target: RemoteTarget,
    operation: Operation,
) -> RemotePlanRequest<'a> {
    RemotePlanRequest {
        definition,
        operation,
        target: Target::Remote(target),
        executable: std::path::PathBuf::from("/opt/bin/llxprt"),
        probe: compatible(1),
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values,
        preflight: Preflight::default(),
        ssh_settings: settings,
    }
}

/// Extract the supported transcript from a remote plan outcome, panicking on
/// any other variant.
fn expect_transcript(outcome: RemotePlanOutcome) -> RemoteTranscript {
    match outcome {
        RemotePlanOutcome::Transcript(t) => *t,
        other => panic!("expected transcript, got {other:?}"),
    }
}

#[test]
fn serializer_wraps_simple_string() {
    assert_eq!(expect_quoted("hello"), "'hello'");
}

#[test]
fn serializer_preserves_empty_string() {
    assert_eq!(expect_quoted(""), "''");
}

#[test]
fn serializer_escapes_single_apostrophe() {
    assert_eq!(expect_quoted("it's"), "'it'\"'\"'s'");
}

#[test]
fn serializer_escapes_multiple_apostrophes() {
    assert_eq!(expect_quoted("a'b'c"), "'a'\"'\"'b'\"'\"'c'");
}

#[test]
fn serializer_escapes_apostrophe_only_string() {
    assert_eq!(expect_quoted("'"), "''\"'\"''");
}

#[test]
fn serializer_rejects_nul_byte() {
    assert_eq!(
        posix_single_quote("a\0b"),
        Err(RemoteSerializeError::NulByte)
    );
}

#[test]
fn serializer_handles_path_with_spaces() {
    assert_eq!(
        expect_quoted("/path with spaces/bin"),
        "'/path with spaces/bin'"
    );
}

#[test]
fn serializer_handles_unicode() {
    assert_eq!(expect_quoted("Ωlé"), "'Ωlé'");
}

#[test]
fn serializer_round_trips_through_split() {
    // Verify that splitting the quoted output on `'"'"'` and joining with `'`
    // reconstructs the original (the POSIX shell does exactly this).
    for input in &["hello", "", "it's", "a'b'c", "x''y", "can't won't"] {
        let quoted = expect_quoted_owned(input);
        let reconstructed = quoted.replace("'\"'\"'", "'");
        // Strip the outer enclosing single quotes.
        let reconstructed = reconstructed
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .unwrap_or(&reconstructed);
        assert_eq!(reconstructed, *input, "round-trip for {input:?}");
    }
}

/// POSIX-quote one input, returning a borrowed expectation via panic on error.
fn expect_quoted(input: &str) -> String {
    let Ok(quoted) = posix_single_quote(input) else {
        panic!("posix_single_quote({input:?}) must succeed");
    };
    quoted
}

/// POSIX-quote one input owning the result, panicking on error with context.
fn expect_quoted_owned(input: &str) -> String {
    match posix_single_quote(input) {
        Ok(quoted) => quoted,
        Err(error) => panic!("posix_single_quote({input:?}) must succeed: {error}"),
    }
}

// ---------------------------------------------------------------------------
// Remote planner tests
// ---------------------------------------------------------------------------

#[test]
fn llxprt_remote_normal_produces_golden_transcript() {
    let definition = llxprt();
    let mut values = LaunchFieldValues::new();
    values.set_repository("profile", FieldValue::String("dev".to_string()));
    values.set_agent("prompt_interactive", FieldValue::Boolean(true));
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    let transcript = match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(t) => *t,
        other => panic!("expected transcript, got {other:?}"),
    };
    assert_eq!(
        transcript.remote_command(),
        "cd '/srv/project' && exec '/opt/bin/llxprt' '--profile-load' 'dev' '--prompt-interactive'"
    );
    let argv: Vec<String> = transcript
        .agent_argv()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        argv,
        &[
            "--profile-load".to_string(),
            "dev".to_string(),
            "--prompt-interactive".to_string(),
        ]
    );
}

#[test]
fn remote_transcript_carries_ssh_arguments_through_boundary() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    let transcript = match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(t) => *t,
        other => panic!("expected transcript, got {other:?}"),
    };
    let ssh_args: Vec<String> = transcript
        .ssh_arguments()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(ssh_args.contains(&"-T".to_string()));
    assert!(ssh_args.contains(&"--".to_string()));
    assert!(ssh_args.contains(&"dev@example.com".to_string()));
    assert_eq!(
        ssh_args.last(),
        Some(&transcript.remote_command().to_string())
    );
}

#[test]
fn remote_plan_carries_remote_target() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    let transcript = match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(t) => *t,
        other => panic!("expected transcript, got {other:?}"),
    };
    assert!(
        !transcript.plan().target.is_local(),
        "plan target must be remote"
    );
    assert_eq!(
        transcript.plan().cwd,
        std::path::PathBuf::from("/srv/project")
    );
}

#[test]
fn unsupported_operation_returns_declared_reason_zero_ssh() {
    let definition = llxprt();
    // LLxprt supports all operations, so mutate to make Resume unsupported.
    let mut definition = definition.clone();
    definition.operations.resume.supported = Support::unsupported("resume not available");
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Resume);
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Unsupported { reason } => {
            assert_eq!(reason, "resume not available");
        }
        other => panic!("expected unsupported, got {other:?}"),
    }
}

#[test]
fn codex_remote_returns_unsupported() {
    let definition = codex();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let mut request = make_request(&definition, &values, &settings, Operation::Normal);
    request.executable = std::path::PathBuf::from("/opt/bin/codex");
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Unsupported { reason } => {
            assert!(
                definition.targets.remote.supported.is_unsupported(),
                "Codex definition declares remote unsupported"
            );
            assert!(!reason.is_empty(), "exact reason present");
        }
        other => panic!("Codex remote must be unsupported, got {other:?}"),
    }
    let _ = Support::unsupported("x"); // exercise the type
}

#[test]
fn local_target_is_rejected() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let request = RemotePlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: Target::Local {
            canonical_cwd: std::path::PathBuf::from("/srv"),
        },
        executable: std::path::PathBuf::from("/opt/bin/llxprt"),
        probe: compatible(1),
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values: &values,
        preflight: Preflight::default(),
        ssh_settings: &settings,
    };
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Error(RemotePlanError::NotRemoteTarget) => {}
        other => panic!("expected NotRemoteTarget, got {other:?}"),
    }
}

#[test]
fn disabled_ssh_settings_return_error() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let mut settings = remote_settings();
    settings.enabled = false;
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Error(RemotePlanError::InvalidSshSettings(_)) => {}
        other => panic!("expected InvalidSshSettings, got {other:?}"),
    }
}

#[test]
fn probe_generation_mismatch_returns_error() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let mut request = make_request(&definition, &values, &settings, Operation::Normal);
    request.probe_generation = 99;
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Error(RemotePlanError::Plan(
            AgentPlanError::ProbeGenerationMismatch { .. },
        )) => {}
        other => panic!("expected generation mismatch, got {other:?}"),
    }
}

#[test]
fn probe_not_found_returns_error() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let mut request = make_request(&definition, &values, &settings, Operation::Normal);
    request.probe = Availability::NotFound;
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Error(RemotePlanError::Plan(AgentPlanError::ProbeNotFound)) => {}
        other => panic!("expected ProbeNotFound, got {other:?}"),
    }
}

#[test]
fn apostrophe_in_value_is_serialized_through_serializer() {
    let definition = llxprt();
    let mut values = LaunchFieldValues::new();
    // Profile value containing an apostrophe exercises the serializer.
    values.set_repository("profile", FieldValue::String("it's".to_string()));
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    let transcript = match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(t) => *t,
        other => panic!("expected transcript, got {other:?}"),
    };
    // The remote command must contain the POSIX-escaped apostrophe.
    assert!(
        transcript.remote_command().contains("'it'\"'\"'s'"),
        "apostrophe serialized through POSIX escape: {}",
        transcript.remote_command()
    );
}

#[test]
fn invalid_ssh_option_returns_error() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let mut settings = remote_settings();
    settings.options = vec!["ProxyCommand=steal-secret".to_string()];
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Error(RemotePlanError::InvalidSshSettings(_)) => {}
        other => panic!("expected InvalidSshSettings, got {other:?}"),
    }
}

#[test]
fn remote_plan_stamps_signature_and_generations() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    let transcript = match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(t) => *t,
        other => panic!("expected transcript, got {other:?}"),
    };
    let plan = transcript.plan();
    assert_eq!(plan.type_id, definition.id);
    assert_eq!(plan.definition_sha256, definition.sha256());
    assert_eq!(plan.probe_generation, 1);
    assert_eq!(plan.target_generation, 1);
    assert!(plan.signature_excludes_secrets());
}

// ---------------------------------------------------------------------------
// Architectural-correction tests (S9)
// ---------------------------------------------------------------------------

#[test]
fn remote_target_identity_mismatch_rejects() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    // Target user diverges from the authorized SSH login_user.
    let mut mismatched = remote_target();
    mismatched.user = "attacker".to_string();
    let request = make_remote_request(
        &definition,
        &values,
        &settings,
        mismatched,
        Operation::Normal,
    );
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Error(RemotePlanError::InvalidSshSettings(_)) => {}
        other => panic!("target identity mismatch must reject, got {other:?}"),
    }
}

#[test]
fn remote_target_host_mismatch_rejects() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let mut mismatched = remote_target();
    mismatched.host = "evil.example".to_string();
    let request = make_remote_request(
        &definition,
        &values,
        &settings,
        mismatched,
        Operation::Normal,
    );
    assert!(matches!(
        plan_remote_launch(&request),
        RemotePlanOutcome::Error(RemotePlanError::InvalidSshSettings(_))
    ));
}

#[test]
fn remote_target_port_mismatch_rejects() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let mut mismatched = remote_target();
    mismatched.port = Some(2222);
    let request = make_remote_request(
        &definition,
        &values,
        &settings,
        mismatched,
        Operation::Normal,
    );
    assert!(matches!(
        plan_remote_launch(&request),
        RemotePlanOutcome::Error(RemotePlanError::InvalidSshSettings(_))
    ));
}

#[test]
fn remote_target_run_as_mismatch_rejects() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let mut settings = remote_settings();
    settings.run_as_user = "deploy".to_string();
    // Target run_as_user empty while settings require a privileged user.
    let target = remote_target();
    let request = make_remote_request(&definition, &values, &settings, target, Operation::Normal);
    assert!(matches!(
        plan_remote_launch(&request),
        RemotePlanOutcome::Error(RemotePlanError::InvalidSshSettings(_))
    ));
}

#[test]
fn plan_target_remains_remote_after_planning() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    let transcript = expect_transcript(plan_remote_launch(&request));
    let plan = transcript.plan();
    assert!(!plan.target.is_local(), "plan target must remain remote");
    let Target::Remote(remote) = &plan.target else {
        panic!(
            "plan target must be the Remote variant, got {:?}",
            plan.target
        );
    };
    assert_eq!(remote.user, "dev");
    assert_eq!(remote.host, "example.com");
    assert_eq!(remote.port, Some(22));
    assert_eq!(
        remote.canonical_cwd,
        std::path::PathBuf::from("/srv/project")
    );
}

#[test]
fn signature_differs_when_remote_user_changes() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let baseline = expect_transcript(plan_remote_launch(&make_request(
        &definition,
        &values,
        &settings,
        Operation::Normal,
    )));
    let mut other_user = remote_target();
    other_user.user = "ops".to_string();
    let mut other_settings = settings.clone();
    other_settings.login_user = "ops".to_string();
    let request = make_remote_request(
        &definition,
        &values,
        &other_settings,
        other_user,
        Operation::Normal,
    );
    let changed = expect_transcript(plan_remote_launch(&request));
    assert_eq!(
        baseline.plan().definition_sha256,
        changed.plan().definition_sha256,
        "definition digest is stable across target identity changes"
    );
    assert_ne!(
        baseline.plan().signature.target_fingerprint,
        changed.plan().signature.target_fingerprint,
        "target fingerprint must differ when remote user changes"
    );
}

#[test]
fn signature_differs_when_remote_host_changes() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let baseline = expect_transcript(plan_remote_launch(&make_request(
        &definition,
        &values,
        &settings,
        Operation::Normal,
    )));
    let mut other = remote_target();
    other.host = "staging.example".to_string();
    let mut other_settings = settings.clone();
    other_settings.host = "staging.example".to_string();
    let request = make_remote_request(
        &definition,
        &values,
        &other_settings,
        other,
        Operation::Normal,
    );
    let changed = expect_transcript(plan_remote_launch(&request));
    assert_ne!(
        baseline.plan().signature.target_fingerprint,
        changed.plan().signature.target_fingerprint,
        "target fingerprint must differ when remote host changes"
    );
}

#[test]
fn signature_differs_when_remote_port_changes() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let baseline = expect_transcript(plan_remote_launch(&make_request(
        &definition,
        &values,
        &settings,
        Operation::Normal,
    )));
    let mut other = remote_target();
    other.port = Some(2222);
    let mut other_settings = settings.clone();
    other_settings.port = Some(2222);
    let request = make_remote_request(
        &definition,
        &values,
        &other_settings,
        other,
        Operation::Normal,
    );
    let changed = expect_transcript(plan_remote_launch(&request));
    assert_ne!(
        baseline.plan().signature.target_fingerprint,
        changed.plan().signature.target_fingerprint,
        "target fingerprint must differ when remote port changes"
    );
}

#[test]
fn signature_differs_when_remote_run_as_changes() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let mut settings = remote_settings();
    settings.run_as_user = "deploy".to_string();
    let mut target = remote_target();
    target.run_as_user = "deploy".to_string();
    let baseline = expect_transcript(plan_remote_launch(&make_remote_request(
        &definition,
        &values,
        &settings,
        target.clone(),
        Operation::Normal,
    )));
    let mut other_target = target.clone();
    other_target.run_as_user = "root".to_string();
    let mut other_settings = settings.clone();
    other_settings.run_as_user = "root".to_string();
    let request = make_remote_request(
        &definition,
        &values,
        &other_settings,
        other_target,
        Operation::Normal,
    );
    let changed = expect_transcript(plan_remote_launch(&request));
    assert_ne!(
        baseline.plan().signature.target_fingerprint,
        changed.plan().signature.target_fingerprint,
        "target fingerprint must differ when remote run_as_user changes"
    );
}

#[test]
fn signature_differs_when_remote_cwd_changes() {
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let baseline = expect_transcript(plan_remote_launch(&make_request(
        &definition,
        &values,
        &settings,
        Operation::Normal,
    )));
    let mut other = remote_target();
    other.canonical_cwd = std::path::PathBuf::from("/srv/other");
    let request = make_remote_request(&definition, &values, &settings, other, Operation::Normal);
    let changed = expect_transcript(plan_remote_launch(&request));
    assert_ne!(
        baseline.plan().signature.target_fingerprint,
        changed.plan().signature.target_fingerprint,
        "target fingerprint must differ when remote cwd changes"
    );
}

/// Build a minimal definition carrying one environment emitter for a string
/// field, so the env serialization path can be exercised in isolation.
fn definition_with_env_emitter() -> AgentDefinition {
    let mut definition = llxprt();
    definition.agent_fields = vec![Field {
        id: "api_key".to_string(),
        kind: FieldKind::String,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: Vec::new(),
        visible_when: None,
        launch_signature: true,
    }];
    definition.emitters = vec![Emitter::Environment {
        name: "API_KEY".to_string(),
        field: "api_key".to_string(),
    }];
    definition
}

#[test]
fn typed_environment_emitter_serialized_via_env_prefix() {
    let definition = definition_with_env_emitter();
    let mut values = LaunchFieldValues::new();
    values.set_agent("api_key", FieldValue::String("s3cr3t".to_string()));
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    let transcript = expect_transcript(plan_remote_launch(&request));
    // The env pair must be serialized as `env 'NAME=VALUE'` before the exec.
    assert!(
        transcript
            .remote_command()
            .contains(" env 'API_KEY=s3cr3t' "),
        "typed env emitter serialized via env 'NAME=VALUE': {}",
        transcript.remote_command()
    );
    // The plan's env carries the typed (name, value) pair.
    let env: Vec<(String, String)> = transcript
        .plan()
        .env
        .iter()
        .map(|(n, v)| {
            (
                n.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect();
    assert_eq!(env, &[("API_KEY".to_string(), "s3cr3t".to_string())]);
}

#[test]
fn environment_emitter_value_apostrophe_is_posix_quoted() {
    let definition = definition_with_env_emitter();
    let mut values = LaunchFieldValues::new();
    values.set_agent("api_key", FieldValue::String("it's".to_string()));
    let settings = remote_settings();
    let request = make_request(&definition, &values, &settings, Operation::Normal);
    let transcript = expect_transcript(plan_remote_launch(&request));
    assert!(
        transcript
            .remote_command()
            .contains(" env 'API_KEY=it'\"'\"'s' "),
        "env value apostrophe POSIX-quoted: {}",
        transcript.remote_command()
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_os_string_target_rejects_without_lossy_conversion() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStrExt;
    // A non-UTF-8 executable cannot be serialized into a POSIX command and
    // must be rejected with the typed NonUtf8 error rather than lossily
    // converted.
    let invalid_bytes: &[u8] = &[0xFF, 0xFE, 0xFD];
    let invalid = std::ffi::OsStr::from_bytes(invalid_bytes);
    assert!(
        invalid.to_str().is_none(),
        "fixture bytes must be non-UTF-8"
    );
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let mut request = make_request(&definition, &values, &settings, Operation::Normal);
    request.executable = std::path::PathBuf::from(OsString::from(invalid));
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Error(RemotePlanError::Serialize(RemoteSerializeError::NonUtf8)) => {}
        other => panic!("non-UTF8 executable must reject, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_os_string_cwd_rejects_without_lossy_conversion() {
    use std::os::unix::ffi::OsStrExt;
    let definition = llxprt();
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let invalid_bytes: &[u8] = &[0xFF, 0xFE, 0xFD];
    let mut target = remote_target();
    target.canonical_cwd = std::path::PathBuf::from(std::ffi::OsStr::from_bytes(invalid_bytes));
    let request = make_remote_request(&definition, &values, &settings, target, Operation::Normal);
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Error(RemotePlanError::Serialize(RemoteSerializeError::NonUtf8)) => {}
        other => panic!("non-UTF8 cwd must reject, got {other:?}"),
    }
}
