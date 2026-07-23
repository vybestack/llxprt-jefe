//! Tests for issue #369: the GitHub Repo field on the New/Edit Repository
//! form must be reachable via Tab navigation and must accept typed input.
//!
//! The root cause was that `RepositoryFormFocus::next()`/`prev()` placed
//! `TransientAgentDir`/`TransientMaxConcurrent` between `DefaultLlxprtVersion`
//! and `GitHubRepo`, but the UI renders them at the bottom of the form.
//! Tabbing forward from the version fields therefore jumped past the
//! visually-adjacent GitHub Repo field.

use crate::state::{AppEvent, AppState, ModalState, RepositoryFormFocus};

/// Opening the New Repository modal should start with focus on Name.
#[test]
fn new_repository_modal_starts_focused_on_name() {
    let state = AppState::default().apply(AppEvent::OpenNewRepository);
    let ModalState::NewRepository { focus, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(focus, RepositoryFormFocus::Name);
}

/// Tabbing forward from DefaultLlxprtVersion must land on GitHubRepo (the
/// visually-adjacent field), not on TransientAgentDir (which is rendered at
/// the bottom of the form). This is the core regression in issue #369.
#[test]
fn llxprt_tab_from_default_version_lands_on_github_repo() {
    let mut state = AppState {
        installed_agent_kinds: vec![crate::domain::AgentKind::Llxprt],
        ..AppState::default()
    };
    state = state.apply(AppEvent::OpenNewRepository);
    // Advance to DefaultLlxprtVersion (skip hidden CodePuppy fields).
    state = state.apply(AppEvent::FormNextField); // Name → BaseDir
    state = state.apply(AppEvent::FormNextField); // BaseDir → DefaultProfile
    state = state.apply(AppEvent::FormNextField); // → DefaultAgentKind
    state = state.apply(AppEvent::FormNextField); // → DefaultLlxprtMode
    state = state.apply(AppEvent::FormNextField); // → DefaultLlxprtVersion

    let ModalState::NewRepository { focus, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(focus, RepositoryFormFocus::DefaultLlxprtVersion);

    // Now Tab forward — must reach GitHubRepo, not TransientAgentDir.
    state = state.apply(AppEvent::FormNextField);
    let ModalState::NewRepository { focus, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(
        focus,
        RepositoryFormFocus::GitHubRepo,
        "Tab from DefaultLlxprtVersion must reach GitHubRepo (issue #369)"
    );
}

/// Shift+Tab backward from GitHubRepo must land on DefaultLlxprtVersion.
#[test]
fn llxprt_shift_tab_from_github_repo_lands_on_default_version() {
    let mut state = AppState {
        installed_agent_kinds: vec![crate::domain::AgentKind::Llxprt],
        ..AppState::default()
    };
    state = state.apply(AppEvent::OpenNewRepository);
    let ModalState::NewRepository { focus, .. } = &mut state.modal else {
        panic!("expected new-repository modal");
    };
    *focus = RepositoryFormFocus::GitHubRepo;

    state = state.apply(AppEvent::FormPrevField);
    let ModalState::NewRepository { focus, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(
        focus,
        RepositoryFormFocus::DefaultLlxprtVersion,
        "Shift+Tab from GitHubRepo must reach DefaultLlxprtVersion (issue #369)"
    );
}

/// Typing into the GitHubRepo field while focused must insert characters.
#[test]
fn github_repo_field_accepts_typed_input_when_focused() {
    let mut state = AppState::default().apply(AppEvent::OpenNewRepository);
    let ModalState::NewRepository { focus, .. } = &mut state.modal else {
        panic!("expected new-repository modal");
    };
    *focus = RepositoryFormFocus::GitHubRepo;

    for ch in "acme/widgets".chars() {
        state = state.apply(AppEvent::FormChar(ch));
    }

    let ModalState::NewRepository { fields, cursor, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(fields.github_repo, "acme/widgets");
    assert_eq!(cursor.github_repo, "acme/widgets".chars().count());
}

/// Tabbing forward from GitHubRepo must land on IssuePrRepo.
#[test]
fn tab_from_github_repo_lands_on_issue_pr_repo() {
    let mut state = AppState::default().apply(AppEvent::OpenNewRepository);
    let ModalState::NewRepository { focus, .. } = &mut state.modal else {
        panic!("expected new-repository modal");
    };
    *focus = RepositoryFormFocus::GitHubRepo;

    state = state.apply(AppEvent::FormNextField);
    let ModalState::NewRepository { focus, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(focus, RepositoryFormFocus::IssuePrRepo);
}

/// TransientAgentDir and TransientMaxConcurrent must be focusable at the
/// end of the chain (after SetupEnvDefault), matching their render position.
#[test]
fn transient_fields_are_reachable_after_setup_env_default() {
    let mut state = AppState {
        installed_agent_kinds: vec![crate::domain::AgentKind::Llxprt],
        ..AppState::default()
    };
    state = state.apply(AppEvent::OpenNewRepository);
    let ModalState::NewRepository { focus, .. } = &mut state.modal else {
        panic!("expected new-repository modal");
    };
    *focus = RepositoryFormFocus::SetupEnvDefault;

    state = state.apply(AppEvent::FormNextField);
    let ModalState::NewRepository { focus, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(focus, RepositoryFormFocus::TransientAgentDir);

    state = state.apply(AppEvent::FormNextField);
    let ModalState::NewRepository { focus, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(focus, RepositoryFormFocus::TransientMaxConcurrent);
}

/// Wrapping backward from Name must land on TransientMaxConcurrent (last
/// visible field at the bottom of the form).
#[test]
fn shift_tab_from_name_wraps_to_transient_max_concurrent() {
    let mut state = AppState {
        installed_agent_kinds: vec![crate::domain::AgentKind::Llxprt],
        ..AppState::default()
    };
    state = state.apply(AppEvent::OpenNewRepository);
    // Focus starts on Name.
    state = state.apply(AppEvent::FormPrevField);
    let ModalState::NewRepository { focus, .. } = state.modal else {
        panic!("expected new-repository modal");
    };
    assert_eq!(focus, RepositoryFormFocus::TransientMaxConcurrent);
}
