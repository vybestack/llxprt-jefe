//! Sort preference persistence coverage extracted from the repository-preference suite.

use super::*;

#[test]
fn enter_issues_mode_restores_remembered_issue_sort_config() {
    let prefs = RepoPreferences {
        issue_sort_config: crate::domain::IssueSortConfig {
            by: crate::domain::IssueSortBy::Number,
            order: crate::domain::SortOrder::Asc,
        },
        ..RepoPreferences::default()
    };
    let state = state_with_repo_and_prefs("repo-1", prefs);
    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    assert_eq!(
        state.issues_state.sort_config.by,
        crate::domain::IssueSortBy::Number
    );
    assert_eq!(
        state.issues_state.sort_config.order,
        crate::domain::SortOrder::Asc
    );
}

#[test]
fn cycle_issue_sort_by_persists_to_prefs() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    state = state.apply(AppEvent::CycleIssueSortByNext).committed_pure();
    let repo_id = RepositoryId("repo-1".to_string());
    let prefs = state.user_preferences.for_repo(&repo_id);
    assert_eq!(
        prefs.issue_sort_config.by,
        crate::domain::IssueSortBy::Priority
    );
}

#[test]
fn cycle_pr_sort_by_persists_to_prefs() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    state = state.apply(AppEvent::PrCycleSortByNext).committed_pure();
    let repo_id = RepositoryId("repo-1".to_string());
    let prefs = state.user_preferences.for_repo(&repo_id);
    assert_eq!(prefs.pr_sort_config.by, crate::domain::PrSortBy::Number);
}

#[test]
fn enter_prs_mode_restores_remembered_pr_sort_config() {
    let prefs = RepoPreferences {
        pr_sort_config: crate::domain::PrSortConfig {
            by: crate::domain::PrSortBy::Created,
            order: crate::domain::SortOrder::Asc,
        },
        ..RepoPreferences::default()
    };
    let state = state_with_repo_and_prefs("repo-1", prefs);
    let state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    assert_eq!(
        state.prs_state.sort_config.by,
        crate::domain::PrSortBy::Created
    );
    assert_eq!(
        state.prs_state.sort_config.order,
        crate::domain::SortOrder::Asc
    );
}
