//! Serial post-commit effect executor (issue #381 CW01-10).
//!
//! The root shell commits a bounded [`crate::state::transition::Transition`],
//! releases every state/`HookState` guard, and only then hands the issued
//! effects to [`run_effects`]. Effects execute serially in transition order
//! through one [`EffectAdapter`]; each terminal outcome is delivered as a
//! typed [`EffectCompletion`] through the caller's deliver callback, which may
//! stage bounded follow-up effects. Follow-ups append after the original
//! batch and the combined follow-up count is capped at
//! [`MAX_TRANSITION_EFFECTS`]; rejected follow-ups are reported, never
//! silently dropped. No queue, worker handle, or adapter enters `AppState`.

use std::collections::VecDeque;

use crate::domain::effects::{
    EffectCompletion, EffectError, EffectErrorKind, EffectResponse, IssuedEffect,
    MAX_TRANSITION_EFFECTS, RetryPolicy,
};

/// Outcome of one adapter execution attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterExecution {
    /// The effect finished with a typed success or failure.
    Completed(Result<EffectResponse, EffectError>),
    /// The adapter accepted the effect and will deliver its completion later
    /// through the message bus (off-thread work). No completion is delivered
    /// now; the pending correlation stays registered.
    Deferred,
}

/// Boundary implemented by the root shell over the real platform adapters.
pub trait EffectAdapter {
    /// Execute one issued effect attempt.
    fn execute(&mut self, issued: &IssuedEffect) -> AdapterExecution;
}

/// Observable summary of one executor run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExecutionReport {
    /// Completions delivered synchronously during this run.
    pub delivered: usize,
    /// Effects handed off for deferred (bus-delivered) completion.
    pub deferred: usize,
    /// Follow-up effects rejected by the combined 64 bound.
    pub rejected_follow_ups: usize,
}

/// Execute committed effects serially in transition order.
///
/// `deliver` receives each terminal completion and returns follow-up effects;
/// follow-ups append after the original batch, with the combined follow-up
/// count capped at [`MAX_TRANSITION_EFFECTS`]. The caller must have released
/// all state access before calling this function; `deliver` is the only place
/// state may be re-borrowed, and the adapter never runs inside it.
pub fn run_effects<A, D>(
    effects: Vec<IssuedEffect>,
    adapter: &mut A,
    mut deliver: D,
) -> ExecutionReport
where
    A: EffectAdapter + ?Sized,
    D: FnMut(EffectCompletion) -> Vec<IssuedEffect>,
{
    let mut queue: VecDeque<IssuedEffect> = effects.into();
    let mut report = ExecutionReport::default();
    let mut accepted_follow_ups = 0usize;

    while let Some(issued) = queue.pop_front() {
        let outcome = execute_with_retry(&issued, adapter);
        let result = match outcome {
            AdapterExecution::Deferred => {
                report.deferred += 1;
                continue;
            }
            AdapterExecution::Completed(result) => enforce_family(&issued, result),
        };
        let completion = EffectCompletion {
            correlation: issued.correlation,
            result,
        };
        report.delivered += 1;
        for follow_up in deliver(completion) {
            if accepted_follow_ups < MAX_TRANSITION_EFFECTS {
                accepted_follow_ups += 1;
                queue.push_back(follow_up);
            } else {
                report.rejected_follow_ups += 1;
            }
        }
    }
    report
}

/// Execute one effect, retrying retryable failures only under an
/// idempotent-query policy, up to its checked total attempts.
fn execute_with_retry<A>(issued: &IssuedEffect, adapter: &mut A) -> AdapterExecution
where
    A: EffectAdapter + ?Sized,
{
    let max_attempts = match issued.retry {
        RetryPolicy::Never => 1,
        RetryPolicy::IdempotentQuery { max_attempts } => usize::from(max_attempts.get()),
    };
    let mut last = adapter.execute(issued);
    for _ in 1..max_attempts {
        match &last {
            AdapterExecution::Completed(Err(error)) if error.retryable => {
                last = adapter.execute(issued);
            }
            _ => break,
        }
    }
    last
}

/// Convert a family-mismatched success into a typed validation failure so a
/// wrong-family payload can never reach the reducer as a success.
fn enforce_family(
    issued: &IssuedEffect,
    result: Result<EffectResponse, EffectError>,
) -> Result<EffectResponse, EffectError> {
    match result {
        Ok(response) if response.family() != issued.effect.family() => Err(EffectError::new(
            EffectErrorKind::Validation,
            false,
            "adapter response family does not match the issued effect family",
        )),
        other => other,
    }
}
