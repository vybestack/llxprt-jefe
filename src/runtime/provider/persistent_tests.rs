//! Unit tests for the pure pieces of the persistent candidate lifecycle
//! (issue #390 CW-10, Slice C2).
//!
//! Process, transcript, and reap behaviour is proven by the integration tests
//! in `tests/issue390_persistent_providers.rs` against the real fixture binary.
//! These tests cover the process-free invariants: duplicate plugin-id rejection
//! before any spawn, deterministic plugin-id startup ordering, the
//! capability-subset rule, and the diagnostic-code split.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitStatus;

use crate::domain::{CanonicalSemver, Id, TypedMap};

use super::dto;
use super::environment::ProviderEnvironment;
use super::error;
use super::identifiers::RequestId;
use super::persistent::{
    CandidateFailure, CandidateHealth, IllegalStdout, PersistentCandidate, PersistentPhase,
    StartupFailure, StdoutProbe, classify_health, duplicate_plugin_id, first_undeclared_capability,
    startup_order,
};
use super::supervisor::SupervisorFailure;

/// Build a minimal persistent candidate with the given plugin id text.
fn candidate(plugin_id: &str) -> PersistentCandidate {
    PersistentCandidate {
        plugin_id: Id::parse(plugin_id).unwrap_or_else(|err| panic!("id: {err:?}")),
        plugin_version: CanonicalSemver::parse("1.0.0")
            .unwrap_or_else(|err| panic!("ver: {err:?}")),
        binary: PathBuf::from("/bin/true"),
        arguments: vec!["persistent-ready".to_owned()],
        working_dir: PathBuf::from("."),
        environment: ProviderEnvironment {
            provider_dir: PathBuf::from("."),
            nonsecret: BTreeMap::new(),
            secret_env: BTreeMap::new(),
            configure_secret_sources: BTreeMap::new(),
        },
        home: PathBuf::from("."),
        tmpdir: PathBuf::from("."),
        locale: "C".to_owned(),
        host_api: "jefe/test".to_owned(),
        generation: 1,
        request_id: RequestId::parse("h-000001").unwrap_or_else(|err| panic!("id: {err:?}")),
        configure: dto::ConfigurePayload {
            config_version: 1,
            config: TypedMap::new(),
            secrets: BTreeMap::new(),
            environment: BTreeMap::new(),
        },
        declared_capabilities: vec![dto::Capability::Actions],
    }
}

#[test]
fn duplicate_plugin_id_is_none_for_distinct_candidates() {
    let candidates = [
        candidate("vendor.alpha"),
        candidate("vendor.beta"),
        candidate("vendor.gamma"),
    ];
    assert!(duplicate_plugin_id(&candidates).is_none());
}

#[test]
fn duplicate_plugin_id_returns_the_repeated_id() {
    let candidates = [
        candidate("vendor.alpha"),
        candidate("vendor.beta"),
        candidate("vendor.alpha"),
    ];
    let dup = duplicate_plugin_id(&candidates)
        .unwrap_or_else(|| panic!("the repeated plugin id is detected before any spawn"));
    assert_eq!(dup.as_str(), "vendor.alpha");
}

#[test]
fn startup_order_sorts_by_canonical_plugin_id_text() {
    // Provided in reverse of canonical text order.
    let candidates = [
        candidate("vendor.zeta"),
        candidate("vendor.alpha"),
        candidate("vendor.mid"),
    ];
    let order = startup_order(&candidates);
    let ordered_ids: Vec<&str> = order
        .iter()
        .map(|idx| candidates[*idx].plugin_id.as_str())
        .collect();
    assert_eq!(
        ordered_ids,
        vec!["vendor.alpha", "vendor.mid", "vendor.zeta"],
        "startup observes plugin-id canonical text order, not input order"
    );
}

#[test]
fn startup_order_preserves_an_already_sorted_batch() {
    let candidates = [candidate("vendor.alpha"), candidate("vendor.zeta")];
    let order = startup_order(&candidates);
    let ordered_ids: Vec<&str> = order
        .iter()
        .map(|idx| candidates[*idx].plugin_id.as_str())
        .collect();
    assert_eq!(ordered_ids, vec!["vendor.alpha", "vendor.zeta"]);
}

#[test]
fn first_undeclared_capability_is_none_when_ready_is_a_subset() {
    let declared = [dto::Capability::Actions, dto::Capability::Panels];
    let ready = [dto::Capability::Actions];
    assert!(first_undeclared_capability(&declared, &ready).is_none());
}

#[test]
fn first_undeclared_capability_returns_the_offending_capability() {
    let declared = [dto::Capability::Actions];
    let ready = [dto::Capability::Actions, dto::Capability::Panels];
    let offender = first_undeclared_capability(&declared, &ready)
        .unwrap_or_else(|| panic!("a ready capability not declared by the manifest is rejected"));
    assert_eq!(offender, dto::Capability::Panels);
}

#[test]
fn a_capability_mismatch_failure_is_a_closed_protocol_fault() {
    use super::persistent::capability_mismatch_failure;
    let failure = capability_mismatch_failure(dto::Capability::Panels);
    match failure {
        SupervisorFailure::Protocol(_) => {}
        other => panic!("capability mismatch must be a protocol fault, got {other:?}"),
    }
    assert_eq!(failure.code(), error::PROTOCOL_FAILURE_CODE);
}

#[test]
fn a_candidate_failure_at_spawn_carries_the_runtime_code() {
    let failure = CandidateFailure {
        plugin_id: Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("id: {err:?}")),
        phase: PersistentPhase::Spawn,
        failure: SupervisorFailure::Spawn("no such binary".to_owned()),
    };
    assert_eq!(failure.code(), error::RUNTIME_UNAVAILABLE_CODE);
}

#[test]
fn a_candidate_failure_at_capability_carries_the_protocol_code() {
    use super::persistent::capability_mismatch_failure;
    let failure = CandidateFailure {
        plugin_id: Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("id: {err:?}")),
        phase: PersistentPhase::Capability,
        failure: capability_mismatch_failure(dto::Capability::Panels),
    };
    assert_eq!(failure.code(), error::PROTOCOL_FAILURE_CODE);
}

#[test]
fn a_duplicate_plugin_id_startup_failure_carries_the_protocol_code() {
    let failure = StartupFailure::DuplicatePluginId {
        plugin_id: Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("id: {err:?}")),
    };
    assert_eq!(failure.code(), error::PROTOCOL_FAILURE_CODE);
}

// ---------------------------------------------------------------------------
// Health classification (issue #390 CW-10 remediation: fail-fast health)
// ---------------------------------------------------------------------------

/// Build a real exited-0 `ExitStatus` for the classify_health unit tests.
#[cfg(unix)]
fn exited_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(0)
}

/// Build a real exited-0 `ExitStatus` for the classify_health unit tests.
#[cfg(not(unix))]
fn exited_status() -> ExitStatus {
    match std::process::Command::new("cmd")
        .args(["/C", "exit", "0"])
        .status()
    {
        Ok(status) => status,
        Err(error) => panic!("helper process for exit status must run: {error}"),
    }
}

#[test]
fn classify_health_idle_and_running_is_ready() {
    let health = classify_health(StdoutProbe::Idle, Ok(None), &[dto::Capability::Actions]);
    assert_eq!(
        health,
        CandidateHealth::Ready {
            capabilities: vec![dto::Capability::Actions],
        }
    );
}

#[test]
fn classify_health_idle_and_exited_is_exited() {
    let health = classify_health(
        StdoutProbe::Idle,
        Ok(Some(exited_status())),
        &[dto::Capability::Actions],
    );
    match health {
        CandidateHealth::Exited { exit_code } => {
            assert_eq!(exit_code, Some(0));
        }
        other => panic!("expected exited, got {other:?}"),
    }
}

#[test]
fn classify_health_a_try_wait_os_error_is_probe_failed_not_ready() {
    let error = std::io::Error::from_raw_os_error(4); // EINTR
    let health = classify_health(StdoutProbe::Idle, Err(error), &[]);
    match health {
        CandidateHealth::ProbeFailed { error } => {
            assert!(!error.is_empty(), "the OS error string is carried");
        }
        other => panic!("a try_wait OS error must fail fast, got {other:?}"),
    }
}

#[test]
fn classify_health_an_unexpected_frame_after_ready_is_a_protocol_fault() {
    let health = classify_health(StdoutProbe::Illegal(IllegalStdout::Frame), Ok(None), &[]);
    assert_eq!(
        health,
        CandidateHealth::ProtocolFault {
            evidence: IllegalStdout::Frame,
        }
    );
}

#[test]
fn classify_health_a_non_frame_fault_after_ready_is_a_protocol_fault() {
    let health = classify_health(StdoutProbe::Illegal(IllegalStdout::Fault), Ok(None), &[]);
    assert_eq!(
        health,
        CandidateHealth::ProtocolFault {
            evidence: IllegalStdout::Fault,
        }
    );
}

#[test]
fn classify_health_a_closed_stdout_while_alive_is_a_protocol_fault() {
    let health = classify_health(StdoutProbe::Closed, Ok(None), &[]);
    assert_eq!(
        health,
        CandidateHealth::ProtocolFault {
            evidence: IllegalStdout::Closed,
        }
    );
}

#[test]
fn classify_health_a_closed_stdout_with_an_exited_process_is_exited() {
    // A normally-exited process whose stdout channel has disconnected must be
    // classified `Exited`, not a closed-while-alive protocol fault: process exit
    // wins over a normal closed channel.
    let health = classify_health(StdoutProbe::Closed, Ok(Some(exited_status())), &[]);
    assert_eq!(
        health,
        CandidateHealth::Exited { exit_code: Some(0) },
        "an exited process wins over a closed stdout channel"
    );
}

#[test]
fn classify_health_illegal_stdout_takes_precedence_over_a_try_wait_error() {
    let error = std::io::Error::from_raw_os_error(4);
    let health = classify_health(StdoutProbe::Illegal(IllegalStdout::Frame), Err(error), &[]);
    assert_eq!(
        health,
        CandidateHealth::ProtocolFault {
            evidence: IllegalStdout::Frame,
        },
        "illegal stdout is the protocol fault regardless of the process probe"
    );
}
