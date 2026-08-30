#[test]
fn dashboard_projects_declared_host_controls_through_the_shared_screen_runtime() {
    let mut state = crate::state::AppState::new(crate::test_support::published_workbench());
    assert!(state.nav.current_mut().overlays_mut().open_search());
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(crate::workbench::DASHBOARD_IDENTITY)
        .unwrap_or_else(|| panic!("dashboard descriptor must be published"));
    let layout = crate::screen_layout::resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| panic!("dashboard layout must resolve"));

    let view = crate::provider_panel_view::project_current_screen(&state, descriptor, &layout)
        .unwrap_or_else(|error| panic!("dashboard projection: {error}"));

    assert_eq!(view.panels.len(), descriptor.panels.len());
    assert!(view.panels.iter().all(|panel| panel.visible));
    for panel in &view.panels {
        assert_eq!(panel.status, PanelStatus::Active, "{}", panel.id);
        assert!(
            !panel.lines.iter().any(|line| line == "provider unavailable"),
            "{} must use its declared host control",
            panel.id
        );
    }
    let terminal = view
        .panels
        .iter()
        .find(|panel| panel.id.as_str() == "terminal")
        .unwrap_or_else(|| panic!("terminal projection must exist"));
    assert_eq!(
        terminal.render,
        crate::provider_panel_view::PanelRender::EmbeddedTerminal
    );

}

#[test]
fn current_projection_rejects_layout_from_a_suspended_same_definition_instance() {
    let mut state = crate::state::AppState::new(crate::test_support::published_workbench());
    let stale_layout = crate::screen_layout::resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| panic!("dashboard layout must resolve"));
    state.enter_provider_route(
        crate::workbench::RouteId::from_static("dashboard"),
        crate::workbench::ActivationValues::empty(),
    );
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(crate::workbench::DASHBOARD_IDENTITY)
        .unwrap_or_else(|| panic!("dashboard descriptor must be published"));

    let result = crate::provider_panel_view::project_current_screen(
        &state,
        descriptor,
        &stale_layout,
    );
    assert!(matches!(
        result,
        Err(crate::provider_panel_view::PanelProjectionError::StaleLayout { .. })
    ));
}

#[test]
fn projection_refuses_descriptor_geometry_drift_without_panicking() {
    let state = crate::state::AppState::new(crate::test_support::published_workbench());
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(crate::workbench::DASHBOARD_IDENTITY)
        .unwrap_or_else(|| panic!("dashboard descriptor must be published"));
    let mut layout = crate::screen_layout::resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| panic!("dashboard layout must resolve"));
    let omitted = layout.panels.remove(0).id;

    let result = project_provider_screen_result(
        descriptor,
        state.nav.current().id.get(),
        state.provider_panels(),
        &layout,
        &state.nav.current().panel_focus,
    );
    let Err(error) = result else {
        panic!("projection must reject missing panel geometry");
    };

    assert!(error.to_string().contains(omitted.as_str()));
}

#[test]
fn same_definition_dashboard_instances_project_only_their_own_search_control() {
    use crate::state::transition::TransitionExt;

    let mut state = crate::state::AppState::new(crate::test_support::published_workbench());
    state = state
        .apply(crate::state::AppEvent::OpenSearch)
        .committed_pure();
    for value in "first".chars() {
        assert!(state.push_search_char(value));
    }
    let first_instance = state.nav.current().id;
    assert_search_projection(&state, "first", None);

    state.enter_provider_route(
        RouteId::from_static("dashboard"),
        crate::workbench::ActivationValues::empty(),
    );
    assert_ne!(state.nav.current().id, first_instance);
    state = state
        .apply(crate::state::AppEvent::OpenSearch)
        .committed_pure();
    for value in "second".chars() {
        assert!(state.push_search_char(value));
    }
    assert_search_projection(&state, "second", Some("first"));

    state.leave_screen();
    assert_eq!(state.nav.current().id, first_instance);
    assert_search_projection(&state, "first", Some("second"));
}

fn assert_search_projection(state: &crate::state::AppState, expected: &str, absent: Option<&str>) {
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .unwrap_or_else(|| panic!("current descriptor must be published"));
    let layout = crate::screen_layout::resolve_screen(state, 120, 40)
        .unwrap_or_else(|| panic!("current layout must resolve"));
    let view = crate::provider_panel_view::project_current_screen(state, descriptor, &layout)
        .unwrap_or_else(|error| panic!("dashboard projection: {error}"));
    let search = view
        .panels
        .iter()
        .find(|panel| panel.id.as_str() == "search")
        .unwrap_or_else(|| panic!("search projection must exist"));
    let text = search.lines.join("\n");
    assert!(text.contains(expected), "{text:?}");
    if let Some(absent) = absent {
        assert!(!text.contains(absent), "{text:?}");
    }
}

#[test]
fn terminals_shell_list_projects_managed_sessions_through_the_declared_host_control() {
    fn repository_fixture() -> crate::domain::Repository {
        crate::domain::Repository::new(
            crate::domain::RepositoryId("repo-t1".to_owned()),
            crate::domain::shipped_agent_type(3),
            crate::domain::TypedMap::new(),
            "widgets".to_owned(),
            "widgets-slug".to_owned(),
            std::path::PathBuf::from("/work/widgets"),
        )
    }
    fn agent_fixture() -> crate::domain::Agent {
        let mut agent = crate::domain::Agent::new(
            crate::domain::AgentId("agent-t1".to_owned()),
            crate::domain::RepositoryId("repo-t1".to_owned()),
            crate::domain::shipped_agent_type(3),
            crate::domain::TypedMap::new(),
            "runner".to_owned(),
            std::path::PathBuf::from("/work/widgets/wt1"),
        );
        agent.status = crate::domain::AgentStatus::Running;
        agent
    }

    let mut state = crate::state::AppState::new(crate::test_support::published_workbench());
    state.repositories = vec![repository_fixture()];
    let agent = agent_fixture();
    state.agents = vec![agent.clone()];
    state.shell_inventory.record(agent.id.clone());
    state.terminal_manager.selected_index = Some(0);
    let _ = state.enter_screen(crate::workbench::ScreenId::Terminals);

    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(crate::workbench::ScreenId::Terminals.into())
        .unwrap_or_else(|| panic!("terminals descriptor must be published"));
    let layout = crate::screen_layout::resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| panic!("terminals layout must resolve"));
    let view = crate::provider_panel_view::project_current_screen(&state, descriptor, &layout)
        .unwrap_or_else(|error| panic!("terminals projection: {error}"));

    let shell_list = view
        .panels
        .iter()
        .find(|panel| panel.id.as_str() == "shell-list")
        .unwrap_or_else(|| panic!("shell-list projection must exist"));
    assert_eq!(shell_list.status, PanelStatus::Active);
    assert_eq!(
        shell_list.render,
        crate::provider_panel_view::PanelRender::Control
    );
    assert!(
        shell_list.lines.iter().any(|line| line.contains("runner")),
        "the managed shell row must project through the host control, got {:?}",
        shell_list.lines
    );
}
