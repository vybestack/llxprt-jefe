//! Selection reveal and explicit scrolling through the shared List window.

use super::*;
#[test]
fn provider_explicit_zero_scroll_yields_to_a_later_selection_change() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 40, 8);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    accept_snapshot(&mut state, panel, window_snapshot(panel, 24, 1));
    assert!(project_view(&descriptor, &state, &layout).panels[0].visible_window_origin > 0);
    state
        .scroll_host_local(panel, 0)
        .unwrap_or_else(|error| panic!("manual zero: {error}"));
    assert_eq!(
        project_view(&descriptor, &state, &layout).panels[0].visible_window_origin,
        0
    );
    accept_snapshot(&mut state, panel, window_snapshot(panel, 24, 2));
    assert_eq!(
        project_view(&descriptor, &state, &layout).panels[0].visible_window_origin,
        0
    );
    accept_snapshot(&mut state, panel, window_snapshot(panel, 23, 3));
    let selected = project_view(&descriptor, &state, &layout);
    assert!(
        selected.panels[0]
            .lines
            .iter()
            .any(|line| line == ">> Item 23")
    );
}

fn window_snapshot(panel: PanelInstanceId, selected: usize, revision: u64) -> PanelSnapshot {
    window_snapshot_at(panel, selected, revision, 1)
}

fn window_snapshot_at(
    panel: PanelInstanceId,
    selected: usize,
    revision: u64,
    generation: u64,
) -> PanelSnapshot {
    let mut snapshot = list_snapshot(
        panel,
        (0..25)
            .map(|index| {
                list_item(
                    &format!("item-{index}"),
                    &format!("Item {index}"),
                    None,
                    &[],
                )
            })
            .collect(),
        &format!("item-{selected}"),
        Vec::new(),
        None,
    );
    snapshot.revision = revision;
    snapshot.generation = generation;
    snapshot
}

/// Reactivation drops the accepted model, so the first republished snapshot has
/// no prior selection to compare against. The retained manual viewport must
/// survive that republication when the provider reports the same selection, and
/// a genuinely changed selection must still reveal itself.
#[test]
fn provider_list_manual_window_survives_resume_and_retry_republication() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 40, 8);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    accept_snapshot(&mut state, panel, window_snapshot(panel, 24, 1));
    state
        .update_host_local(
            panel,
            HostLocal {
                scroll_offset: 3,
                ..HostLocal::default()
            },
        )
        .unwrap_or_else(|error| panic!("manual scroll: {error}"));
    assert_eq!(
        project_view(&descriptor, &state, &layout).panels[0].visible_window_origin,
        3
    );

    state
        .suspend(panel)
        .unwrap_or_else(|error| panic!("suspend: {error}"));
    state
        .resume(panel)
        .unwrap_or_else(|error| panic!("resume: {error}"));
    accept_snapshot(&mut state, panel, window_snapshot_at(panel, 24, 1, 2));
    let resumed = project_view(&descriptor, &state, &layout);
    assert_eq!(resumed.panels[0].visible_window_origin, 3);
    assert_eq!(resumed.panels[0].lines[0], "   Item 3");

    state
        .retry(panel)
        .unwrap_or_else(|error| panic!("retry: {error}"));
    accept_snapshot(&mut state, panel, window_snapshot_at(panel, 24, 1, 3));
    assert_eq!(
        project_view(&descriptor, &state, &layout).panels[0].visible_window_origin,
        3
    );

    accept_snapshot(&mut state, panel, window_snapshot_at(panel, 0, 2, 3));
    let revealed = project_view(&descriptor, &state, &layout);
    assert_eq!(revealed.panels[0].visible_window_origin, 0);
    assert!(
        revealed.panels[0]
            .lines
            .iter()
            .any(|line| line == ">> Item 0")
    );
}

#[test]
fn provider_list_manual_window_survives_projection_and_same_selection_publication() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 40, 8);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    accept_snapshot(&mut state, panel, window_snapshot(panel, 24, 1));
    let restored = project_view(&descriptor, &state, &layout);
    assert!(
        restored.panels[0]
            .lines
            .iter()
            .any(|line| line == ">> Item 24")
    );
    state
        .update_host_local(
            panel,
            HostLocal {
                scroll_offset: 3,
                ..HostLocal::default()
            },
        )
        .unwrap_or_else(|error| panic!("manual scroll: {error}"));
    for revision in 2..=3 {
        accept_snapshot(&mut state, panel, window_snapshot(panel, 24, revision));
        let view = project_view(&descriptor, &state, &layout);
        let main = &view.panels[0];
        assert_eq!(main.visible_window_origin, 3);
        assert_eq!(main.lines[0], "   Item 3");
        assert_eq!(
            main.hit_targets[0],
            Some(PanelHitTarget::ListItem(id("item-3")))
        );
        assert_eq!(main.content, restored.panels[0].content);
    }
}

#[test]
fn provider_list_selection_change_reveals_from_nonzero_manual_origin() {
    let descriptor = make_descriptor();
    let layout = resolve(&descriptor, 40, 8);
    let mut state = ProviderPanelState::new();
    let panel = declare_and_activate_panel(
        &mut state,
        &PanelId::from_static("main"),
        1,
        &[BodyKind::List],
    );
    accept_snapshot(&mut state, panel, window_snapshot(panel, 0, 1));
    state
        .update_host_local(
            panel,
            HostLocal {
                scroll_offset: 5,
                ..HostLocal::default()
            },
        )
        .unwrap_or_else(|error| panic!("manual scroll: {error}"));
    assert_eq!(
        project_view(&descriptor, &state, &layout).panels[0].visible_window_origin,
        5
    );
    accept_snapshot(&mut state, panel, window_snapshot(panel, 24, 2));
    let selected = project_view(&descriptor, &state, &layout);
    assert!(
        selected.panels[0]
            .lines
            .iter()
            .any(|line| line == ">> Item 24")
    );
    assert!(selected.panels[0].visible_window_origin > 5);
    let prior = state.host_local(panel).cloned().unwrap_or_default();
    state
        .update_host_local(
            panel,
            HostLocal {
                selected_id: Some(id("item-1")),
                ..prior
            },
        )
        .unwrap_or_else(|error| panic!("local selection: {error}"));
    assert_eq!(
        project_view(&descriptor, &state, &layout).panels[0].visible_window_origin,
        1
    );
}

fn repository_window_fixture() -> (
    crate::state::AppState,
    crate::workbench::HostPanelCapability,
) {
    let mut state = crate::state::AppState::new(crate::test_support::published_workbench());
    state.repositories = (0..25)
        .map(|index| crate::test_support::host_panel_repository(&format!("r{index}")))
        .collect();
    state.selected_repository_index = Some(24);
    let capability = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .and_then(|descriptor| descriptor.panel(&PanelId::from_static("repositories")))
        .and_then(crate::workbench::PanelDescriptor::host_capability)
        .unwrap_or_else(|| panic!("repository capability"));
    state.resolved_layout = crate::screen_layout::resolve_screen(&state, 100, 25);
    (state, capability)
}

fn repository_window(state: &crate::state::AppState) -> PanelProjection {
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .unwrap_or_else(|| panic!("dashboard descriptor"));
    let layout = state
        .resolved_layout
        .as_ref()
        .unwrap_or_else(|| panic!("resolved layout"));
    super::super::project_current_screen(state, descriptor, layout)
        .unwrap_or_else(|error| panic!("projection: {error}"))
        .panels
        .into_iter()
        .find(|panel| panel.id.as_str() == "repositories")
        .unwrap_or_else(|| panic!("repository panel"))
}

#[test]
fn host_list_manual_scroll_uses_visible_origin_and_selection_change_reveals() {
    let (mut state, capability) = repository_window_fixture();
    let restored = repository_window(&state);
    assert!(restored.visible_window_origin > 0);
    let height = usize::from(restored.content.height);
    assert!(state.scroll_host_panel(capability, -1, height));
    assert_eq!(
        repository_window(&state).visible_window_origin,
        restored.visible_window_origin - 1
    );
    for _ in 0..3 {
        assert!(state.scroll_host_panel(capability, -1, height));
    }
    let manual = repository_window(&state);
    assert!(manual.visible_window_origin > 0);
    assert_eq!(repository_window(&state), manual);
    assert!(!manual.hit_targets.contains(&Some(PanelHitTarget::ListItem(
        crate::domain::Id::internal_indexed(crate::domain::InternalId::RepositoryItem, 24)
    ))));
    assert!(state.apply_host_panel_action(
        capability,
        crate::host_controls::ControlAction::Previous,
        100,
        25
    ));
    let selected = repository_window(&state);
    assert!(
        selected
            .hit_targets
            .contains(&Some(PanelHitTarget::ListItem(
                crate::domain::Id::internal_indexed(crate::domain::InternalId::RepositoryItem, 23)
            )))
    );
    assert_eq!(selected.content, restored.content);
}
