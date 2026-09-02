//! Dashboard filtering through the shared, instance-owned Search overlay.
//!
//! The Dashboard, local screens, and package screens use the same declared
//! Search control. Its runtime query is owned by the exact open screen instance,
//! filters repositories and agents case-insensitively, composes with active-only,
//! and is never persisted.
//!
//! @plan project-plans/issue705-plan.md
//! @requirement CWR2-02
//! @requirement CWR2-04
//! @requirement CWR2-09

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::persistence::State as PersistedState;
use jefe::state::transition::TransitionExt as _;
use jefe::state::{AppEvent, AppState, PaneFocus};
use jefe::workbench::OverlayKind;

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
    let mut agent = Agent::new(
        AgentId(id.into()),
        RepositoryId(repo_id.into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        name.into(),
        PathBuf::from(format!("/{repo_id}/{id}")),
    );
    agent.status = AgentStatus::Running;
    agent
}

fn dashboard_state() -> AppState {
    let mut state = crate::common_app_state::app_state();
    state.repositories = vec![
        repository("r1", "alpha"),
        repository("r2", "beta"),
        repository("r3", "gamma"),
    ];
    state.agents = vec![
        running_agent("a1", "zig", "r1"),
        running_agent("a2", "cargo", "r1"),
        running_agent("a3", "rustc", "r1"),
    ];
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);
    state.pane_focus = PaneFocus::Repositories;
    state.restore_navigation_root(jefe::workbench::DASHBOARD_IDENTITY);
    state.normalize_selection_indices();
    state
}

fn open_search(state: AppState) -> AppState {
    state.apply(AppEvent::OpenSearch).committed_pure()
}

fn set_search_query(mut state: AppState, query: &str) -> AppState {
    if state.active_overlay_kind() != Some(OverlayKind::Search) {
        state = open_search(state);
    }
    while state
        .search_query()
        .is_some_and(|current| !current.is_empty())
    {
        state = state.apply(AppEvent::FormBackspace).committed_pure();
    }
    for value in query.chars() {
        state = state.apply(AppEvent::FormChar(value)).committed_pure();
    }
    state
}

/// The Dashboard "search lite" filter lives on the active Search overlay, and that
/// instance-owned query is what narrows the visible lists. Closing the editor
/// (SearchApply/CloseModal) dismisses the overlay and with it the query, restoring
/// the full view; the filter is editor-scoped, so `close_search_equal` is the
/// explicit-clear path.
#[test]
fn closing_search_clears_the_filter_and_restores_the_full_view() {
    let typed = set_search_query(dashboard_state(), "al");
    assert_eq!(typed.search_query(), Some("al"));
    assert_eq!(typed.active_overlay_kind(), Some(OverlayKind::Search));

    let closed = typed.apply(AppEvent::CloseModal).committed_pure();

    assert_eq!(closed.active_overlay_kind(), None);
    assert_eq!(closed.search_query(), None);
    assert_eq!(closed.visible_repository_indices(), vec![0, 1, 2]);
}

#[test]
fn slash_action_opens_the_declared_dashboard_search_overlay() {
    let state = dashboard_state();
    assert_eq!(state.active_overlay_kind(), None);

    let after = open_search(state);

    assert_eq!(after.active_overlay_kind(), Some(OverlayKind::Search));
    assert_eq!(after.search_query(), Some(""));
}

#[test]
fn search_filters_repositories_by_name() {
    let state = dashboard_state();
    assert_eq!(state.visible_repository_indices().len(), 3);

    let after = set_search_query(state, "al");

    let visible = after.visible_repository_indices();
    assert_eq!(visible, vec![0]);
    assert_eq!(after.repositories[visible[0]].name, "alpha");
}

#[test]
fn repository_search_is_case_insensitive() {
    let mut state = dashboard_state();
    state.repositories = vec![repository("r1", "Alpha"), repository("r2", "BETA")];

    let alpha = set_search_query(state.clone(), "aLp");
    assert_eq!(alpha.visible_repository_indices(), vec![0]);

    let beta = set_search_query(state, "beTA");
    assert_eq!(beta.visible_repository_indices(), vec![1]);
}

#[test]
fn clearing_the_overlay_query_disables_the_search_filter() {
    let filtered = set_search_query(dashboard_state(), "al");
    assert_eq!(filtered.visible_repository_indices(), vec![0]);

    let restored = set_search_query(filtered, "");

    assert_eq!(restored.visible_repository_indices(), vec![0, 1, 2]);
    assert_eq!(restored.search_query(), Some(""));
}

#[test]
fn search_composes_with_active_only() {
    let mut state = dashboard_state();
    state.agents = vec![running_agent("a1", "alpha-agent", "r1")];
    let active_only = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert_eq!(active_only.visible_repository_indices(), vec![0]);

    let idle_match = set_search_query(active_only.clone(), "be");
    assert!(idle_match.visible_repository_indices().is_empty());

    let active_match = set_search_query(active_only, "al");
    assert_eq!(active_match.visible_repository_indices(), vec![0]);
}

#[test]
fn search_filters_agents_by_name() {
    let state = set_search_query(dashboard_state(), "ru");

    let visible = state.visible_agents_for_repository(&RepositoryId("r1".into()));
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "rustc");
}

#[test]
fn shared_search_backspace_updates_the_query_and_results() {
    let typed = set_search_query(dashboard_state(), "ab");
    assert_eq!(typed.search_query(), Some("ab"));

    let popped = typed.apply(AppEvent::FormBackspace).committed_pure();

    assert_eq!(popped.search_query(), Some("a"));
    let visible = popped.visible_agents_for_repository(&RepositoryId("r1".into()));
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].name, "cargo");
}

#[test]
fn selection_normalizes_when_the_search_filter_shrinks_the_visible_set() {
    let mut state = dashboard_state();
    state.selected_repository_index = Some(2);
    state.normalize_selection_indices();

    let filtered = set_search_query(state, "al");

    assert_eq!(filtered.visible_repository_indices(), vec![0]);
    assert_eq!(filtered.selected_repository_index, Some(0));
}

#[test]
fn search_is_the_same_declared_overlay_on_another_compiled_screen() {
    let mut state = dashboard_state();
    state.restore_navigation_root(jefe::workbench::REPOSITORIES_IDENTITY);

    let after = open_search(state);

    assert_eq!(after.active_overlay_kind(), Some(OverlayKind::Search));
    assert_eq!(after.search_query(), Some(""));
}

#[test]
fn active_search_and_active_only_project_the_filter_indicator() {
    let state = dashboard_state();
    assert!(jefe::ui::dashboard_filter_indicator(&state).is_none());

    let with_search = set_search_query(state.clone(), "al");
    let search_indicator = jefe::ui::dashboard_filter_indicator(&with_search).unwrap_or_default();
    assert!(search_indicator.contains("al"));

    let active_only = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    let active_indicator = jefe::ui::dashboard_filter_indicator(&active_only).unwrap_or_default();
    assert!(active_indicator.contains("active"));
}

#[test]
fn unfiltered_dashboard_projects_no_filter_indicator() {
    assert!(jefe::ui::dashboard_filter_indicator(&dashboard_state()).is_none());
}

#[test]
fn dashboard_search_query_is_not_persisted() {
    let with_search = set_search_query(dashboard_state(), "persistent?");
    assert_eq!(with_search.search_query(), Some("persistent?"));

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
    let json = serde_json::to_string(&persisted).unwrap_or_default();
    assert!(!json.contains("persistent?"));
    assert!(!json.contains("search_query"));
}
