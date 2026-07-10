//! Per-repository user-preference persistence tests (issue #163).
//!
//! Covers enter-mode restore, apply/clear persist, filter-controls field_index
//! restore, per-repo isolation, and merge-chooser restore+persist.

use crate::domain::{
    IssueFilter, IssueFilterState, MergeMethod, PrFilter, PrFilterState, RepoPreferences,
    Repository, RepositoryId, UserPreferences,
};
use crate::state::AppState;
use crate::state::types::{AppEvent, ScreenMode};

use super::prs_test_fixtures::prs_state_with_detail;

// ── Helpers ───────────────────────────────────────────────────────────────

/// Build an AppState with the given repo selected and seeded preferences.
fn state_with_repo_and_prefs(repo_id: &str, prefs: RepoPreferences) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state
        .user_preferences
        .update_for_repo(RepositoryId(repo_id.to_string()), prefs);
    state
}

/// Build an AppState with two repos and seeded preferences for each.
fn state_with_two_repos(
    repo1: &str,
    prefs1: RepoPreferences,
    repo2: &str,
    prefs2: RepoPreferences,
) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo1.to_string()),
        "Repo 1".to_string(),
        repo1.to_string(),
        std::path::PathBuf::from("/tmp/repo1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId(repo2.to_string()),
        "Repo 2".to_string(),
        repo2.to_string(),
        std::path::PathBuf::from("/tmp/repo2"),
    ));
    state.selected_repository_index = Some(0);
    state
        .user_preferences
        .update_for_repo(RepositoryId(repo1.to_string()), prefs1);
    state
        .user_preferences
        .update_for_repo(RepositoryId(repo2.to_string()), prefs2);
    state
}

// ── enter_prs_mode ────────────────────────────────────────────────────────

#[test]
fn enter_prs_mode_restores_remembered_pr_filter() {
    let prefs = RepoPreferences {
        pr_filter: PrFilter {
            state: Some(PrFilterState::Closed),
            ..PrFilter::default()
        },
        ..RepoPreferences::default()
    };
    let state = state_with_repo_and_prefs("repo-1", prefs);
    let state = state.apply(AppEvent::EnterPrsMode);
    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Closed)
    );
    assert_eq!(
        state.prs_state.draft_filter.state,
        Some(PrFilterState::Closed)
    );
}

#[test]
fn enter_prs_mode_defaults_to_open_when_no_prefs() {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Test".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp"),
    ));
    state.selected_repository_index = Some(0);
    let state = state.apply(AppEvent::EnterPrsMode);
    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Open)
    );
}

#[test]
fn enter_prs_mode_restores_search_query() {
    let prefs = RepoPreferences {
        pr_search_query: "foo".to_string(),
        ..RepoPreferences::default()
    };
    let state = state_with_repo_and_prefs("repo-1", prefs);
    let state = state.apply(AppEvent::EnterPrsMode);
    assert_eq!(state.prs_state.search_query, "foo");
}

#[test]
fn enter_prs_mode_restores_field_index() {
    let prefs = RepoPreferences {
        pr_filter_field_index: 3,
        ..RepoPreferences::default()
    };
    let state = state_with_repo_and_prefs("repo-1", prefs);
    let state = state.apply(AppEvent::EnterPrsMode);
    assert_eq!(state.prs_state.filter_ui.field_index, 3);
}

// ── PR apply/clear persist ───────────────────────────────────────────────

#[test]
fn pr_apply_filter_persists_to_prefs() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state.screen_mode = ScreenMode::DashboardPullRequests;
    state.prs_state.active = true;
    state.prs_state.filter_ui.controls_open = true;
    state.prs_state.draft_filter.state = Some(PrFilterState::Closed);
    state.prs_state.draft_filter.author = "alice".to_string();

    let state = state.apply(AppEvent::PrApplyFilter);
    let stored = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(stored.pr_filter, state.prs_state.committed_filter);
}

#[test]
fn pr_clear_filter_persists_open_default() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state.screen_mode = ScreenMode::DashboardPullRequests;
    state.prs_state.active = true;

    let state = state.apply(AppEvent::PrClearFilter);
    let stored = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(stored.pr_filter.state, Some(PrFilterState::Open));
}

#[test]
fn pr_open_filter_controls_restores_field_index() {
    let prefs = RepoPreferences {
        pr_filter_field_index: 2,
        ..RepoPreferences::default()
    };
    let mut state = state_with_repo_and_prefs("repo-1", prefs);
    state.screen_mode = ScreenMode::DashboardPullRequests;
    state.prs_state.active = true;

    let state = state.apply(AppEvent::PrOpenFilterControls);
    assert_eq!(state.prs_state.filter_ui.field_index, 2);
}

// ── Per-repo isolation ───────────────────────────────────────────────────

#[test]
fn pr_prefs_are_per_repo() {
    let prefs1 = RepoPreferences {
        pr_filter: PrFilter {
            state: Some(PrFilterState::Closed),
            ..PrFilter::default()
        },
        ..RepoPreferences::default()
    };
    let prefs2 = RepoPreferences {
        pr_filter: PrFilter {
            state: Some(PrFilterState::Merged),
            ..PrFilter::default()
        },
        ..RepoPreferences::default()
    };
    let mut state = state_with_two_repos("repo-1", prefs1, "repo-2", prefs2);
    state.screen_mode = ScreenMode::DashboardPullRequests;
    state.prs_state.active = true;

    // Enter PR mode with repo-1 selected → Closed.
    let state = state.apply(AppEvent::EnterPrsMode);
    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Closed)
    );

    // Switch to repo-2 → reset_prs_for_repo_change restores Merged.
    let state = state.apply(AppEvent::SelectRepository(1));
    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Merged)
    );
}

// ── enter_issues_mode ────────────────────────────────────────────────────

#[test]
fn enter_issues_mode_restores_remembered_issue_filter() {
    let prefs = RepoPreferences {
        issue_filter: IssueFilter {
            state: Some(IssueFilterState::All),
            ..IssueFilter::default()
        },
        ..RepoPreferences::default()
    };
    let state = state_with_repo_and_prefs("repo-1", prefs);
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::All)
    );
}

#[test]
fn enter_issues_mode_defaults_to_open() {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Test".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp"),
    ));
    state.selected_repository_index = Some(0);
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::Open)
    );
}

#[test]
fn issue_open_filter_controls_restores_field_index() {
    let prefs = RepoPreferences {
        issue_filter_field_index: 4,
        ..RepoPreferences::default()
    };
    let mut state = state_with_repo_and_prefs("repo-1", prefs);
    state.screen_mode = ScreenMode::DashboardIssues;
    state.issues_state.active = true;

    let state = state.apply(AppEvent::OpenFilterControls);
    assert_eq!(state.issues_state.filter_ui.field_index, 4);
}

#[test]
fn issue_apply_filter_persists() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state.screen_mode = ScreenMode::DashboardIssues;
    state.issues_state.active = true;
    state.issues_state.filter_ui.controls_open = true;
    state.issues_state.draft_filter.author = "alice".to_string();

    let state = state.apply(AppEvent::ApplyFilter);
    let stored = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(stored.issue_filter, state.issues_state.committed_filter);
}

#[test]
fn issue_clear_filter_persists() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state.screen_mode = ScreenMode::DashboardIssues;
    state.issues_state.active = true;

    let state = state.apply(AppEvent::ClearFilter);
    let stored = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(stored.issue_filter, state.issues_state.committed_filter);
}

// ── Merge chooser restore + persist ──────────────────────────────────────

#[test]
fn merge_chooser_restores_last_method() {
    let prefs = RepoPreferences {
        last_merge_method: MergeMethod::Squash,
        ..RepoPreferences::default()
    };
    let mut state = prs_state_with_detail("repo-1", 42);
    state
        .user_preferences
        .update_for_repo(RepositoryId("repo-1".to_string()), prefs);

    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let selected = state
        .prs_state
        .merge_chooser
        .as_ref()
        .map_or(usize::MAX, |c| c.selected_index);
    assert_eq!(selected, 1, "Squash is index 1 in MERGE_METHODS");
}

#[test]
fn merge_chooser_defaults_to_merge_when_no_prefs() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let selected = state
        .prs_state
        .merge_chooser
        .as_ref()
        .map_or(usize::MAX, |c| c.selected_index);
    assert_eq!(selected, 0, "Merge is index 0 (default)");
}

#[test]
fn merge_confirm_persists_method() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    // Navigate to Rebase (index 2)
    let state = state.apply(AppEvent::PrMergeNavigateDown);
    let state = state.apply(AppEvent::PrMergeNavigateDown);
    // Confirm twice
    let state = state.apply(AppEvent::PrMergeConfirm);
    let state = state.apply(AppEvent::PrMergeConfirm);

    let stored = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(stored.last_merge_method, MergeMethod::Rebase);
}

#[test]
fn merge_methods_loaded_clamps_disabled_last_method() {
    let prefs = RepoPreferences {
        last_merge_method: MergeMethod::Squash,
        ..RepoPreferences::default()
    };
    let mut state = prs_state_with_detail("repo-1", 42);
    state
        .user_preferences
        .update_for_repo(RepositoryId("repo-1".to_string()), prefs);

    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeMethodsLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 42,
        allowed_methods: vec![MergeMethod::Merge, MergeMethod::Rebase],
    });
    let selected = state
        .prs_state
        .merge_chooser
        .as_ref()
        .map_or(usize::MAX, |c| c.selected_index);
    assert_eq!(
        selected, 0,
        "Squash disabled → clamp to first enabled (Merge=0)"
    );
}

#[test]
fn merge_methods_loaded_keeps_last_method_when_allowed() {
    let prefs = RepoPreferences {
        last_merge_method: MergeMethod::Squash,
        ..RepoPreferences::default()
    };
    let mut state = prs_state_with_detail("repo-1", 42);
    state
        .user_preferences
        .update_for_repo(RepositoryId("repo-1".to_string()), prefs);

    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeMethodsLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 42,
        allowed_methods: vec![MergeMethod::Merge, MergeMethod::Squash],
    });
    let selected = state
        .prs_state
        .merge_chooser
        .as_ref()
        .map_or(usize::MAX, |c| c.selected_index);
    assert_eq!(selected, 1, "Squash allowed → stays at index 1");
}

// ── UserPreferences domain unit tests ────────────────────────────────────

#[test]
fn user_preferences_for_repo_returns_open_defaults_when_absent() {
    let prefs = UserPreferences::default();
    let result = prefs.for_repo(&RepositoryId("unknown".to_string()));
    assert_eq!(result.issue_filter.state, Some(IssueFilterState::Open));
    assert_eq!(result.pr_filter.state, Some(PrFilterState::Open));
}

#[test]
fn user_preferences_update_for_repo_upserts() {
    let mut prefs = UserPreferences::default();
    let repo = RepositoryId("repo-1".to_string());
    prefs.update_for_repo(
        repo.clone(),
        RepoPreferences {
            pr_search_query: "first".to_string(),
            ..RepoPreferences::default()
        },
    );
    assert_eq!(prefs.by_repo.len(), 1);

    // Upsert replaces existing entry.
    prefs.update_for_repo(
        repo.clone(),
        RepoPreferences {
            pr_search_query: "second".to_string(),
            ..RepoPreferences::default()
        },
    );
    assert_eq!(prefs.by_repo.len(), 1);
    assert_eq!(prefs.for_repo(&repo).pr_search_query, "second");
}
