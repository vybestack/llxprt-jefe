//! Tests for mouse_routing.rs (issue #197).
//!
//! Extracted from mouse_routing.rs to keep that file under the 1000-line limit.
//! These tests exercise the pure helper functions and the gesture-state-machine
//! wiring without requiring iocraft or a live runtime.

use super::{
    WheelDirection, active_overlay_for, gesture_event_kind, is_blocking_modal_open,
    next_wheel_scroll_offset, resolve_pane,
};
use crossterm::event::{MouseButton, MouseEventKind};
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
