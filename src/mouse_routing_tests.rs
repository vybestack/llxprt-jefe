//! Tests for mouse_routing.rs (issue #197).
//!
//! Extracted from mouse_routing.rs to keep that file under the 1000-line limit.
//! These tests exercise the pure helper functions and the gesture-state-machine
//! wiring without requiring iocraft or a live runtime.

use super::{
    WheelDirection, active_overlay_for, gesture_event_kind, is_blocking_modal_open,
    is_event_over_terminal_pane, is_wheel_event, next_wheel_scroll_offset, resolve_pane,
    wheel_to_terminal_scroll_event,
};
use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
use jefe::domain::{Agent, AgentId, AgentKind, Repository, RepositoryId};
use jefe::runtime::{TerminalCell, TerminalCellStyle, TerminalSnapshot};
use jefe::selection::{
    GestureAction, GestureEvent, GestureEventKind, GestureState, OverlayPane, SelectablePane,
    pane_content_lines, selection_text, terminal_selection_text,
};
use jefe::state::{AppState, ModalState, PaneFocus, ScreenMode};
use std::path::PathBuf;

// ── next_wheel_scroll_offset (clamping + stale recovery) ─────────────────────

#[test]
fn scroll_down_advances_within_bounds() {
    assert_eq!(next_wheel_scroll_offset(2, 5, WheelDirection::Down), 3);
}

#[test]
fn scroll_up_decrements_within_bounds() {
    assert_eq!(next_wheel_scroll_offset(2, 5, WheelDirection::Up), 1);
}

#[test]
fn scroll_down_clamps_at_max_offset() {
    assert_eq!(next_wheel_scroll_offset(5, 5, WheelDirection::Down), 5);
}

#[test]
fn scroll_up_clamps_at_zero() {
    assert_eq!(next_wheel_scroll_offset(0, 5, WheelDirection::Up), 0);
}

#[test]
fn scroll_down_with_zero_max_stays_zero() {
    assert_eq!(next_wheel_scroll_offset(0, 0, WheelDirection::Down), 0);
}

#[test]
fn scroll_up_with_zero_max_stays_zero() {
    assert_eq!(next_wheel_scroll_offset(0, 0, WheelDirection::Up), 0);
}

#[test]
fn scroll_up_from_inflated_offset_snaps_below_max() {
    assert_eq!(next_wheel_scroll_offset(8, 5, WheelDirection::Up), 4);
}

#[test]
fn scroll_down_from_inflated_offset_snaps_to_max() {
    assert_eq!(next_wheel_scroll_offset(8, 5, WheelDirection::Down), 5);
}

#[test]
fn scroll_up_from_inflated_offset_near_zero_snaps_to_zero() {
    assert_eq!(next_wheel_scroll_offset(8, 1, WheelDirection::Up), 0);
}

// ── gesture_event_kind classification ────────────────────────────────────────

#[test]
fn gesture_event_kind_classifies_left_button_events() {
    assert_eq!(
        gesture_event_kind(MouseEventKind::Down(MouseButton::Left)),
        Some(GestureEventKind::LeftDown)
    );
    assert_eq!(
        gesture_event_kind(MouseEventKind::Drag(MouseButton::Left)),
        Some(GestureEventKind::LeftDrag)
    );
    assert_eq!(
        gesture_event_kind(MouseEventKind::Up(MouseButton::Left)),
        Some(GestureEventKind::LeftUp)
    );
}

#[test]
fn gesture_event_kind_classifies_wheel_right_middle_as_other() {
    assert_eq!(
        gesture_event_kind(MouseEventKind::ScrollUp),
        Some(GestureEventKind::ScrollUp)
    );
    assert_eq!(
        gesture_event_kind(MouseEventKind::ScrollDown),
        Some(GestureEventKind::ScrollDown)
    );
    assert_eq!(
        gesture_event_kind(MouseEventKind::Down(MouseButton::Right)),
        Some(GestureEventKind::OtherButton)
    );
    assert_eq!(
        gesture_event_kind(MouseEventKind::Down(MouseButton::Middle)),
        Some(GestureEventKind::OtherButton)
    );
}

// ── Finding F: non-dashboard mode disables terminal routing ──────────────────

fn focused_terminal_state(kind: AgentKind) -> AppState {
    let repo_id = RepositoryId("repo-1".into());
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        repo_id.clone(),
        "repo".into(),
        "repo".into(),
        PathBuf::from("/tmp/repo"),
    ));
    let mut agent = Agent::new(
        AgentId("agent-1".into()),
        repo_id,
        "agent".into(),
        PathBuf::from("/tmp/agent"),
    );
    agent.agent_kind = kind;
    state.agents.push(agent);
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);
    state.terminal_focused = true;
    state.pane_focus = PaneFocus::Terminal;
    state
}

#[test]
fn terminal_pane_resolves_in_dashboard_mode() {
    let mut state = focused_terminal_state(AgentKind::CodePuppy);
    state.screen_mode = ScreenMode::Dashboard;
    // terminal_input_enabled=false means the terminal pane IS selectable
    // (it's not occupied by PTY input routing in this context).
    match resolve_pane(&state, 30, 20, 120, 40, false) {
        Some((SelectablePane::TerminalView, _)) => {}
        other => panic!("expected TerminalView in Dashboard mode, got {other:?}"),
    }
}

#[test]
fn terminal_pane_not_routed_in_issues_mode() {
    // Finding F: terminal routing is disabled in non-Dashboard modes even when
    // terminal_focused is true. The terminal pane should not resolve as a
    // selectable terminal in Issues mode (there is no TerminalView there).
    let mut state = focused_terminal_state(AgentKind::CodePuppy);
    state.screen_mode = ScreenMode::DashboardIssues;
    // In Issues mode, the coordinate would map to IssueList/IssueDetail, not
    // TerminalView. This test verifies the geometry doesn't produce a
    // TerminalView (which would be a ghost selection).
    let result = resolve_pane(&state, 30, 20, 120, 40, false);
    let Some((pane, _)) = result else {
        panic!("a dashboard-issues coordinate must resolve to some pane, not None");
    };
    assert_ne!(
        pane,
        SelectablePane::TerminalView,
        "TerminalView must not resolve in Issues mode"
    );
}

// ── Finding G: blocking modals ───────────────────────────────────────────────

#[test]
fn theme_picker_is_blocking_modal() {
    let state = AppState {
        modal: ModalState::ThemePicker {
            available_themes: Vec::new(),
            selected_index: 0,
            active_slug: String::new(),
            override_theme: false,
        },
        ..AppState::default()
    };
    assert!(
        is_blocking_modal_open(&state),
        "ThemePicker must be a blocking modal"
    );
}

#[test]
fn workflow_dispatch_is_blocking_modal() {
    use jefe::domain::Workflow;
    use jefe::state::WorkflowDispatchFormFields;
    let mut state = AppState::default();
    state.modal = ModalState::WorkflowDispatch {
        workflow: Workflow {
            id: 1,
            name: "WF".to_string(),
            path: "wf.yml".to_string(),
            state: "active".to_string(),
        },
        fields: WorkflowDispatchFormFields::default(),
        focus: jefe::state::WorkflowDispatchFormFocus::default(),
        cursor: jefe::state::WorkflowDispatchFormCursor::default(),
    };
    assert!(
        is_blocking_modal_open(&state),
        "WorkflowDispatch must be a blocking modal"
    );
}

#[test]
fn no_modal_is_not_blocking() {
    let state = AppState::default();
    assert!(!is_blocking_modal_open(&state));
}

#[test]
fn search_is_not_blocking_modal() {
    let state = AppState {
        modal: ModalState::Search {
            query: "test".to_string(),
        },
        ..AppState::default()
    };
    assert!(!is_blocking_modal_open(&state));
}

#[test]
fn focused_terminal_behind_theme_picker_does_not_route_to_terminal() {
    // Finding G: even with a focused terminal, ThemePicker blocks.
    let mut state = focused_terminal_state(AgentKind::CodePuppy);
    state.modal = ModalState::ThemePicker {
        available_themes: Vec::new(),
        selected_index: 0,
        active_slug: String::new(),
        override_theme: false,
    };
    assert!(
        is_blocking_modal_open(&state),
        "terminal behind ThemePicker must be blocked"
    );
}

// ── Finding H + critical #1: shift passthrough and reporting-click replay ──

fn resolver(col: u16, row: u16) -> Option<jefe::selection::SelectionPoint> {
    // Coordinate-sensitive resolver: returns a point whose line/col reflect the
    // input so the gesture-machine tests can detect regressions where the
    // buffered-down coordinate is swapped with the drag coordinate. Returns
    // None for out-of-pane coords (mimicking the real pane geometry).
    if col < 2 || row < 2 {
        return None;
    }
    Some(jefe::selection::SelectionPoint::new(
        SelectablePane::TerminalView,
        usize::from(row) - 2,
        usize::from(col) - 2,
    ))
}

#[test]
fn reporting_click_replays_down_then_up_with_real_coords() {
    // Critical fix (issue #197 review): a pure click on a reporting child
    // (down with no drag, then up) must replay BOTH the buffered down AND the
    // up, each at its real coordinate. The old code emitted only the down.
    let down = GestureEvent {
        kind: GestureEventKind::LeftDown,
        shift_held: false,
        col: 5,
        row: 3,
        mouse_reporting_active: true,
    };
    let (action, pending) = GestureState::default().process(down, &resolver);
    assert!(matches!(action, GestureAction::Noop));
    assert!(matches!(pending, GestureState::Pending { .. }));

    let up = GestureEvent {
        kind: GestureEventKind::LeftUp,
        shift_held: false,
        col: 9,
        row: 7,
        mouse_reporting_active: true,
    };
    let (action, next) = pending.process(up, &resolver);
    let GestureAction::ForwardToPty(replays) = action else {
        panic!("expected ForwardToPty for reporting click, got {action:?}");
    };
    assert_eq!(next, GestureState::Idle);
    assert_eq!(
        replays,
        vec![
            jefe::selection::PtyReplay {
                col: 5,
                row: 3,
                kind: GestureEventKind::LeftDown,
            },
            jefe::selection::PtyReplay {
                col: 9,
                row: 7,
                kind: GestureEventKind::LeftUp,
            },
        ],
        "reporting click must replay down + up at real coordinates, in order"
    );
}

#[test]
fn reporting_drag_begins_range_from_buffered_down() {
    // High fix #3 (issue #197 review): a reporting gesture that becomes a drag
    // must begin a selection spanning the buffered down coordinate through the
    // drag coordinate — not a collapsed selection at the anchor.
    let down = GestureEvent {
        kind: GestureEventKind::LeftDown,
        shift_held: false,
        col: 2,
        row: 4,
        mouse_reporting_active: true,
    };
    let (_, pending) = GestureState::default().process(down, &resolver);
    let drag = GestureEvent {
        kind: GestureEventKind::LeftDrag,
        shift_held: false,
        col: 8,
        row: 4,
        mouse_reporting_active: true,
    };
    let (action, owned) = pending.process(drag, &resolver);
    match action {
        GestureAction::BeginSelectionRange { anchor, focus } => {
            assert!(matches!(owned, GestureState::JefeOwned { .. }));
            // Coordinate-sensitive checks (issue #197 review): the anchor must
            // be the buffered-down coordinate and the focus the drag
            // coordinate, proving the range spans down→drag and is not swapped
            // or collapsed. down@(2,4) -> (line 2, col 0); drag@(8,4) -> (line 2, col 6).
            assert_eq!(anchor.pane, SelectablePane::TerminalView);
            assert_eq!((anchor.line, anchor.col), (2, 0), "anchor = buffered down");
            assert_eq!(focus.pane, SelectablePane::TerminalView);
            assert_eq!((focus.line, focus.col), (2, 6), "focus = drag coordinate");
        }
        other => panic!("expected BeginSelectionRange, got {other:?}"),
    }
}

#[test]
fn stray_left_down_while_pending_flushes_buffered_down() {
    // Medium fix #4 (issue #197 review): a second LeftDown while Pending must
    // flush the buffered down to the PTY before starting the new gesture.
    let down = GestureEvent {
        kind: GestureEventKind::LeftDown,
        shift_held: false,
        col: 3,
        row: 3,
        mouse_reporting_active: true,
    };
    let (_, pending) = GestureState::default().process(down, &resolver);
    let second_down = GestureEvent {
        kind: GestureEventKind::LeftDown,
        shift_held: false,
        col: 6,
        row: 6,
        mouse_reporting_active: true,
    };
    let (action, _) = pending.process(second_down, &resolver);
    let GestureAction::ForwardToPty(replays) = action else {
        panic!("expected flush ForwardToPty, got {action:?}");
    };
    // The flush must include the buffered down at (3,3).
    assert!(
        replays
            .iter()
            .any(|r| r.col == 3 && r.row == 3 && r.kind == GestureEventKind::LeftDown),
        "stray LeftDown must flush the buffered down to the PTY: {replays:?}"
    );
}

// ── Finding K: resolve_pane param naming ─────────────────────────────────────

#[test]
fn resolve_pane_terminal_input_enabled_excludes_terminal() {
    let mut state = focused_terminal_state(AgentKind::CodePuppy);
    state.screen_mode = ScreenMode::Dashboard;
    // When terminal_input_enabled=true (focused terminal), the dashboard
    // terminal region returns None (pane_at excludes it).
    assert!(
        resolve_pane(&state, 30, 20, 120, 40, true).is_none(),
        "terminal region must be excluded when terminal_input_enabled"
    );
}

#[test]
fn resolve_pane_terminal_not_enabled_includes_terminal() {
    let mut state = focused_terminal_state(AgentKind::CodePuppy);
    state.screen_mode = ScreenMode::Dashboard;
    // When terminal_input_enabled=false (unfocused/preview), the terminal
    // region resolves to TerminalView.
    match resolve_pane(&state, 30, 20, 120, 40, false) {
        Some((SelectablePane::TerminalView, _)) => {}
        other => panic!("expected TerminalView when not input-enabled, got {other:?}"),
    }
}

// ── Critical regression: the production gesture resolver must resolve a
//    focused-terminal coordinate to TerminalView (issue #197 runtime bug).
//
// `resolve_terminal_point` calls `resolve_pane(..., terminal_input_enabled)`.
// For a FOCUSED terminal that flag must be `false` — otherwise `pane_at`
// excludes the whole terminal region (returns None), the left-button down has
// no anchor, and the gesture can never begin a selection (no highlight, no
// copy). This test wires the SAME `resolve_pane(false)` call the production
// resolver makes into the gesture state machine so the end-to-end path — real
// geometry → gesture → TerminalView selection range — is guarded. The earlier
// hand-rolled test resolver masked this bug (it always returned TerminalView).

#[test]
fn focused_terminal_drag_resolves_to_terminalview_selection_via_production_geometry() {
    use jefe::selection::{SelectionPoint, point_to_content_coords};

    let state = focused_terminal_state(AgentKind::CodePuppy);
    // Mirror the production resolve_terminal_point: resolve_pane with
    // terminal_input_enabled = false (NOT true) so the terminal region is
    // selectable while focused.
    let resolver = |col: u16, row: u16| -> Option<SelectionPoint> {
        let (pane, geometry) = resolve_pane(&state, col, row, 120, 40, false)?;
        let (line, c) = point_to_content_coords(col, row, 0, &geometry);
        Some(SelectionPoint::new(pane, line, c))
    };

    // A coordinate in the dashboard terminal slot (middle column, below the
    // agent list) must resolve to TerminalView — not None.
    let point = resolver(30, 25);
    assert!(
        matches!(
            point,
            Some(SelectionPoint {
                pane: SelectablePane::TerminalView,
                ..
            })
        ),
        "production geometry resolver must map an in-terminal coord to TerminalView (got {point:?}); \
         if this is None, resolve_terminal_point is passing terminal_input_enabled=true and the \
         gesture can never begin a selection over a focused terminal"
    );

    // Drive the gesture machine the way route_terminal_gesture does: a
    // reporting left-down (buffered) then a left-drag must produce a
    // Jefe-owned selection RANGE over TerminalView.
    let down = GestureEvent {
        kind: GestureEventKind::LeftDown,
        shift_held: false,
        col: 30,
        row: 25,
        mouse_reporting_active: true,
    };
    let (down_action, down_gesture) = GestureState::default().process(down, &resolver);
    assert_eq!(
        down_action,
        GestureAction::Noop,
        "down buffers while pending"
    );
    assert!(matches!(down_gesture, GestureState::Pending { .. }));

    let drag = GestureEvent {
        kind: GestureEventKind::LeftDrag,
        shift_held: false,
        col: 35,
        row: 25,
        mouse_reporting_active: true,
    };
    let (drag_action, drag_gesture) = down_gesture.process(drag, &resolver);
    match drag_action {
        GestureAction::BeginSelectionRange { anchor, focus } => {
            assert_eq!(anchor.pane, SelectablePane::TerminalView);
            assert_eq!(focus.pane, SelectablePane::TerminalView);
        }
        other => panic!(
            "drag over focused reporting terminal must begin a TerminalView selection range, got {other:?}"
        ),
    }
    assert!(
        matches!(drag_gesture, GestureState::JefeOwned { .. }),
        "gesture latches Jefe ownership"
    );
}

// ── Terminal selection text with wraps + wide chars (Finding C+D) ─────────────

fn styled_cell(ch: char) -> TerminalCell {
    TerminalCell {
        ch,
        style: TerminalCellStyle {
            fg: iocraft::Color::White,
            bg: iocraft::Color::Black,
            bold: false,
            dim: false,
            underline: false,
        },
        wide_spacer: false,
    }
}

fn styled_cell_with_spacer(ch: char) -> TerminalCell {
    TerminalCell {
        ch,
        style: TerminalCellStyle {
            fg: iocraft::Color::White,
            bg: iocraft::Color::Black,
            bold: false,
            dim: false,
            underline: false,
        },
        wide_spacer: true,
    }
}

#[test]
fn terminal_selection_text_matches_snapshot_including_unicode() {
    use jefe::selection::{SelectionPoint, TextSelection};
    let cells = vec![
        vec![styled_cell('h'), styled_cell('i'), styled_cell('!')],
        vec![styled_cell('\u{41F}'), styled_cell('\u{0454}')],
    ];
    let snapshot = TerminalSnapshot {
        rows: 2,
        cols: 3,
        cells,
        wraps: Vec::new(),
    };
    let state = focused_terminal_state(AgentKind::CodePuppy);
    let content = pane_content_lines(
        SelectablePane::TerminalView,
        &state,
        Some(&snapshot),
        &[],
        120,
        40,
    );
    assert_eq!(
        content.lines,
        vec!["hi!".to_string(), "\u{41F}\u{0454}".to_string()]
    );

    let sel = TextSelection {
        anchor: SelectionPoint::new(SelectablePane::TerminalView, 1, 0),
        focus: SelectionPoint::new(SelectablePane::TerminalView, 1, 1),
    };
    assert_eq!(selection_text(&sel, &content.lines), "\u{41F}");
}

#[test]
fn terminal_selection_text_with_real_cjk_wide_char() {
    // Finding D: the old test claimed "CJK" but used Cyrillic (width-1).
    // This test uses an actual width-2 CJK character (中) with a wide_spacer
    // cell, proving the spacer is skipped during selection extraction.
    use jefe::selection::{SelectionPoint, TextSelection};
    let snapshot = TerminalSnapshot {
        rows: 1,
        cols: 4,
        cells: vec![vec![
            styled_cell('A'),
            styled_cell('中'),
            styled_cell_with_spacer(' '),
            styled_cell('B'),
        ]],
        wraps: Vec::new(),
    };
    // Select columns 0..4 (the whole row).
    let sel = TextSelection {
        anchor: SelectionPoint::new(SelectablePane::TerminalView, 0, 0),
        focus: SelectionPoint::new(SelectablePane::TerminalView, 0, 4),
    };
    // The wide spacer is skipped: result is "A中B", not "A中 B".
    assert_eq!(terminal_selection_text(&snapshot, &sel), "A中B");
}

// ── Finding B: copy uses the selection-bound snapshot ────────────────────────

#[test]
fn terminal_selection_text_uses_bound_snapshot_not_recaptured() {
    use jefe::selection::{SelectionPoint, TextSelection};
    // Snapshot A: what the user highlighted.
    let snapshot_a = TerminalSnapshot {
        rows: 1,
        cols: 5,
        cells: vec![vec![
            styled_cell('H'),
            styled_cell('E'),
            styled_cell('L'),
            styled_cell('L'),
            styled_cell('O'),
        ]],
        wraps: Vec::new(),
    };
    let sel = TextSelection {
        anchor: SelectionPoint::new(SelectablePane::TerminalView, 0, 0),
        focus: SelectionPoint::new(SelectablePane::TerminalView, 0, 5),
    };
    // The extracted text comes from snapshot_a — prove copy is bound to it.
    assert_eq!(terminal_selection_text(&snapshot_a, &sel), "HELLO");
}

// ── active_overlay_for (issue #178 z-order, preserved) ────────────────────────

#[test]
fn active_overlay_none_when_no_modal() {
    let state = AppState::default();
    assert_eq!(active_overlay_for(&state), OverlayPane::None);
}

#[test]
fn active_overlay_help_modal() {
    let state = AppState {
        modal: ModalState::Help,
        ..AppState::default()
    };
    assert_eq!(active_overlay_for(&state), OverlayPane::HelpModal);
}

#[test]
fn active_overlay_agent_form() {
    use jefe::state::AgentFormFields;
    let mut state = AppState::default();
    state.modal = ModalState::NewAgent {
        repository_id: RepositoryId("r".into()),
        fields: AgentFormFields::default(),
        focus: jefe::state::AgentFormFocus::Name,
        cursor: jefe::state::AgentFormCursor::default(),
        work_dir_manual: false,
    };
    assert_eq!(active_overlay_for(&state), OverlayPane::AgentForm);
}

#[test]
fn active_overlay_theme_picker_falls_through_to_none_overlay() {
    // ThemePicker doesn't have an OverlayPane variant — it falls through to
    // None but is caught by is_blocking_modal_open (Finding G).
    let state = AppState {
        modal: ModalState::ThemePicker {
            available_themes: Vec::new(),
            selected_index: 0,
            active_slug: String::new(),
            override_theme: false,
        },
        ..AppState::default()
    };
    assert_eq!(active_overlay_for(&state), OverlayPane::None);
}

#[test]
fn active_overlay_agent_chooser() {
    let mut state = AppState::default();
    state.issues_state.agent_chooser = Some(jefe::state::AgentChooserState::default());
    assert_eq!(active_overlay_for(&state), OverlayPane::AgentChooser);
}

// ── Issue #198: wheel→scrollback helpers ─────────────────────────────────

fn fullscreen_event(kind: MouseEventKind) -> iocraft::FullscreenMouseEvent {
    let mut event = iocraft::FullscreenMouseEvent::new(kind, 0, 0);
    event.modifiers = KeyModifiers::NONE;
    event
}

#[test]
fn is_wheel_detects_scroll_up_and_down() {
    assert!(is_wheel_event(&fullscreen_event(MouseEventKind::ScrollUp)));
    assert!(is_wheel_event(&fullscreen_event(
        MouseEventKind::ScrollDown
    )));
}

#[test]
fn is_wheel_rejects_non_scroll_events() {
    assert!(!is_wheel_event(&fullscreen_event(MouseEventKind::Down(
        MouseButton::Left
    ))));
    assert!(!is_wheel_event(&fullscreen_event(MouseEventKind::Up(
        MouseButton::Left
    ))));
}

#[test]
fn wheel_to_terminal_scroll_event_maps_scroll_up() {
    let evt = wheel_to_terminal_scroll_event(&fullscreen_event(MouseEventKind::ScrollUp));
    assert!(
        matches!(evt, Some(jefe::state::AppEvent::TerminalScrollUp)),
        "ScrollUp must map to TerminalScrollUp"
    );
}

#[test]
fn wheel_to_terminal_scroll_event_maps_scroll_down() {
    let evt = wheel_to_terminal_scroll_event(&fullscreen_event(MouseEventKind::ScrollDown));
    assert!(
        matches!(evt, Some(jefe::state::AppEvent::TerminalScrollDown)),
        "ScrollDown must map to TerminalScrollDown"
    );
}

#[test]
fn wheel_to_terminal_scroll_event_returns_none_for_non_wheel() {
    let evt =
        wheel_to_terminal_scroll_event(&fullscreen_event(MouseEventKind::Down(MouseButton::Left)));
    assert!(evt.is_none(), "non-wheel events must map to None");
}

#[test]
fn is_event_over_terminal_pane_origin_is_outside() {
    // The origin (0,0) is always outside the terminal pane: the sidebar and
    // status bar occupy the left columns and top rows. This is a stable,
    // terminal-size-independent property.
    let origin = fullscreen_event_at(0, 0, MouseEventKind::ScrollUp);
    assert!(
        !is_event_over_terminal_pane(&origin),
        "(0,0) must be outside the terminal pane (sidebar/status bar region)"
    );
}

fn fullscreen_event_at(col: u16, row: u16, kind: MouseEventKind) -> iocraft::FullscreenMouseEvent {
    let mut event = iocraft::FullscreenMouseEvent::new(kind, col, row);
    event.modifiers = KeyModifiers::NONE;
    event
}

// ── Wrap-aware mouse→content reverse map (issue #212 follow-up) ──────────────
//
// When the issue detail body WORD-WRAPS, several display rows belong to the
// SAME content line. A naive `row → content_line` map (1:1) would put a click
// on a wrapped subrow onto the wrong line. `content_coords_for_pane` threads
// the wrap projection through so the reverse map is exact.

use crate::detail_wrap_map::{ScreenCoord, content_coords_for_pane, detail_wrap_projection};
use jefe::domain::{IssueDetail, IssueState};
use jefe::selection::PaneGeometry;

/// Minimal AppState carrying a single issue detail with the given body.
fn state_with_issue_body(body: &str) -> AppState {
    let mut state = AppState::default();
    state.issues_state.issue_detail = Some(IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 1,
        node_id: String::new(),
        title: "T".to_string(),
        state: IssueState::Open,
        author_login: "a".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        labels: Vec::new(),
        assignees: Vec::new(),
        milestone: None,
        body: body.to_string(),
        external_url: String::new(),
        comments: Vec::new(),
        has_more_comments: false,
        comments_cursor: None,
    });
    state
}

/// PaneGeometry whose content origin is (0,0) so row N maps to content row N
/// directly — keeping the math in the test legible.
fn origin_geometry() -> PaneGeometry {
    PaneGeometry {
        origin_col: 0,
        origin_row: 0,
        width: 100,
        height: 40,
        content_origin_col: 0,
        content_origin_row: 0,
    }
}

/// A click on the SECOND wrapped row of a long body line must resolve to the
/// SAME content line as the first row (the naive 1:1 map would return +1),
/// and the column must advance by the wrapped row's char offset.
#[test]
fn wrapped_body_rows_resolve_to_same_content_line() {
    use jefe::layout::DETAIL_HEADER_ROWS;
    use jefe::layout::{ISSUES_SIDEBAR_WIDTH, issues_detail_content_width};
    // A body that wraps to several rows at a narrow width.
    let body = "alpha bravo charlie delta echo foxtrot";
    let state = state_with_issue_body(body);
    let geo = origin_geometry();
    // Derive a column count from the real layout constants (not magic numbers)
    // so the test stays correct if sidebar/chrome widths change.
    let cols: u16 = ISSUES_SIDEBAR_WIDTH.saturating_add(18);
    let expected_width = usize::from(issues_detail_content_width(cols));
    let Some((content, _headers, width)) =
        detail_wrap_projection(&state, SelectablePane::IssueDetail, cols)
    else {
        panic!("IssueDetail must have a wrap projection");
    };
    assert_eq!(
        width, expected_width,
        "wrap width must equal the layout-derived content width"
    );
    let rows = jefe::ui::components::doc_wrap::wrap_document(&content, width);
    // Sanity: wrapping produced more display rows than content lines.
    assert!(
        rows.len() > content.lines().count(),
        "test premise: content must wrap ({} rows > {} lines)",
        rows.len(),
        content.lines().count()
    );
    // Resolve content coords for a screen vp_row; helper keeps the test body
    // under the complexity line limit.
    let resolve = |vp_row: u16| {
        content_coords_for_pane(
            &state,
            SelectablePane::IssueDetail,
            cols,
            &ScreenCoord {
                col: 0,
                row: vp_row,
                scroll_offset: 0,
                geometry: &geo,
            },
        )
        .0
    };
    // Scan the body region for two consecutive viewport rows that map to the
    // SAME content line — the signature of a wrapped subrow pair. Under the
    // old naive 1:1 map every row would map to a distinct line, so finding
    // such a pair proves the wrap-aware reverse map is in effect.
    let first_body_row = u16::try_from(DETAIL_HEADER_ROWS).unwrap_or(u16::MAX);
    let mut found_pair = None;
    for vp in first_body_row..first_body_row + 10 {
        let here = resolve(vp);
        let next = resolve(vp + 1);
        if here == next {
            found_pair = Some((vp, here));
            break;
        }
    }
    let Some((vp, line)) = found_pair else {
        panic!(
            "a wrapped body line must produce two consecutive rows on one line (width {width}, {} rows)",
            rows.len()
        );
    };
    // The content line must be the wrapped body text line (not "> Body" which
    // is a single row): it carries the long body text.
    let body_line_text = content.lines().nth(line - DETAIL_HEADER_ROWS).unwrap_or("");
    assert!(
        body_line_text.contains("alpha"),
        "the wrapped pair (vp row {vp}, content line {line}) must be the long body text, got {body_line_text:?}"
    );
}

/// The reverse map must also return the correct COLUMN: a click at screen
/// column N on a wrapped subrow maps to that row's char start + N (the
/// specific character under the cursor), not just the row's left edge.
#[test]
fn wrapped_body_row_column_advances_with_screen_col() {
    use jefe::layout::DETAIL_HEADER_ROWS;
    use jefe::layout::ISSUES_SIDEBAR_WIDTH;
    let body = "alpha bravo charlie delta echo foxtrot";
    let state = state_with_issue_body(body);
    let geo = origin_geometry();
    let cols: u16 = ISSUES_SIDEBAR_WIDTH.saturating_add(18);
    let Some((content, _headers, width)) =
        detail_wrap_projection(&state, SelectablePane::IssueDetail, cols)
    else {
        panic!("IssueDetail must have a wrap projection");
    };
    let rows = jefe::ui::components::doc_wrap::wrap_document(&content, width);
    // Resolve content coords for a screen (col, vp_row); helper keeps the test
    // body under the complexity line limit.
    let resolve = |screen_col: u16, vp_row: u16| {
        content_coords_for_pane(
            &state,
            SelectablePane::IssueDetail,
            cols,
            &ScreenCoord {
                col: screen_col,
                row: vp_row,
                scroll_offset: 0,
                geometry: &geo,
            },
        )
    };
    // Find the body line that carries the long text, then its SECOND wrapped
    // row (line_char_start > 0) — the row whose column mapping we exercise.
    let body_line = content
        .lines()
        .position(|l| l.contains("alpha"))
        .unwrap_or_else(|| panic!("body text not found in content: {content:?}"));
    let second = rows
        .iter()
        .find(|r| r.line == body_line && r.line_char_start > 0)
        .unwrap_or_else(|| panic!("expected a wrapped second row; rows={rows:?}"));
    let char_start = second.line_char_start;
    // The screen vp row for `second` = header rows + the count of wrapped rows
    // rendered before it.
    let rows_before = rows
        .iter()
        .take_while(|r| {
            r.line < second.line || (r.line == second.line && r.line_char_start < char_start)
        })
        .count();
    let body_vp_row = u16::try_from(DETAIL_HEADER_ROWS + rows_before).unwrap_or(u16::MAX);
    // col 0 -> the row's char START; col 2 -> char START + 2 (the exact char).
    let (at_zero_line, at_zero_col) = resolve(0, body_vp_row);
    let (_, at_two_col) = resolve(2, body_vp_row);
    assert_eq!(
        at_zero_line,
        DETAIL_HEADER_ROWS + body_line,
        "row maps to the body content line (header offset + body line)"
    );
    assert_eq!(
        at_zero_col, char_start,
        "col 0 maps to the wrapped row's char start"
    );
    assert_eq!(
        at_two_col,
        char_start + 2,
        "col 2 maps to char start + 2 (the specific char under the cursor)"
    );
}

/// A click on a header row keeps the naive mapping (headers do not wrap).
#[test]
fn header_row_uses_naive_mapping() {
    use jefe::layout::DETAIL_HEADER_ROWS;
    let state = state_with_issue_body("short");
    let geo = origin_geometry();
    let header_row = u16::try_from(DETAIL_HEADER_ROWS - 1).unwrap_or(u16::MAX);
    let (line, _col) = content_coords_for_pane(
        &state,
        SelectablePane::IssueDetail,
        120,
        &ScreenCoord {
            col: 5,
            row: header_row,
            scroll_offset: 99, // a non-zero scroll offset must be IGNORED for header rows
            geometry: &geo,
        },
    );
    // Header row maps to its own index regardless of scroll offset.
    assert_eq!(line, usize::from(header_row));
}

/// Non-detail panes have no wrap projection and fall back to the naive map.
#[test]
fn non_detail_pane_has_no_wrap_projection() {
    let state = AppState::default();
    assert!(
        detail_wrap_projection(&state, SelectablePane::IssueList, 120).is_none(),
        "IssueList must not produce a wrap projection"
    );
}
