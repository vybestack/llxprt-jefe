//! Dashboard search mode for repositories and agents (issue #405).
//!
//! Power users accumulate many repositories and agents. This module proves the
//! "filter but lite" search mode: typing a query in the dashboard narrows the
//! repository sidebar AND the agent pane by name, case-insensitively,
//! AND-composed with the existing active-only (`v`) filter. The search query
//! is runtime-only (never persisted), and a dashboard filtered-view indicator
//! makes it obvious when the visible list is reduced by either the search or
//! active-only mode.
//!
//! @plan project-plans/issue405-plan.md
//! @requirement REQ-FUNC-002

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::persistence::State as PersistedState;
use jefe::state::transition::TransitionExt as _;
use jefe::state::{AppEvent, AppState, PaneFocus, ScreenId};

fn repository(id: &str, name: &str) -> Repository {
    Repository::new(
        RepositoryId(id.into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        name.into(),
        id.into(),
        PathBuf::from(format!("/{id}")),
    )
}

fn running_agent(id: &str, name: &str, repo_id: &str) -> Agent {
    let mut a = Agent::new(
        AgentId(id.into()),
        RepositoryId(repo_id.into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        name.into(),
        PathBuf::from(format!("/{repo_id}/{id}")),
    );
    a.status = AgentStatus::Running;
    a
}

/// A dashboard with three named repositories, the first of which owns running
/// agents whose names span distinct fragments.
fn dashboard_state() -> AppState {
    let mut state = AppState {
        repositories: vec![
            repository("r1", "alpha"),
            repository("r2", "beta"),
            repository("r3", "gamma"),
        ],
        agents: vec![
            running_agent("a1", "zig", "r1"),
            running_agent("a2", "cargo", "r1"),
            running_agent("a3", "rustc", "r1"),
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Repositories,
        screen: ScreenId::Dashboard,
        ..AppState::default()
    };
    state.normalize_selection_indices();
    state
}

// =============================================================================
// A1: `/` focuses the dashboard search input
// =============================================================================

#[test]
fn slash_focuses_dashboard_search() {
    let state = dashboard_state();
    assert!(!state.dashboard_search.input_focused);

    let after = state.apply(AppEvent::FocusDashboardSearch).committed_pure();
    assert!(
        after.dashboard_search.input_focused,
        "`/` (FocusDashboardSearch) must focus the dashboard search input"
    );
}

// =============================================================================
// A2: Typing filters repositories live
// =============================================================================

#[test]
fn search_filters_repositories_by_name() {
    let state = dashboard_state();
    assert_eq!(state.visible_repository_indices().len(), 3);

    let after = state
        .apply(AppEvent::FocusDashboardSearch)
        .committed_pure()
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "al".to_string(),
        })
        .committed_pure();

    let visible = after.visible_repository_indices();
    assert_eq!(
        visible.len(),
        1,
        "only the 'alpha' repo matches the 'al' query"
    );
    assert_eq!(
        after.repositories[visible[0]].name, "alpha",
        "the matched repo must be 'alpha'"
    );
}

// =============================================================================
// A3: Repository match is case-insensitive
// =============================================================================

#[test]
fn repo_search_is_case_insensitive() {
    let state = AppState {
        repositories: vec![repository("r1", "Alpha"), repository("r2", "BETA")],
        selected_repository_index: Some(0),
        screen: ScreenId::Dashboard,
        ..AppState::default()
    };

    // Verify case-insensitive matching: 'aLp' matches 'Alpha' but not 'BETA'.
    let after = state
        .clone()
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "aLp".to_string(),
        })
        .committed_pure();
    let visible = after.visible_repository_indices();
    assert_eq!(
        visible.len(),
        1,
        "case-insensitive 'aLp' must match 'Alpha'"
    );

    // A query with opposite case to the stored name must still match.
    let after2 = state
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "beTA".to_string(),
        })
        .committed_pure();
    let visible2 = after2.visible_repository_indices();
    assert_eq!(
        visible2.len(),
        1,
        "case-insensitive 'beTA' must match 'BETA'"
    );
    assert_eq!(after2.repositories[visible2[0]].name, "BETA");
}

// =============================================================================
// A4: Empty query disables the search filter
// =============================================================================

#[test]
fn empty_query_disables_search_filter() {
    let state = dashboard_state();
    let filtered = state
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "al".to_string(),
        })
        .committed_pure();
    assert_eq!(filtered.visible_repository_indices().len(), 1);

    let restored = filtered
        .apply(AppEvent::SetDashboardSearchQuery {
            query: String::new(),
        })
        .committed_pure();
    assert_eq!(
        restored.visible_repository_indices().len(),
        3,
        "clearing the query must restore all repositories"
    );
}

// =============================================================================
// A5: Search AND-composes with active-only
// =============================================================================

#[test]
fn search_composes_with_active_only() {
    // r1 has running agents; r2 and r3 are idle.
    let mut state = AppState {
        repositories: vec![
            repository("r1", "alpha"),
            repository("r2", "beta"),
            repository("r3", "gamma"),
        ],
        agents: vec![running_agent("a1", "alpha-agent", "r1")],
        selected_repository_index: Some(0),
        pane_focus: PaneFocus::Repositories,
        screen: ScreenId::Dashboard,
        ..AppState::default()
    };
    state.normalize_selection_indices();

    // Turn active-only ON.
    let active_only = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert_eq!(
        active_only.visible_repository_indices().len(),
        1,
        "active-only keeps only r1 (has a running agent)"
    );

    // A query matching an idle repo (beta) must NOT reveal it — active-only wins.
    let with_search = active_only
        .clone()
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "be".to_string(),
        })
        .committed_pure();
    assert!(
        with_search.visible_repository_indices().is_empty(),
        "active-only AND search: beta is idle so it must stay hidden even though it matches the query"
    );

    // A query matching r1 (alpha, has a running agent) must reveal it.
    let alpha_match = active_only
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "al".to_string(),
        })
        .committed_pure();
    let visible = alpha_match.visible_repository_indices();
    assert_eq!(visible.len(), 1);
    assert_eq!(alpha_match.repositories[visible[0]].name, "alpha");
}

// =============================================================================
// A6: Search filters agents in the selected repository
// =============================================================================

#[test]
fn search_filters_agents_by_name() {
    let state = dashboard_state();
    let repo_id = RepositoryId("r1".into());
    assert_eq!(state.visible_agents_for_repository(&repo_id).len(), 3);

    let after = state
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "ru".to_string(),
        })
        .committed_pure();
    let visible = after.visible_agents_for_repository(&repo_id);
    assert_eq!(
        visible.len(),
        1,
        "only the 'rustc' agent matches the 'ru' query"
    );
    assert_eq!(visible[0].name, "rustc");
}

// =============================================================================
// A7: Backspace pops the last character (via SetDashboardSearchQuery)
// =============================================================================

#[test]
fn backspace_pops_search_char() {
    let state = dashboard_state();
    let typed = state
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "ab".to_string(),
        })
        .committed_pure();
    assert_eq!(typed.dashboard_search.query, "ab");

    // The key resolver emits a SetDashboardSearchQuery with the popped query.
    let popped = typed
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "a".to_string(),
        })
        .committed_pure();
    assert_eq!(popped.dashboard_search.query, "a");
    // The agent list must refilter: 'a' still matches 'cargo' and 'alpha'
    // agents are in r1; only 'cargo' contains 'a' within r1's {zig, cargo, rustc}.
    let visible = popped.visible_agents_for_repository(&RepositoryId("r1".into()));
    assert_eq!(
        visible.len(),
        1,
        "after backspace the 'a' query matches only 'cargo' among r1's agents"
    );
    assert_eq!(visible[0].name, "cargo");
}

// =============================================================================
// A8: Esc clears a non-empty query then blurs
// =============================================================================

#[test]
fn esc_clears_nonempty_search() {
    let state = dashboard_state();
    let typed = state
        .apply(AppEvent::FocusDashboardSearch)
        .committed_pure()
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "ab".to_string(),
        })
        .committed_pure();
    assert!(typed.dashboard_search.input_focused);
    assert!(!typed.dashboard_search.query.is_empty());

    let after_esc = typed.apply(AppEvent::ClearDashboardSearch).committed_pure();
    assert!(
        after_esc.dashboard_search.query.is_empty(),
        "Esc on a non-empty query must clear the query"
    );
    assert!(
        !after_esc.dashboard_search.input_focused,
        "Esc on a non-empty query must blur the input"
    );
    assert_eq!(
        after_esc.visible_repository_indices().len(),
        3,
        "clearing the query must restore the full repo list"
    );
}

// =============================================================================
// A9: Esc on an empty query blurs only (no crash)
// =============================================================================

#[test]
fn esc_on_empty_query_blurs_only() {
    let state = dashboard_state();
    let focused = state.apply(AppEvent::FocusDashboardSearch).committed_pure();
    assert!(focused.dashboard_search.input_focused);
    assert!(focused.dashboard_search.query.is_empty());

    let after_esc = focused
        .apply(AppEvent::BlurDashboardSearch)
        .committed_pure();
    assert!(
        !after_esc.dashboard_search.input_focused,
        "Esc on an empty query must blur the input"
    );
    assert!(
        after_esc.dashboard_search.query.is_empty(),
        "Esc on an empty query must leave the query empty"
    );
    assert_eq!(
        after_esc.visible_repository_indices().len(),
        3,
        "no filtering when the query is empty"
    );
}

// =============================================================================
// A10: Enter blurs but keeps the query (filter persists)
// =============================================================================

#[test]
fn enter_blurs_keeps_query() {
    let state = dashboard_state();
    let typed = state
        .apply(AppEvent::FocusDashboardSearch)
        .committed_pure()
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "al".to_string(),
        })
        .committed_pure();

    let after_enter = typed.apply(AppEvent::BlurDashboardSearch).committed_pure();
    assert!(
        !after_enter.dashboard_search.input_focused,
        "Enter must blur the search input"
    );
    assert_eq!(
        after_enter.dashboard_search.query, "al",
        "Enter must retain the query so the filter persists"
    );
    assert_eq!(
        after_enter.visible_repository_indices().len(),
        1,
        "the filtered view must persist after Enter blurs"
    );
}

// =============================================================================
// A11: Selection normalizes when the filter shrinks the visible set
// =============================================================================

#[test]
fn selection_normalizes_when_filtered() {
    // Select the 'gamma' repo (index 2) before filtering.
    let mut state = dashboard_state();
    state.selected_repository_index = Some(2);
    state.normalize_selection_indices();

    // Apply a query that only matches 'alpha' (index 0). The selection must
    // clamp to a still-visible repo rather than dangle at index 2.
    let filtered = state
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "al".to_string(),
        })
        .committed_pure();
    let visible = filtered.visible_repository_indices();
    assert!(visible.contains(&0));
    assert!(!visible.contains(&2));
    assert_eq!(
        filtered.selected_repository_index,
        Some(0),
        "selection must clamp to a visible repo (no dangling index, no panic)"
    );
}

// =============================================================================
// A12: `/` is dashboard-only — the dashboard search events are independent of
// the SplitScreen ModalState::Search path (regression contract).
// =============================================================================

#[test]
fn dashboard_search_does_not_open_split_search_modal() {
    use jefe::state::ModalState;
    let mut state = dashboard_state();
    state.screen = ScreenId::Repositories;
    // FocusDashboardSearch must NOT open the split's ModalState::Search.
    let after = state.apply(AppEvent::FocusDashboardSearch).committed_pure();
    // The split screen's search is a ModalState::Search; the dashboard search
    // is a separate focused-input state. They must remain distinct.
    assert!(
        after.dashboard_search.input_focused,
        "dashboard search input must be focusable in any active screen (state-level)"
    );
    assert!(
        !matches!(after.modal, ModalState::Search { .. }),
        "dashboard search must not open the split screen's ModalState::Search"
    );
}

// =============================================================================
// A13/A14/A15: Filtered-view indicator projection
// =============================================================================

#[test]
fn search_active_renders_indicator() {
    let state = dashboard_state();
    // Empty query + active-only off => no filter active.
    assert!(
        jefe::ui::dashboard_filter_indicator(&state).is_none(),
        "no indicator when unfiltered"
    );

    let with_search = state
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "al".to_string(),
        })
        .committed_pure();
    let indicator = jefe::ui::dashboard_filter_indicator(&with_search).unwrap_or_default();
    assert!(
        indicator.contains("al"),
        "indicator must name the active query when search is active, got: {indicator}"
    );
}

#[test]
fn active_only_renders_indicator() {
    let state = dashboard_state();
    // Empty query + active-only off => no indicator.
    assert!(
        jefe::ui::dashboard_filter_indicator(&state).is_none(),
        "no indicator when unfiltered"
    );

    // Turn active-only ON with the query empty.
    let active_only = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    let indicator = jefe::ui::dashboard_filter_indicator(&active_only).unwrap_or_default();
    assert!(
        !indicator.is_empty(),
        "indicator must be present when active-only is on, got: {indicator:?}"
    );
    assert!(
        indicator.contains("active"),
        "indicator must signal active-only, got: {indicator}"
    );
}

#[test]
fn unfiltered_renders_no_indicator() {
    let state = dashboard_state();
    // Default state: no query, active-only off.
    assert!(
        jefe::ui::dashboard_filter_indicator(&state).is_none(),
        "no indicator must render when neither search nor active-only is active"
    );
}

// =============================================================================
// A16: The dashboard search query is NOT persisted
// =============================================================================

#[test]
fn dashboard_search_query_not_persisted() {
    let state = dashboard_state();
    let with_search = state
        .apply(AppEvent::SetDashboardSearchQuery {
            query: "persistent?".to_string(),
        })
        .committed_pure();
    assert_eq!(with_search.dashboard_search.query, "persistent?");

    // The persisted DTO (`jefe::persistence::State`) has no field for the
    // dashboard search query, so a round-trip must yield an empty query.
    let persisted = PersistedState {
        schema_version: jefe::persistence::STATE_SCHEMA_VERSION,
        repositories: with_search.repositories.clone(),
        agents: with_search.agents.clone(),
        selected_repository_index: with_search.selected_repository_index,
        selected_agent_index: with_search.selected_agent_index,
        hide_idle_repositories: with_search.hide_idle_repositories,
        last_selected_agent_by_repo: with_search.last_selected_agent_by_repo.clone(),
        pane_focus: String::new(),
        terminal_focused: false,
        user_preferences: with_search.user_preferences.clone(),
    };
    let json = serde_json::to_string(&persisted).unwrap_or_else(|_| String::new());
    assert!(
        !json.contains("dashboard_search"),
        "dashboard_search state must NOT appear in the persisted DTO: {json}"
    );
}
