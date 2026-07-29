//! Test-only panic helpers for clippy-clean assertions.

use std::fmt::Debug;
use std::sync::{Mutex, MutexGuard, OnceLock};

use jefe::domain::agent_definition::AgentLaunchPlan;
use jefe::runtime::agent_execution_guard::{
    AuthorizationResult, ExecutionEvidence, authorize_execution,
};
use jefe::runtime::agent_preflight::{
    AuthorizedLaunchPlan, PreparationOutcome, ProcessSandboxInspector, prepare_execution,
};

pub trait TestOptionExt<T> {
    fn test_unwrap(self, context: &str) -> T;
}

impl<T> TestOptionExt<T> for Option<T> {
    fn test_unwrap(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}"),
        }
    }
}

pub trait TestResultExt<T> {
    fn test_unwrap(self, context: &str) -> T;
}

impl<T, E> TestResultExt<T> for Result<T, E>
where
    E: Debug,
{
    fn test_unwrap(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

pub fn nested_cargo_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .test_unwrap("nested Cargo command lock should not be poisoned")
}

/// Build an [`AuthorizedLaunchPlan`] from a fixture plan through the real
/// authorization + preflight proof chain.
///
/// This constructs the runtime launch proof the same way production does:
/// `authorize_execution` (S8) then `prepare_execution` (S10) then
/// [`AuthorizedLaunchPlan::from_cleared`]. The evidence is derived from the
/// plan's own generation-bearing fields so the fixture's defaults (all zero)
/// authorize trivially. No private field is forged and no backdoor is used —
/// the proof is assembled entirely through the public typed APIs.
#[must_use]
pub fn authorized_launch_plan(plan: &AgentLaunchPlan) -> AuthorizedLaunchPlan {
    let evidence = ExecutionEvidence::new(
        plan.definition_sha256,
        plan.executable_fingerprint.clone(),
        plan.probe_generation,
        plan.target_generation,
        plan.activation_generation,
    );
    let authorized = match authorize_execution(plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(error) => {
            panic!("fixture plan must authorize: {error}")
        }
    };
    let cleared = match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
        PreparationOutcome::Cleared(cleared) => cleared,
        PreparationOutcome::Unavailable(reason) => {
            panic!("fixture plan must clear preflight: {reason}")
        }
    };
    AuthorizedLaunchPlan::from_cleared(cleared, plan.clone(), evidence)
        .unwrap_or_else(|error| panic!("fixture plan must seal: {error}"))
}
