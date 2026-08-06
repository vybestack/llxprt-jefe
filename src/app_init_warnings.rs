//! Turning startup problems into something the operator actually reads.
//!
//! Every function here exists because a startup problem that is only logged is
//! a startup problem nobody sees. They share one rule: warnings accumulate
//! rather than replace, so no diagnostic can silently displace another.

use jefe::domain::{AgentId, UncleanRun};
use jefe::state::AppState;
use tracing::warn;

pub(super) fn append_warning(state: &mut AppState, warning: String) {
    state.warning_message = Some(match state.warning_message.take() {
        Some(existing) => format!("{existing} {warning}"),
        None => warning,
    });
}

/// Record a held durable read and put the reason where the operator will see
/// it.
///
/// Holding writes without saying so leaves a jefe that looks healthy while
/// persisting nothing, so the hold and its visible explanation are set
/// together rather than by separate callers who might do only one of the two
/// (issue #541).
pub(super) fn surface_durable_read_hold(state: &mut AppState, held: Option<String>) {
    let Some(reason) = held else {
        return;
    };
    append_warning(state, reason.clone());
    state.durable_read_held = Some(reason);
}

/// Report agents whose state startup could not determine.
///
/// A held agent keeps its persisted `Running` status and its binding, which is
/// the correct refusal to guess. But the liveness cycle builds its targets
/// from the runtime's session map, and a held agent was never registered
/// there, so nothing probes it again. Left silent that is a Running agent the
/// operator cannot attach to and is given no reason for, so the hold has to be
/// stated even though it is the safe outcome (issue #541).
pub(super) fn surface_startup_holds(state: &mut AppState, held: &[(AgentId, String)]) {
    let Some((_, first_reason)) = held.first() else {
        return;
    };
    append_warning(
        state,
        format!(
            "{} agent(s) could not be checked at startup and were left untouched: {first_reason}. \
             Their state is unknown, not confirmed.",
            held.len()
        ),
    );
}

pub(super) fn apply_startup_warning(state: &mut AppState, warning: Option<String>) {
    if let Some(warning) = warning {
        append_warning(state, warning);
    }
}

/// Name every prior run that ended without recording a reason.
///
/// The log already carries the finding, but a log nobody opens is how the
/// original incident became undiagnosable; the operator is told in the
/// interface instead (issue #662). Reports accumulate rather than replace, so a
/// vanished run cannot silently displace another startup warning.
pub(super) fn surface_unclean_prior_runs(state: &mut AppState, runs: &[UncleanRun], now_unix: u64) {
    for run in runs {
        append_warning(state, run.notice(now_unix));
    }
}

/// Read the wall clock at the boundary and hand the detected runs to the pure
/// reporter, keeping the clock out of the tested reporting behavior.
///
/// The runs are taken, not borrowed: the next start is the only moment a
/// vanished run can be attributed, so it is reported exactly once.
pub(super) fn report_unclean_prior_runs(state: &mut AppState, ctx: &mut crate::AppContext) {
    let runs = std::mem::take(&mut ctx.unclean_prior_runs);
    surface_unclean_prior_runs(state, &runs, jefe::run_diagnostics::now_unix());
}

/// Decide what a durable-state read means for startup.
///
/// A read that fails is not a read that found nothing. Defaulting here is what
/// let #445 turn an unreadable document into an empty one, because the empty
/// result was then projected straight back over the file. The returned message,
/// when present, both tells the operator what happened and marks writes as held.
pub(super) fn resolve_durable_read(
    read: Result<
        jefe::state::durable_projection::RestoredState,
        jefe::persistence::PersistenceError,
    >,
) -> (
    jefe::state::durable_projection::RestoredState,
    Option<String>,
) {
    match read {
        Ok(value) => (value, None),
        Err(error) => {
            warn!(error = %error, "could not read durable state; holding writes");
            (
                jefe::state::durable_projection::RestoredState::default(),
                Some(format!(
                    "Durable state could not be read ({error}). Agents shown may be incomplete and saving is paused so the existing file is not overwritten."
                )),
            )
        }
    }
}
