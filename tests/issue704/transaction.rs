//! Required-provider startup transaction behavioral tests
//! (issue #704, slice S2: CWR1-00, CWR1-03, CWR1-04, CWR1-05).
//!
//! These tests drive the real `jefe-provider-fixture` through the full
//! `PublishedWorkbench` → `run_provider_transaction` path to prove:
//! - Only Required persistent providers start; one-shot and declaration-empty
//!   providers spawn zero processes (CWR1-00, CWR1-04).
//! - Deterministic plugin-id ordering of preparation and startup (CWR1-03).
//! - Every required candidate completes executable, containment-directory,
//!   and environment/secret preparation before any provider spawns, with the
//!   typed failure naming the exact owner and cause (CWR1-05).
//! - A genuine post-preflight spawn defect (a binary that passes every
//!   metadata/executability check but cannot be loaded) still fails the
//!   transaction and reaps every earlier ready provider (CWR1-03).
//! - First/middle/last provider failures reap all started candidates before
//!   the caller receives `Err` (CWR1-03, CWR1-05).
//! - Unrelated sentinel processes survive transaction failure (no broad kill).
//!
//! The product composition intentionally supplies `arguments: Vec::new()`, so
//! each staged package receives a copy of the fixture executable (with the
//! platform's executable extension and permissions) plus a test-only
//! `<executable>.control` sidecar — read by the fixture only when it is
//! invoked without argv — naming the fixture mode, the record directory, and,
//! for providers that must never start, a fail-if-spawned marker. No shell
//! wrappers are involved, so every test below runs natively on Unix and
//! Windows.

use std::fs;

use jefe::runtime::provider::SupervisorFailure;
use jefe::runtime::provider::environment::EnvironmentError;
use jefe::runtime::provider::persistent::{
    CandidateFailure, PersistentPhase, PersistentStartupFailure, StartupFailure,
};
use jefe::startup_transaction::{
    PreparationCause, ProviderTransactionFailure, ProviderTransactionResult,
};

use super::support::provider_exe_name;
use super::transaction_support::{
    FIXTURE, Scene, assert_all_reaped, assert_nothing_spawned, process_budget, process_is_gone,
    read_pid, startup_sequence,
};

// ---------------------------------------------------------------------------
// Tests — native process tests on every platform
// (CWR1-00, CWR1-03, CWR1-04, CWR1-05).
// ---------------------------------------------------------------------------

#[test]
fn cwr1_00_required_persistent_provider_starts_and_reaches_ready() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-ready");
    let workbench = scene.build_workbench(&["vendor.alpha"]);

    let mut result = scene
        .run_transaction(&workbench)
        .unwrap_or_else(|e| panic!("transaction must succeed: {e}"));

    assert_eq!(
        result.publication.ready().len(),
        1,
        "one required provider published"
    );
    assert_eq!(
        result.publication.ready()[0].plugin_id.as_str(),
        "vendor.alpha"
    );
    assert_eq!(
        result.supervisor.candidate_count(),
        1,
        "supervisor owns one candidate"
    );

    // Clean shutdown reaps the candidate.
    let shutdown = result.supervisor.shutdown();
    assert_eq!(shutdown.len(), 1);
    assert!(
        shutdown[0].process_reaped,
        "candidate is reaped on explicit shutdown"
    );
}

#[test]
fn cwr1_03_multiple_required_providers_start_in_deterministic_plugin_id_order() {
    let _budget = process_budget();
    let scene = Scene::new();
    // Stage in reverse of canonical plugin-id text order.
    scene.stage_required("vendor.zeta", "persistent-ready");
    scene.stage_required("vendor.alpha", "persistent-ready");
    let workbench = scene.build_workbench(&["vendor.zeta", "vendor.alpha"]);

    let mut result = scene
        .run_transaction(&workbench)
        .unwrap_or_else(|e| panic!("transaction must succeed: {e}"));

    // The publication lists candidates in canonical plugin-id order.
    let ids: Vec<&str> = result
        .publication
        .ready()
        .iter()
        .map(|c| c.plugin_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["vendor.alpha", "vendor.zeta"],
        "publication is in canonical plugin-id order"
    );

    // The fixtures recorded the order they actually received hello.
    assert_eq!(
        startup_sequence(&scene.record_dir),
        vec!["vendor.alpha", "vendor.zeta"],
        "candidates start in deterministic plugin-id order, not input order"
    );

    let shutdown = result.supervisor.shutdown();
    assert_eq!(shutdown.len(), 2);
    assert!(shutdown.iter().all(|e| e.process_reaped));
}

#[test]
fn cwr1_04_one_shot_provider_spawns_zero_processes() {
    let _budget = process_budget();
    let scene = Scene::new();
    // A one-shot provider (actions declared, mode one-shot) is NotRequired.
    scene.stage_one_shot("vendor.oneshot");
    // A required provider so the transaction does something.
    scene.stage_required("vendor.alpha", "persistent-ready");
    let workbench = scene.build_workbench(&["vendor.oneshot", "vendor.alpha"]);

    let mut result = scene
        .run_transaction(&workbench)
        .unwrap_or_else(|e| panic!("transaction must succeed: {e}"));

    // Only the required provider started.
    assert_eq!(result.publication.ready().len(), 1);
    assert_eq!(
        result.publication.ready()[0].plugin_id.as_str(),
        "vendor.alpha"
    );

    // The one-shot provider's fail-if-spawned marker must NOT exist.
    let marker = scene.record_dir.join("vendor.oneshot.spawned");
    assert!(
        !marker.exists(),
        "one-shot provider must not be spawned during the transaction"
    );

    result.supervisor.shutdown();
}

#[test]
fn cwr1_04_declaration_empty_persistent_provider_spawns_zero_processes() {
    let _budget = process_budget();
    let scene = Scene::new();
    // A declaration-empty persistent provider is NotRequired(DeclarationEmpty).
    // The composition creates a PersistentCandidate for it (because it is
    // persistent), but the transaction must not start it.
    scene.stage_declaration_empty("vendor.empty");
    // A required provider so the transaction does something.
    scene.stage_required("vendor.alpha", "persistent-ready");
    let workbench = scene.build_workbench(&["vendor.empty", "vendor.alpha"]);

    let mut result = scene
        .run_transaction(&workbench)
        .unwrap_or_else(|e| panic!("transaction must succeed: {e}"));

    // Only the required provider started.
    assert_eq!(result.publication.ready().len(), 1);
    assert_eq!(
        result.publication.ready()[0].plugin_id.as_str(),
        "vendor.alpha"
    );

    // The declaration-empty provider's fail-if-spawned marker must NOT exist.
    let marker = scene.record_dir.join("vendor.empty.spawned");
    assert!(
        !marker.exists(),
        "declaration-empty persistent provider must not be spawned during the transaction"
    );

    result.supervisor.shutdown();
}

/// The fail-if-spawned trap itself must fire: spawning a staged trap
/// executable with no argv — exactly what an erroneous host spawn looks
/// like, since the composition supplies no arguments — records the marker
/// before any protocol traffic. This proves the trap the one-shot and
/// declaration-empty tests rely on is genuinely armed on this platform.
#[test]
fn cwr1_04_fail_if_spawned_trap_records_marker_when_spawned() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_one_shot("vendor.oneshot");

    // Null stdio delivers an immediate EOF, so the trap fixture records its
    // marker, fails its first protocol read, and exits nonzero — bounded,
    // with no hanging process.
    let exe = scene
        .plugins_root()
        .join("vendor.oneshot")
        .join("1.0.0")
        .join("bin")
        .join(provider_exe_name());
    let mut child = std::process::Command::new(&exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn trap executable: {e:?}"));
    let status = child
        .wait()
        .unwrap_or_else(|e| panic!("wait for trap executable: {e:?}"));

    assert!(
        scene.record_dir.join("vendor.oneshot.spawned").exists(),
        "the trap marker is recorded the instant the fixture is spawned"
    );
    assert!(
        !status.success(),
        "the trap fixture exits nonzero after stdin EOF, got {status}"
    );
}

// ---------------------------------------------------------------------------
// Preparation: every required candidate prepares before any provider spawns
// (CWR1-05). Each defect below is staged in the LAST candidate while an
// earlier healthy candidate exists, proving the earlier provider is never
// started.
// ---------------------------------------------------------------------------

#[test]
fn cwr1_05_last_candidate_missing_binary_prepares_before_any_spawn() {
    let _budget = process_budget();
    let scene = Scene::new();
    // alpha is a healthy required provider; zeta's binary is missing.
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_missing_binary("vendor.zeta");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.zeta"]);

    let (owner, cause) = expect_preparation_failure(scene.run_transaction(&workbench));
    assert_eq!(owner.as_str(), "vendor.zeta", "failure names its owner");
    match cause {
        PreparationCause::BinaryMissing { path, .. } => assert!(
            path.ends_with(provider_exe_name()),
            "missing-binary cause carries the binary path, got {path:?}"
        ),
        other => panic!("expected BinaryMissing, got {other:?}"),
    }
    assert_nothing_spawned(&scene);
}

#[test]
fn cwr1_05_preparation_reports_first_defect_in_plugin_id_order() {
    let _budget = process_budget();
    let scene = Scene::new();
    // Both candidates are defective; settings order is reverse of plugin-id
    // order. The reported owner must follow plugin-id order, not input order.
    scene.stage_missing_binary("vendor.zeta");
    scene.stage_missing_binary("vendor.alpha");
    let workbench = scene.build_workbench(&["vendor.zeta", "vendor.alpha"]);

    let (owner, _cause) = expect_preparation_failure(scene.run_transaction(&workbench));
    assert_eq!(
        owner.as_str(),
        "vendor.alpha",
        "the first defect in deterministic plugin-id order is reported"
    );
    assert_nothing_spawned(&scene);
}

#[test]
fn cwr1_05_binary_that_is_not_a_file_prepares_before_any_spawn() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_directory_binary("vendor.zeta");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.zeta"]);

    let (owner, cause) = expect_preparation_failure(scene.run_transaction(&workbench));
    assert_eq!(owner.as_str(), "vendor.zeta");
    assert!(
        matches!(cause, PreparationCause::BinaryNotAFile { .. }),
        "expected BinaryNotAFile, got {cause:?}"
    );
    assert_nothing_spawned(&scene);
}

/// Unix only: the executable bit is a distinct permission there. Windows has
/// no equivalent, so executability defects surface at spawn instead.
#[cfg(unix)]
#[test]
fn cwr1_05_binary_without_executable_bit_prepares_before_any_spawn() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_non_executable_binary("vendor.zeta");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.zeta"]);

    let (owner, cause) = expect_preparation_failure(scene.run_transaction(&workbench));
    assert_eq!(owner.as_str(), "vendor.zeta");
    assert!(
        matches!(cause, PreparationCause::BinaryNotExecutable { .. }),
        "expected BinaryNotExecutable, got {cause:?}"
    );
    assert_nothing_spawned(&scene);
}

#[test]
fn cwr1_05_unresolved_secret_in_last_candidate_prepares_before_any_spawn() {
    let _budget = process_budget();
    let scene = Scene::new();
    // alpha is healthy; beta declares a secret-reference config field that
    // the empty host environment cannot resolve.
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_secret_ref_required("vendor.beta", "JEFEE_TEST_UNSET_TOKEN");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.beta"]);

    let (owner, cause) = expect_preparation_failure(scene.run_transaction(&workbench));
    assert_eq!(owner.as_str(), "vendor.beta");
    let PreparationCause::Environment(error) = cause else {
        panic!("expected Environment cause, got {cause:?}");
    };
    // Composition keys each configure secret source by the host variable it
    // names, so the unresolved binding and source are that variable.
    assert_eq!(
        error,
        EnvironmentError::UnresolvedSecret {
            binding: "JEFEE_TEST_UNSET_TOKEN".to_owned(),
            source: "JEFEE_TEST_UNSET_TOKEN".to_owned(),
        },
        "the exact environment-resolution defect is carried"
    );
    assert_nothing_spawned(&scene);
}

#[test]
fn cwr1_05_containment_failure_names_owner_and_prevents_any_spawn() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_required("vendor.zeta", "persistent-ready");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.zeta"]);

    // Create a file where the working_dir should be so create_dir_all fails.
    let work_path = scene.containment_base.join("work");
    fs::create_dir_all(scene.containment_base.as_path())
        .unwrap_or_else(|e| panic!("containment base: {e:?}"));
    fs::write(&work_path, b"not a directory").unwrap_or_else(|e| panic!("block file: {e:?}"));

    let (owner, cause) = expect_preparation_failure(scene.run_transaction(&workbench));
    // Both candidates share one containment base; alpha sorts first, so
    // alpha is the owner whose directory could not be created.
    assert_eq!(owner.as_str(), "vendor.alpha");
    match cause {
        PreparationCause::ContainmentDirectory { directory, .. } => {
            assert_eq!(directory, work_path);
        }
        other => panic!("expected ContainmentDirectory failure, got {other:?}"),
    }
    assert_nothing_spawned(&scene);
}

// ---------------------------------------------------------------------------
// Genuine post-preflight spawn failure: a binary that passes every
// preparation check but cannot be loaded by the OS (CWR1-03).
// ---------------------------------------------------------------------------

#[test]
fn cwr1_03_unloadable_binary_returns_typed_spawn_failure() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_unloadable_binary("vendor.alpha");
    let workbench = scene.build_workbench(&["vendor.alpha"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::Spawn,
                failure: SupervisorFailure::Spawn(_),
                ..
            })
        ),
        "expected spawn failure, got {:?}",
        failure.failure
    );
    // No process existed, so rollback is empty.
    assert!(
        failure.rollback.is_empty(),
        "no candidate was started, so rollback is empty"
    );
}

#[test]
fn cwr1_03_later_unloadable_binary_reaps_earlier_ready_providers() {
    let _budget = process_budget();
    let scene = Scene::new();
    // alpha reaches ready; zeta's binary passes preparation but cannot be
    // loaded, so zeta fails at OS spawn after alpha already started.
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_unloadable_binary("vendor.zeta");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.zeta"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    // alpha was started, reached ready, and was reaped; zeta failed at spawn
    // (no process ever existed for it, so it has no reap evidence).
    let reaped_ids: Vec<&str> = failure
        .rollback
        .iter()
        .map(|e| e.plugin_id.as_str())
        .collect();
    assert_eq!(
        reaped_ids,
        vec!["vendor.alpha"],
        "the earlier ready provider is reaped; the unloadable candidate never existed"
    );
    assert_all_reaped(&failure.rollback);

    // Only alpha ever spawned.
    assert_eq!(
        startup_sequence(&scene.record_dir),
        vec!["vendor.alpha"],
        "zeta never spawned; alpha started first and was rolled back"
    );
    let alpha_pid = read_pid(&scene.record_dir, "vendor.alpha");
    assert!(
        process_is_gone(alpha_pid),
        "alpha's process was reaped before the error was returned"
    );
}

// ---------------------------------------------------------------------------
// Handshake failures and rollback evidence (CWR1-03, CWR1-05).
// ---------------------------------------------------------------------------

#[test]
fn cwr1_03_hello_timeout_returns_typed_error_with_cleanup() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-hello-hang");
    let workbench = scene.build_workbench(&["vendor.alpha"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::HelloAck,
                failure: SupervisorFailure::HandshakeTimeout,
                ..
            })
        ),
        "expected hello-ack timeout, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cwr1_03_protocol_fault_returns_typed_error_with_cleanup() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-protocol");
    let workbench = scene.build_workbench(&["vendor.alpha"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::HelloAck,
                failure: SupervisorFailure::Protocol(_),
                ..
            })
        ),
        "expected protocol fault, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cwr1_03_exit_before_ready_has_exact_phase_cause_and_cleanup() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-exit-before-ready");
    let workbench = scene.build_workbench(&["vendor.alpha"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                plugin_id,
                phase: PersistentPhase::Ready,
                failure: SupervisorFailure::Crashed { exit: None },
            }) if plugin_id.as_str() == "vendor.alpha"
        ),
        "expected exact Ready-phase exit, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cwr1_03_ready_timeout_returns_typed_error_with_cleanup() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-ready-hang");
    let workbench = scene.build_workbench(&["vendor.alpha"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::Ready,
                failure: SupervisorFailure::HandshakeTimeout,
                ..
            })
        ),
        "expected ready timeout, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cwr1_03_undeclared_capability_returns_typed_error_with_cleanup() {
    let _budget = process_budget();
    let scene = Scene::new();
    // The host declares only actions (from PackageSpec::persistent_actions);
    // the fixture reports actions + panels (undeclared).
    scene.stage_required("vendor.alpha", "persistent-undeclared-cap");
    let workbench = scene.build_workbench(&["vendor.alpha"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::Capability,
                failure: SupervisorFailure::Protocol(_),
                ..
            })
        ),
        "expected capability rejection, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cwr1_03_last_provider_failure_rolls_back_first() {
    let _budget = process_budget();
    let scene = Scene::new();
    // alpha starts and reaches ready; zeta hangs at hello-ack and fails.
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_required("vendor.zeta", "persistent-hello-hang");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.zeta"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    // alpha was started first and rolled back; zeta failed last.
    let reaped_ids: Vec<&str> = failure
        .rollback
        .iter()
        .map(|e| e.plugin_id.as_str())
        .collect();
    assert_eq!(
        reaped_ids,
        vec!["vendor.alpha", "vendor.zeta"],
        "every previously started and the failing candidate are reaped in start order"
    );
    assert_all_reaped(&failure.rollback);

    // alpha's process is gone after the error is returned.
    let alpha_pid = read_pid(&scene.record_dir, "vendor.alpha");
    assert!(
        process_is_gone(alpha_pid),
        "alpha's process was reaped before the error was returned"
    );
}

#[test]
fn cwr1_03_middle_provider_failure_rolls_back_first_and_skips_third() {
    let _budget = process_budget();
    let scene = Scene::new();
    // Three providers: alpha (ready), beta (hello-hang), gamma (ready).
    // Canonical order: alpha, beta, gamma. Beta fails in the middle.
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_required("vendor.beta", "persistent-hello-hang");
    scene.stage_required("vendor.gamma", "persistent-ready");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.beta", "vendor.gamma"]);

    let result = scene.run_transaction(&workbench);
    let failure = expect_startup_failure(result);

    // alpha was started and rolled back; beta failed; gamma never started.
    let reaped_ids: Vec<&str> = failure
        .rollback
        .iter()
        .map(|e| e.plugin_id.as_str())
        .collect();
    assert_eq!(
        reaped_ids,
        vec!["vendor.alpha", "vendor.beta"],
        "alpha and beta are in rollback; gamma never started"
    );
    assert_all_reaped(&failure.rollback);

    // gamma was never spawned: no pid file, no startup-sequence entry.
    assert!(
        !scene.record_dir.join("vendor.gamma.pid").exists(),
        "gamma was never spawned"
    );
    let sequence = startup_sequence(&scene.record_dir);
    assert!(
        !sequence.contains(&"vendor.gamma".to_owned()),
        "gamma never appears in the startup sequence"
    );
}

#[test]
fn cwr1_05_unrelated_sentinel_survives_transaction_failure() {
    let _budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("vendor.alpha", "persistent-hello-hang");
    let workbench = scene.build_workbench(&["vendor.alpha"]);

    // An unrelated sentinel the transaction must never touch: the fixture
    // binary itself, in its hang-forever child mode, spawned outside every
    // provider tree with null stdio so it shares no pipe with any provider.
    let mut sentinel = std::process::Command::new(FIXTURE)
        .arg("descendant-hang-child")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn sentinel: {e:?}"));
    let sentinel_pid = sentinel.id();

    let result = scene.run_transaction(&workbench);
    assert!(
        result.is_err(),
        "transaction must fail (hello-hang provider)"
    );

    // The sentinel must still be alive — the transaction must not use broad
    // kill patterns that destroy unrelated processes.
    assert!(
        !process_is_gone(sentinel_pid),
        "unrelated sentinel process survived the transaction failure"
    );

    // Clean up the sentinel through its owned handle.
    sentinel
        .kill()
        .unwrap_or_else(|e| panic!("kill sentinel: {e:?}"));
    let _ = sentinel.wait();
}

#[test]
fn cwr1_05_cleanup_completes_before_error_is_observable() {
    let _budget = process_budget();
    let scene = Scene::new();
    // Two required providers: alpha reaches ready, zeta hangs at hello-ack.
    scene.stage_required("vendor.alpha", "persistent-ready");
    scene.stage_required("vendor.zeta", "persistent-hello-hang");
    let workbench = scene.build_workbench(&["vendor.alpha", "vendor.zeta"]);

    let result = scene.run_transaction(&workbench);

    // The error must be returned (not hung) — cleanup completed before Err.
    let failure = expect_startup_failure(result);
    assert_all_reaped(&failure.rollback);

    // Alpha's PID was recorded; by the time we get here, alpha must be gone.
    let alpha_pid = read_pid(&scene.record_dir, "vendor.alpha");
    assert!(
        process_is_gone(alpha_pid),
        "alpha's leader process was reaped before the caller received Err"
    );

    // Zeta may or may not have a PID file (it hangs at hello-ack, which is
    // before the fixture writes the pid file). If it does, it must be gone.
    let zeta_pid_path = scene.record_dir.join("vendor.zeta.pid");
    if zeta_pid_path.exists() {
        let zeta_pid = read_pid(&scene.record_dir, "vendor.zeta");
        assert!(
            process_is_gone(zeta_pid),
            "zeta's leader process was reaped before the caller received Err"
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Unwrap a preparation failure: panics with context on any other outcome
/// and returns the owning plugin id plus the exact cause.
fn expect_preparation_failure(
    result: Result<ProviderTransactionResult, ProviderTransactionFailure>,
) -> (jefe::domain::Id, PreparationCause) {
    match result {
        Err(ProviderTransactionFailure::Preparation { owner, cause }) => (owner, cause),
        Err(other) => panic!("expected Preparation failure, got: {other}"),
        Ok(_) => panic!("expected failure, got Ok"),
    }
}

/// Unwrap a startup failure (panics on an unexpected Ok with context).
fn expect_startup_failure(
    result: Result<ProviderTransactionResult, ProviderTransactionFailure>,
) -> PersistentStartupFailure {
    match result {
        Err(ProviderTransactionFailure::Startup(failure)) => failure,
        Err(other) => panic!("expected Startup failure, got: {other}"),
        Ok(_) => panic!("expected failure, got Ok"),
    }
}
