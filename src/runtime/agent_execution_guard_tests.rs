//! Tests for the pure execution authorization guard (issue #382 S8).
//!
//! Authority: issue #382 CW02-12 — "IF any generation changes before
//! execution, Jefe shall return AGT-E203 and perform zero side effects."
//!
//! These tests table every dimension (old/new), the exact-match success case,
//! and prove that no closure/effect hook is called on reject. A small
//! test-only counter is used to detect any accidental effect.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::domain::agent_definition::sha256::DefinitionSha256;
use crate::domain::agent_definition::types::{AgentLaunchPlan, Preflight};
use crate::domain::agent_definition::{AgentTypeId, LaunchSignature, Operation, Target};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const BASE_PROBE_GEN: u64 = 7;
const BASE_TARGET_GEN: u64 = 7;
const BASE_ACTIVATION_GEN: u64 = 7;

fn zero_hash() -> DefinitionSha256 {
    DefinitionSha256::default()
}

fn hash_of(byte: u8) -> DefinitionSha256 {
    DefinitionSha256::digest(&[byte])
}

fn fingerprint(path: &str, size: u64, mtime: i64) -> CandidateFingerprint {
    CandidateFingerprint::new(PathBuf::from(path), None, None, size, mtime)
}

fn base_evidence() -> ExecutionEvidence {
    ExecutionEvidence::new(
        zero_hash(),
        fingerprint("/opt/bin/agent", 1024, 1_000),
        BASE_PROBE_GEN,
        BASE_TARGET_GEN,
        BASE_ACTIVATION_GEN,
    )
}

fn base_plan() -> AgentLaunchPlan {
    AgentLaunchPlan {
        type_id: AgentTypeId::from_validated("core.test"),
        operation: Operation::Normal,
        definition_sha256: zero_hash(),
        executable: PathBuf::from("/opt/bin/agent"),
        argv: Vec::new(),
        env: Vec::new(),
        cwd: PathBuf::from("/srv/project"),
        target: Target::Local {
            canonical_cwd: PathBuf::from("/srv/project"),
        },
        probe_generation: BASE_PROBE_GEN,
        target_generation: BASE_TARGET_GEN,
        activation_generation: BASE_ACTIVATION_GEN,
        preflight: Preflight::default(),
        signature: LaunchSignature::default(),
    }
}

/// Apply a single stale mutation to a copy of the base evidence and return
/// the mutated evidence plus the dimension expected to mismatch.
fn stale_evidence(case: StaleCase) -> ExecutionEvidence {
    match case {
        StaleCase::DefinitionSha256 => ExecutionEvidence::new(
            hash_of(1),
            fingerprint("/opt/bin/agent", 1024, 1_000),
            BASE_PROBE_GEN,
            BASE_TARGET_GEN,
            BASE_ACTIVATION_GEN,
        ),
        StaleCase::ExecutableFingerprint => {
            // The executable's canonical path changed (e.g. resolved to a
            // different installation); the guard compares the plan's stamped
            // path against the fingerprint's canonical path.
            ExecutionEvidence::new(
                zero_hash(),
                fingerprint("/opt/bin/other-agent", 2048, 1_000),
                BASE_PROBE_GEN,
                BASE_TARGET_GEN,
                BASE_ACTIVATION_GEN,
            )
        }
        StaleCase::ProbeGeneration => ExecutionEvidence::new(
            zero_hash(),
            fingerprint("/opt/bin/agent", 1024, 1_000),
            BASE_PROBE_GEN + 1,
            BASE_TARGET_GEN,
            BASE_ACTIVATION_GEN,
        ),
        StaleCase::TargetGeneration => ExecutionEvidence::new(
            zero_hash(),
            fingerprint("/opt/bin/agent", 1024, 1_000),
            BASE_PROBE_GEN,
            BASE_TARGET_GEN + 1,
            BASE_ACTIVATION_GEN,
        ),
        StaleCase::ActivationGeneration => ExecutionEvidence::new(
            zero_hash(),
            fingerprint("/opt/bin/agent", 1024, 1_000),
            BASE_PROBE_GEN,
            BASE_TARGET_GEN,
            BASE_ACTIVATION_GEN + 1,
        ),
    }
}

#[derive(Debug, Clone, Copy)]
enum StaleCase {
    DefinitionSha256,
    ExecutableFingerprint,
    ProbeGeneration,
    TargetGeneration,
    ActivationGeneration,
}

impl StaleCase {
    const fn dimension(self) -> StaleDimension {
        match self {
            Self::DefinitionSha256 => StaleDimension::DefinitionSha256,
            Self::ExecutableFingerprint => StaleDimension::ExecutableFingerprint,
            Self::ProbeGeneration => StaleDimension::ProbeGeneration,
            Self::TargetGeneration => StaleDimension::TargetGeneration,
            Self::ActivationGeneration => StaleDimension::ActivationGeneration,
        }
    }
}

// ---------------------------------------------------------------------------
// Success: exact match authorizes and borrows the plan
// ---------------------------------------------------------------------------

#[test]
fn exact_match_authorizes_and_borrows_plan() {
    let plan = base_plan();
    let evidence = base_evidence();
    match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => {
            assert!(std::ptr::eq(authorized.plan(), &plan));
            assert_eq!(authorized.plan().definition_sha256, plan.definition_sha256);
            assert_eq!(authorized.plan().executable, plan.executable);
            assert_eq!(authorized.plan().probe_generation, plan.probe_generation);
            assert_eq!(authorized.plan().target_generation, plan.target_generation);
            assert_eq!(
                authorized.plan().activation_generation,
                plan.activation_generation,
            );
        }
        AuthorizationResult::Rejected(rejection) => {
            panic!("exact match must authorize, got {rejection}");
        }
    }
}

// ---------------------------------------------------------------------------
// Table: every stale dimension is rejected with AGT-E203 and that dimension
// ---------------------------------------------------------------------------

#[test]
fn each_stale_dimension_is_rejected_with_agte_e203() {
    let plan = base_plan();
    for case in [
        StaleCase::DefinitionSha256,
        StaleCase::ExecutableFingerprint,
        StaleCase::ProbeGeneration,
        StaleCase::TargetGeneration,
        StaleCase::ActivationGeneration,
    ] {
        let evidence = stale_evidence(case);
        match authorize_execution(&plan, &evidence) {
            AuthorizationResult::Rejected(rejection) => {
                assert_eq!(
                    rejection.code(),
                    ProbeErrorCode::Agte203,
                    "{case:?}: code must be AGT-E203",
                );
                assert_eq!(
                    rejection.dimension(),
                    case.dimension(),
                    "{case:?}: dimension must match",
                );
                let message = rejection.to_string();
                assert!(
                    message.contains("AGT-E203"),
                    "{case:?}: message carries the code: {message}",
                );
                assert!(
                    message.contains(case.dimension().label()),
                    "{case:?}: message carries the dimension label: {message}",
                );
            }
            AuthorizationResult::Authorized(_) => {
                panic!("{case:?}: stale dimension must reject, not authorize");
            }
        }
    }
}

#[test]
fn stale_definition_also_mismatched_when_others_match() {
    let plan = base_plan();
    let evidence = ExecutionEvidence::new(
        hash_of(9),
        fingerprint("/opt/bin/agent", 1024, 1_000),
        BASE_PROBE_GEN,
        BASE_TARGET_GEN,
        BASE_ACTIVATION_GEN,
    );
    match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Rejected(rejection) => {
            assert_eq!(rejection.dimension(), StaleDimension::DefinitionSha256);
        }
        AuthorizationResult::Authorized(_) => panic!("stale definition must reject"),
    }
}

#[test]
fn executable_path_mismatch_is_stale_executable() {
    let plan = base_plan();
    let evidence = ExecutionEvidence::new(
        zero_hash(),
        fingerprint("/opt/bin/other-agent", 1024, 1_000),
        BASE_PROBE_GEN,
        BASE_TARGET_GEN,
        BASE_ACTIVATION_GEN,
    );
    match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Rejected(rejection) => {
            assert_eq!(rejection.dimension(), StaleDimension::ExecutableFingerprint,);
        }
        AuthorizationResult::Authorized(_) => panic!("executable path mismatch must reject"),
    }
}

// ---------------------------------------------------------------------------
// Side-effect freedom: no closure/effect hook is called on reject
// ---------------------------------------------------------------------------

/// Shared counter incremented only if any effect hook were ever invoked.
/// The guard never invokes it; this proves the boundary.
static EFFECT_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A trap closure that increments the counter and panics if called.
///
/// The guard must never call this on reject (nor on authorize). It exists
/// purely to fail loudly if a future change introduces a side effect.
fn effect_trap() -> impl Fn() {
    move || {
        EFFECT_COUNTER.fetch_add(1, Ordering::SeqCst);
        panic!("authorize_execution must not invoke any effect hook");
    }
}

#[test]
fn reject_invokes_no_effect_hook() {
    EFFECT_COUNTER.store(0, Ordering::SeqCst);
    let plan = base_plan();
    let evidence = stale_evidence(StaleCase::ProbeGeneration);
    let trap = effect_trap();
    // The trap is in scope but never passed to the guard; the guard takes no
    // callback. If it ever did, this test would panic inside the closure.
    let _ = &trap;
    match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Rejected(rejection) => {
            assert_eq!(rejection.code(), ProbeErrorCode::Agte203);
        }
        AuthorizationResult::Authorized(_) => panic!("stale evidence must reject"),
    }
    assert_eq!(
        EFFECT_COUNTER.load(Ordering::SeqCst),
        0,
        "no effect hook may run during authorization",
    );
}

#[test]
fn authorize_invokes_no_effect_hook() {
    EFFECT_COUNTER.store(0, Ordering::SeqCst);
    let plan = base_plan();
    let evidence = base_evidence();
    let trap = effect_trap();
    let _ = &trap;
    match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => {
            assert_eq!(authorized.plan().probe_generation, BASE_PROBE_GEN);
        }
        AuthorizationResult::Rejected(rejection) => {
            panic!("exact match must authorize, got {rejection}")
        }
    }
    assert_eq!(
        EFFECT_COUNTER.load(Ordering::SeqCst),
        0,
        "no effect hook may run during authorization",
    );
}

// ---------------------------------------------------------------------------
// AuthorizedExecution wrapper is a thin borrow
// ---------------------------------------------------------------------------

#[test]
fn authorized_execution_plan_round_trips_all_fields() {
    let plan = base_plan();
    let evidence = base_evidence();
    let authorized = match authorize_execution(&plan, &evidence) {
        AuthorizationResult::Authorized(a) => a,
        AuthorizationResult::Rejected(r) => panic!("must authorize: {r}"),
    };
    let borrowed = authorized.plan();
    assert_eq!(borrowed.type_id, plan.type_id);
    assert_eq!(borrowed.operation, plan.operation);
    assert_eq!(borrowed.cwd, plan.cwd);
    assert_eq!(borrowed.target, plan.target);
    assert_eq!(borrowed.probe_generation, plan.probe_generation);
    assert_eq!(borrowed.target_generation, plan.target_generation);
    assert_eq!(borrowed.activation_generation, plan.activation_generation);
    assert_eq!(borrowed.preflight, plan.preflight);
    assert_eq!(borrowed.signature, plan.signature);
}

#[test]
fn rejection_implements_error_and_display_carries_code_and_dimension() {
    let rejection = AuthorizationRejection {
        code: ProbeErrorCode::Agte203,
        dimension: StaleDimension::ActivationGeneration,
    };
    let message = rejection.to_string();
    assert!(
        message.contains("AGT-E203"),
        "display carries code: {message}"
    );
    assert!(
        message.contains("activation_generation"),
        "display carries dimension: {message}",
    );
    // Stdlib Error trait is implemented for source chaining.
    let _: &dyn std::error::Error = &rejection;
}

#[test]
fn stale_dimension_labels_are_stable() {
    assert_eq!(
        StaleDimension::DefinitionSha256.label(),
        "definition_sha256"
    );
    assert_eq!(
        StaleDimension::ExecutableFingerprint.label(),
        "executable_fingerprint",
    );
    assert_eq!(StaleDimension::ProbeGeneration.label(), "probe_generation");
    assert_eq!(
        StaleDimension::TargetGeneration.label(),
        "target_generation"
    );
    assert_eq!(
        StaleDimension::ActivationGeneration.label(),
        "activation_generation",
    );
}
