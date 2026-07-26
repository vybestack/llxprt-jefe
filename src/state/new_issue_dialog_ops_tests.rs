//! Behavior tests for the inline New Issue form reducer (issue #407).
//!
//! After the rework the form lives in `IssuesState::new_issue_form` (inline
//! composer) instead of `ModalState::NewIssue`. These tests assert the same
//! observable behavior through the inline state.

use crate::domain::{Repository, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt;
use crate::state::types::{InlineState, NewIssueTemplate};

fn issues_mode_state_with_repo(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.apply(AppEvent::EnterIssuesMode).committed_pure()
}

/// Helper: assert the inline New Issue form is open and return a reference
/// to the dialog state.
fn expect_form_open(state: &AppState) -> &crate::state::NewIssueDialogState {
    state
        .issues_state
        .new_issue_form
        .as_ref()
        .unwrap_or_else(|| panic!("expected new_issue_form to be Some"))
}

/// A1: `OpenNewIssueComposer` opens the inline New Issue form and defaults the
/// template to Blank.
#[test]
fn open_new_issue_composer_opens_inline_form_with_blank_template() {
    let state = issues_mode_state_with_repo("repo-1");
    let state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    let dialog = expect_form_open(&state);
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
    // The inline composer must also be active.
    assert!(
        matches!(
            state.issues_state.inline_state,
            InlineState::Composer { .. }
        ),
        "inline_state must be Composer when the form is open"
    );
}

/// A12: Opening the form for a repo with stored sticky milestone/project
/// restores those defaults into the dialog draft fields.
#[test]
fn open_new_issue_composer_restores_sticky_milestone_and_project() {
    let mut state = issues_mode_state_with_repo("repo-1");
    // Seed sticky per-repo defaults.
    state
        .user_preferences
        .update_field_for_repo(&RepositoryId("repo-1".to_string()), |prefs| {
            prefs.last_new_issue_milestone = Some("v1.2".to_string());
            prefs.last_new_issue_project_ids = vec!["PVT_1".to_string()];
        });

    let state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    let dialog = expect_form_open(&state);
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
    let state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    let state = state.apply(AppEvent::NewIssueTemplateNext).committed_pure();
    let dialog = expect_form_open(&state);
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

/// A11: `NewIssueCancel` (Esc) closes the form and discards the draft.
#[test]
fn new_issue_cancel_closes_form_and_discards_draft() {
    let state = issues_mode_state_with_repo("repo-1");
    let state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    let state = state
        .apply(AppEvent::NewIssueTitleChar('x'))
        .committed_pure();
    let state = state.apply(AppEvent::NewIssueCancel).committed_pure();
    assert!(
        state.issues_state.new_issue_form.is_none(),
        "Esc must close the New Issue form"
    );
    assert!(
        matches!(state.issues_state.inline_state, InlineState::None),
        "Esc must also clear the inline composer"
    );
    // Reopen: the draft must be empty (cancel discards, not just hides).
    let state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    let dialog = expect_form_open(&state);
    assert!(
        dialog.title_text.is_empty(),
        "title draft must be discarded on cancel"
    );
    assert!(
        dialog.body_text.is_empty(),
        "body draft must be discarded on cancel"
    );
}

/// A14: A repo switch while the form is open closes the form (mirrors the
/// property-editor reset on repo change).
#[test]
fn repo_change_closes_new_issue_form() {
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
    state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    assert!(state.issues_state.new_issue_form.is_some());
    // Switch repos.
    state = state.apply(AppEvent::SelectRepository(1)).committed_pure();
    assert!(
        state.issues_state.new_issue_form.is_none(),
        "repo change must close the New Issue form"
    );
}

/// A10 (negative): Submitting with an empty title surfaces a footer error and
/// keeps the form open (no spawn).
#[test]
fn submit_with_empty_title_keeps_form_open_and_surfaces_error() {
    let state = issues_mode_state_with_repo("repo-1");
    let state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    let state = state.apply(AppEvent::NewIssueSubmit).committed_pure();
    let dialog = expect_form_open(&state);
    assert!(
        dialog.error.as_deref().is_some_and(|e| e.contains("title")),
        "footer error must mention title, got {:?}",
        dialog.error
    );
}

/// A13: After a successful submit, sticky milestone/project preferences are
/// remembered for the current repo.
#[test]
fn remember_new_issue_preferences_persists_milestone_and_project() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    if let Some(dialog) = state.issues_state.new_issue_form.as_mut() {
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
