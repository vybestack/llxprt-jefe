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
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.apply(AppEvent::EnterIssuesMode).committed_pure()
}

/// Helper: assert the inline New Issue form is open and return a reference
/// to the form state.
fn expect_form_open(state: &AppState) -> &crate::state::NewIssueFormState {
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
    let form = expect_form_open(&state);
    assert_eq!(
        form.template,
        NewIssueTemplate::Blank,
        "template must default to Blank"
    );
    assert!(
        form.title_text.is_empty(),
        "title must start empty for Blank template"
    );
    assert!(
        form.body_text.is_empty(),
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
/// restores those defaults into the form draft fields.
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
    let form = expect_form_open(&state);
    assert_eq!(
        form.milestone,
        Some("v1.2".to_string()),
        "sticky milestone must be restored"
    );
    assert_eq!(
        form.project_ids,
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
    let form = expect_form_open(&state);
    assert_eq!(form.template, NewIssueTemplate::Bug);
    assert!(
        form.body_text.contains("## What happened?"),
        "Bug template must scaffold the body, got {:?}",
        form.body_text
    );
    assert!(
        form.title_text.is_empty(),
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
    let form = expect_form_open(&state);
    assert!(
        form.title_text.is_empty(),
        "title draft must be discarded on cancel"
    );
    assert!(
        form.body_text.is_empty(),
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
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("repo-2".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
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
    assert!(
        matches!(state.issues_state.inline_state, InlineState::None),
        "repo change must also clear the inline composer state"
    );
}

/// A10 (negative): Submitting with an empty title surfaces a footer error and
/// keeps the form open (no spawn).
#[test]
fn submit_with_empty_title_keeps_form_open_and_surfaces_error() {
    let state = issues_mode_state_with_repo("repo-1");
    let state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    let state = state.apply(AppEvent::NewIssueSubmit).committed_pure();
    let form = expect_form_open(&state);
    assert!(
        form.error.as_deref().is_some_and(|e| e.contains("title")),
        "footer error must mention title, got {:?}",
        form.error
    );
}

/// A9: A successful submit inserts the issue into the list, closes the form,
/// shows a draft notice, clears `mutation_pending`, and remembers sticky
/// milestone/project preferences.
#[test]
fn new_issue_created_closes_form_inserts_issue_and_remembers_preferences() {
    use crate::domain::{Issue, IssueState};

    let mut state = issues_mode_state_with_repo("repo-1");
    state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    if let Some(form) = state.issues_state.new_issue_form.as_mut() {
        form.title_text = "My new issue".to_string();
        form.milestone = Some("v9.9".to_string());
        form.project_ids = vec!["PVT_42".to_string()];
    }
    // Simulate the app_input layer marking the mutation pending (it does this
    // before spawning the create task).
    state.issues_state.next_mutation_id = 1;
    state.issues_state.mutation_pending = Some(crate::state::IssueMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        id: 1,
        target: InlineState::None,
    });
    let issue = Issue {
        number: 77,
        node_id: "I_node".to_string(),
        title: "My new issue".to_string(),
        state: IssueState::Open,
        author_login: "tester".to_string(),
        updated_at: "2026-07-25T00:00:00Z".to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
        state_reason: None,
    };
    let state = state
        .apply(AppEvent::NewIssueCreated {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            mutation_id: 1,
            issue: Box::new(issue),
        })
        .committed_pure();
    assert!(
        state.issues_state.new_issue_form.is_none(),
        "form must be closed after a successful create"
    );
    assert_eq!(
        state.issues_state.mutation_pending, None,
        "mutation_pending must be cleared"
    );
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("Created issue #77"),
        "draft notice must announce the new issue number"
    );
    assert_eq!(state.issues_state.issues().len(), 1);
    assert_eq!(state.issues_state.issues()[0].number, 77);
    let prefs = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(prefs.last_new_issue_milestone, Some("v9.9".to_string()));
    assert_eq!(prefs.last_new_issue_project_ids, vec!["PVT_42".to_string()]);
}

/// A13: After a successful submit, sticky milestone/project preferences are
/// remembered for the current repo.
#[test]
fn remember_new_issue_preferences_persists_milestone_and_project() {
    let mut state = issues_mode_state_with_repo("repo-1");
    state = state.apply(AppEvent::OpenNewIssueComposer).committed_pure();
    if let Some(form) = state.issues_state.new_issue_form.as_mut() {
        form.milestone = Some("v9.9".to_string());
        form.project_ids = vec!["PVT_42".to_string()];
    }
    // Apply the sticky-remember op directly (the submit pipeline calls this).
    state.remember_new_issue_preferences();
    let prefs = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(prefs.last_new_issue_milestone, Some("v9.9".to_string()));
    assert_eq!(prefs.last_new_issue_project_ids, vec!["PVT_42".to_string()]);
}
