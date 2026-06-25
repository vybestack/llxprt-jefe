//! PR-mode PR-list reload/fetch dispatch helpers (stub surface).
//!
//! Compiling, panic-free stubs mirroring `issues_list_dispatch.rs`. The
//! reload helper routes through the reducer via `apply_and_persist` (no
//! spawn yet); the fetch/request helpers are TOTAL NO-OPS. Real behavior is
//! filled in by the P10 RED -> P11 GREEN cycle.
//!
//! @plan PLAN-20260624-PR-MODE.P09
//! @requirement REQ-PR-006
//! @requirement REQ-PR-007
//! @pseudocode component-004 lines 127-137

use jefe::messages::PullRequestsMessage;
use jefe::state::AppEvent;

use super::{AppStateHandle, SharedContext, apply_and_persist};

/// Apply a list-reload message through the reducer (stub — no spawn yet).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-006
/// @requirement REQ-PR-007
/// @pseudocode component-004 lines 101-102
pub fn dispatch_pr_list_reload(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: PullRequestsMessage,
) {
    // P11 adds the gh list fetch after the reducer applies the message; stub
    // routes through the reducer only (no spawn).
    apply_and_persist(app_state, ctx, AppEvent::from(message));
    request_pr_list_reload(app_state, ctx);
}

/// Fetch the PR list page via gh (stub — no I/O).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-006
/// @pseudocode component-004 lines 127-137
pub(super) fn dispatch_pr_list_fetch(_app_state: &mut AppStateHandle, _ctx: &SharedContext) {
    // P11 spawns the gh list fetch; stub returns without spawning.
}

/// Request a fresh PR list reload (stub — no I/O).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-006
/// @requirement REQ-PR-007
/// @pseudocode component-004 lines 127-137
pub(super) fn request_pr_list_reload(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // P11 validates the slug + spawns the reload; stub delegates to the fetch
    // helper so the symbol is reachable (no dead code).
    dispatch_pr_list_fetch(app_state, ctx);
}
