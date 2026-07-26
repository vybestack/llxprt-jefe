//! Behavior tests for the New Issue dialog reducer (issue #407).
//!
//! RED stage: these tests assert behavior that does not yet exist
//! (`ModalState::NewIssue`, `OpenNewIssueDialog`, sticky milestone/project
//! restore) and must fail to compile/run until Slice A lands the state type,
//! the modal variant, and the reducer op.

use crate::domain::{Repository, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{ModalState, NewIssueTemplate};

fn issues_mode_state_with_repo(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.apply(AppEvent::EnterIssuesMode)
}

/// A1: `OpenNewIssueDialog` opens the New Issue form modal and defaults the
/// template to Blank.
#[test]
fn open_new_issue_dialog_opens_modal_with_blank_template() {
    let state = issues_mode_state_with_repo("repo-1");
    let state = state.apply(AppEvent::OpenNewIssueDialog);
    assert!(
        matches!(state.modal, ModalState::NewIssue { .. }),
        "expected ModalState::NewIssue, got {:?}",
        state.modal
    );
    if let ModalState::NewIssue { state: dialog, .. } = &state.modal {
        assert_eq!(
            dialog.template,
            NewIssueTemplate::Blank,
            "template must default to Blank"
        );
        assert!(
            dialog.title_text.is_empty(),
            "title must start empty for Blank template"
        );
        assert!(
            dialog.body_text.is_empty(),
            "body must start empty for Blank template"
        );
    }
}

/// A12: Opening the dialog for a repo with stored sticky milestone/project
/// restores those defaults into the dialog draft fields.
#[test]
fn open_new_issue_dialog_restores_sticky_milestone_and_project() {
    let mut state = issues_mode_state_with_repo("repo-1");
    // Seed sticky per-repo defaults.
    state
        .user_preferences
        .update_field_for_repo(&RepositoryId("repo-1".to_string()), |prefs| {
            prefs.last_new_issue_milestone = Some("v1.2".to_string());
            prefs.last_new_issue_project_ids = vec!["PVT_1".to_string()];
        });

    let state = state.apply(AppEvent::OpenNewIssueDialog);
    let ModalState::NewIssue { state: dialog, .. } = &state.modal else {
        panic!("expected ModalState::NewIssue");
    };
    assert_eq!(
        dialog.milestone,
        Some("v1.2".to_string()),
        "sticky milestone must be restored"
    );
    assert_eq!(
        dialog.project_ids,
        vec!["PVT_1".to_string()],
        "sticky project must be restored"
    );
}

/// A2: Cycling the template Blank -> Bug prefills the Bug body scaffold and
/// leaves the title empty for the user to type.
#[test]
fn cycling_template_to_bug_prefills_body_scaffold() {
    let state = issues_mode_state_with_repo("repo-1");
    let state = state.apply(AppEvent::OpenNewIssueDialog);
    let state = state.apply(AppEvent::NewIssueTemplateNext);
    let ModalState::NewIssue { state: dialog, .. } = &state.modal else {
        panic!("expected ModalState::NewIssue");
    };
    assert_eq!(dialog.template, NewIssueTemplate::Bug);
    assert!(
        dialog.body_text.contains("## What happened?"),
        "Bug template must scaffold the body, got {:?}",
        dialog.body_text
    );
    assert!(
        dialog.title_text.is_empty(),
        "title must stay empty for the user to fill"
    );
}

/// A11: `NewIssueCancel` (Esc) closes the dialog and discards the draft.
#[test]
fn new_issue_cancel_closes_modal_and_discards_draft() {
    let state = issues_mode_state_with_repo("repo-1");
    let state = state.apply(AppEvent::OpenNewIssueDialog);
    let state = state.apply(AppEvent::NewIssueTitleChar('x'));
    let state = state.apply(AppEvent::NewIssueCancel);
    assert!(
        matches!(state.modal, ModalState::None),
        "Esc must close the New Issue dialog"
    );
}

/// A14: A repo switch while the dialog is open closes the dialog (mirrors the
/// property-editor reset on repo change).
#[test]
fn repo_change_closes_new_issue_dialog() {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        "Repo 2".to_string(),
        "repo-2".to_string(),
        std::path::PathBuf::from("/tmp/r2"),
    ));
    state.selected_repository_index = Some(0);
    state = state.apply(AppEvent::EnterIssuesMode);
    state = state.apply(AppEvent::OpenNewIssueDialog);
    assert!(matches!(state.modal, ModalState::NewIssue { .. }));
    // Switch repos.
    state = state.apply(AppEvent::SelectRepository(1));
    assert!(
        matches!(state.modal, ModalState::None),
        "repo change must close the New Issue dialog"
    );
}

/// A10 (negative): Submitting with an empty title surfaces a footer error and
/// keeps the dialog open (no spawn).
#[test]
fn submit_with_empty_title_keeps_dialog_open_and_surfaces_error() {
    let state = issues_mode_state_with_repo("repo-1");
    let state = state.apply(AppEvent::OpenNewIssueDialog);
    let state = state.apply(AppEvent::NewIssueSubmit);
    assert!(
        matches!(state.modal, ModalState::NewIssue { .. }),
        "empty-title submit must keep the dialog open"
    );
    if let ModalState::NewIssue { state: dialog, .. } = &state.modal {
        assert!(
            dialog.error.as_deref().is_some_and(|e| e.contains("title")),
            "footer error must mention title, got {:?}",
            dialog.error
        );
    }
}

/// A13: After a successful submit, sticky milestone/project preferences are
/// remembered for the current repo.
#[test]
fn remember_new_issue_preferences_persists_milestone_and_project() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state = state.apply(AppEvent::OpenNewIssueDialog);
    if let ModalState::NewIssue { state: dialog, .. } = &mut state.modal {
        dialog.milestone = Some("v9.9".to_string());
        dialog.project_ids = vec!["PVT_42".to_string()];
    }
    // Apply the sticky-remember op directly (the submit pipeline calls this).
    state.remember_new_issue_preferences();
    let prefs = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(prefs.last_new_issue_milestone, Some("v9.9".to_string()));
    assert_eq!(prefs.last_new_issue_project_ids, vec!["PVT_42".to_string()]);
}
