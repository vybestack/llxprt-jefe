//! Snapshot-production tests (issue #384, CW04-04).

use crate::domain::AgentId;
use crate::screen_layout::{hidden_panel_ids, resolve_screen, screen_rect};
use crate::state::AppState;
use crate::workbench::{PanelId, ScreenId, screen_descriptor};

fn state_on(screen: ScreenId) -> AppState {
    AppState {
        screen,
        ..AppState::default()
    }
}

fn resolved(screen: ScreenId, cols: u16, rows: u16) -> crate::workbench::ResolvedLayout {
    resolve_screen(&state_on(screen), cols, rows)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"))
}

#[test]
fn every_screen_resolves_at_a_normal_terminal_size() {
    for screen in ScreenId::ALL {
        let layout = resolved(screen, 120, 40);
        assert!(
            layout.too_small.is_none(),
            "screen {screen} must fit a 120x40 terminal"
        );
        assert!(layout.visible_panels().count() >= 2, "screen {screen}");
    }
}

#[test]
fn global_chrome_is_removed_exactly_once() {
    // The status bar owns the first row and the keybind bar the last, so the
    // screen rectangle starts at row 1 and is two rows shorter than the render
    // grid.
    let rect = screen_rect(120, 40);
    assert_eq!(rect.row, 1, "the status bar owns row 0");
    let (render_cols, render_rows) = crate::layout::effective_render_size(120, 40);
    assert_eq!(rect.width, render_cols);
    assert_eq!(rect.height, render_rows - 2);
}

#[test]
fn no_panel_overlaps_the_status_or_keybind_bar() {
    for screen in ScreenId::ALL {
        let rect = screen_rect(120, 40);
        for panel in resolved(screen, 120, 40).visible_panels() {
            assert!(
                panel.chrome.row >= rect.row,
                "screen {screen} panel {} overlaps the status bar",
                panel.id
            );
            assert!(
                panel.chrome.bottom() <= rect.bottom(),
                "screen {screen} panel {} overlaps the keybind bar",
                panel.id
            );
        }
    }
}

#[test]
fn each_resolution_is_a_distinct_snapshot_with_a_stable_panel_set() {
    // The identity is what lets a consumer prove it read the geometry the
    // renderer used rather than deriving its own, so two resolutions must never
    // share one, while the geometry they produce for equal inputs is identical.
    let first = resolved(ScreenId::Issues, 120, 40);
    let second = resolved(ScreenId::Issues, 120, 40);
    assert_ne!(first.screen_instance, second.screen_instance);
    assert_eq!(
        first.panels, second.panels,
        "the same inputs must produce the same panels"
    );
}

#[test]
fn a_resize_produces_a_different_geometry() {
    let wide = resolved(ScreenId::Issues, 120, 40);
    let narrow = resolved(ScreenId::Issues, 80, 24);
    let wide_list = wide
        .panel(&PanelId::from_static("issue-list"))
        .map(|panel| panel.chrome);
    let narrow_list = narrow
        .panel(&PanelId::from_static("issue-list"))
        .map(|panel| panel.chrome);
    assert_ne!(wide_list, narrow_list, "a resize must move the panes");
}

#[test]
fn the_filter_band_is_hidden_until_the_filter_is_open() {
    let mut state = state_on(ScreenId::Issues);
    state.issues_state.filter_ui.controls_open = false;
    let closed = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    assert_eq!(
        closed
            .panel(&PanelId::from_static("issue-list-filter"))
            .map(|panel| panel.visible),
        Some(false)
    );

    state.issues_state.filter_ui.controls_open = true;
    let open = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    assert_eq!(
        open.panel(&PanelId::from_static("issue-list-filter"))
            .map(|panel| panel.visible),
        Some(true)
    );
}

#[test]
fn opening_the_filter_band_pushes_the_list_down() {
    let mut state = state_on(ScreenId::Issues);
    state.issues_state.filter_ui.controls_open = false;
    let closed = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    state.issues_state.filter_ui.controls_open = true;
    let open = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));

    let list = PanelId::from_static("issue-list");
    let closed_row = closed.panel(&list).map(|panel| panel.chrome.row);
    let open_row = open.panel(&list).map(|panel| panel.chrome.row);
    assert!(
        open_row > closed_row,
        "the list must start lower once the band takes rows: {closed_row:?} -> {open_row:?}"
    );
}

#[test]
fn the_notice_banner_is_hidden_until_there_is_a_message() {
    let mut state = state_on(ScreenId::PullRequests);
    state.error_message = None;
    let quiet = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    assert_eq!(
        quiet
            .panel(&PanelId::from_static("pr-list-banner"))
            .map(|panel| panel.visible),
        Some(false)
    );

    state.error_message = Some("rate limited".to_owned());
    let noisy = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    assert_eq!(
        noisy
            .panel(&PanelId::from_static("pr-list-banner"))
            .map(|panel| panel.visible),
        Some(true)
    );
}

#[test]
fn the_dashboard_search_row_appears_only_while_searching() {
    let mut state = state_on(ScreenId::Dashboard);
    state.dashboard_search.input_focused = false;
    let idle = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    assert_eq!(
        idle.panel(&PanelId::from_static("search"))
            .map(|panel| panel.visible),
        Some(false)
    );

    state.dashboard_search.input_focused = true;
    let searching = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    assert_eq!(
        searching
            .panel(&PanelId::from_static("search"))
            .map(|panel| panel.visible),
        Some(true)
    );
}

#[test]
fn the_terminal_pane_never_resolves_to_zero_cells() {
    for cols in 1_u16..=100 {
        for rows in 1_u16..=30 {
            let layout = resolved(ScreenId::Dashboard, cols, rows);
            let Some(terminal) = layout.panel(&PanelId::from_static("terminal")) else {
                unreachable!("the dashboard always declares a terminal panel");
            };
            if terminal.visible {
                assert!(
                    terminal.content.width >= 1 && terminal.content.height >= 1,
                    "terminal collapsed to nothing at {cols}x{rows}"
                );
            }
        }
    }
}

/// One state per branch of the hiding rules, so the assertion below sees every
/// identity the module can name on the given screen.
fn hiding_states(screen: ScreenId) -> Vec<AppState> {
    let quiet = state_on(screen);

    let mut showing = state_on(screen);
    showing.error_message = Some("rate limited".to_owned());
    showing.issues_state.filter_ui.controls_open = true;
    showing.prs_state.filter_ui.controls_open = true;
    showing.actions_state.ui.filter_ui_open = true;
    showing.dashboard_search.input_focused = true;

    let mut overlay = state_on(screen);
    overlay.open_shell_overlay(AgentId("agent-1".to_owned()));

    vec![quiet, showing, overlay]
}

#[test]
fn every_panel_the_application_hides_is_declared_by_its_screen() {
    // The hiding rules name panels by identity literal, so a descriptor that
    // renamed a panel would leave them addressing nothing and the band would
    // stay on screen forever. Nothing else would notice.
    for screen in ScreenId::ALL {
        let Ok(descriptor) = screen_descriptor(screen) else {
            unreachable!("every screen has a compiled descriptor");
        };
        let mut named = 0_usize;
        for state in hiding_states(screen) {
            for panel in hidden_panel_ids(&state) {
                assert!(
                    descriptor
                        .panels
                        .iter()
                        .any(|declared| declared.id == panel),
                    "screen {screen} hides {panel}, which its descriptor does not declare"
                );
                named += 1;
            }
        }
        // Guard the assertion against becoming vacuous: the screens that carry
        // conditional panels must actually produce some.
        if matches!(
            screen,
            ScreenId::Repositories | ScreenId::Errors | ScreenId::Terminals
        ) {
            assert_eq!(named, 0, "screen {screen} hides nothing conditionally");
        } else {
            assert!(named > 0, "screen {screen} must exercise its hiding rules");
        }
    }
}

#[test]
fn a_terminal_too_small_for_the_screen_keeps_one_required_panel() {
    for screen in ScreenId::ALL {
        for (cols, rows) in [(10_u16, 4_u16), (5, 3), (20, 5)] {
            let layout = resolved(screen, cols, rows);
            let Some(_) = layout.too_small else {
                continue;
            };
            assert_eq!(
                layout.visible_panels().count(),
                1,
                "screen {screen} at {cols}x{rows} must keep exactly one panel"
            );
        }
    }
}
