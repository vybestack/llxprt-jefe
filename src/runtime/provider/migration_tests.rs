//! Unit tests for the provisional migration primitive's pre-spawn invariants
//! (issue #391, Slice E).
//!
//! Lifecycle ordering, wrong-identity/generation rejection, timeout/EOF/
//! malformed cleanup, the no-Configure transcript, and secret-reference
//! preservation are proven against the real fixture binary in
//! `tests/issue391.rs`. These tests cover the request
//! validation that needs no process.

use crate::test_support::Must;
use std::path::PathBuf;

use super::environment::{HostEnv, ProviderEnvironment};
use super::error::ProviderError;
use super::identifiers::RequestId;
use super::migration::{MigrationOutcome, MigrationRequest, run_migration};
use super::panel_model::MigrateConfigPayload;
use super::supervisor::{SupervisorBounds, SupervisorFailure};

use crate::domain::{CanonicalSemver, Id, TypedMap};

/// A host environment that resolves nothing. The pre-spawn failures under test
/// return before the environment is constructed, so the resolver is never used.
#[derive(Debug, Default)]
struct NoEnv;

impl HostEnv for NoEnv {
    fn get(&self, _name: &str) -> Option<String> {
        None
    }
}

fn migrate_payload() -> MigrateConfigPayload {
    MigrateConfigPayload {
        from_version: 1,
        to_version: 2,
        config: TypedMap::new(),
        draft_token: 7,
    }
}

fn base_request() -> MigrationRequest {
    MigrationRequest {
        binary: PathBuf::from("/nonexistent/provider"),
        arguments: Vec::new(),
        working_dir: PathBuf::from("/tmp"),
        environment: ProviderEnvironment::default(),
        home: PathBuf::from("/tmp"),
        tmpdir: PathBuf::from("/tmp"),
        locale: "C".to_owned(),
        host_api: "jefe".to_owned(),
        plugin_id: Id::parse("vendor.pkg").must("valid plugin id"),
        plugin_version: CanonicalSemver::parse("1.0.0").must("valid semver"),
        generation: 1,
        request_id: RequestId::new_host(1).must("valid host request id"),
        migrate: migrate_payload(),
    }
}

#[test]
fn migration_rejects_zero_generation_before_spawn() {
    let mut request = base_request();
    request.generation = 0;
    let result = run_migration(&request, &SupervisorBounds::PRODUCTION, &NoEnv);
    let MigrationOutcome::Failed(SupervisorFailure::Protocol(ProviderError::InvalidGeneration {
        value,
    })) = &result.outcome
    else {
        panic!(
            "expected InvalidGeneration failure, got {:?}",
            result.outcome
        );
    };
    assert_eq!(*value, 0, "the offending generation value is reported");
    assert!(!result.process_reaped, "no process was spawned");
    assert!(
        result.transcript.entries().is_empty(),
        "no lifecycle was observed before spawn"
    );
    assert_eq!(result.exit_code, None);
    assert!(result.cleanup_failure.is_none());
}

#[test]
fn migration_rejects_non_host_request_id_before_spawn() {
    let mut request = base_request();
    request.request_id = RequestId::parse("p-000001").must("valid provider request id");
    let result = run_migration(&request, &SupervisorBounds::PRODUCTION, &NoEnv);
    let MigrationOutcome::Failed(failure) = &result.outcome else {
        panic!("expected failure, got {:?}", result.outcome);
    };
    assert!(
        matches!(
            failure,
            SupervisorFailure::Protocol(ProviderError::InvalidRequestOrigin { .. })
        ),
        "expected an InvalidRequestOrigin protocol failure, got {failure:?}"
    );
    assert!(!result.process_reaped, "no process was spawned");
    assert!(
        result.transcript.entries().is_empty(),
        "no lifecycle was observed before spawn"
    );
    assert!(result.cleanup_failure.is_none());
}
