//! Cross-mode transition tests for Issues ↔ PR mode switching (issue #164).
//!
//! Extracted from `preferences_tests.rs` to keep file sizes under the
//! project's source-file-size hard limit. Covers the cross-mode `i`/`p`
//! navigation keys and the mode-exclusivity / terminal-focus-hygiene
//! invariants enforced by the `enter_issues_mode` / `enter_prs_mode`
//! reducers.

use super::*;

// ── Cross-mode navigation regression (issue #164) ────────────────────────

/// EnterIssuesMode from PR mode must switch the screen to DashboardIssues
/// (issue #164: `i` from the PR screen enters Issues mode).
#[test]
fn enter_issues_mode_from_prs_mode_switches_screen() {
    let state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    let state = state.apply(AppEvent::EnterPrsMode);
    assert_eq!(state.screen_mode, ScreenMode::DashboardPullRequests);
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(
        state.screen_mode,
        ScreenMode::DashboardIssues,
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
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
    let state = state.apply(AppEvent::EnterPrsMode);
    assert_eq!(
        state.screen_mode,
        ScreenMode::DashboardPullRequests,
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
    let state = state.apply(AppEvent::EnterPrsMode);
    assert!(state.prs_state.active);
    assert_eq!(state.screen_mode, ScreenMode::DashboardPullRequests);

    let state = state.apply(AppEvent::EnterIssuesMode);
    assert!(
        state.issues_state.active,
        "EnterIssuesMode must activate the issues state"
    );
    assert!(
        !state.prs_state.active,
        "EnterIssuesMode from PR mode must deactivate prs_state.active"
    );
    assert_eq!(
        state.screen_mode,
        ScreenMode::DashboardIssues,
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
    let state = state.apply(AppEvent::EnterIssuesMode);
    assert!(state.issues_state.active);
    assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);

    let state = state.apply(AppEvent::EnterPrsMode);
    assert!(
        state.prs_state.active,
        "EnterPrsMode must activate the prs state"
    );
    assert!(
        !state.issues_state.active,
        "EnterPrsMode from Issues mode must deactivate issues_state.active"
    );
    assert_eq!(
        state.screen_mode,
        ScreenMode::DashboardPullRequests,
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
    let mut state = state.apply(AppEvent::EnterPrsMode);
    state.terminal_focused = true;
    state.pane_focus = PaneFocus::Terminal;

    let state = state.apply(AppEvent::EnterIssuesMode);
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
    let mut state = state.apply(AppEvent::EnterIssuesMode);
    state.terminal_focused = true;
    state.pane_focus = PaneFocus::Terminal;

    let state = state.apply(AppEvent::EnterPrsMode);
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

/// EnterIssuesMode must not clobber an existing saved `prior_agent_focus`
/// when re-entering Issues mode after a Dashboard → Issues → PRs → Issues
/// round-trip (Finding 1).
#[test]
fn enter_issues_mode_does_not_clobber_existing_prior_focus() {
    use crate::domain::{Agent, AgentId};
    use crate::state::PaneFocus;
    use crate::state::PriorAgentFocus;

    let mut state = state_with_repo_and_prefs("repo-1", RepoPreferences::default());
    // Add an agent so selected_agent_index is Some(0) after enter-mode saves.
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent One".to_string(),
        std::path::PathBuf::from("/tmp/agent"),
    ));
    state.selected_agent_index = Some(0);

    // Enter Issues mode so prior_agent_focus is set by the reducer.
    let mut state = state.apply(AppEvent::EnterIssuesMode);
    // Now overwrite with a KNOWN sentinel value that differs from what the
    // current pane_focus/selections would produce on re-entry.
    let original_prior = PriorAgentFocus {
        pane_focus: PaneFocus::Repositories,
        selected_repository_index: Some(0),
        selected_agent_index: None, // sentinel: None, but live selection is Some(0)
    };
    state.issues_state.prior_agent_focus = Some(original_prior.clone());

    // Switch to PR mode then back to Issues mode.
    let state = state.apply(AppEvent::EnterPrsMode);
    let state = state.apply(AppEvent::EnterIssuesMode);

    let saved = state
        .issues_state
        .prior_agent_focus
        .as_ref()
        .unwrap_or_else(|| panic!("prior_agent_focus must be Some after EnterIssuesMode"));
    assert_eq!(
        saved.pane_focus, original_prior.pane_focus,
        "prior_agent_focus must not be clobbered by re-entry"
    );
    assert_eq!(
        saved.selected_repository_index, original_prior.selected_repository_index,
        "selected_repository_index must be preserved"
    );
    assert_eq!(
        saved.selected_agent_index, original_prior.selected_agent_index,
        "selected_agent_index must be preserved"
    );
}
