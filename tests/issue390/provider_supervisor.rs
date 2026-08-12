//! Focused integration tests for the one-shot provider supervisor
//! (issue #390 CW-10, Slice C1).
//!
//! These drive the cross-platform `jefe-provider-fixture` binary through the
//! real supervisor to prove the CW10-02 lifecycle transcript, the CW10-11
//! staged shutdown/reap, and the CW10-14 secret redaction. CW10-06's outbound
//! queue bound is proven by its focused unit test in `outbound.rs`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use jefe::domain::action_registry::ActionId;
use jefe::domain::{CanonicalSemver, Id, TypedMap};
use jefe::runtime::provider::environment::{EnvironmentError, HostEnv, ProviderEnvironment};
use jefe::runtime::provider::protocol::{
    ConfigurePayload, EnvName, InvokeActionPayload, InvokeContext, MessageKind, RequestId,
};
use jefe::runtime::provider::supervisor::{
    CleanupFailure, OneShotOutcome, OneShotRequest, OneShotResult, SupervisorBounds,
    SupervisorFailure, TranscriptEntry, run_one_shot, run_one_shot_streaming,
};

/// A distinctive secret canary used across the redaction tests.
const SECRET: &str = "SUPER-secret-canary-390";

fn join_or_resume<T>(handle: std::thread::JoinHandle<T>) -> T {
    match handle.join() {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-provider-fixture");

/// Bounds tuned for fast, deterministic tests while preserving the staged order.
fn fast_bounds() -> SupervisorBounds {
    SupervisorBounds {
        handshake: Duration::from_secs(3),
        invocation: Duration::from_secs(5),
        shutdown_ack: Duration::from_millis(600),
        stdin_close: Duration::from_millis(600),
        final_drain: Duration::from_millis(600),
    }
}

/// A deterministic host environment resolver (never touches the real process env).
struct FixedEnv {
    vars: Vec<(String, String)>,
}

impl FixedEnv {
    fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            vars: pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        }
    }
}

impl HostEnv for FixedEnv {
    fn get(&self, name: &str) -> Option<String> {
        self.vars
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

struct Scene {
    home: tempfile::TempDir,
    provider_dir: PathBuf,
}

impl Scene {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap_or_else(|error| panic!("home tempdir: {error:?}"));
        let provider_dir = home.path().join("bin");
        std::fs::create_dir_all(&provider_dir)
            .unwrap_or_else(|error| panic!("create provider dir: {error:?}"));
        Self { home, provider_dir }
    }

    fn request(&self, mode: &str, env: ProviderEnvironment) -> OneShotRequest {
        self.request_with(mode, env, Vec::new())
    }

    fn request_with(
        &self,
        mode: &str,
        env: ProviderEnvironment,
        extra_args: Vec<String>,
    ) -> OneShotRequest {
        let mut arguments = vec![mode.to_owned()];
        arguments.extend(extra_args);
        OneShotRequest {
            binary: PathBuf::from(FIXTURE),
            arguments,
            working_dir: self.provider_dir.clone(),
            environment: env,
            home: self.home.path().join("home"),
            tmpdir: self.home.path().join("tmp"),
            locale: "C".to_owned(),
            host_api: "jefe/test".to_owned(),
            plugin_id: Id::parse("vendor.pkg")
                .unwrap_or_else(|error| panic!("plugin id: {error:?}")),
            plugin_version: CanonicalSemver::parse("1.0.0")
                .unwrap_or_else(|error| panic!("version: {error:?}")),
            generation: 1,
            request_id: RequestId::parse("h-000001")
                .unwrap_or_else(|error| panic!("request id: {error:?}")),
            configure: ConfigurePayload {
                config_version: 1,
                config: TypedMap::new(),
                secrets: BTreeMap::new(),
                environment: BTreeMap::new(),
            },
            invocation: InvokeActionPayload {
                invocation_id: Id::parse("vendor.pkg.inv")
                    .unwrap_or_else(|error| panic!("invocation id: {error:?}")),
                action_id: ActionId::parse("vendor.pkg.run")
                    .unwrap_or_else(|error| panic!("action id: {error:?}")),
                arguments: TypedMap::new(),
                context: InvokeContext {
                    screen_id: Id::parse("vendor.pkg.screen")
                        .unwrap_or_else(|error| panic!("screen id: {error:?}")),
                    screen_instance: Id::parse("vendor.pkg.inst")
                        .unwrap_or_else(|error| panic!("instance id: {error:?}")),
                    resource_refs: TypedMap::new(),
                },
                continuation: None,
            },
        }
    }
}

fn env_name(value: &str) -> EnvName {
    EnvName::parse(value).unwrap_or_else(|error| panic!("valid env name: {error:?}"))
}

fn base_env(provider_dir: PathBuf) -> ProviderEnvironment {
    ProviderEnvironment {
        provider_dir,
        nonsecret: BTreeMap::new(),
        secret_env: BTreeMap::new(),
        configure_secret_sources: BTreeMap::new(),
    }
}

#[test]
fn cw10_02_happy_one_shot_lifecycle_has_exact_transcript_and_is_reaped() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("happy", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));

    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed outcome, got {:?}",
        result.outcome
    );
    let transcript = result.transcript.entries();
    // hello, hello-ack, configure, ready, invoke-action, 3x progress, outcome,
    // shutdown, shutdown-ack, EOF, reap.
    let sent: Vec<_> = transcript
        .iter()
        .filter(|e| matches!(e, TranscriptEntry::Sent(_)))
        .map(|e| format!("{e:?}"))
        .collect();
    let expected_sent = [
        "Sent(Hello)",
        "Sent(Configure)",
        "Sent(InvokeAction)",
        "Sent(Shutdown)",
    ];
    assert_eq!(
        sent.iter().map(String::as_str).collect::<Vec<_>>(),
        expected_sent,
        "host sends in exact order"
    );
    let progress_count = transcript
        .iter()
        .filter(|e| matches!(e, TranscriptEntry::Progress(_)))
        .count();
    assert_eq!(progress_count, 3, "three progress events");
    let progresses: Vec<u16> = transcript
        .iter()
        .filter_map(|e| match e {
            TranscriptEntry::Progress(seq) => Some(*seq),
            _ => None,
        })
        .collect();
    assert_eq!(progresses, vec![1, 2, 3], "progress is monotonic from 1");
    assert!(transcript.ends_with(&[
        TranscriptEntry::Received(MessageKind::Outcome),
        TranscriptEntry::Sent(MessageKind::Shutdown),
        TranscriptEntry::Received(MessageKind::ShutdownAck),
        TranscriptEntry::Eof,
        TranscriptEntry::Reaped,
    ]));
    assert!(result.process_reaped, "process was reaped");
}

#[test]
fn cw10_02_progress_emits_up_to_256() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("progress-256", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    assert!(matches!(result.outcome, OneShotOutcome::Completed(_)));
    let max_seq = result
        .transcript
        .entries()
        .iter()
        .filter_map(|e| match e {
            TranscriptEntry::Progress(seq) => Some(*seq),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    assert_eq!(max_seq, 256, "progress reaches the 256 maximum");
    assert!(result.process_reaped);
}

#[test]
fn cw10_02_provider_error_is_a_typed_terminal() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("error", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    match result.outcome {
        OneShotOutcome::ProviderError(payload) => {
            assert_eq!(payload.code, "PLG-EX");
        }
        other => panic!("expected provider error, got {other:?}"),
    }
    assert!(result.process_reaped, "error path is reaped");
}

#[test]
fn cw10_02_never_ready_fails_handshake_and_is_reaped() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("never-ready", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    assert!(
        matches!(
            result.outcome,
            OneShotOutcome::Failed(SupervisorFailure::HandshakeTimeout)
        ),
        "expected handshake timeout, got {:?}",
        result.outcome
    );
    assert!(result.process_reaped, "hung provider is reaped on stage B");
}

#[test]
fn cw10_02_crash_after_ready_is_reaped_as_a_crash() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("crash-after-ready", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    assert!(
        matches!(
            result.outcome,
            OneShotOutcome::Failed(SupervisorFailure::Crashed { .. })
        ),
        "expected crash, got {:?}",
        result.outcome
    );
    assert!(result.process_reaped);
}

#[test]
fn cw10_02_generation_drift_is_a_protocol_failure() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("generation-drift", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    assert!(
        matches!(
            result.outcome,
            OneShotOutcome::Failed(SupervisorFailure::Protocol(_))
        ),
        "expected protocol failure, got {:?}",
        result.outcome
    );
    assert!(result.process_reaped);
}

#[test]
fn cw10_11_hang_after_shutdown_is_reaped_by_staged_escalation() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("hang-shutdown", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    // The provider reached a terminal outcome before hanging on shutdown.
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed outcome, got {:?}",
        result.outcome
    );
    assert!(
        result.process_reaped,
        "staged escalation reaps the hung provider"
    );
}

#[test]
fn cw10_09_duplicate_terminal_keeps_the_first_result() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("duplicate-terminal", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    // The first outcome wins; the second is a post-terminal fault that cannot
    // replace it.
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed outcome, got {:?}",
        result.outcome
    );
    assert!(result.process_reaped);
}

#[test]
fn cw10_14_a_secret_echoed_to_stderr_is_redacted() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let mut env = base_env(scene.provider_dir.clone());
    // A Configure secret sourced from the host; the fixture echoes it to stderr.
    env.configure_secret_sources
        .insert(env_name("DEPLOY_KEY"), env_name("HOST_DEPLOY_KEY"));
    let request = scene.request("secret-stderr", env);
    let result = run_one_shot(
        &request,
        &fast_bounds(),
        &FixedEnv::from_pairs(&[("HOST_DEPLOY_KEY", "TOPSECRET-canary-value")]),
    );
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed outcome, got {:?}",
        result.outcome
    );
    assert!(
        !result.retained_stderr.contains("TOPSECRET-canary-value"),
        "secret leaked into retained stderr: <redacted by test>"
    );
    assert!(result.process_reaped);
}

#[test]
fn cw10_14_environment_is_isolated_and_configure_secret_stays_in_configure() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let record_dir = scene.home.path().join("observations");
    let mut env = base_env(scene.provider_dir.clone());
    env.nonsecret
        .insert(env_name("PLUGIN_MODE"), "strict".to_owned());
    env.configure_secret_sources
        .insert(env_name("DEPLOY_KEY"), env_name("HOST_DEPLOY_KEY"));
    let request = scene.request_with(
        "record",
        env,
        vec![record_dir.to_string_lossy().into_owned()],
    );
    let secret = "TOPSECRET-never-in-env";
    let result = run_one_shot(
        &request,
        &fast_bounds(),
        &FixedEnv::from_pairs(&[("HOST_DEPLOY_KEY", secret)]),
    );
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed outcome, got {:?}",
        result.outcome
    );

    let env_text = std::fs::read_to_string(record_dir.join("env.txt"))
        .unwrap_or_else(|error| panic!("env observations: {error:?}"));
    assert!(
        !env_text.contains(secret),
        "Configure secret leaked into the provider process environment"
    );
    // The five fixed names plus the declared nonsecret binding are present.
    assert!(env_text.contains("HOME="));
    assert!(env_text.contains("PATH="));
    assert!(env_text.contains("TMPDIR="));
    assert!(env_text.contains("LC_ALL="));
    assert!(env_text.contains("LANG="));
    assert!(env_text.contains("PLUGIN_MODE=strict"));

    let configure = std::fs::read_to_string(record_dir.join("configure.json"))
        .unwrap_or_else(|error| panic!("configure: {error:?}"));
    assert!(
        configure.contains(secret),
        "the owning Configure received the secret (asserted without printing)"
    );
    // No undeclared host variable leaks through.
    assert!(!env_text.contains("HOST_DEPLOY_KEY="));
}

/// Build an environment that sources one Configure secret from the host.
fn configure_secret_env(provider_dir: PathBuf) -> ProviderEnvironment {
    let mut env = base_env(provider_dir);
    env.configure_secret_sources
        .insert(env_name("DEPLOY_KEY"), env_name("HOST_DEPLOY_KEY"));
    env
}

/// Run a secret-echo scenario and assert the secret never reaches an
/// operator-visible surface: retained stderr, the outcome `Debug` render, the
/// transcript, or a cleanup failure.
fn assert_secret_redacted_across_surfaces(result: &OneShotResult, label: &str) {
    assert!(
        !result.retained_stderr.contains(SECRET),
        "{label}: secret leaked into retained stderr"
    );
    assert!(
        !format!("{:?}", result.outcome).contains(SECRET),
        "{label}: secret leaked into the outcome Debug render"
    );
    assert!(
        !format!("{:?}", result.transcript).contains(SECRET),
        "{label}: secret leaked into the transcript"
    );
    if let Some(failure) = &result.cleanup_failure {
        assert!(
            !format!("{failure:?}").contains(SECRET),
            "{label}: secret leaked into the cleanup failure"
        );
    }
}

fn run_secret_scenario(scene: &Scene, mode: &str) -> OneShotResult {
    let request = scene.request(mode, configure_secret_env(scene.provider_dir.clone()));
    run_one_shot(
        &request,
        &fast_bounds(),
        &FixedEnv::from_pairs(&[("HOST_DEPLOY_KEY", SECRET)]),
    )
}

// ---- CW10-14: recursive redaction across every provider-owned surface (defect 6)

#[test]
fn cw10_14_a_secret_echoed_in_a_navigate_activation_is_redacted() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let result = run_secret_scenario(&scene, "secret-navigate");
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed outcome, got {:?}",
        result.outcome
    );
    assert_secret_redacted_across_surfaces(&result, "navigate activation");
    assert!(result.process_reaped);
}

#[test]
fn cw10_14_a_secret_echoed_in_a_refresh_resource_ref_is_redacted() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let result = run_secret_scenario(&scene, "secret-refresh");
    assert!(matches!(result.outcome, OneShotOutcome::Completed(_)));
    assert_secret_redacted_across_surfaces(&result, "refresh resource_ref");
    assert!(result.process_reaped);
}

#[test]
fn cw10_14_a_secret_echoed_in_a_legacy_replace_panel_outcome_is_rejected_and_redacted() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let result = run_secret_scenario(&scene, "secret-panel");
    assert!(matches!(
        result.outcome,
        OneShotOutcome::Failed(SupervisorFailure::Protocol(_))
    ));
    assert_secret_redacted_across_surfaces(&result, "legacy replace-panel outcome");
    assert!(result.process_reaped);
}

#[test]
fn cw10_14_a_secret_echoed_in_a_legacy_migrated_config_outcome_is_rejected_and_redacted() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let result = run_secret_scenario(&scene, "secret-migrated");
    assert!(matches!(
        result.outcome,
        OneShotOutcome::Failed(SupervisorFailure::Protocol(_))
    ));
    assert_secret_redacted_across_surfaces(&result, "legacy migrated-config outcome");
    assert!(result.process_reaped);
}

#[test]
fn cw10_14_a_secret_echoed_in_a_confirmation_surface_is_redacted() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let result = run_secret_scenario(&scene, "secret-confirm");
    assert!(matches!(result.outcome, OneShotOutcome::Completed(_)));
    assert_secret_redacted_across_surfaces(&result, "request-host-confirmation");
    assert!(result.process_reaped);
}

#[test]
fn cw10_14_a_secret_echoed_in_an_error_payload_is_redacted() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let result = run_secret_scenario(&scene, "secret-error");
    match &result.outcome {
        OneShotOutcome::ProviderError(payload) => {
            // The stable code is structural and never the secret.
            assert_eq!(payload.code, "PLG-EX");
        }
        other => panic!("expected provider error, got {other:?}"),
    }
    assert_secret_redacted_across_surfaces(&result, "error payload");
    assert!(result.process_reaped);
}

// ---- CW10-14: caller-supplied Configure secrets are rejected (defect 7)

#[test]
fn cw10_14_a_caller_supplied_configure_secret_is_rejected_before_spawn() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let env = base_env(scene.provider_dir.clone());
    let mut request = scene.request("happy", env);
    request
        .configure
        .secrets
        .insert(env_name("CALLER_KEY"), "caller-injected-value".to_owned());
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    match &result.outcome {
        OneShotOutcome::Failed(SupervisorFailure::Environment(
            EnvironmentError::UndeclaredConfigureSecret { binding },
        )) => {
            assert_eq!(
                binding, "CALLER_KEY",
                "the undeclared binding is named without its value"
            );
        }
        other => panic!("expected undeclared configure secret, got {other:?}"),
    }
    assert!(
        !result.process_reaped,
        "no process was spawned for a rejected request"
    );
    assert!(
        result.retained_stderr.is_empty(),
        "no stderr was captured before spawn"
    );
}

// ---- CW10-11: bounded cleanup with a lingering descendant (defect 3)

#[test]
fn cw10_11_a_descendant_holding_pipes_is_reaped_within_the_bound() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("descendant-hang", base_env(scene.provider_dir.clone()));
    let bounds = fast_bounds();
    // Two `final_drain` terms, not one: with a descendant holding the inherited
    // pipes, both the final stdout drain and the retained-stderr collection run
    // to their own bound in sequence.
    let worst_case = bounds.handshake
        + bounds.handshake
        + bounds.invocation
        + bounds.shutdown_ack
        + bounds.stdin_close
        + bounds.final_drain
        + bounds.final_drain;
    let start = Instant::now();
    let result = run_one_shot(&request, &bounds, &FixedEnv::from_pairs(&[]));
    let elapsed = start.elapsed();
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed outcome, got {:?}",
        result.outcome
    );
    assert!(
        result.process_reaped,
        "the leader was reaped despite the lingering descendant"
    );
    // The provider never acknowledged shutdown, so the lifecycle failure is
    // visible as a cleanup failure without replacing the terminal outcome.
    assert!(
        matches!(result.cleanup_failure, Some(CleanupFailure::ShutdownAck(_))),
        "expected a shutdown-ack cleanup failure, got {:?}",
        result.cleanup_failure
    );
    assert!(
        elapsed <= worst_case,
        "run_one_shot exceeded the aggregate bound: {elapsed:?} > {worst_case:?}"
    );
}

// ---- CW10-11: strict shutdown-ack lifecycle validation (defect 4)

#[test]
fn cw10_11_an_ack_with_the_wrong_kind_is_a_visible_cleanup_failure() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("ack-wrong-kind", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "the first terminal remains authoritative"
    );
    assert!(
        matches!(result.cleanup_failure, Some(CleanupFailure::ShutdownAck(_))),
        "a wrong-kind ack is a lifecycle failure, got {:?}",
        result.cleanup_failure
    );
    assert!(result.process_reaped);
}

#[test]
fn cw10_11_a_missing_ack_is_a_visible_cleanup_failure() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("ack-missing", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    assert!(matches!(result.outcome, OneShotOutcome::Completed(_)));
    assert!(
        matches!(result.cleanup_failure, Some(CleanupFailure::ShutdownAck(_))),
        "a missing ack is a lifecycle failure, got {:?}",
        result.cleanup_failure
    );
    assert!(result.process_reaped);
}

#[test]
fn cw10_11_eof_before_ack_is_a_visible_cleanup_failure() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("ack-eof-before", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    assert!(matches!(result.outcome, OneShotOutcome::Completed(_)));
    assert!(
        matches!(result.cleanup_failure, Some(CleanupFailure::ShutdownAck(_))),
        "EOF before ack is a lifecycle failure, got {:?}",
        result.cleanup_failure
    );
    assert!(result.process_reaped);
}

#[test]
fn cw10_11_data_after_ack_is_a_visible_cleanup_failure() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("ack-data-after", base_env(scene.provider_dir.clone()));
    let result = run_one_shot(&request, &fast_bounds(), &FixedEnv::from_pairs(&[]));
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "the first terminal remains authoritative"
    );
    assert!(
        matches!(result.cleanup_failure, Some(CleanupFailure::ShutdownAck(_))),
        "data after ack is a lifecycle failure, got {:?}",
        result.cleanup_failure
    );
    assert!(result.process_reaped);
}

// ---------------------------------------------------------------------------
// S16/S17: live one-shot progress/cancel delivery (remediation slice E)
// ---------------------------------------------------------------------------

/// Collect each live progress payload's sequence from `rx` until `count`
/// arrive, failing if the channel closes or the deadline passes first.
fn recv_progress(
    rx: &mpsc::Receiver<jefe::runtime::provider::protocol::ProgressPayload>,
    count: usize,
) -> Vec<u16> {
    let mut seqs = Vec::new();
    while seqs.len() < count {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(payload) => seqs.push(payload.sequence),
            Err(error) => panic!("expected {count} live progress frames, got {seqs:?}: {error}"),
        }
    }
    seqs
}

#[test]
fn s16_streaming_delivers_each_progress_payload_live_before_terminal() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("happy", base_env(scene.provider_dir.clone()));
    let (tx, rx) = mpsc::channel();
    // Run the streaming lifecycle on a dedicated thread so progress is observed
    // on the channel *while the invocation is still live*, before the terminal
    // result is produced. The blocking entry point would only reveal progress
    // after completion (S10's old defect).
    let handle = thread::spawn(move || {
        run_one_shot_streaming(
            &request,
            &fast_bounds(),
            &FixedEnv::from_pairs(&[]),
            Some(&tx),
            None,
        )
    });
    let seqs = recv_progress(&rx, 3);
    assert_eq!(
        seqs,
        vec![1, 2, 3],
        "each progress payload delivered live, in order"
    );
    let result = join_or_resume(handle);
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "terminal outcome preserved: {:?}",
        result.outcome
    );
    assert!(result.process_reaped);
}

#[test]
fn s17_cancel_is_observed_promptly_for_a_live_silent_invocation() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("progress-then-hang", base_env(scene.provider_dir.clone()));
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = cancel.clone();
    // A long invocation deadline: a silent provider must not hide a host cancel
    // behind it. The driver re-checks the cancel flag between bounded reads, so
    // cancel is observed within the poll slice rather than after 30 s. Against
    // the pre-fix code (one full-deadline read) this would take ~30 s.
    let bounds = SupervisorBounds {
        invocation: Duration::from_secs(30),
        ..fast_bounds()
    };
    let handle = thread::spawn(move || {
        run_one_shot_streaming(
            &request,
            &bounds,
            &FixedEnv::from_pairs(&[]),
            Some(&tx),
            Some(&cancel_for_thread),
        )
    });
    // Prove the invocation is live: its progress frames arrive on the channel.
    let seqs = recv_progress(&rx, 3);
    assert_eq!(
        seqs,
        vec![1, 2, 3],
        "invocation reached live progress before cancel"
    );

    let start = Instant::now();
    cancel.store(true, Ordering::SeqCst);
    let result = join_or_resume(handle);
    let elapsed = start.elapsed();

    assert!(
        matches!(result.outcome, OneShotOutcome::Cancelled),
        "host cancel is the session terminal: {:?}",
        result.outcome
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "cancel observed in {elapsed:?}, not hidden behind the 30 s invocation deadline"
    );

    assert!(
        result.process_reaped,
        "cancelled invocation is still reaped"
    );
}

#[test]
fn cw10_09_a_queued_one_shot_terminal_wins_over_a_later_cancel() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("happy", base_env(scene.provider_dir.clone()));
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = cancel.clone();
    let handle = thread::spawn(move || {
        run_one_shot_streaming(
            &request,
            &fast_bounds(),
            &FixedEnv::from_pairs(&[]),
            Some(&tx),
            Some(&cancel_for_thread),
        )
    });

    assert_eq!(recv_progress(&rx, 3), vec![1, 2, 3]);
    thread::sleep(Duration::from_millis(100));
    cancel.store(true, Ordering::SeqCst);
    let result = join_or_resume(handle);

    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "a provider terminal queued before cancel is authoritative: {:?}",
        result.outcome
    );
}

#[test]
fn s15_descriptor_timeout_carried_exactly_into_streaming_bounds() {
    // The streaming entry point honors the caller-supplied bounds exactly:
    // a 1-second invocation deadline times out a hung provider in ~1 s, not
    // the 60 s production default. This proves the descriptor-selected
    // timeout_seconds (1..=600) flows into invocation timing (S15).
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let request = scene.request("progress-then-hang", base_env(scene.provider_dir.clone()));
    let bounds = SupervisorBounds {
        invocation: Duration::from_secs(1),
        ..fast_bounds()
    };
    let start = Instant::now();
    let result = run_one_shot_streaming(&request, &bounds, &FixedEnv::from_pairs(&[]), None, None);
    let elapsed = start.elapsed();
    assert!(
        matches!(
            result.outcome,
            OneShotOutcome::Failed(SupervisorFailure::InvocationTimeout)
        ),
        "exact 1 s deadline fires for a hung provider: {:?}",
        result.outcome
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "timed out near the 1 s deadline, not the default: {elapsed:?}"
    );
    assert!(result.process_reaped);
}
