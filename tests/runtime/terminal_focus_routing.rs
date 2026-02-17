//! Terminal focus routing tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P07
//! @requirement REQ-FUNC-005
//! @pseudocode component-002 lines 15-20
//!
//! Tests for input routing based on terminal focus state.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, LaunchSignature, RepositoryId};
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
        mode_flags: agent.mode_flags.clone(),
        pass_continue: agent.pass_continue,
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
        .expect("spawn");
    mgr.attach(&agent.id).expect("attach");

    let result = mgr.write_input(b"test input");
    assert!(result.is_ok(), "write should succeed when attached");
}

#[test]
fn write_input_fails_after_detach() {
    let mut mgr = StubRuntimeManager::default();
    let agent = make_agent("agent-1", "repo-1");
    let sig = make_signature(&agent);

    mgr.spawn_session(&agent.id, &agent.work_dir, &sig)
        .expect("spawn");
    mgr.attach(&agent.id).expect("attach");
    mgr.detach().expect("detach");

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
        .expect("spawn");
    mgr.attach(&agent.id).expect("attach");

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
