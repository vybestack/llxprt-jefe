//! Forms and modals behavior tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P10
//! @requirement REQ-FUNC-004
//! @requirement REQ-FUNC-005
//!
//! These tests verify form input and modal behavior.
//! Acceptance criteria from: analysis/crud-validation-error-matrix.md

use jefe::domain::{Agent, AgentId, Repository, RepositoryId};
use jefe::state::{AgentFormFocus, AppEvent, AppState, ModalState, RepositoryFormFocus};
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
            delete_work_dir: false,
            confirm_focus: jefe::state::ConfirmFocus::Cancel,
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

#[test]
fn submit_new_repository_form_creates_repository() {
    let mut state = create_form_test_state();
    let initial_count = state.repositories.len();

    state = state.apply(AppEvent::OpenNewRepository);
    for c in "NewRepo".chars() {
        state = state.apply(AppEvent::FormChar(c));
    }
    state = state.apply(AppEvent::SubmitForm);

    assert_eq!(state.repositories.len(), initial_count + 1);
    assert_eq!(
        state.repositories.last().map(|r| r.name.as_str()),
        Some("NewRepo")
    );
    assert_eq!(state.modal, ModalState::None);
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
        confirm_focus: jefe::state::ConfirmFocus::Cancel,
    };

    state = state.apply(AppEvent::ToggleDeleteWorkDir);

    assert_eq!(
        state.modal,
        ModalState::ConfirmDeleteAgent {
            id: agent_id,
            delete_work_dir: true,
            confirm_focus: jefe::state::ConfirmFocus::Cancel,
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
        confirm_focus: jefe::state::ConfirmFocus::Cancel,
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

#[test]
fn open_new_agent_form_initializes_llxprt_debug_blank() {
    let mut state = create_form_test_state();
    state.modal = ModalState::None;
    let repo_id = RepositoryId("repo-1".into());

    state = state.apply(AppEvent::OpenNewAgent(repo_id));

    let ModalState::NewAgent { fields, .. } = state.modal else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert!(fields.llxprt_debug.is_empty());
}

#[test]
fn open_edit_agent_form_copies_llxprt_debug_value() {
    let mut state = create_form_test_state();
    state.agents[0].llxprt_debug = "trace=agent".into();
    state.modal = ModalState::None;
    let agent_id = AgentId("agent-1".into());

    state = state.apply(AppEvent::OpenEditAgent(agent_id));

    let ModalState::EditAgent { fields, .. } = state.modal else {
        panic!("expected edit-agent modal, got {:?}", state.modal);
    };
    assert_eq!(fields.llxprt_debug, "trace=agent");
}

#[test]
fn submit_new_agent_form_trims_llxprt_debug() {
    let mut state = create_form_test_state();
    state.modal = ModalState::None;
    let repo_id = RepositoryId("repo-1".into());

    state = state.apply(AppEvent::OpenNewAgent(repo_id));

    let ModalState::NewAgent { fields, .. } = &mut state.modal else {
        panic!("expected new-agent modal");
    };
    fields.name = "Agent With Debug".into();
    fields.work_dir = "/tmp/agent-with-debug".into();
    fields.llxprt_debug = "   io=trace   ".into();

    state = state.apply(AppEvent::SubmitForm);
    let Some(created) = state
        .agents
        .iter()
        .find(|agent| agent.name == "Agent With Debug")
    else {
        panic!("new agent should be created");
    };

    assert_eq!(created.llxprt_debug, "io=trace");
}

#[test]
fn repository_form_cursor_moves_and_inserts_in_place() {
    let mut state = create_form_test_state();

    state = state.apply(AppEvent::OpenNewRepository);
    state = state.apply(AppEvent::FormChar('a'));
    state = state.apply(AppEvent::FormChar('c'));
    state = state.apply(AppEvent::FormMoveCursorLeft);
    state = state.apply(AppEvent::FormChar('b'));

    let ModalState::NewRepository {
        fields,
        focus,
        cursor,
    } = state.modal
    else {
        panic!("expected new-repository modal, got {:?}", state.modal);
    };
    assert_eq!(focus, RepositoryFormFocus::Name);
    assert_eq!(fields.name, "abc");
    assert_eq!(cursor.name, 2);
}

#[test]
fn repository_form_toggles_remote_fields() {
    let mut state = create_form_test_state();

    state = state.apply(AppEvent::OpenNewRepository);
    state = state.apply(AppEvent::FormNextField); // Base Dir
    state = state.apply(AppEvent::FormNextField); // Default Profile
    state = state.apply(AppEvent::FormNextField); // Default Agent Kind (hidden Code Puppy model skipped)
    state = state.apply(AppEvent::FormNextField); // GitHub Repo
    state = state.apply(AppEvent::FormNextField); // Issues / PRs Repo
    state = state.apply(AppEvent::FormNextField); // RemoteEnabled
    state = state.apply(AppEvent::FormToggleCheckbox);
    state = state.apply(AppEvent::FormNextField); // LoginUser
    state = state.apply(AppEvent::FormChar('o'));
    state = state.apply(AppEvent::FormChar('p'));
    state = state.apply(AppEvent::FormNextField); // Host
    state = state.apply(AppEvent::FormChar('1'));
    state = state.apply(AppEvent::FormChar('0'));
    state = state.apply(AppEvent::FormNextField); // SshPort
    state = state.apply(AppEvent::FormNextField); // IdentityFile
    state = state.apply(AppEvent::FormNextField); // SshOptions
    state = state.apply(AppEvent::FormNextField); // RunAsUser
    state = state.apply(AppEvent::FormChar('m'));
    state = state.apply(AppEvent::FormNextField); // SetupEnvDefault
    state = state.apply(AppEvent::FormToggleCheckbox);

    let ModalState::NewRepository {
        fields,
        focus,
        cursor,
    } = state.modal
    else {
        panic!("expected new-repository modal, got {:?}", state.modal);
    };
    assert_eq!(focus, RepositoryFormFocus::SetupEnvDefault);
    assert!(!fields.default_agent_kind.is_empty());
    assert_eq!(
        fields.default_agent_kind,
        jefe::domain::AgentKind::Llxprt.label()
    );
    assert!(fields.remote_enabled);
    assert_eq!(fields.login_user, "op");
    assert_eq!(fields.host, "10");
    assert_eq!(fields.run_as_user, "m");
    assert!(fields.setup_env_default);
    assert_eq!(cursor.login_user, 2);
    assert_eq!(cursor.host, 2);
    assert_eq!(cursor.run_as_user, 1);
}

#[test]
fn agent_form_cursor_delete_and_backspace_are_caret_based() {
    let mut state = create_form_test_state();

    state = state.apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".into())));
    state = state.apply(AppEvent::FormNextField); // Name
    state = state.apply(AppEvent::FormChar('a'));
    state = state.apply(AppEvent::FormChar('b'));
    state = state.apply(AppEvent::FormChar('c'));
    state = state.apply(AppEvent::FormMoveCursorLeft); // ab|c
    state = state.apply(AppEvent::FormDelete); // remove c => ab|
    state = state.apply(AppEvent::FormBackspace); // remove b => a|

    let ModalState::NewAgent {
        fields,
        focus,
        cursor,
        ..
    } = state.modal
    else {
        panic!("expected new-agent modal, got {:?}", state.modal);
    };
    assert_eq!(focus, AgentFormFocus::Name);
    assert_eq!(fields.name, "a");
    assert_eq!(cursor.name, 1);
}
