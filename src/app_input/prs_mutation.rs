//! PR-mode inline-mutation dispatch helpers (stub surface).
//!
//! Compiling, panic-free stub mirroring `issues_mutation::handle_inline_submit`.
//! Returns without spawning any I/O; real behavior is filled in by the
//! P10 RED -> P11 GREEN cycle.
//!
//! @plan PLAN-20260624-PR-MODE.P09
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011
//! @pseudocode component-003 lines 109-119

use super::{AppStateHandle, SharedContext};

/// Handle an inline submit for PR Mode (stub — no I/O).
///
/// Called from the dispatch layer when `PrInlineSubmit` is applied. P11
/// resolves the submit info and spawns the gh comment-create task; stub
/// returns without spawning.
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-010
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 109-119
pub fn handle_pr_inline_submit(_app_state: &mut AppStateHandle, _ctx: &SharedContext) {
    // P11 spawns the gh PR comment create; stub returns without spawning.
}
