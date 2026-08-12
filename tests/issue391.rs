//! Behavioral integration coverage for issue #391 provider migration.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use jefe::domain::{CanonicalSemver, Id, TypedMap};
use jefe::runtime::provider::environment::{HostEnv, ProviderEnvironment};
use jefe::runtime::provider::protocol::{MessageKind, MigrateConfigPayload, RequestId};
use jefe::runtime::provider::{
    MigrationOutcome, MigrationRequest, SupervisorBounds, TranscriptEntry, run_migration,
};

const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-provider-fixture");

struct EmptyEnv;

impl HostEnv for EmptyEnv {
    fn get(&self, _name: &str) -> Option<String> {
        None
    }
}

struct Scene {
    root: tempfile::TempDir,
    provider_dir: PathBuf,
}

impl Scene {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error:?}"));
        let provider_dir = root.path().join("provider");
        std::fs::create_dir_all(&provider_dir)
            .unwrap_or_else(|error| panic!("provider dir: {error:?}"));
        Self { root, provider_dir }
    }

    fn request(&self, mode: &str) -> MigrationRequest {
        let version =
            CanonicalSemver::parse("2.0.0").unwrap_or_else(|error| panic!("version: {error:?}"));
        let request_id =
            RequestId::parse("h-000042").unwrap_or_else(|error| panic!("request id: {error:?}"));
        MigrationRequest {
            plugin_id: Id::parse("vendor.migrate")
                .unwrap_or_else(|error| panic!("plugin id: {error:?}")),
            plugin_version: version,
            binary: PathBuf::from(FIXTURE),
            arguments: vec![mode.to_owned()],
            working_dir: self.provider_dir.clone(),
            environment: ProviderEnvironment {
                provider_dir: self.provider_dir.clone(),
                nonsecret: BTreeMap::new(),
                secret_env: BTreeMap::new(),
                configure_secret_sources: BTreeMap::new(),
            },
            home: self.root.path().join("home"),
            tmpdir: self.root.path().join("tmp"),
            locale: "C".to_owned(),
            host_api: "jefe/test".to_owned(),
            generation: 1,
            request_id,
            migrate: MigrateConfigPayload {
                from_version: 1,
                to_version: 2,
                config: TypedMap::new(),
                draft_token: 17,
            },
        }
    }
}

fn bounds() -> SupervisorBounds {
    SupervisorBounds {
        handshake: Duration::from_secs(2),
        invocation: Duration::from_millis(250),
        shutdown_ack: Duration::from_secs(1),
        stdin_close: Duration::from_secs(1),
        final_drain: Duration::from_secs(1),
    }
}

#[test]
fn migration_runs_before_configure_and_reaps_the_provisional_provider() {
    let scene = Scene::new();
    let result = run_migration(&scene.request("migration-happy"), &bounds(), &EmptyEnv);

    let MigrationOutcome::Migrated(payload) = result.outcome else {
        panic!("expected migrated outcome: {result:?}");
    };
    assert_eq!(payload.from_version, 1);
    assert_eq!(payload.to_version, 2);
    assert_eq!(payload.draft_token, 17);
    assert_eq!(payload.config, TypedMap::new());
    assert_eq!(payload.target_config, TypedMap::new());
    assert_eq!(payload.notes, vec!["fixture migration"]);
    assert!(result.process_reaped);
    assert!(result.cleanup_failure.is_none());
    assert_eq!(
        result.transcript.entries(),
        [
            TranscriptEntry::Sent(MessageKind::Hello),
            TranscriptEntry::Received(MessageKind::HelloAck),
            TranscriptEntry::Sent(MessageKind::MigrateConfig),
            TranscriptEntry::Received(MessageKind::MigratedConfig),
            TranscriptEntry::Sent(MessageKind::Shutdown),
            TranscriptEntry::Received(MessageKind::ShutdownAck),
            TranscriptEntry::Eof,
            TranscriptEntry::Reaped,
        ]
    );
}

#[test]
fn migration_rejects_wrong_response_identity_and_reaps() {
    let scene = Scene::new();
    let result = run_migration(
        &scene.request("migration-wrong-request"),
        &bounds(),
        &EmptyEnv,
    );

    assert!(matches!(result.outcome, MigrationOutcome::Failed(_)));
    assert!(result.process_reaped);
}

#[test]
fn migration_rejects_wrong_process_generation_and_reaps() {
    let scene = Scene::new();
    let result = run_migration(
        &scene.request("migration-wrong-generation"),
        &bounds(),
        &EmptyEnv,
    );

    assert!(matches!(result.outcome, MigrationOutcome::Failed(_)));
    assert!(result.process_reaped);
}

#[test]
fn migration_rejects_mismatched_payload_identity_and_reaps() {
    for mode in [
        "migration-wrong-source-version",
        "migration-wrong-target-version",
        "migration-wrong-source-config",
        "migration-wrong-draft-token",
    ] {
        let scene = Scene::new();
        let result = run_migration(&scene.request(mode), &bounds(), &EmptyEnv);
        assert!(
            matches!(result.outcome, MigrationOutcome::Failed(_)),
            "{mode} unexpectedly succeeded: {result:?}"
        );
        assert!(result.process_reaped, "{mode} process was not reaped");
    }
}

#[test]
fn migration_timeout_eof_and_malformed_response_never_succeed() {
    for mode in ["migration-timeout", "migration-eof", "migration-malformed"] {
        let scene = Scene::new();
        let result = run_migration(&scene.request(mode), &bounds(), &EmptyEnv);
        assert!(
            matches!(result.outcome, MigrationOutcome::Failed(_)),
            "{mode} unexpectedly succeeded: {result:?}"
        );
        assert!(result.process_reaped, "{mode} process was not reaped");
    }
}
