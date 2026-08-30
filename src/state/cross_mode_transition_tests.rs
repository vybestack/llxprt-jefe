//! Cross-mode transition tests for Issues ↔ PR mode switching (issue #164).
//!
//! Extracted from `preferences_tests.rs` to keep file sizes under the
//! project's source-file-size hard limit. Covers the cross-mode `i`/`p`
//! navigation keys and the mode-exclusivity / terminal-focus-hygiene
//! invariants enforced by the `enter_issues_mode` / `enter_prs_mode`
//! reducers.

use super::*;
use crate::state::transition::TransitionExt;

// ── Cross-mode navigation regression (issue #164) ────────────────────────

/// EnterIssuesMode from PR mode must switch the screen to DashboardIssues
/// (issue #164: `i` from the PR screen enters Issues mode).
#[test]
fn enter_issues_mode_from_prs_mode_switches_screen() {
    let state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    let state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    assert_eq!(state.screen(), ScreenId::PullRequests);
    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    assert_eq!(
        state.screen(),
        ScreenId::Issues,
        "EnterIssuesMode from PR mode must switch to DashboardIssues"
    );
    assert!(
        state.issues_state.active,
        "EnterIssuesMode must activate the issues state"
    );
}

/// EnterPrsMode from Issues mode must switch the screen to
/// DashboardPullRequests (issue #164: `p` from the Issues screen enters PR
/// mode).
#[test]
fn enter_prs_mode_from_issues_mode_switches_screen() {
    let state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    assert_eq!(state.screen(), ScreenId::Issues);
    let state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    assert_eq!(
        state.screen(),
        ScreenId::PullRequests,
        "EnterPrsMode from Issues mode must switch to DashboardPullRequests"
    );
    assert!(
        state.prs_state.active,
        "EnterPrsMode must activate the prs state"
    );
}

// ── Cross-mode exclusivity & terminal-focus normalization (issue #164
//    review findings 1 & 2) ──────────────────────────────────────────────

/// EnterIssuesMode from PR mode must deactivate the PR state so the
/// exclusivity invariant holds: at most one of `issues_state.active` /
/// `prs_state.active` is true (Finding 1).
#[test]
fn enter_issues_mode_from_prs_deactivates_prs() {
    use crate::state::PrFocus;

    let state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    let state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    assert!(state.prs_state.active);
    assert_eq!(state.screen(), ScreenId::PullRequests);

    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    assert!(
        state.issues_state.active,
        "EnterIssuesMode must activate the issues state"
    );
    assert!(
        !state.prs_state.active,
        "EnterIssuesMode from PR mode must deactivate prs_state.active"
    );
    assert_eq!(
        state.screen(),
        ScreenId::Issues,
        "screen must be DashboardIssues after EnterIssuesMode"
    );
    // PR overlays must be cleared.
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrList);
    assert_eq!(state.prs_state.inline_state, InlineState::None);
    assert!(state.prs_state.agent_chooser.is_none());
    assert!(state.prs_state.merge_chooser.is_none());
    assert!(!state.prs_state.filter_ui.controls_open);
    assert!(!state.prs_state.search_input_focused);
}

/// EnterPrsMode from Issues mode must deactivate the Issues state so the
/// exclusivity invariant holds (Finding 1).
#[test]
fn enter_prs_mode_from_issues_deactivates_issues() {
    use crate::state::IssueFocus;

    let state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    assert!(state.issues_state.active);
    assert_eq!(state.screen(), ScreenId::Issues);

    let state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    assert!(
        state.prs_state.active,
        "EnterPrsMode must activate the prs state"
    );
    assert!(
        !state.issues_state.active,
        "EnterPrsMode from Issues mode must deactivate issues_state.active"
    );
    assert_eq!(
        state.screen(),
        ScreenId::PullRequests,
        "screen must be DashboardPullRequests after EnterPrsMode"
    );
    // Issues overlays must be cleared.
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);
    assert_eq!(state.issues_state.inline_state, InlineState::None);
    assert!(state.issues_state.agent_chooser.is_none());
    assert!(!state.issues_state.filter_ui.controls_open);
    assert!(!state.issues_state.search_input_focused);
}

/// EnterIssuesMode from PR mode must clear `terminal_focused` and set
/// `pane_focus` to a coherent app-focused value (Finding 2).
#[test]
fn enter_issues_mode_clears_terminal_focus() {
    use crate::state::PaneFocus;

    let state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    let mut state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    state.terminal_focused = true;
    state.pane_focus = PaneFocus::Terminal;

    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    assert!(
        !state.terminal_focused,
        "EnterIssuesMode must clear terminal_focused"
    );
    assert_ne!(
        state.pane_focus,
        PaneFocus::Terminal,
        "EnterIssuesMode must not leave pane_focus on Terminal"
    );
    assert_eq!(
        state.pane_focus,
        PaneFocus::Agents,
        "EnterIssuesMode should set pane_focus to Agents"
    );
}

/// EnterPrsMode from Issues mode must clear `terminal_focused` and set
/// `pane_focus` to a coherent app-focused value (Finding 2).
#[test]
fn enter_prs_mode_clears_terminal_focus() {
    use crate::state::PaneFocus;

    let state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    let mut state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    state.terminal_focused = true;
    state.pane_focus = PaneFocus::Terminal;

    let state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    assert!(
        !state.terminal_focused,
        "EnterPrsMode must clear terminal_focused"
    );
    assert_ne!(
        state.pane_focus,
        PaneFocus::Terminal,
        "EnterPrsMode must not leave pane_focus on Terminal"
    );
    assert_eq!(
        state.pane_focus,
        PaneFocus::Agents,
        "EnterPrsMode should set pane_focus to Agents"
    );
}

/// Replacing a sibling list mode retains the exact original source for Back.
#[test]
fn replacing_a_sibling_list_mode_restores_the_exact_original_source() {
    use crate::state::PaneFocus;

    let mut dashboard = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    dashboard.pane_focus = PaneFocus::Repositories;
    let dashboard_id = dashboard.nav.current().id;

    let mut issues = dashboard.apply(AppEvent::EnterIssuesMode).committed_pure();
    issues.pane_focus = PaneFocus::Agents;

    let mut prs = issues.apply(AppEvent::EnterPrsMode).committed_pure();
    prs.pane_focus = PaneFocus::Agents;
    let dashboard = prs.apply(AppEvent::ExitPrsMode).committed_pure();

    assert_eq!(dashboard.nav.current().id, dashboard_id);
    assert_eq!(dashboard.screen(), crate::workbench::DASHBOARD_IDENTITY);
    assert_eq!(dashboard.pane_focus, PaneFocus::Repositories);
}

/// The mirrored direction: Dashboard → Issues (replaced by PRs) → Exit PRs
/// must restore the exact original Dashboard instance, not the Issues replacement.
#[test]
fn replacing_a_sibling_list_mode_in_the_other_direction_restores_dashboard() {
    use crate::state::PaneFocus;

    let mut dashboard = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    dashboard.pane_focus = PaneFocus::Repositories;
    dashboard.selected_repository_index = Some(0);
    let dashboard_id = dashboard.nav.current().id;

    let mut issues = dashboard.apply(AppEvent::EnterIssuesMode).committed_pure();
    issues.pane_focus = PaneFocus::Agents;
    issues.selected_agent_index = Some(0);

    // PR mode replaces the Issues instance (cross-mode `p`).
    let mut prs = issues.apply(AppEvent::EnterPrsMode).committed_pure();
    prs.pane_focus = PaneFocus::Agents;
    let before_back_id = prs.nav.current().id;
    assert_ne!(before_back_id, dashboard_id);

    // Exit PRs: Back restores the exact instance that opened Issues mode,
    // i.e. the Dashboard, not the replaced Issues screen.
    let dashboard_again = prs.apply(AppEvent::ExitPrsMode).committed_pure();
    assert_eq!(dashboard_again.screen(), crate::workbench::DASHBOARD_IDENTITY);
    assert_eq!(dashboard_again.nav.current().id, dashboard_id);
    assert_eq!(dashboard_again.pane_focus, PaneFocus::Repositories);
    assert_eq!(dashboard_again.selected_repository_index, Some(0));
}

/// Exact-source replacement preserves each screen's own selection/presentation, so
/// the two instances are distinguishable beyond their instance ids.
#[test]
fn sibling_replacement_exact_instances_keep_distinct_presentations() {
    use crate::state::PaneFocus;

    let mut issues_source = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    issues_source = issues_source.apply(AppEvent::EnterIssuesMode).committed_pure();
    issues_source.pane_focus = PaneFocus::Agents;
    issues_source.selected_agent_index = Some(0);

    let source_repository_index = issues_source.selected_repository_index;
    let prs = issues_source.apply(AppEvent::EnterPrsMode).committed_pure();
    // PR entry seeds its own route context (repository 0, agents) and pane.
    assert_eq!(prs.selected_repository_index, source_repository_index);
    assert_eq!(prs.pane_focus, PaneFocus::Agents);

    let prs_id_before_back = prs.nav.current().id;
    let restored = prs.apply(AppEvent::ExitPrsMode).committed_pure();
    let prs_id = prs_id_before_back;
    assert_eq!(restored.screen(), crate::workbench::DASHBOARD_IDENTITY);
    assert_eq!(restored.selected_repository_index, Some(0));
    // The restored instance is the original Dashboard that opened Issues mode,
    // so its pane focus is the pre-jump Repositories selection, not Agents.
    assert_eq!(restored.pane_focus, PaneFocus::Repositories);
    assert_ne!(restored.nav.current().id, prs_id);
}


#[test]
fn leaving_issues_mode_invalidates_pending_detail_load() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default())
        .apply(AppEvent::EnterIssuesMode)
        .committed_pure();
    state.mark_issue_detail_loading_with_request_id(RepositoryId("repo-1".to_owned()), 621, 7);

    let state = state.apply(AppEvent::ExitIssuesMode).committed_pure();

    assert!(state.issues_state.detail_pending.is_none());
    assert!(!state.issues_state.loading.detail);
}

#[test]
fn entering_prs_mode_invalidates_pending_issue_detail_load() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default())
        .apply(AppEvent::EnterIssuesMode)
        .committed_pure();
    state.mark_issue_detail_loading_with_request_id(RepositoryId("repo-1".to_owned()), 621, 7);

    let state = state.apply(AppEvent::EnterPrsMode).committed_pure();

    assert!(state.issues_state.detail_pending.is_none());
    assert!(!state.issues_state.loading.detail);
}

#[test]
fn leaving_prs_mode_invalidates_pending_detail_load() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default())
        .apply(AppEvent::EnterPrsMode)
        .committed_pure();
    state.mark_pr_detail_loading(RepositoryId("repo-1".to_owned()), 621, 7);

    let state = state.apply(AppEvent::ExitPrsMode).committed_pure();

    assert!(state.prs_state.detail_pending.is_none());
    assert!(!state.prs_state.loading.detail);
}

#[test]
fn entering_issues_mode_invalidates_pending_pr_detail_load() {
    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default())
        .apply(AppEvent::EnterPrsMode)
        .committed_pure();
    state.mark_pr_detail_loading(RepositoryId("repo-1".to_owned()), 621, 7);

    let state = state.apply(AppEvent::EnterIssuesMode).committed_pure();

    assert!(state.prs_state.detail_pending.is_none());
    assert!(!state.prs_state.loading.detail);
}
