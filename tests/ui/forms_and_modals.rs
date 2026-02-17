//! Forms and modals behavior tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P10
//! @requirement REQ-FUNC-004
//! @requirement REQ-FUNC-005
//!
//! These tests verify form input and modal behavior.
//! Acceptance criteria from: analysis/crud-validation-error-matrix.md

use jefe::domain::{Agent, AgentId, Repository, RepositoryId};
use jefe::state::{AppEvent, AppState, ModalState};
use std::path::PathBuf;

/// Create a test state with form-related fields.
fn create_form_test_state() -> AppState {
    let mut state = AppState::default();

    let repo = Repository::new(
        RepositoryId("repo-1".into()),
        "llxprt-code".into(),
        "llxprt-code".into(),
        PathBuf::from("/projects/llxprt-code"),
    );
    state.repositories = vec![repo];

    let agent = Agent::new(
        AgentId("agent-1".into()),
        RepositoryId("repo-1".into()),
        "Test Agent".into(),
        PathBuf::from("/worktrees/test"),
    );
    state.agents = vec![agent];

    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);

    state
}

// ============================================================================
// Modal Open/Close (REQ-FUNC-002)
// ============================================================================

#[test]
fn open_help_modal() {
    let mut state = create_form_test_state();
    state.modal = ModalState::None;

    state = state.apply(AppEvent::OpenHelp);

    assert_eq!(state.modal, ModalState::Help);
}

#[test]
fn close_help_modal() {
    let mut state = create_form_test_state();
    state.modal = ModalState::Help;

    state = state.apply(AppEvent::CloseModal);

    assert_eq!(state.modal, ModalState::None);
}

#[test]
fn open_confirm_delete_agent_modal() {
    let mut state = create_form_test_state();
    state.modal = ModalState::None;
    let agent_id = AgentId("agent-1".into());

    state = state.apply(AppEvent::OpenDeleteAgent(agent_id.clone()));

    assert_eq!(
        state.modal,
        ModalState::ConfirmDeleteAgent {
            id: agent_id,
            delete_work_dir: false
        }
    );
}

#[test]
fn open_new_agent_form() {
    let mut state = create_form_test_state();
    state.modal = ModalState::None;
    let repo_id = RepositoryId("repo-1".into());

    state = state.apply(AppEvent::OpenNewAgent(repo_id.clone()));

    assert!(matches!(
        state.modal,
        ModalState::NewAgent {
            repository_id: id,
            ..
        } if id == repo_id
    ));
}

#[test]
fn open_new_repository_form() {
    let mut state = create_form_test_state();
    state.modal = ModalState::None;

    state = state.apply(AppEvent::OpenNewRepository);

    assert!(matches!(state.modal, ModalState::NewRepository { .. }));
}

// ============================================================================
// Confirm Delete Behavior (REQ-FUNC-004)
// ============================================================================

#[test]
fn toggle_delete_work_dir_in_confirm_modal() {
    let mut state = create_form_test_state();
    let agent_id = AgentId("agent-1".into());
    state.modal = ModalState::ConfirmDeleteAgent {
        id: agent_id.clone(),
        delete_work_dir: false,
    };

    state = state.apply(AppEvent::ToggleDeleteWorkDir);

    assert_eq!(
        state.modal,
        ModalState::ConfirmDeleteAgent {
            id: agent_id,
            delete_work_dir: true
        }
    );
}

#[test]
fn cancel_delete_closes_modal_without_deletion() {
    let mut state = create_form_test_state();
    let agent_id = AgentId("agent-1".into());
    state.modal = ModalState::ConfirmDeleteAgent {
        id: agent_id,
        delete_work_dir: false,
    };

    state = state.apply(AppEvent::CloseModal);

    assert_eq!(state.agents.len(), 1); // Agent still exists
    assert_eq!(state.modal, ModalState::None);
}

// ============================================================================
// Error Visibility (from analysis/crud-validation-error-matrix.md)
// ============================================================================

#[test]
fn error_message_can_be_cleared() {
    let mut state = create_form_test_state();
    state.error_message = Some("Previous error".into());

    state = state.apply(AppEvent::ClearError);

    assert!(state.error_message.is_none());
}

#[test]
fn error_message_persists_until_cleared() {
    let mut state = create_form_test_state();
    state.error_message = Some("Validation failed".into());

    // Do some other action
    state = state.apply(AppEvent::NavigateDown);

    // Error should still be present
    assert!(state.error_message.is_some());
}

// ============================================================================
// Edit Modal Behavior
// ============================================================================

#[test]
fn open_edit_repository_modal() {
    let mut state = create_form_test_state();
    state.modal = ModalState::None;
    let repo_id = RepositoryId("repo-1".into());

    state = state.apply(AppEvent::OpenEditRepository(repo_id.clone()));

    assert!(matches!(
        state.modal,
        ModalState::EditRepository { id, .. } if id == repo_id
    ));
}

#[test]
fn open_edit_agent_modal() {
    let mut state = create_form_test_state();
    state.modal = ModalState::None;
    let agent_id = AgentId("agent-1".into());

    state = state.apply(AppEvent::OpenEditAgent(agent_id.clone()));

    assert!(matches!(
        state.modal,
        ModalState::EditAgent { id, .. } if id == agent_id
    ));
}
