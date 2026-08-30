//! Equivalence between snapshot hit-testing and the superseded arithmetic
//! (issue #384, CW04-04).
//!
//! While both paths exist they must agree on which pane owns a cell. These
//! tests are what make routing clicks through the snapshot safe; once the
//! mirror arithmetic is deleted they become the record of what it produced.

use crate::screen_layout::resolve_screen;
use crate::selection::geometry::pane_at;
use crate::selection::text::SelectablePane;
use crate::selection::{ScreenLayout, panel_to_selectable, selectable_to_panel};
use crate::state::{AppState, ScreenId};
use crate::workbench::{PanelId, ResolvedLayout, ScreenIdentity};

fn state_on(screen: impl Into<ScreenIdentity>) -> AppState {
    let mut state = AppState::test_fixture();
    state.restore_navigation_root(screen);
    state
}

fn snapshot(screen: impl Into<ScreenIdentity>, cols: u16, rows: u16) -> ResolvedLayout {
    resolve_screen(&state_on(screen), cols, rows)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"))
}

fn legacy(screen: ScreenId, cols: u16, rows: u16) -> ScreenLayout {
    ScreenLayout::new(cols, rows, screen.into(), false, false)
}

#[test]
fn the_pane_vocabulary_maps_in_both_directions() {
    for pane in [
        SelectablePane::Sidebar,
        SelectablePane::AgentList,
        SelectablePane::TerminalView,
        SelectablePane::Preview,
        SelectablePane::IssueList,
        SelectablePane::IssueDetail,
        SelectablePane::PrList,
        SelectablePane::PrDetail,
        SelectablePane::ActionsList,
        SelectablePane::ActionsDetail,
        SelectablePane::ErrorList,
        SelectablePane::ErrorDetail,
    ] {
        let Some(panel) = selectable_to_panel(pane, crate::workbench::DASHBOARD_IDENTITY) else {
            unreachable!("{pane:?} must map to a panel");
        };
        assert_eq!(
            panel_to_selectable(panel),
            Some(pane),
            "{pane:?} must round-trip"
        );
    }
}

#[test]
fn the_terminal_pane_names_the_right_panel_on_each_screen() {
    // One pane, two panel identities: resolving without the screen would miss
    // the Terminal Manager's preview entirely.
    assert_eq!(
        selectable_to_panel(
            SelectablePane::TerminalView,
            crate::workbench::DASHBOARD_IDENTITY
        ),
        Some(PanelId::from_static("terminal"))
    );
    assert_eq!(
        selectable_to_panel(SelectablePane::TerminalView, ScreenId::Terminals),
        Some(PanelId::from_static("shell-preview"))
    );
}

#[test]
fn every_screens_panels_resolve_back_to_themselves() {
    // The reverse mapping must name a panel that actually exists on the screen
    // asked about, or a consumer holding a pane gets no geometry at all.
    for screen in ScreenId::ALL {
        let resolved = snapshot(screen, 120, 40);
        for panel in resolved.visible_panels() {
            let Some(pane) = panel_to_selectable(panel.id) else {
                continue;
            };
            assert_eq!(
                selectable_to_panel(pane, screen),
                Some(panel.id),
                "screen {screen} panel {} does not resolve back to itself",
                panel.id
            );
        }
    }
}

#[test]
fn overlay_panes_map_to_no_descriptor_panel() {
    // Overlays are deliberately not modelled by descriptors, so they must not
    // silently claim a panel identity.
    for pane in [
        SelectablePane::HelpModal,
        SelectablePane::AgentForm,
        SelectablePane::RepositoryForm,
        SelectablePane::AgentChooser,
        SelectablePane::StatusBar,
        SelectablePane::KeybindBar,
    ] {
        assert_eq!(
            selectable_to_panel(pane, crate::workbench::DASHBOARD_IDENTITY),
            None,
            "{pane:?}"
        );
    }
}

#[test]
fn a_band_is_not_selectable() {
    for band in [
        "search",
        "filter",
        "issue-list-banner",
        "issue-list-filter",
        "pr-list-banner",
        "pr-list-filter",
        "action-list-banner",
        "action-list-filter",
    ] {
        assert_eq!(
            panel_to_selectable(PanelId::from_static(band)),
            None,
            "{band} was never selectable"
        );
    }
}

#[test]
fn a_focused_terminal_is_not_selectable_through_the_resolved_path() {
    let resolved = snapshot(crate::workbench::DASHBOARD_IDENTITY, 120, 40);
    let legacy_layout = legacy(ScreenId::Repositories, 120, 40);
    let Some(terminal) = resolved.panel(&PanelId::from_static("terminal")) else {
        unreachable!("the dashboard always declares a terminal panel");
    };
    let (col, row) = (terminal.content.col, terminal.content.row);

    assert_eq!(
        pane_at(col, row, Some(&resolved), true, &legacy_layout).map(|(pane, _)| pane),
        None,
        "a focused terminal forwards its mouse events to the child process"
    );
    assert_eq!(
        pane_at(col, row, Some(&resolved), false, &legacy_layout).map(|(pane, _)| pane),
        Some(SelectablePane::TerminalView)
    );
}

#[test]
fn every_visible_selectable_panel_is_reachable_by_its_own_origin() {
    for screen in ScreenId::ALL {
        let resolved = snapshot(screen, 120, 40);
        let legacy_layout = legacy(screen, 120, 40);
        for panel in resolved.visible_panels() {
            let Some(expected) = panel_to_selectable(panel.id) else {
                continue;
            };
            let hit = pane_at(
                panel.chrome.col,
                panel.chrome.row,
                Some(&resolved),
                false,
                &legacy_layout,
            );
            assert_eq!(
                hit.map(|(pane, _)| pane),
                Some(expected),
                "screen {screen} panel {} is not reachable at its own origin",
                panel.id
            );
        }
    }
}

#[test]
fn a_hidden_panel_owns_no_cell() {
    let mut state = state_on(ScreenId::Issues);
    state.issues_state.filter_ui.controls_open = false;
    let resolved = resolve_screen(&state, 120, 40)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    let legacy_layout = legacy(ScreenId::Issues, 120, 40);
    for col in (0_u16..120).step_by(7) {
        for row in (1_u16..39).step_by(3) {
            let hit = pane_at(col, row, Some(&resolved), false, &legacy_layout);
            let Some((pane, _)) = hit else {
                continue;
            };
            let Some(panel) = selectable_to_panel(pane, ScreenId::Issues) else {
                continue;
            };
            assert_eq!(
                resolved.panel(&panel).map(|found| found.visible),
                Some(true),
                "a hidden panel claimed ({col}, {row})"
            );
        }
    }
}

#[test]
fn the_snapshot_wrap_width_matches_the_width_the_renderer_wraps_at() {
    // The reverse map must wrap at exactly the width the detail pane drew at,
    // or a click on a wrapped subrow lands on the wrong content line.
    for cols in [80_u16, 100, 120, 160, 200] {
        let issues = snapshot(ScreenId::Issues, cols, 40);
        let (render_cols, _) = crate::layout::effective_render_size(cols, 40);
        assert_eq!(
            crate::selection::detail_wrap_width(
                &issues,
                SelectablePane::IssueDetail,
                ScreenId::Issues
            ),
            Some(usize::from(crate::layout::issues_detail_content_width(
                render_cols
            ))),
            "issue detail wrap width diverged at {cols} columns"
        );

        let prs = snapshot(ScreenId::PullRequests, cols, 40);
        assert_eq!(
            crate::selection::detail_wrap_width(
                &prs,
                SelectablePane::PrDetail,
                ScreenId::PullRequests
            ),
            Some(usize::from(crate::layout::prs_detail_content_width(
                render_cols
            ))),
            "pr detail wrap width diverged at {cols} columns"
        );
    }
}

#[test]
fn a_hidden_detail_pane_has_no_wrap_width() {
    let mut state = state_on(ScreenId::Issues);
    state.issues_state.filter_ui.controls_open = true;
    // A terminal too small to seat the detail pane leaves nothing to wrap.
    let tiny = resolve_screen(&state, 30, 8)
        .unwrap_or_else(|| unreachable!("the shipped registry always resolves"));
    assert_eq!(
        crate::selection::detail_wrap_width(&tiny, SelectablePane::IssueDetail, ScreenId::Issues),
        None
    );
}

#[test]
fn the_content_origin_sits_inside_the_pane_box() {
    for screen in ScreenId::ALL {
        let resolved = snapshot(screen, 120, 40);
        let legacy_layout = legacy(screen, 120, 40);
        for panel in resolved.visible_panels() {
            if panel_to_selectable(panel.id).is_none() {
                continue;
            }
            let Some((_, geometry)) = pane_at(
                panel.chrome.col,
                panel.chrome.row,
                Some(&resolved),
                false,
                &legacy_layout,
            ) else {
                continue;
            };
            assert!(
                geometry.content_origin_col >= geometry.origin_col
                    && geometry.content_origin_row >= geometry.origin_row,
                "screen {screen} panel {} has a content origin outside its box",
                panel.id
            );
        }
    }
}
