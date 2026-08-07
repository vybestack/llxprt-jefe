//! Integration tests for provider effect worker execution
//! (issue #390 CW-10, Slice D).
//!
//! These drive the real one-shot supervisor through the coordinator and
//! effect-worker translation pipeline, proving the full flow:
//! descriptor → OneShotRequest → supervisor → typed ProviderMessages.
//!
//! The tests do NOT run the async `use_future` loop — they exercise the
//! synchronous core that the background worker calls inside `smol::unblock`,
//! which is where all the interesting logic lives.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jefe::domain::action_registry::ActionId;
use jefe::domain::plugin::provider::ProviderMode;
use jefe::domain::{CanonicalSemver, Id, TypedMap};
use jefe::messages::ProviderMessage;
use jefe::runtime::provider::coordinator::{
    ProviderActionDescriptor, ProviderCatalog, ProviderCoordinator, build_invocation_payload,
};
use jefe::runtime::provider::environment::{HostEnv, ProcessHostEnv, ProviderEnvironment};
use jefe::runtime::provider::protocol::{ConfigurePayload, EnvName, Outcome};
use jefe::runtime::provider::supervisor::{SupervisorBounds, run_one_shot};
use jefe::services::provider_effect_worker::build_execution_result;
use jefe::state::provider_requests::ActionPolicy;

use jefe::domain::effects::{
    Correlation, CorrelationId, EffectFamily, ProviderInvocation, ProviderRequestKey, SemanticKey,
};
use jefe::domain::plugin::action::{ActionConfirmation, ActionOutcome};

const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-provider-fixture");

/// A host environment exposing exactly one declared secret.
struct SecretEnv<'a>(&'a str);

impl HostEnv for SecretEnv<'_> {
    fn get(&self, name: &str) -> Option<String> {
        (name == "HOST_DEPLOY_KEY").then(|| self.0.to_owned())
    }
}

struct Scene {
    home: tempfile::TempDir,
    provider_dir: PathBuf,
}

impl Scene {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
        let provider_dir = home.path().join("bin");
        std::fs::create_dir_all(&provider_dir).unwrap_or_else(|e| panic!("mkdir: {e:?}"));
        Self { home, provider_dir }
    }

    fn descriptor(&self, mode: &str) -> ProviderActionDescriptor {
        let action_id =
            ActionId::parse("vendor.pkg.run").unwrap_or_else(|e| panic!("action id: {e:?}"));
        ProviderActionDescriptor {
            action_id,
            plugin_id: Id::parse("vendor.pkg").unwrap_or_else(|e| panic!("plugin id: {e:?}")),
            plugin_version: CanonicalSemver::parse("1.0.0")
                .unwrap_or_else(|e| panic!("version: {e:?}")),
            mode: ProviderMode::OneShot,
            binary: PathBuf::from(FIXTURE),
            provider_args: vec![mode.to_owned()],
            working_dir: self.provider_dir.clone(),
            home: self.home.path().join("home"),
            tmpdir: self.home.path().join("tmp"),
            locale: "C".to_owned(),
            host_api: "jefe/test".to_owned(),
            environment: ProviderEnvironment {
                provider_dir: self.provider_dir.clone(),
                nonsecret: BTreeMap::new(),
                secret_env: BTreeMap::new(),
                configure_secret_sources: BTreeMap::new(),
            },
            configure: ConfigurePayload {
                config_version: 1,
                config: TypedMap::new(),
                secrets: BTreeMap::new(),
                environment: BTreeMap::new(),
            },
            policy: ActionPolicy::new(
                ActionConfirmation::None,
                vec![
                    ActionOutcome::NavigateDeclaredRoute,
                    ActionOutcome::RefreshCurrentResource,
                    ActionOutcome::Notice,
                ],
                false,
            ),
            timeout_seconds: 60,
        }
    }

    fn invocation(generation: u64) -> ProviderInvocation {
        ProviderInvocation {
            key: ProviderRequestKey {
                owner: Id::parse("host").unwrap_or_else(|e| panic!("owner: {e:?}")),
                action_id: Id::parse("vendor.pkg.run").unwrap_or_else(|e| panic!("action: {e:?}")),
                generation,
            },
            arguments: TypedMap::new(),
            context_screen: Id::parse("core.dashboard").unwrap_or_else(|e| panic!("screen: {e:?}")),
            context_instance: Id::parse("inst-1").unwrap_or_else(|e| panic!("instance: {e:?}")),
            context_refs: TypedMap::new(),
            continuation: None,
        }
    }
}

fn correlation() -> Correlation {
    Correlation {
        correlation_id: CorrelationId::new(1),
        owner: Id::parse("host").unwrap_or_else(|e| panic!("owner: {e:?}")),
        screen_generation: 0,
        activation_generation: 0,
        semantic_key: SemanticKey::new(EffectFamily::Provider, "test"),
    }
}

fn fast_bounds() -> SupervisorBounds {
    SupervisorBounds {
        handshake: std::time::Duration::from_secs(3),
        invocation: std::time::Duration::from_secs(5),
        shutdown_ack: std::time::Duration::from_millis(600),
        stdin_close: std::time::Duration::from_millis(600),
        final_drain: std::time::Duration::from_millis(600),
    }
}

/// The coordinator builds a valid OneShotRequest from a descriptor + invocation,
/// and the request-id is unique per call (monotonic counter, not generation).
#[test]
fn coordinator_builds_one_shot_with_unique_request_ids() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let descriptor = scene.descriptor("happy");
    let coordinator = ProviderCoordinator::empty();
    let inv1 = Scene::invocation(1);
    let inv2 = Scene::invocation(2);

    let req1 = coordinator
        .build_one_shot(&descriptor, &inv1)
        .unwrap_or_else(|e| panic!("build_one_shot 1: {e:?}"));
    let req2 = coordinator
        .build_one_shot(&descriptor, &inv2)
        .unwrap_or_else(|e| panic!("build_one_shot 2: {e:?}"));

    assert_ne!(
        req1.request_id, req2.request_id,
        "request ids must be unique"
    );
    assert_eq!(req1.generation, 1);
    assert_eq!(req2.generation, 2);
}

/// A happy-path one-shot produces a Navigate outcome and progress messages
/// through the full pipeline: descriptor → build_one_shot → run_one_shot →
/// build_execution_result → typed ProviderMessages.
#[test]
fn happy_path_produces_progress_and_navigate_outcome() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let descriptor = scene.descriptor("happy");
    let coordinator = ProviderCoordinator::empty();
    let invocation = Scene::invocation(1);

    let request = coordinator
        .build_one_shot(&descriptor, &invocation)
        .unwrap_or_else(|e| panic!("build_one_shot: {e:?}"));

    let result = run_one_shot(&request, &fast_bounds(), &ProcessHostEnv);

    assert!(result.process_reaped, "process must be reaped");
    let key = &invocation.key;
    let exec_result = build_execution_result(correlation(), &result, key);

    assert!(exec_result.terminal, "must be terminal");
    assert!(
        exec_result.process_reaped,
        "process must be reaped in exec result"
    );

    // The happy fixture emits 3 progress events then a Navigate outcome.
    let has_progress = exec_result
        .messages
        .iter()
        .any(|m| matches!(m, ProviderMessage::Progress { .. }));
    assert!(
        has_progress,
        "must have progress messages: {:?}",
        exec_result.messages
    );

    let has_outcome = exec_result.messages.iter().any(|m| {
        matches!(
            m,
            ProviderMessage::Outcome {
                outcome: Outcome::Navigate { .. },
                ..
            }
        )
    });
    assert!(
        has_outcome,
        "must have a Navigate outcome: {:?}",
        exec_result.messages
    );
}

/// An error-mode fixture produces a ProviderMessage::Error terminal.
#[test]
fn error_mode_produces_error_terminal() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let descriptor = scene.descriptor("error");
    let coordinator = ProviderCoordinator::empty();
    let invocation = Scene::invocation(1);

    let request = coordinator
        .build_one_shot(&descriptor, &invocation)
        .unwrap_or_else(|e| panic!("build_one_shot: {e:?}"));

    let result = run_one_shot(&request, &fast_bounds(), &ProcessHostEnv);

    let key = &invocation.key;
    let exec_result = build_execution_result(correlation(), &result, key);

    assert!(exec_result.terminal);
    let has_error = exec_result
        .messages
        .iter()
        .any(|m| matches!(m, ProviderMessage::Error { .. }));
    assert!(
        has_error,
        "must have error message: {:?}",
        exec_result.messages
    );
}

/// A never-ready fixture produces a GenerationFailed (supervisor failure).
#[test]
fn never_ready_produces_generation_failed() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let descriptor = scene.descriptor("never-ready");
    let coordinator = ProviderCoordinator::empty();
    let invocation = Scene::invocation(1);

    let request = coordinator
        .build_one_shot(&descriptor, &invocation)
        .unwrap_or_else(|e| panic!("build_one_shot: {e:?}"));

    let result = run_one_shot(&request, &fast_bounds(), &ProcessHostEnv);

    let key = &invocation.key;
    let exec_result = build_execution_result(correlation(), &result, key);

    assert!(exec_result.terminal);
    let has_gen_failed = exec_result
        .messages
        .iter()
        .any(|m| matches!(m, ProviderMessage::GenerationFailed { .. }));
    assert!(
        has_gen_failed,
        "must have generation failed: {:?}",
        exec_result.messages
    );
}

/// The ProviderCatalog stores and retrieves descriptors by ActionId.
#[test]
fn catalog_stores_and_retrieves_descriptors() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let descriptor = scene.descriptor("happy");
    let action_id = descriptor.action_id.clone();

    let mut catalog = ProviderCatalog::new();
    assert!(catalog.is_empty());
    catalog.insert(action_id.clone(), descriptor.clone());
    assert_eq!(catalog.len(), 1);
    assert!(!catalog.is_empty());

    let Some(retrieved) = catalog.get(&action_id) else {
        panic!("the catalog must find the descriptor it just stored");
    };
    assert_eq!(retrieved.action_id, action_id);
    assert_eq!(retrieved.mode, ProviderMode::OneShot);
}

/// The coordinator's catalog is accessible and returns the registered actions.
#[test]
fn coordinator_catalog_is_accessible() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let descriptor = scene.descriptor("happy");
    let action_id = descriptor.action_id.clone();

    let mut catalog = ProviderCatalog::new();
    catalog.insert(action_id.clone(), descriptor);
    let coordinator = ProviderCoordinator::from_startup(
        jefe::runtime::provider::PersistentStartupResult::Failed(
            jefe::runtime::provider::PersistentStartupFailure {
                failure: jefe::runtime::provider::StartupFailure::DuplicatePluginId {
                    plugin_id: Id::parse("test").unwrap_or_else(|e| panic!("id: {e:?}")),
                },
                rollback: Vec::new(),
            },
        ),
        catalog,
    );

    assert!(!coordinator.has_persistent());
    assert!(!coordinator.catalog().is_empty());
}

/// Secrets never appear in the execution result messages or retained stderr.
#[test]
fn secrets_never_appear_in_execution_messages() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let secret_value = "SUPER-secret-canary-390";

    let mut descriptor = scene.descriptor("secret-stderr");
    descriptor.environment.configure_secret_sources.insert(
        EnvName::parse("DEPLOY_KEY").unwrap_or_else(|e| panic!("env name: {e:?}")),
        EnvName::parse("HOST_DEPLOY_KEY").unwrap_or_else(|e| panic!("env name: {e:?}")),
    );

    let coordinator = ProviderCoordinator::empty();
    let invocation = Scene::invocation(1);

    let request = coordinator
        .build_one_shot(&descriptor, &invocation)
        .unwrap_or_else(|e| panic!("build_one_shot: {e:?}"));

    let result = run_one_shot(&request, &fast_bounds(), &SecretEnv(secret_value));

    let key = &invocation.key;
    let exec_result = build_execution_result(correlation(), &result, key);

    // The secret must not appear in any message or the retained stderr.
    for message in &exec_result.messages {
        let text = format!("{message:?}");
        assert!(
            !text.contains(secret_value),
            "secret leaked in message: {text}"
        );
    }
    assert!(
        !result.retained_stderr.contains(secret_value),
        "secret leaked in retained stderr"
    );
}

/// build_invocation_payload produces the correct action_id from the descriptor
/// (pre-parsed, no runtime parsing needed).
#[test]
fn invocation_payload_uses_descriptor_action_id() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let descriptor = scene.descriptor("happy");
    let invocation = Scene::invocation(1);

    let Ok(payload) = build_invocation_payload(&descriptor, &invocation) else {
        panic!("an ordinary plugin id and generation must compose an invocation id");
    };
    assert_eq!(payload.action_id, descriptor.action_id);
    assert_eq!(payload.context.screen_id, invocation.context_screen);
    assert_eq!(payload.arguments, invocation.arguments);
    assert_eq!(payload.context.screen_instance, invocation.context_instance);
    assert!(
        payload.continuation.is_none(),
        "a first invocation carries no continuation"
    );
}

/// A plugin id long enough that appending a generation overflows the `Id`
/// bound must be reported, not silently collapsed onto the bare plugin id —
/// which would give every invocation of that package the same invocation id.
#[test]
fn an_invocation_id_that_cannot_be_built_is_reported() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let mut descriptor = scene.descriptor("happy");
    // Exactly the 128-byte `Id` limit, so appending ".1" overflows it.
    let long = format!("vendor.{}", "a".repeat(121));
    descriptor.plugin_id = Id::parse(&long).unwrap_or_else(|e| panic!("long id: {e:?}"));

    let result = build_invocation_payload(&descriptor, &Scene::invocation(1));

    assert!(
        result.is_err(),
        "an oversized invocation id must be an error, not a duplicate id"
    );
}
/// CW10-07: the reducer's progress model is defined by message, completed and
/// total, so the worker must deliver what the provider actually sent. A
/// sequence number with an empty payload is not progress the operator can read.
#[test]
fn delivered_progress_carries_the_message_and_counts_the_provider_sent() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let descriptor = scene.descriptor("happy");
    let invocation = Scene::invocation(1);
    let request = match jefe::runtime::provider::coordinator::build_one_shot_request(
        &descriptor,
        &invocation,
        1,
    ) {
        Ok(request) => request,
        Err(error) => panic!("request must build: {error:?}"),
    };
    let result = run_one_shot(&request, &fast_bounds(), &ProcessHostEnv);
    let execution = build_execution_result(correlation(), &result, &invocation.key);

    let progress: Vec<_> = execution
        .messages
        .iter()
        .filter_map(|message| match message {
            ProviderMessage::Progress { payload, .. } => Some(payload.clone()),
            _ => None,
        })
        .collect();

    assert!(!progress.is_empty(), "the happy fixture emits progress");
    for payload in &progress {
        assert_eq!(
            payload.message, "step",
            "the provider's progress message must reach the reducer"
        );
        assert_eq!(
            payload.completed,
            Some(u64::from(payload.sequence)),
            "the provider's completed count must reach the reducer"
        );
        assert_eq!(
            payload.total,
            Some(256),
            "the provider's total must reach the reducer"
        );
    }
}
