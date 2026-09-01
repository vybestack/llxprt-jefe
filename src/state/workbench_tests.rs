//! Workbench state tests (issue #626).
//!
//! Reducer-level tests proving:
//! - The default status filter is all-on (not the projection's all-off).
//! - Toggling a bucket flips exactly that bucket and resets the page to 0.
//! - Next/prev page are clamped at both ends (no wrap).

use super::workbench_filter::WorkbenchUiState;
use super::{AppEvent, AppState};
use crate::state::transition::TransitionExt;
use crate::test_support::Must;
use crate::workbench_view::{StatusBucket, StatusFilterMask};

#[test]
fn default_status_filter_is_all_on() {
    let state = AppState::test_fixture();
    let mask = state.workbench.status_filter.mask();
    assert!(mask.allows(StatusBucket::NeedsYou));
    assert!(mask.allows(StatusBucket::Working));
    assert!(mask.allows(StatusBucket::Ready));
    assert!(mask.allows(StatusBucket::Stale));
    assert!(!mask.all_off(), "default mask must not be all-off");
}

#[test]
fn toggling_a_bucket_flips_only_that_bucket() {
    let state = AppState::test_fixture();
    let after = state
        .apply(AppEvent::ToggleWorkbenchStatusBucket(StatusBucket::Ready))
        .committed_pure();

    let mask = after.workbench.status_filter.mask();
    assert!(
        mask.allows(StatusBucket::NeedsYou),
        "unrelated buckets stay on"
    );
    assert!(mask.allows(StatusBucket::Working));
    assert!(
        !mask.allows(StatusBucket::Ready),
        "toggled bucket flips off"
    );
    assert!(mask.allows(StatusBucket::Stale));
}

#[test]
fn toggling_a_bucket_resets_page_to_zero() {
    let mut state = AppState::test_fixture();
    state.workbench = WorkbenchUiState {
            page: 5,
            ..WorkbenchUiState::default()
        };
    let after = state
        .apply(AppEvent::ToggleWorkbenchStatusBucket(StatusBucket::Working))
        .committed_pure();
    assert_eq!(
        after.workbench.page, 0,
        "toggling a filter must reset the page"
    );
}

/// A workbench state rooted on the split screen with `count` visible agents.
fn workbench_state_with_agents(count: usize) -> AppState {
    let mut state = AppState::test_fixture();
    state.nav = crate::state::navigation::NavState::rooted(crate::state::ScreenId::Repositories);
    state.repositories = vec![crate::test_support::host_panel_repository("one")];
    for index in 0..count {
        let agent = crate::test_support::host_panel_agent(
            &format!("agent{index:02}"),
            "repo-one",
            crate::domain::AgentStatus::Running,
        );
        state
            .observations
            .insert(agent.id.clone(), crate::test_support::ready_observation());
        state.agents.push(agent);
    }
    state
}

/// The same state with a committed frame for one terminal size, the way the
/// render loop publishes one before any input is reduced.
fn committed_workbench_state(count: usize, cols: u16, rows: u16) -> AppState {
    let mut state = workbench_state_with_agents(count);
    state.resolved_layout = crate::screen_layout::resolve_screen(&state, cols, rows);
    state
}

/// The page count the painted split-screen grid shows for a render size.
///
/// The legacy screen builds its view from the effective render size, so this
/// is the display clamp every input path has to agree with.
fn display_page_count(state: &AppState, render_cols: usize, render_rows: usize) -> usize {
    let inputs = crate::host_panel_models::workbench_agent_inputs(state);
    crate::workbench_view::build_workbench_view_ref(
        &inputs,
        state.workbench.status_filter.mask(),
        None,
        render_cols,
        render_rows,
        0,
    )
    .layout
    .page_count
}

/// The declared host capability of the split screen's card grid.
fn cards_capability(state: &AppState) -> crate::workbench::HostPanelCapability {
    state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .and_then(|descriptor| {
            descriptor.panels.iter().find(|panel| {
                panel.host_capability.is_some_and(|capability| {
                    capability.model_source() == crate::workbench::HostPanelModelSource::WorkbenchCards
                })
            })
        })
        .and_then(|panel| panel.host_capability)
        .must("the split screen declares the cards host control")
}

/// The committed content rectangle of the card grid panel.
fn cards_content_rect(state: &AppState) -> (usize, usize) {
    let layout = state
        .resolved_layout
        .as_ref()
        .must("the fixture commits a frame");
    let panel = layout
        .panel(&crate::workbench::PanelId::from_static("cards"))
        .must("the cards panel must place");
    (
        usize::from(panel.content.width),
        usize::from(panel.content.height),
    )
}

#[test]
fn next_page_advances_page() {
    let state = committed_workbench_state(30, 120, 40);
    let page_count = display_page_count(&state, 120, 40);
    assert!(page_count >= 2, "30 agents must page at 120x40");
    let after = state.apply(AppEvent::WorkbenchNextPage).committed_pure();
    assert_eq!(after.workbench.page, 1);
}

/// `WorkbenchNextPage` is the live keyboard path (`split.page-down`). The
/// retained page counter must hold at the last page the display can show:
/// the painted grid clamps its own page index, so a counter past that makes
/// `WorkbenchPrevPage` walk back through pages nothing can display.
#[test]
fn next_page_holds_at_the_last_page_the_display_shows() {
    let state = committed_workbench_state(30, 120, 40);
    let page_count = display_page_count(&state, 120, 40);
    assert!(page_count >= 2, "30 agents must page at 120x40");

    let mut after = state;
    for _ in 0..(page_count + 3) {
        after = after.apply(AppEvent::WorkbenchNextPage).committed_pure();
    }
    assert_eq!(
        after.workbench.page,
        page_count - 1,
        "next page must hold at the display's last page (display page_count {page_count})"
    );
}

/// Without a committed frame there is no grid geometry to page within, so
/// the counter must not drift ahead of what any display can show.
#[test]
fn next_page_is_inert_without_a_committed_frame() {
    let state = workbench_state_with_agents(30);
    let after = state.apply(AppEvent::WorkbenchNextPage).committed_pure();
    assert_eq!(
        after.workbench.page, 0,
        "no committed frame means there is no page to advance to"
    );
}

/// The host-panel input clamp (boundary or mouse PageNext on the cards
/// control) must bound the retained page counter by the page count the
/// display path computes. The display builds its view from the full render
/// size; the clamp used to be handed the panel's content rectangle and the
/// shared helpers subtract full-terminal chrome themselves, so it counted
/// more pages than the display can show and paged past the last real page
/// (issue #706).
#[test]
fn host_panel_page_next_holds_at_the_display_page_count() {
    let mut state = committed_workbench_state(30, 120, 40);
    let (content_width, content_height) = cards_content_rect(&state);
    let page_count = display_page_count(&state, 120, 40);
    assert!(page_count >= 2, "30 agents must page at 120x40");
    // The fixture must expose the disagreement: the content rectangle,
    // fed through chrome-subtracting helpers, yields a different page
    // count than the display's render-size basis.
    let content_basis =
        crate::workbench_view::grid_page_count(content_width, content_height, 30);
    assert_ne!(
        content_basis, page_count,
        "the fixture must distinguish the two geometry bases"
    );

    let capability = cards_capability(&state);
    for _ in 0..(page_count + 3) {
        assert!(state.apply_host_panel_action(
            capability,
            crate::host_controls::ControlAction::PageNext,
            content_width,
            content_height,
        ));
    }
    assert_eq!(
        state.workbench.page,
        page_count - 1,
        "the input clamp must agree with the display's page count (display {page_count}, content-basis {content_basis})"
    );
}

/// An out-of-range page is clamped by the projection, not the reducer.
#[test]
fn projection_clamps_a_page_beyond_the_last() {
    let view = crate::workbench_view::build_workbench_view_ref(
        &[],
        crate::workbench_view::StatusFilterMask::all_on(),
        None,
        200,
        40,
        99,
    );
    assert_eq!(
        view.layout.page, 0,
        "a page past the end must clamp to the last page"
    );
}

/// The filter cursor keys must actually resolve in the workbench context. This
/// pins the binding itself, which a projection test cannot see and which a
/// terminal scenario can only observe indirectly.
#[test]
fn filter_cursor_keys_resolve_in_the_workbench_context() {
    use crate::domain::default_action_inventory::compiled_inventory;
    use crate::domain::keymap::Chord;

    let Ok(inventory) = compiled_inventory() else {
        panic!("the default action inventory must compile");
    };

    let action_for = |text: &str| {
        let Ok(chord) = Chord::parse(text) else {
            panic!("chord {text:?} must parse");
        };
        inventory
            .bindings
            .iter()
            .find(|b| b.context.as_str() == "split" && b.chords.contains(&chord))
            .map(|b| b.action.as_str().to_owned())
    };

    // Shortcut jump is a global action, and the workbench context stack is
    // ["split", "global"], so it already reaches this screen without a
    // split-scoped binding of its own.
    let global_action_for = |text: &str| {
        let Ok(chord) = Chord::parse(text) else {
            panic!("chord {text:?} must parse");
        };
        inventory
            .bindings
            .iter()
            .find(|b| b.context.as_str() == "global" && b.chords.contains(&chord))
            .map(|b| b.action.as_str().to_owned())
    };
    assert_eq!(
        global_action_for("Alt+2").as_deref(),
        Some("core.jump-agent.2"),
        "Alt+N must jump to the agent in shortcut slot N"
    );

    assert_eq!(action_for("Down").as_deref(), Some("split.navigate-down"));
    assert_eq!(action_for("Up").as_deref(), Some("split.navigate-up"));
    assert_eq!(
        action_for(" ").as_deref(),
        Some("split.toggle-status-filter")
    );
}

/// Moving the cursor and toggling must change the mask for the bucket the
/// cursor landed on, not the one it started on.
#[test]
fn cursor_move_then_toggle_affects_the_second_bucket() {
    let state = AppState::test_fixture();
    let after = state
        .apply(AppEvent::WorkbenchFilterCursorNext)
        .committed_pure();
    assert_eq!(after.workbench.filter_cursor, 1);
    assert_eq!(
        after.workbench_filter_cursor_bucket(),
        crate::workbench_view::StatusBucket::Working
    );

    let toggled = after
        .apply(AppEvent::ToggleWorkbenchStatusBucket(
            crate::workbench_view::StatusBucket::Working,
        ))
        .committed_pure();
    assert!(
        !toggled
            .workbench
            .status_filter
            .mask()
            .allows(crate::workbench_view::StatusBucket::Working),
        "Working must be filtered out after toggling it at the cursor"
    );
}

/// Enter is bound on the workbench, and it attaches rather than opening the
/// edit form the dashboard opens.
#[test]
fn enter_attaches_on_the_workbench() {
    use crate::domain::default_action_inventory::compiled_inventory;
    use crate::domain::keymap::Chord;

    let Ok(inventory) = compiled_inventory() else {
        panic!("the default action inventory must compile");
    };
    let Ok(enter) = Chord::parse("Enter") else {
        panic!("Enter must parse");
    };
    let action = inventory
        .bindings
        .iter()
        .find(|b| b.context.as_str() == "split" && b.chords.contains(&enter))
        .map(|b| b.action.as_str().to_owned());
    assert_eq!(action.as_deref(), Some("split.activate-selection"));
}

/// Attaching with nothing selected must not strand the user on a dashboard
/// with a focused terminal and no agent.
#[test]
fn attach_without_a_selection_is_inert() {
    let state = AppState::test_fixture();
    let after = state.apply(AppEvent::WorkbenchAttach).committed_pure();
    assert!(
        !after.terminal_focused,
        "an empty grid must not focus a terminal"
    );
}

#[test]
fn prev_page_clamps_at_zero() {
    let state = AppState::test_fixture();
    let after = state.apply(AppEvent::WorkbenchPrevPage).committed_pure();
    assert_eq!(
        after.workbench.page, 0,
        "prev page at 0 must clamp, not wrap"
    );
}

#[test]
fn prev_page_decrements_from_positive() {
    let mut state = AppState::test_fixture();
    state.workbench = WorkbenchUiState {
            page: 3,
            ..WorkbenchUiState::default()
        };
    let after = state.apply(AppEvent::WorkbenchPrevPage).committed_pure();
    assert_eq!(after.workbench.page, 2);
}

#[test]
fn next_page_then_prev_returns_to_start() {
    let state = committed_workbench_state(30, 120, 40);
    let mid = state.apply(AppEvent::WorkbenchNextPage).committed_pure();
    let after = mid.apply(AppEvent::WorkbenchPrevPage).committed_pure();
    assert_eq!(after.workbench.page, 0);
}

#[test]
fn toggling_all_buckets_off_yields_all_off_mask() {
    let state = AppState::test_fixture();
    let mut after = state;
    for bucket in [
        StatusBucket::NeedsYou,
        StatusBucket::Working,
        StatusBucket::Ready,
        StatusBucket::Stale,
    ] {
        after = after
            .apply(AppEvent::ToggleWorkbenchStatusBucket(bucket))
            .committed_pure();
    }
    assert!(
        after.workbench.status_filter.mask().all_off(),
        "toggling every bucket off must yield all-off"
    );
}

#[test]
fn toggling_a_bucket_back_on_restores_all_on() {
    let state = AppState::test_fixture();
    let off = state
        .apply(AppEvent::ToggleWorkbenchStatusBucket(StatusBucket::Stale))
        .committed_pure();
    let on = off
        .apply(AppEvent::ToggleWorkbenchStatusBucket(StatusBucket::Stale))
        .committed_pure();
    assert_eq!(
        on.workbench.status_filter.mask(),
        StatusFilterMask::all_on(),
        "toggling twice must restore the original mask"
    );
}
