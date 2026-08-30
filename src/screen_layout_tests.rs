//! Snapshot-production tests (issue #384, CW04-04).

use crate::domain::AgentId;
use crate::screen_layout::{
    committed_pty_content_rect, hidden_panel_ids, initial_runtime_geometry, pty_resize_viewport,
    resolve_screen, screen_rect,
};
use crate::state::transition::TransitionExt;
use crate::state::{AppEvent, AppState};
use crate::workbench::{PanelId, ScreenId, ScreenIdentity, builtin_screens};

fn state_on(screen: impl Into<ScreenIdentity>) -> AppState {
    let mut state = AppState::test_fixture();
    state.restore_navigation_root(screen);
    state
}

fn resolved(
    screen: impl Into<ScreenIdentity>,
    cols: u16,
    rows: u16,
) -> crate::workbench::ResolvedLayout {
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
fn initial_runtime_geometry_comes_from_the_resolved_frame() {
    let mut dashboard = state_on(crate::workbench::DASHBOARD_IDENTITY);
    dashboard.resolved_layout = resolve_screen(&dashboard, 120, 40);
    let layout = dashboard
        .resolved_layout
        .as_ref()
        .unwrap_or_else(|| unreachable!("dashboard resolves"));
    let descriptor = dashboard
        .published_workbench()
        .screen_registry()
        .get_identity(crate::workbench::DASHBOARD_IDENTITY)
        .unwrap_or_else(|| unreachable!("dashboard is compiled"));
    let terminal =
        crate::workbench::pty_content_rect(descriptor, layout, &PanelId::from_static("terminal"))
            .unwrap_or_else(|| unreachable!("dashboard terminal is visible"));
    assert_eq!(
        initial_runtime_geometry(&dashboard),
        Some((terminal.height, terminal.width))
    );

    let mut settings = state_on(ScreenId::Settings);
    settings.resolved_layout = resolve_screen(&settings, 120, 40);
    let outer = settings
        .resolved_layout
        .as_ref()
        .unwrap_or_else(|| unreachable!("settings resolves"))
        .outer;
    assert_eq!(
        initial_runtime_geometry(&settings),
        Some((outer.height, outer.width)),
        "a screen without a PTY commits its resolved frame rather than ambient terminal size"
    );
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
fn a_resize_targets_the_active_screens_resolved_pty_viewport() {
    // The Terminal Manager's PTY is the shell preview below the list, not the
    // dashboard pane the mirror arithmetic models. The resize a child receives
    // must be the committed frame's rectangle for whichever screen is showing.
    let state = state_on(ScreenId::Terminals);
    let viewport = pty_resize_viewport(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("terminals shows its shell preview"));
    let layout =
        resolve_screen(&state, 120, 40).unwrap_or_else(|| unreachable!("terminals resolves"));
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .unwrap_or_else(|| unreachable!("terminals is compiled"));
    let pty_panel = descriptor
        .panels
        .iter()
        .find(|panel| panel.panel_type.as_str() == crate::workbench::PTY_PANEL_TYPE)
        .unwrap_or_else(|| unreachable!("terminals declares a shell-preview PTY panel"));
    let rect = crate::workbench::pty_content_rect(descriptor, &layout, &pty_panel.id)
        .unwrap_or_else(|| unreachable!("the shell preview is visible at 120x40"));
    assert_eq!(viewport, (rect.height, rect.width));
    let mirror = crate::layout::compute_pty_layout_for_windowed(120, 40, false);
    assert_ne!(
        viewport,
        (mirror.pty_rows, mirror.pty_cols),
        "the dashboard mirror must stop answering for the Terminal Manager screen"
    );
}

#[test]
fn a_screen_without_a_visible_pty_panel_sends_no_resize() {
    let state = state_on(ScreenId::Settings);
    assert_eq!(
        pty_resize_viewport(&state, 120, 40),
        None,
        "no fabricated resize may leave the resolver"
    );
}

#[test]
fn the_committed_frame_answers_for_the_terminal_pane_rectangle() {
    // Mouse hit-testing and replay translation need the on-screen rectangle
    // the renderer drew, so they read the committed frame instead of
    // re-deriving a mirror from the terminal size (issue #706).
    let mut state = state_on(ScreenId::Terminals);
    state.resolved_layout = resolve_screen(&state, 120, 40);
    let rect = committed_pty_content_rect(&state)
        .unwrap_or_else(|| unreachable!("the committed terminals frame shows its shell preview"));
    let layout = state
        .resolved_layout
        .as_ref()
        .unwrap_or_else(|| unreachable!("the frame was just committed"));
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .unwrap_or_else(|| unreachable!("terminals is compiled"));
    let pty_panel = descriptor
        .panels
        .iter()
        .find(|panel| panel.panel_type.as_str() == crate::workbench::PTY_PANEL_TYPE)
        .unwrap_or_else(|| unreachable!("terminals declares a shell-preview PTY panel"));
    let content = crate::workbench::pty_content_rect(descriptor, layout, &pty_panel.id)
        .unwrap_or_else(|| unreachable!("the shell preview is visible at 120x40"));
    assert_eq!(rect, content);
    let mirror = crate::layout::compute_pty_layout_for_windowed(120, 40, false);
    assert_ne!(
        (rect.row, rect.col),
        (mirror.pane_row0, mirror.pane_col0),
        "the dashboard mirror must stop answering for the Terminal Manager screen"
    );
}

#[test]
fn a_frame_without_a_visible_pty_panel_has_no_terminal_pane_rectangle() {
    let mut state = state_on(ScreenId::Settings);
    state.resolved_layout = resolve_screen(&state, 120, 40);
    assert!(
        committed_pty_content_rect(&state).is_none(),
        "no PTY is on screen, so no pane rectangle exists to hit-test"
    );
}

#[test]
fn without_a_committed_frame_there_is_no_terminal_pane_rectangle() {
    let state = state_on(ScreenId::Terminals);
    assert!(
        committed_pty_content_rect(&state).is_none(),
        "nothing has been rendered yet, so no rectangle may be fabricated"
    );
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
    let state = state_on(crate::workbench::DASHBOARD_IDENTITY);
    let idle = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    assert_eq!(
        idle.panel(&PanelId::from_static("search"))
            .map(|panel| panel.visible),
        Some(false)
    );

    let state = state.apply(AppEvent::OpenSearch).committed_pure();
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
            let layout = resolved(crate::workbench::DASHBOARD_IDENTITY, cols, rows);
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
    showing = showing.apply(AppEvent::OpenSearch).committed_pure();

    let mut overlay = state_on(screen);
    overlay.open_shell_overlay(AgentId("agent-1".to_owned()));

    vec![quiet, showing, overlay]
}

#[test]
fn every_panel_the_application_hides_is_declared_by_its_screen() {
    // The hiding rules name panels by identity literal, so a descriptor that
    // renamed a panel would leave them addressing nothing and the band would
    // stay on screen forever. Nothing else would notice.
    let registry = builtin_screens()
        .unwrap_or_else(|_| unreachable!("the compiled registry must be well formed"));
    for screen in ScreenId::ALL {
        let Some(descriptor) = registry.get(screen) else {
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
