//! Terminal focus routing tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P07
//! @requirement REQ-FUNC-005
//! @pseudocode component-002 lines 15-20
//!
//! Tests for input routing based on terminal focus state.

use crate::support::TestResultExt;

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, LaunchSignature, RemoteRepositorySettings, RepositoryId};
use jefe::runtime::{RuntimeError, RuntimeManager, StubRuntimeManager};
use jefe::state::{AppEvent, AppState, PaneFocus};

fn make_agent(id: &str, repo_id: &str) -> Agent {
    Agent::new(
        AgentId(id.into()),
        RepositoryId(repo_id.into()),
        format!("Test Agent {id}"),
        PathBuf::from(format!("/tmp/test/{id}")),
    )
}

fn make_signature(agent: &Agent) -> LaunchSignature {
    LaunchSignature {
        work_dir: agent.work_dir.clone(),
        profile: agent.profile.clone(),
        code_puppy_model: String::new(),
        code_puppy_yolo: Some(false),
        mode_flags: agent.mode_flags.clone(),
        llxprt_debug: agent.llxprt_debug.clone(),
        pass_continue: agent.pass_continue,
        sandbox_enabled: agent.sandbox_enabled,
        sandbox_engine: agent.sandbox_engine,
        sandbox_flags: agent.sandbox_flags.clone(),
        remote: RemoteRepositorySettings::default(),
        agent_kind: jefe::domain::AgentKind::Llxprt,
    }
}

// =============================================================================
// Terminal Focus State (component-002 lines 15-17)
// =============================================================================

#[test]
fn terminal_unfocused_by_default() {
    let state = AppState::default();
    assert!(
        !state.terminal_focused,
        "terminal should be unfocused by default"
    );
}

#[test]
fn toggle_terminal_focus_enables_focus() {
    let state = AppState::default();
    let state = state.apply(AppEvent::ToggleTerminalFocus);
    assert!(
        state.terminal_focused,
        "terminal should be focused after toggle"
    );
}

#[test]
fn toggle_terminal_focus_twice_disables_focus() {
    let state = AppState::default();
    let state = state.apply(AppEvent::ToggleTerminalFocus);
    let state = state.apply(AppEvent::ToggleTerminalFocus);
    assert!(
        !state.terminal_focused,
        "terminal should be unfocused after two toggles"
    );
}

// =============================================================================
// Input Routing - Write Input (component-002 lines 15-20)
// =============================================================================

#[test]
fn write_input_fails_without_attached_viewer() {
    let mut mgr = StubRuntimeManager::default();
    let result = mgr.write_input(b"test input");
    assert!(
        matches!(result, Err(RuntimeError::NoAttachedViewer)),
        "write should fail when no viewer attached"
    );
}

#[test]
fn write_input_succeeds_with_attached_viewer() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .test_unwrap("spawn");
    mgr.attach(&agent.id).test_unwrap("attach");

    let result = mgr.write_input(b"test input");
    assert!(result.is_ok(), "write should succeed when attached");
}

#[test]
fn write_input_fails_after_detach() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .test_unwrap("spawn");
    mgr.attach(&agent.id).test_unwrap("attach");
    mgr.detach().test_unwrap("detach");

    let result = mgr.write_input(b"test input");
    assert!(
        matches!(result, Err(RuntimeError::NoAttachedViewer)),
        "write should fail after detach"
    );
}

// =============================================================================
// Resize Routing
// =============================================================================

#[test]
fn resize_fails_without_attached_viewer() {
    let mut mgr = StubRuntimeManager::default();
    let result = mgr.resize(24, 80);
    assert!(
        matches!(result, Err(RuntimeError::NoAttachedViewer)),
        "resize should fail when no viewer attached"
    );
}

#[test]
fn resize_succeeds_with_attached_viewer() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .test_unwrap("spawn");
    mgr.attach(&agent.id).test_unwrap("attach");

    let result = mgr.resize(24, 80);
    assert!(result.is_ok(), "resize should succeed when attached");
}

// =============================================================================
// Focus + Attachment Integration
// =============================================================================

#[test]
fn pane_focus_can_reach_terminal() {
    let state = AppState::default();
    assert_eq!(state.pane_focus, PaneFocus::Repositories);

    let state = state.apply(AppEvent::CyclePaneFocus);
    assert_eq!(state.pane_focus, PaneFocus::Agents);

    let state = state.apply(AppEvent::CyclePaneFocus);
    assert_eq!(state.pane_focus, PaneFocus::Terminal);

    let state = state.apply(AppEvent::CyclePaneFocus);
    assert_eq!(state.pane_focus, PaneFocus::Repositories);
}

#[test]
fn terminal_focus_is_independent_of_pane_focus() {
    // terminal_focused flag is separate from pane_focus
    let state = AppState::default();

    // Cycle to Terminal pane
    let state = state.apply(AppEvent::CyclePaneFocus);
    let state = state.apply(AppEvent::CyclePaneFocus);
    assert_eq!(state.pane_focus, PaneFocus::Terminal);
    assert!(
        !state.terminal_focused,
        "terminal_focused should still be false"
    );

    // Toggle terminal focus
    let state = state.apply(AppEvent::ToggleTerminalFocus);
    assert!(
        state.terminal_focused,
        "terminal_focused should now be true"
    );

    // Cycle away from Terminal pane
    let state = state.apply(AppEvent::CyclePaneFocus);
    assert_eq!(state.pane_focus, PaneFocus::Repositories);
    assert!(
        state.terminal_focused,
        "terminal_focused should persist across pane changes"
    );
}

// =============================================================================
// Issue #197: terminal text selection & copy for Code Puppy sessions
// =============================================================================
//
// These tests exercise the production gesture-ownership state machine
// (`jefe::selection::GestureState`), which the terminal mouse router uses to
// decide, per gesture, whether Jefe paints a selection/copy or the events
// forward to the child PTY. A left-button gesture has a single latched owner,
// decided at gesture start.

use jefe::selection::{
    GestureAction, GestureEvent, GestureEventKind, GestureState, SelectablePane, SelectionPoint,
};

/// A resolver that maps in-pane coordinates to a TerminalView selection point.
/// Returns `None` for coordinates outside the content area (mimicking the real
/// pane geometry) so the function is not trivially always-`Some`.
fn resolver(col: u16, row: u16) -> Option<SelectionPoint> {
    if col < 2 || row < 2 {
        return None;
    }
    Some(SelectionPoint::new(SelectablePane::TerminalView, 0, 0))
}

fn event(kind: GestureEventKind, shift: bool, reporting: bool) -> GestureEvent {
    // Use in-pane coordinates (>= 2) so the resolver yields a selection point.
    GestureEvent {
        kind,
        shift_held: shift,
        col: 5,
        row: 5,
        mouse_reporting_active: reporting,
    }
}

/// The left-button text-selection gesture is Jefe-owned, so a drag over the
/// focused terminal paints a Jefe selection (and copies on release) even when
/// the child advertises mouse reporting. This is the core #197 regression:
/// Code Puppy drags were forwarded to the PTY and lost their selection.
#[test]
fn left_button_drag_selects_even_with_mouse_reporting() {
    // Reporting child, non-shift left-down → pending (buffered).
    let (action, state) =
        GestureState::default().process(event(GestureEventKind::LeftDown, false, true), &resolver);
    assert!(matches!(action, GestureAction::Noop));
    assert!(matches!(state, GestureState::Pending { .. }));
    // Drag → Jefe owns the gesture (selection range begins, spanning the
    // buffered down through the drag coordinate).
    let (action, state) = state.process(event(GestureEventKind::LeftDrag, false, true), &resolver);
    assert!(
        matches!(action, GestureAction::BeginSelectionRange { .. }),
        "drag over reporting child must begin a Jefe selection range, got {action:?}"
    );
    assert!(matches!(state, GestureState::JefeOwned { .. }));

    // Complete the gesture: LeftUp from JefeOwned must finalize + copy.
    let (action, state) = state.process(event(GestureEventKind::LeftUp, false, true), &resolver);
    assert_eq!(action, GestureAction::FinalizeAndCopy);
    assert_eq!(state, GestureState::Idle);
}

/// Shift-drag must never silently disappear. A Shift left-down latches Jefe
/// ownership immediately, so the gesture highlights cells and copies — it is
/// no longer a no-op (#197).
#[test]
fn shift_drag_always_routes_to_app_selection() {
    for reporting in [false, true] {
        let (action, state) = GestureState::default().process(
            event(GestureEventKind::LeftDown, true, reporting),
            &resolver,
        );
        assert!(
            matches!(action, GestureAction::BeginSelection(_)),
            "shift left-down must begin a Jefe selection for reporting={reporting}, got {action:?}"
        );
        assert!(matches!(state, GestureState::JefeOwned { .. }));
    }
}

/// A pure click (down then up, no drag) on a reporting child forwards BOTH the
/// down and the up to the PTY — preserving terminal left-click interaction for
/// agents and transient menus that genuinely drive mouse reporting (the #197
/// requirement). This also guards the critical fix: the up was previously
/// never emitted, leaving the child's button stuck.
#[test]
fn reporting_click_forwards_down_and_up_to_pty() {
    let (action, pending) =
        GestureState::default().process(event(GestureEventKind::LeftDown, false, true), &resolver);
    assert!(matches!(action, GestureAction::Noop));
    let (action, state) = pending.process(event(GestureEventKind::LeftUp, false, true), &resolver);
    match action {
        GestureAction::ForwardToPty(replays) => {
            assert_eq!(
                replays.len(),
                2,
                "reporting click must replay down + up, got {replays:?}"
            );
            assert_eq!(replays[0].kind, GestureEventKind::LeftDown);
            assert_eq!(replays[1].kind, GestureEventKind::LeftUp);
        }
        other => panic!("expected ForwardToPty for reporting click, got {other:?}"),
    }
    assert_eq!(state, GestureState::Idle);
}

/// Without mouse reporting, a non-selection gesture (wheel/right/middle) does
/// not forward to the PTY; it falls through (Noop) so app-level handling
/// applies.
#[test]
fn non_selection_gesture_noops_when_mouse_reporting_inactive() {
    let (action, state) = GestureState::default()
        .process(event(GestureEventKind::ScrollDown, false, false), &resolver);
    assert!(matches!(action, GestureAction::Noop));
    assert_eq!(state, GestureState::default());
}

/// With mouse reporting, a wheel event forwards to the child PTY.
#[test]
fn wheel_forwards_when_mouse_reporting_active() {
    let (action, _state) = GestureState::default()
        .process(event(GestureEventKind::ScrollDown, false, true), &resolver);
    match action {
        GestureAction::ForwardToPty(replays) => {
            assert_eq!(replays.len(), 1);
            assert_eq!(replays[0].kind, GestureEventKind::ScrollDown);
        }
        other => panic!("expected ForwardToPty for reporting wheel, got {other:?}"),
    }
}
