
use super::*;
use jefe::workbench::{
    PanelId, PluginScreenId, Rect, ResolvedLayout, ResolvedPanel, ScreenIdentity, ScreenInstanceId,
};

fn panel(id: &'static str, chrome: Rect) -> ResolvedPanel {
    ResolvedPanel {
        id: PanelId::from_static(id),
        visible: true,
        chrome,
        content: chrome,
        depth_first_index: 0,
        hit_region: Some(chrome),
    }
}

fn provider_layout() -> ResolvedLayout {
    ResolvedLayout {
        screen_instance: ScreenInstanceId::preview(),
        outer: Rect::new(0, 0, 80, 24),
        panels: vec![
            panel("left", Rect::new(0, 1, 40, 22)),
            panel("right", Rect::new(41, 1, 39, 22)),
        ],
        too_small: None,
    }
}

fn provider_state() -> AppState {
    let mut state = AppState::default();
    state.nav.current_mut().screen =
        ScreenIdentity::Package(PluginScreenId::from_static("vendor.pkg"));
    state.resolved_layout = Some(provider_layout());
    state
}

#[test]
fn click_inside_a_provider_panel_sets_focus_to_it() {
    let mut state = provider_state();
    set_provider_panel_focus(&mut state, 5, 5);
    assert_eq!(
        state.nav.current().panel_focus,
        PanelId::from_static("left")
    );
}

#[test]
fn click_in_the_second_panel_sets_focus_to_it() {
    let mut state = provider_state();
    set_provider_panel_focus(&mut state, 60, 10);
    assert_eq!(
        state.nav.current().panel_focus,
        PanelId::from_static("right")
    );
}

#[test]
fn click_outside_any_panel_does_not_change_focus() {
    let mut state = provider_state();
    let prior = state.nav.current().panel_focus;
    set_provider_panel_focus(&mut state, 0, 0);
    assert_eq!(state.nav.current().panel_focus, prior);
}

#[test]
fn routing_uses_only_the_resolved_layout_snapshot() {
    let mut state = provider_state();
    set_provider_panel_focus(&mut state, 39, 5);
    assert_eq!(
        state.nav.current().panel_focus,
        PanelId::from_static("left")
    );
    set_provider_panel_focus(&mut state, 41, 5);
    assert_eq!(
        state.nav.current().panel_focus,
        PanelId::from_static("right")
    );
}

#[test]
fn missing_resolved_layout_does_not_crash() {
    let mut state = provider_state();
    state.resolved_layout = None;
    let prior = state.nav.current().panel_focus;
    set_provider_panel_focus(&mut state, 5, 5);
    assert_eq!(state.nav.current().panel_focus, prior);
}
