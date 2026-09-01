// #706: ahead of the identity flip, the Repositories screen must project its
// retained card grid and filter band through the shared panel runtime. The
// grid keeps its bespoke renderer (maintainer decision) fed by the retained
// workbench view; the band carries the same search line the legacy left rail
// showed. Included into the provider_panel_view test module, so names must
// not collide with the sibling includes.

use crate::domain::observation::{
    AgentObservation, FieldState, NativeActivityState, NativeActivityValue, ObservationHealth,
    Provenance,
};
use crate::domain::{Agent, AgentId, AgentStatus, AgentTypeId, Repository, RepositoryId};
use crate::provider_panel_view::{PanelRender, project_current_screen, workbench_view_from_state};
use crate::state::AppState;
use std::path::PathBuf;

fn repositories_state() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Repositories);
    state
}

fn workbench_repository(id: &str, github_repo: &str) -> Repository {
    let mut repository = Repository::new(
        RepositoryId(format!("repo-{id}")),
        AgentTypeId::default(),
        TypedMap::default(),
        format!("Repo {id}"),
        format!("repo-{id}"),
        PathBuf::from("/tmp"),
    );
    repository.github_repo = github_repo.to_owned();
    repository
}

fn workbench_agent(name: &str, repository_id: &str) -> Agent {
    let mut agent = Agent::new(
        AgentId(name.to_owned()),
        RepositoryId(repository_id.to_owned()),
        AgentTypeId::default(),
        TypedMap::default(),
        name.to_owned(),
        PathBuf::from("/tmp"),
    );
    agent.status = AgentStatus::Running;
    agent
}

fn ready_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        activity: FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Idle,
            },
        ),
        wait: FieldState::known(Provenance::Authoritative, None),
        turn: FieldState::known(Provenance::Authoritative, None),
        terminal: FieldState::known(Provenance::Authoritative, None),
        ..AgentObservation::default()
    }
}

fn seed_workbench_agent(state: &mut AppState, name: &str, repository_id: &str) {
    let agent = workbench_agent(name, repository_id);
    state
        .observations
        .insert(agent.id.clone(), ready_observation());
    state.agents.push(agent);
}

fn project_repositories(state: &AppState) -> ProviderScreenView {
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(ScreenId::Repositories.into())
        .unwrap_or_else(|| panic!("repositories descriptor must be published"));
    let layout = crate::screen_layout::resolve_screen(state, 120, 40)
        .unwrap_or_else(|| panic!("repositories layout must resolve"));
    project_current_screen(state, descriptor, &layout)
        .unwrap_or_else(|error| panic!("repositories projection: {error}"))
}

fn panel_of<'a>(view: &'a ProviderScreenView, id: &str) -> &'a PanelProjection {
    view.panels
        .iter()
        .find(|panel| panel.id.as_str() == id)
        .unwrap_or_else(|| panic!("panel {id} must project"))
}

#[test]
fn cards_grid_projects_through_the_bespoke_renderer() {
    let mut state = repositories_state();
    state.repositories = vec![workbench_repository("one", "acoliver/jefe")];
    seed_workbench_agent(&mut state, "alpha", "repo-one");

    let view = project_repositories(&state);

    let cards = panel_of(&view, "cards");
    assert_eq!(cards.render, PanelRender::WorkbenchCards);
    assert_eq!(cards.status, PanelStatus::Active);
    assert_eq!(cards.title, "Workbench");
    assert_eq!(
        cards.max_scroll_offset, 0,
        "the grid pages itself; there is no host-local scroll"
    );
    assert!(
        cards.lines.is_empty() && cards.hit_targets.is_empty(),
        "the bespoke renderer owns the grid content"
    );

    // The rest of the screen still runs on the shared host controls.
    let status = panel_of(&view, "status");
    assert_eq!(status.render, PanelRender::Control);
    assert!(
        status.lines.iter().any(|line| line.contains("Needs you")),
        "status lines: {:?}",
        status.lines
    );
    let sidebar = panel_of(&view, "repositories");
    assert_eq!(sidebar.render, PanelRender::Control);
    assert_eq!(sidebar.status, PanelStatus::Active);
}

#[test]
fn filter_band_carries_the_legacy_search_line() {
    let state = repositories_state();

    let view = project_repositories(&state);

    let band = panel_of(&view, "filter");
    assert_eq!(band.status, PanelStatus::Active);
    assert_eq!(band.render, PanelRender::Control);
    assert_eq!(
        band.lines.len(),
        1,
        "the band is one search line: {:?}",
        band.lines
    );
    assert_eq!(band.hit_targets.len(), band.lines.len());
    // The legacy rail rendered the search row plus a terminal-style cursor.
    let expected = crate::overlay_controls::project_search(
        &state,
        usize::from(crate::layout::LEFT_COL_WIDTH),
    )
    .rows
    .into_iter()
    .next()
    .map_or_else(|| "Filter: ".to_owned(), |row| row.text);
    assert_eq!(
        band.lines.first().map(String::as_str),
        Some(format!("{expected}_").as_str())
    );
}

#[test]
fn cards_grid_carries_card_hit_targets_over_their_rectangles() {
    // Issue #706: the host capability advertises card selection through
    // the shared input contract, so the projection must offer one hit
    // target per visible card, keyed to the same ids the model carries.
    let mut state = repositories_state();
    state.repositories = vec![workbench_repository("one", "acoliver/jefe")];
    seed_workbench_agent(&mut state, "alpha", "repo-one");
    seed_workbench_agent(&mut state, "beta", "repo-one");
    seed_workbench_agent(&mut state, "gamma", "repo-one");

    let view = project_repositories(&state);
    let cards = panel_of(&view, "cards");

    let model = crate::host_panel_models::project_host_panel(
        &state,
        crate::workbench::HostPanelModelSource::WorkbenchCards,
    );
    let PanelBody::List(body) = &model.body else {
        panic!("the cards model is a list body");
    };
    let expected: Vec<_> = body.items.iter().map(|item| item.id.clone()).collect();
    assert!(expected.len() >= 3, "the fixture must page its cards");

    // The grid paints its page window of the model, not the whole list, so
    // derive the window the same way the bespoke renderer does.
    let view = workbench_view_from_state(&state, cards.content.width, cards.content.height);
    let columns = view.layout.columns.max(1);
    let cards_per_page = view.layout.rows_visible.saturating_mul(columns).max(1);
    let page_start = view.layout.page.saturating_mul(cards_per_page);
    let painted = view.cards.len();
    assert!(
        page_start + painted < expected.len(),
        "the fixture must exercise the page offset into the model ids"
    );

    let projected: Vec<_> = cards
        .rect_hit_targets
        .iter()
        .map(|(_, target)| match target {
            crate::provider_panel_view::PanelHitTarget::ListItem(id) => id.clone(),
            other => panic!("card targets are list items, got {other:?}"),
        })
        .collect();
    assert_eq!(
        projected.len(),
        painted,
        "every painted card carries a hit target"
    );
    assert!(
        projected
            .iter()
            .zip(&expected[page_start..page_start + painted])
            .all(|(a, b)| a == b),
        "projected ids {projected:?} must equal the model's page window {}..{} of {expected:?}",
        page_start,
        page_start + painted
    );
}

#[test]
fn workbench_view_from_state_wires_origin_filter_and_page() {
    let mut state = repositories_state();
    state.repositories = vec![
        workbench_repository("one", "acoliver/jefe"),
        workbench_repository("two", ""),
    ];
    seed_workbench_agent(&mut state, "alpha", "repo-one");
    seed_workbench_agent(&mut state, "beta", "repo-two");

    // Configured origins reach the card header; unconfigured ones fall back.
    let view = workbench_view_from_state(&state, 120, 40);
    let alpha = view
        .cards
        .iter()
        .find(|card| card.agent_id == AgentId("alpha".to_owned()))
        .unwrap_or_else(|| panic!("alpha card must render"));
    assert!(
        alpha.header.repo_name.text.contains("acoliver/jefe"),
        "header: {:?}",
        alpha.header.repo_name.text
    );
    let beta = view
        .cards
        .iter()
        .find(|card| card.agent_id == AgentId("beta".to_owned()))
        .unwrap_or_else(|| panic!("beta card must render"));
    assert!(
        beta.header.repo_name.text.contains('?'),
        "unconfigured origin falls back: {:?}",
        beta.header.repo_name.text
    );

    // The split filter scopes the grid exactly as the legacy screen did.
    state.split_filter = Some(RepositoryId("repo-one".to_owned()));
    let filtered = workbench_view_from_state(&state, 120, 40);
    assert_eq!(filtered.cards.len(), 1, "only repo-one agents pass");
    assert_eq!(filtered.cards[0].agent_id, AgentId("alpha".to_owned()));

    // The retained page counter plumbs through (12 agents page at 200x24).
    state.split_filter = None;
    for index in 0..12 {
        seed_workbench_agent(&mut state, &format!("page{index}"), "repo-one");
    }
    state.workbench.page = 1;
    let paged = workbench_view_from_state(&state, 200, 24);
    assert!(
        paged.layout.page_count >= 2,
        "12 agents must page at 200x24"
    );
    assert_eq!(paged.layout.page, 1);
    state.workbench.page = 0;
    let first = workbench_view_from_state(&state, 200, 24);
    let paged_ids: Vec<_> = paged.cards.iter().map(|card| &card.agent_id).collect();
    let first_ids: Vec<_> = first.cards.iter().map(|card| &card.agent_id).collect();
    assert_ne!(paged_ids, first_ids, "page 1 is a different window");
}
