//! Filter-controls key routing for PR Mode (stub surface).
//!
//! Compiling, panic-free stub mirroring `issues_filter::resolve_filter_key_event`.
//! Returns `None` for every key until P11 fills in the real arms.
//!
//! @plan PLAN-20260624-PR-MODE.P09
//! @requirement REQ-PR-008
//! @pseudocode component-003 lines 134-146

use iocraft::prelude::*;

use jefe::state::{AppEvent, AppState};

/// Resolve a key event while PR filter controls are open (stub).
///
/// @plan PLAN-20260624-PR-MODE.P09
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 134-146
pub(super) fn handle_pr_filter_controls_key(
    _state: &AppState,
    _key_event: &KeyEvent,
) -> Option<AppEvent> {
    // P11 wires the eight-field filter cycling; stub returns None.
    None
}
