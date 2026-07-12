//! Per-repository user-preference persistence tests (issue #163).
//!
//! Covers enter-mode restore, apply/clear persist, filter-controls field_index
//! restore, per-repo isolation, and merge-chooser restore+persist.

use crate::domain::{
    IssueFilter, IssueFilterState, MergeMethod, PrFilter, PrFilterState, RepoPreferences,
    Repository, RepositoryId, UserPreferences,
};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::ScreenMode;
use crate::state::{ISSUE_FILTER_FIELD_COUNT, PR_FILTER_FIELD_COUNT};

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
        .update_for_repo(&RepositoryId(repo_id.to_string()), prefs);
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
        .update_for_repo(&RepositoryId(repo1.to_string()), prefs1);
    state
        .user_preferences
        .update_for_repo(&RepositoryId(repo2.to_string()), prefs2);
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
    assert_eq!(stored.pr_filter.state, Some(PrFilterState::Closed));
    assert_eq!(stored.pr_filter.author, "alice");
}

#[test]
fn pr_clear_filter_persists_open_default() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state.screen_mode = ScreenMode::DashboardPullRequests;
    state.prs_state.active = true;
    // Seed a search query so we can prove ClearFilter also clears it.
    state.prs_state.search_query = "stale query".to_string();

    let state = state.apply(AppEvent::PrClearFilter);
    let stored = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(stored.pr_filter.state, Some(PrFilterState::Open));
    // Clearing all filters must also clear the persisted search query so the
    // restored state stays consistent (issue #163).
    assert!(
        stored.pr_search_query.is_empty(),
        "PrClearFilter must clear the search query, got: {}",
        stored.pr_search_query
    );
    assert!(
        state.prs_state.search_query.is_empty(),
        "live search_query must be cleared by PrClearFilter"
    );
}

#[test]
fn pr_open_filter_controls_keeps_restored_field_index() {
    let prefs = RepoPreferences {
        pr_filter_field_index: 2,
        ..RepoPreferences::default()
    };
    let mut state = state_with_repo_and_prefs("repo-1", prefs);
    state.screen_mode = ScreenMode::DashboardPullRequests;
    state.prs_state.active = true;

    // Enter mode restores field_index=2 into live state; opening filter
    // controls must preserve it (not reset to 0).
    let state = state.apply(AppEvent::EnterPrsMode);
    assert_eq!(state.prs_state.filter_ui.field_index, 2);
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
fn issue_open_filter_controls_keeps_restored_field_index() {
    let prefs = RepoPreferences {
        issue_filter_field_index: 4,
        ..RepoPreferences::default()
    };
    let mut state = state_with_repo_and_prefs("repo-1", prefs);
    state.screen_mode = ScreenMode::DashboardIssues;
    state.issues_state.active = true;

    // Enter mode restores field_index=4 into live state; opening filter
    // controls must preserve it (not reset to 0).
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(state.issues_state.filter_ui.field_index, 4);
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
    assert_eq!(stored.issue_filter.author, "alice");
}

#[test]
fn issue_clear_filter_persists() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state.screen_mode = ScreenMode::DashboardIssues;
    state.issues_state.active = true;
    // Seed a search query so we can prove ClearFilter also clears it.
    state.issues_state.search_query = "stale query".to_string();

    let state = state.apply(AppEvent::ClearFilter);
    let stored = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(
        stored.issue_filter,
        IssueFilter {
            state: Some(IssueFilterState::Open),
            ..IssueFilter::default()
        }
    );
    // Clearing all filters must also clear the persisted search query so the
    // restored state stays consistent (issue #163).
    assert!(
        stored.issue_search_query.is_empty(),
        "ClearFilter must clear the search query, got: {}",
        stored.issue_search_query
    );
    assert!(
        state.issues_state.search_query.is_empty(),
        "live search_query must be cleared by ClearFilter"
    );
}

#[test]
fn issue_clear_draft_filter_defaults_to_open() {
    // ClearDraftFilter resets the in-progress draft to the Open default (issue
    // #163: the filter-state default is Open, matching ClearFilter).
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    state.screen_mode = ScreenMode::DashboardIssues;
    state.issues_state.active = true;
    state.issues_state.draft_filter.state = Some(IssueFilterState::Closed);

    let state = state.apply(AppEvent::ClearDraftFilter);
    assert_eq!(
        state.issues_state.draft_filter.state,
        Some(IssueFilterState::Open),
        "ClearDraftFilter must reset the draft state to the Open default"
    );
}

// ── Merge chooser restore + persist ──────────────────────────────────────

#[test]
fn merge_chooser_restores_last_method() {
    let prefs = RepoPreferences {
        last_merge_method: Some(MergeMethod::Squash),
        ..RepoPreferences::default()
    };
    let mut state = prs_state_with_detail("repo-1", 42);
    state
        .user_preferences
        .update_for_repo(&RepositoryId("repo-1".to_string()), prefs);

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
    assert_eq!(stored.last_merge_method, Some(MergeMethod::Rebase));
}

#[test]
fn merge_methods_loaded_clamps_disabled_last_method() {
    let prefs = RepoPreferences {
        last_merge_method: Some(MergeMethod::Squash),
        ..RepoPreferences::default()
    };
    let mut state = prs_state_with_detail("repo-1", 42);
    state
        .user_preferences
        .update_for_repo(&RepositoryId("repo-1".to_string()), prefs);

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
        last_merge_method: Some(MergeMethod::Squash),
        ..RepoPreferences::default()
    };
    let mut state = prs_state_with_detail("repo-1", 42);
    state
        .user_preferences
        .update_for_repo(&RepositoryId("repo-1".to_string()), prefs);

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
        &repo,
        RepoPreferences {
            pr_search_query: "first".to_string(),
            ..RepoPreferences::default()
        },
    );
    assert_eq!(prefs.by_repo.len(), 1);

    // Upsert replaces existing entry.
    prefs.update_for_repo(
        &repo,
        RepoPreferences {
            pr_search_query: "second".to_string(),
            ..RepoPreferences::default()
        },
    );
    assert_eq!(prefs.by_repo.len(), 1);
    assert_eq!(prefs.for_repo(&repo).pr_search_query, "second");
}

// ── FIX 5: RepoPreferences Default produces Open filters ─────────────────

#[test]
fn repo_preferences_default_has_open_filters() {
    let prefs = RepoPreferences::default();
    assert_eq!(prefs.issue_filter.state, Some(IssueFilterState::Open));
    assert_eq!(prefs.pr_filter.state, Some(PrFilterState::Open));
}

#[test]
fn partial_repo_preferences_deserializes_to_open_filters() {
    let json = serde_json::json!({
        "issue_search_query": "needle",
    });
    let prefs: RepoPreferences =
        serde_json::from_value(json).unwrap_or_else(|e| panic!("deserialize partial: {e:?}"));
    assert_eq!(prefs.issue_filter.state, Some(IssueFilterState::Open));
    assert_eq!(prefs.pr_filter.state, Some(PrFilterState::Open));
    assert_eq!(prefs.issue_search_query, "needle");
}

// ── FIX 6: Deleted repo prunes its preferences ───────────────────────────

#[test]
fn deleted_repo_prunes_its_preferences() {
    use crate::state::delete_selected_repository;

    let prefs1 = RepoPreferences {
        issue_search_query: "repo1-search".to_string(),
        ..RepoPreferences::default()
    };
    let prefs2 = RepoPreferences {
        issue_search_query: "repo2-search".to_string(),
        ..RepoPreferences::default()
    };
    let mut state = state_with_two_repos("repo-1", prefs1, "repo-2", prefs2);

    // Delete repo-1. delete_selected_repository removes it from self.repositories
    // AND prunes its stored preferences (issue #163).
    delete_selected_repository(&mut state, &RepositoryId("repo-1".to_string()));

    // repo-1 prefs should be gone, repo-2 prefs should remain.
    assert!(
        state
            .user_preferences
            .by_repo
            .iter()
            .all(|(id, _)| id != &RepositoryId("repo-1".to_string()))
    );
    let repo2_prefs = state
        .user_preferences
        .for_repo(&RepositoryId("repo-2".to_string()));
    assert_eq!(repo2_prefs.issue_search_query, "repo2-search");
}

// ── Save-before-switch: uncommitted selections must persist before the
//    repo index changes (issue #163). Without this, move_repo_selection
//    changes the selected repo and reset_*_for_repo_change overwrites the
//    live filter/search with the NEW repo's prefs, silently discarding the
//    OLD repo's uncommitted state.

/// Switching repos in issues mode must persist the OLD repo's current
/// search query before the new repo's preferences are restored.
#[test]
fn issue_repo_switch_persists_old_search_before_switch() {
    use crate::state::IssueFocus;

    // repo-1 has empty prefs; repo-2 has empty prefs. Enter issues mode on
    // repo-1, type a search query (not yet applied), then switch down to
    // repo-2 — the typed query must be saved for repo-1.
    let mut state = state_with_two_repos(
        "repo-1",
        RepoPreferences::default(),
        "repo-2",
        RepoPreferences::default(),
    );
    state = state.apply(AppEvent::EnterIssuesMode);
    state.issues_state.issue_focus = IssueFocus::RepoList;
    state = state.apply(AppEvent::SetSearchQuery {
        query: "rust-bug".to_string(),
    });

    // Switch down to repo-2.
    state = state.apply(AppEvent::IssuesNavigateDown);

    // repo-1's typed search query must now be in its stored prefs.
    let repo1_prefs = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(
        repo1_prefs.issue_search_query, "rust-bug",
        "old repo's search query must be persisted before the repo switch"
    );

    // Switch back up to repo-1 and re-enter mode to confirm it restores.
    state = state.apply(AppEvent::IssuesNavigateUp);
    state = state.apply(AppEvent::RefocusIssueList);
    assert_eq!(state.issues_state.search_query, "rust-bug");
}

/// Switching repos in PR mode must persist the OLD repo's current search
/// query before the new repo's preferences are restored.
#[test]
fn pr_repo_switch_persists_old_search_before_switch() {
    use crate::state::PrFocus;

    let mut state = state_with_two_repos(
        "repo-1",
        RepoPreferences::default(),
        "repo-2",
        RepoPreferences::default(),
    );
    state = state.apply(AppEvent::EnterPrsMode);
    state.prs_state.pr_focus = PrFocus::RepoList;
    state = state.apply(AppEvent::PrSetSearchQuery {
        query: "draft-prs".to_string(),
    });

    // Switch down to repo-2.
    state = state.apply(AppEvent::PrNavigateDown);

    let repo1_prefs = state
        .user_preferences
        .for_repo(&RepositoryId("repo-1".to_string()));
    assert_eq!(
        repo1_prefs.pr_search_query, "draft-prs",
        "old repo's search query must be persisted before the repo switch"
    );

    // Switch back up to repo-1 and confirm it restores.
    state = state.apply(AppEvent::PrNavigateUp);
    state = state.apply(AppEvent::RefocusPrList);
    assert_eq!(state.prs_state.search_query, "draft-prs");
}

/// A stale/corrupted persisted field_index beyond the valid range is clamped
/// on mode entry so it cannot drive the filter cursor out of bounds.
#[test]
fn restore_clamps_stale_pr_filter_field_index() {
    // Seed an out-of-range pr_filter_field_index (well beyond the field count).
    let prefs = RepoPreferences {
        pr_filter_field_index: 999,
        ..RepoPreferences::default()
    };
    let state = state_with_repo_and_prefs("repo-1", prefs);
    let state = state.apply(AppEvent::EnterPrsMode);
    assert!(
        state.prs_state.filter_ui.field_index < PR_FILTER_FIELD_COUNT,
        "restored field_index must be clamped within the valid field range"
    );
}

/// A stale/corrupted persisted issue filter field_index is clamped on entry.
#[test]
fn restore_clamps_stale_issue_filter_field_index() {
    let prefs = RepoPreferences {
        issue_filter_field_index: 999,
        ..RepoPreferences::default()
    };
    let state = state_with_repo_and_prefs("repo-1", prefs);
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert!(
        state.issues_state.filter_ui.field_index < ISSUE_FILTER_FIELD_COUNT,
        "restored field_index must be clamped within the valid field range"
    );
}

// ── Dashboard repo selection must not leak filters across repos (issue #163).
//    select_repository_by_index (the dashboard / jump-to-agent path) flips the
//    selected index and then restores the NEW repo's prefs. It must persist
//    the OLD repo's live filter first, otherwise the OLD repo's selection is
//    lost AND the live prs_state/issues_state filter is wrongly carried over.

/// Switching repos from the dashboard while PR mode is active must save the
/// old repo's applied PR filter before restoring the new repo's filter, so
/// the filter does NOT leak across repos.
#[test]
fn pr_dashboard_repo_switch_does_not_leak_filter() {
    let mut state = state_with_two_repos(
        "llxprt-code",
        RepoPreferences::default(),
        "jefe",
        RepoPreferences::default(),
    );
    // Enter PR mode on repo-1 (llxprt-code).
    state = state.apply(AppEvent::EnterPrsMode);
    // Apply a Closed filter on repo-1.
    state = state.apply(AppEvent::PrOpenFilterControls);
    state = state.apply(AppEvent::PrCycleFilterState); // Open -> Closed
    state = state.apply(AppEvent::PrApplyFilter);
    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Closed)
    );

    // Switch to repo-2 (jefe) via the dashboard selection path.
    state = state.apply(AppEvent::SelectRepository(1));

    // repo-2 must NOT carry repo-1's Closed filter — it must be Open (default).
    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Open),
        "repo-2 (jefe) must not inherit repo-1's (llxprt-code) filter"
    );
    // repo-1's filter must have been persisted before the switch.
    let repo1_prefs = state
        .user_preferences
        .for_repo(&RepositoryId("llxprt-code".to_string()));
    assert_eq!(
        repo1_prefs.pr_filter.state,
        Some(PrFilterState::Closed),
        "repo-1's applied filter must be persisted before the repo switch"
    );
}

/// Switching repos from the dashboard while issues mode is active must save
/// the old repo's applied issue filter before restoring the new repo's.
#[test]
fn issue_dashboard_repo_switch_does_not_leak_filter() {
    let mut state = state_with_two_repos(
        "llxprt-code",
        RepoPreferences::default(),
        "jefe",
        RepoPreferences::default(),
    );
    // Enter issues mode on repo-1 (llxprt-code).
    state = state.apply(AppEvent::EnterIssuesMode);
    // Apply a Closed filter on repo-1.
    state = state.apply(AppEvent::OpenFilterControls);
    state = state.apply(AppEvent::CycleFilterState); // Open -> Closed
    state = state.apply(AppEvent::ApplyFilter);
    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::Closed)
    );

    // Switch to repo-2 (jefe) via the dashboard selection path.
    state = state.apply(AppEvent::SelectRepository(1));

    // repo-2 must NOT carry repo-1's Closed filter.
    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::Open),
        "repo-2 (jefe) must not inherit repo-1's (llxprt-code) filter"
    );
    let repo1_prefs = state
        .user_preferences
        .for_repo(&RepositoryId("llxprt-code".to_string()));
    assert_eq!(
        repo1_prefs.issue_filter.state,
        Some(IssueFilterState::Closed),
        "repo-1's applied filter must be persisted before the repo switch"
    );
}

/// Switching repos via in-mode RepoList navigation (arrow keys) while PR mode
/// is active must not leak repo-1's applied filter into repo-2.
#[test]
fn pr_inmode_repo_switch_does_not_leak_applied_filter() {
    use crate::state::PrFocus;

    let mut state = state_with_two_repos(
        "llxprt-code",
        RepoPreferences::default(),
        "jefe",
        RepoPreferences::default(),
    );
    state = state.apply(AppEvent::EnterPrsMode);
    state.prs_state.pr_focus = PrFocus::RepoList;
    // Apply a Closed filter on repo-1.
    state = state.apply(AppEvent::PrOpenFilterControls);
    state = state.apply(AppEvent::PrCycleFilterState); // Open -> Closed
    state = state.apply(AppEvent::PrApplyFilter);
    state.prs_state.pr_focus = PrFocus::RepoList;

    // Switch down to repo-2 (jefe) via in-mode repo navigation.
    state = state.apply(AppEvent::PrNavigateDown);

    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Open),
        "repo-2 (jefe) must not inherit repo-1's (llxprt-code) Closed filter"
    );
}

/// Exit PR mode on repo-1, switch to repo-2, re-enter PR mode: repo-2 must
/// restore its OWN prefs, not repo-1's filter (issue #163 per-repo isolation).
#[test]
fn pr_exit_switch_reenter_does_not_leak_filter() {
    let mut state = state_with_two_repos(
        "llxprt-code",
        RepoPreferences::default(),
        "jefe",
        RepoPreferences::default(),
    );
    // Enter PR mode on repo-1, apply a Closed filter, exit.
    state = state.apply(AppEvent::EnterPrsMode);
    state = state.apply(AppEvent::PrOpenFilterControls);
    state = state.apply(AppEvent::PrCycleFilterState); // Open -> Closed
    state = state.apply(AppEvent::PrApplyFilter);
    state = state.apply(AppEvent::ExitPrsMode);

    // Switch to repo-2 (jefe) from the dashboard.
    state = state.apply(AppEvent::SelectRepository(1));
    // Re-enter PR mode on repo-2.
    state = state.apply(AppEvent::EnterPrsMode);

    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Open),
        "repo-2 (jefe) must restore its own Open default, not repo-1's Closed"
    );
}

/// Jumping to an agent in a different repo while PR mode is active must save
/// the old repo's filter and restore the new repo's filter — the filter must
/// NOT leak across repos via the shortcut-jump path (issue #163).
#[test]
fn pr_jump_to_agent_in_other_repo_does_not_leak_filter() {
    use crate::domain::{Agent, AgentId};

    let mut state = state_with_two_repos(
        "llxprt-code",
        RepoPreferences::default(),
        "jefe",
        RepoPreferences::default(),
    );
    // Add an agent in repo-2 (jefe) on shortcut slot 1.
    state.agents.push(Agent::new(
        AgentId("jefe-agent".to_string()),
        RepositoryId("jefe".to_string()),
        "Jefe Agent".to_string(),
        std::path::PathBuf::from("/tmp/jefe"),
    ));
    state.agents[0].shortcut_slot = Some(1);

    // Enter PR mode on repo-1 (llxprt-code) and apply a Closed filter.
    state = state.apply(AppEvent::EnterPrsMode);
    state = state.apply(AppEvent::PrOpenFilterControls);
    state = state.apply(AppEvent::PrCycleFilterState); // Open -> Closed
    state = state.apply(AppEvent::PrApplyFilter);
    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Closed)
    );

    // Jump to the jefe agent (slot 1), which switches the selected repo.
    state = state.apply(AppEvent::JumpToAgentByShortcut(1));

    // repo-2 (jefe) must NOT carry repo-1's Closed filter.
    assert_eq!(
        state.prs_state.committed_filter.state,
        Some(PrFilterState::Open),
        "repo-2 (jefe) must not inherit repo-1's (llxprt-code) filter via jump"
    );
    // repo-1's filter must have been persisted before the jump.
    let repo1_prefs = state
        .user_preferences
        .for_repo(&RepositoryId("llxprt-code".to_string()));
    assert_eq!(
        repo1_prefs.pr_filter.state,
        Some(PrFilterState::Closed),
        "repo-1's applied filter must be persisted before the jump"
    );
}

/// Jumping to an agent in a different repo while issues mode is active must
/// save the old repo's issue filter and restore the new repo's filter (issue #163).
#[test]
fn issue_jump_to_agent_in_other_repo_does_not_leak_filter() {
    use crate::domain::{Agent, AgentId};

    let mut state = state_with_two_repos(
        "llxprt-code",
        RepoPreferences::default(),
        "jefe",
        RepoPreferences::default(),
    );
    state.agents.push(Agent::new(
        AgentId("jefe-agent".to_string()),
        RepositoryId("jefe".to_string()),
        "Jefe Agent".to_string(),
        std::path::PathBuf::from("/tmp/jefe"),
    ));
    state.agents[0].shortcut_slot = Some(1);

    // Enter issues mode on repo-1 (llxprt-code) and apply a Closed filter.
    state = state.apply(AppEvent::EnterIssuesMode);
    state = state.apply(AppEvent::OpenFilterControls);
    state = state.apply(AppEvent::CycleFilterState); // Open -> Closed
    state = state.apply(AppEvent::ApplyFilter);
    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::Closed)
    );

    // Jump to the jefe agent (slot 1), which switches the selected repo.
    state = state.apply(AppEvent::JumpToAgentByShortcut(1));

    assert_eq!(
        state.issues_state.committed_filter.state,
        Some(IssueFilterState::Open),
        "repo-2 (jefe) must not inherit repo-1's (llxprt-code) issue filter via jump"
    );
    let repo1_prefs = state
        .user_preferences
        .for_repo(&RepositoryId("llxprt-code".to_string()));
    assert_eq!(
        repo1_prefs.issue_filter.state,
        Some(IssueFilterState::Closed),
        "repo-1's applied issue filter must be persisted before the jump"
    );
}
