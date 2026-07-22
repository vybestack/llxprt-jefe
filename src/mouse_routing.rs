use crate::app_shell::{CtxArc, HookState, capture_terminal_snapshot};

#[path = "mouse_routing_detail.rs"]
mod mouse_routing_detail;
use crate::pty_encoding::mouse_event_to_bytes;
use jefe::clipboard;
use jefe::layout::{compute_pty_layout, compute_shell_overlay_pty_layout};
use jefe::pane_content_projection::projected_pane_content;
use jefe::runtime::RuntimeManager;
use jefe::selection::{
    GestureAction, GestureEvent, GestureEventKind, GestureState, PtyReplay, ScreenLayout,
    SelectablePane, SelectionPoint, TextSelection, pane_at, point_to_content_coords,
    selection_text, terminal_selection_text,
};
use jefe::state::{AppState, PaneFocus, ScreenMode};
use mouse_routing_detail::refresh_detail_viewport_rows;

pub type ClipboardWriter = fn(&str) -> Result<(), std::io::Error>;

/// Terminal size fallback for the default 120x40 geometry.
fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((120, 40))
}

fn active_pty_layout(cols: u16, rows: u16, overlay_active: bool) -> jefe::layout::PtyLayout {
    if overlay_active {
        compute_shell_overlay_pty_layout(cols, rows)
    } else {
        compute_pty_layout(cols, rows)
    }
}

fn refresh_terminal_scroll_geometry_from_ctx(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    overlay_active: bool,
) {
    let (cols, rows) = terminal_size();
    let pty_layout = active_pty_layout(cols, rows, overlay_active);

    // Cache-only history read: no multiplexer subprocess while holding the
    // context guard (issue #374 S3). On cold miss/contention, preserve prior
    // geometry instead of zeroing it (which would clear the scroll offset
    // and jump to follow-tail during attach).
    let (history_count, live_rows) = match ctx {
        Some(ctx_arc) => {
            let Some(geometry) =
                crate::app_shell_workers::try_capture_history_geometry_from_cache(Some(ctx_arc))
            else {
                return;
            };
            geometry
        }
        None => return,
    };

    let mut state = app_state.write();
    let old_total = state.terminal_total_lines;
    let viewport_rows = usize::from(pty_layout.pty_rows);

    let (new_offset, new_total) = jefe::state::scrollback_ops::compute_terminal_scroll_geometry(
        state.terminal_history_offset,
        old_total,
        history_count,
        live_rows,
        viewport_rows,
    );
    state.terminal_history_offset = new_offset;
    state.terminal_viewport_rows = viewport_rows;
    state.terminal_total_lines = new_total;
}

fn gesture_event_kind(kind: crossterm::event::MouseEventKind) -> Option<GestureEventKind> {
    use crossterm::event::{MouseButton, MouseEventKind};
    match kind {
        MouseEventKind::Down(MouseButton::Left) => Some(GestureEventKind::LeftDown),
        MouseEventKind::Drag(MouseButton::Left) => Some(GestureEventKind::LeftDrag),
        MouseEventKind::Up(MouseButton::Left) => Some(GestureEventKind::LeftUp),
        MouseEventKind::ScrollUp => Some(GestureEventKind::ScrollUp),
        MouseEventKind::ScrollDown => Some(GestureEventKind::ScrollDown),
        MouseEventKind::Down(MouseButton::Right | MouseButton::Middle)
        | MouseEventKind::Drag(MouseButton::Right | MouseButton::Middle)
        | MouseEventKind::Up(MouseButton::Right | MouseButton::Middle) => {
            Some(GestureEventKind::OtherButton)
        }
        _ => None,
    }
}

/// Clear any active mouse selection.
///
/// Called on every non-mouse terminal event (key, paste, resize) so a
/// selection doesn't linger after the user moves on to keyboard interaction.
/// Also resets the terminal gesture state: a Pending gesture (which has no
/// `selection` yet) must not survive a keyboard/paste/resize event, otherwise
/// a buffered reporting down could leak into a later gesture against a
/// different agent or screen (issue #197 review: gesture/snapshot invalidation).
pub fn clear_selection(app_state: &mut HookState<AppState>) {
    let mut state = app_state.write();
    state.selection = None;
    state.selection_snapshot = None;
    state.selection_dashboard_git_info = None;
    state.terminal_gesture_state = GestureState::default();
}

/// Route a fullscreen mouse event to PTY forwarding or app-level selection.
pub fn handle_fullscreen_mouse(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    mouse_event: iocraft::FullscreenMouseEvent,
) {
    let shift_held = mouse_event.modifiers.contains(iocraft::KeyModifiers::SHIFT);

    // Determine whether the terminal is the active input target and read the
    // reporting flag ONCE under a single lock (Finding E + F + G + J).
    let (terminal_active, mouse_reporting_active) = terminal_target_info(ctx, app_state);
    let overlay_active = app_state.read().shell_overlay_active();

    // If the terminal is the active input target, route through the gesture
    // state machine. Otherwise, fall through to app-level pane selection.
    if terminal_active
        && route_terminal_gesture(
            ctx,
            app_state,
            &mouse_event,
            shift_held,
            mouse_reporting_active,
            overlay_active,
        )
    {
        return;
    }

    // Non-terminal routing: shift-modified non-left-button events are host
    // passthrough (Finding H) — return early.
    if shift_held && !is_left_button(mouse_event.kind) {
        return;
    }

    // App selection over the rendered panes (terminal not focused or not
    // dashboard). The terminal pane is selectable only when unfocused.
    match mouse_event.kind {
        crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            begin_app_selection(app_state, mouse_event.column, mouse_event.row);
        }
        crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
            update_app_selection(app_state, mouse_event.column, mouse_event.row);
        }
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
            finalize_and_copy_selection(ctx, app_state, clipboard::write_osc52);
        }
        crossterm::event::MouseEventKind::ScrollUp
        | crossterm::event::MouseEventKind::ScrollDown => {
            dispatch_detail_scroll(app_state, &mouse_event);
        }
        _ => {}
    }
}

fn is_left_button(kind: crossterm::event::MouseEventKind) -> bool {
    matches!(
        kind,
        crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
            | crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left)
            | crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left)
    )
}

/// Route a mouse event over the focused terminal through the gesture-ownership
/// state machine (Finding A). Returns `true` when the event was consumed
/// (handled by terminal routing), `false` when it should fall through to
/// app-level pane selection (e.g. an unmapped event kind like plain move).
fn route_terminal_gesture(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    mouse_event: &iocraft::FullscreenMouseEvent,
    shift_held: bool,
    mouse_reporting_active: bool,
    overlay_active: bool,
) -> bool {
    let (gesture_state, kennel_mode) = {
        let state = app_state.read();
        (state.terminal_gesture_state.clone(), state.is_kennel_mode())
    };

    let Some(event_kind) = gesture_event_kind(mouse_event.kind) else {
        // Unmapped event kind (e.g. plain move) — reset gesture state and let
        // the caller fall through to app selection.
        app_state.write().terminal_gesture_state = GestureState::default();
        return false;
    };

    // Issue #198 + #245: wheel events over the focused terminal pane move the
    // Jefe scrollback viewport BEFORE the gesture state machine or PTY
    // forwarding — but ONLY for kennel (Code Puppy) agents. Non-kennel agents
    // (llxprt) handle their own scrolling via SGR mouse reporting, so their
    // wheel events must fall through to the gesture state machine below which
    // forwards them to the PTY (issue #245). Shift+wheel is host passthrough.
    // Clicks/drags always flow through the gesture machine so reporting
    // children stay interactive.
    if wheel_intercept_active_for_agent(kennel_mode, shift_held)
        && is_wheel_event(mouse_event)
        && is_event_over_terminal_pane(mouse_event, overlay_active)
    {
        // Refresh scroll geometry from runtime + layout BEFORE applying so
        // the reducer's clamp bounds match the rendered content (mirrors the
        // keyboard dispatch path). Runs at event time, never at render time.
        refresh_terminal_scroll_geometry_from_ctx(ctx, app_state, overlay_active);
        if let Some(scroll_evt) = wheel_to_terminal_scroll_event(mouse_event) {
            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(scroll_evt);
        }
        return true;
    }

    let event = GestureEvent {
        kind: event_kind,
        shift_held,
        col: mouse_event.column,
        row: mouse_event.row,
        mouse_reporting_active,
        kennel_mode,
    };
    let resolver = |col: u16, row: u16| resolve_terminal_point(app_state, col, row);

    let (action, new_gesture_state) = gesture_state.process(event, &resolver);
    app_state.write().terminal_gesture_state = new_gesture_state;

    execute_gesture_action(ctx, app_state, action, mouse_event);
    true
}

/// Read the terminal target info (active + reporting) under a single lock
/// acquisition (Finding E: no TOCTOU; Finding F: dashboard-only; Finding G:
/// blocking overlay check; Finding J: log lock poisoning).
///
/// Returns `(false, false)` when the terminal is not the active input target.
fn terminal_target_info(ctx: Option<&CtxArc>, app_state: &HookState<AppState>) -> (bool, bool) {
    let (terminal_focused, pane_focus, screen_mode, modal_blocking) = {
        let state = app_state.read();
        (
            state.terminal_focused,
            state.pane_focus,
            state.screen_mode,
            is_blocking_modal_open(&state),
        )
    };

    // Finding F: terminal routing only in Dashboard mode.
    // Finding G: blocking modal intercepts mouse input.
    let terminal_active = terminal_focused
        && pane_focus == PaneFocus::Terminal
        && screen_mode == ScreenMode::Dashboard
        && !modal_blocking;

    if !terminal_active {
        return (false, false);
    }

    // Read reporting once under a single lock (Finding E).
    let reporting = match ctx {
        Some(ctx_arc) => {
            if let Ok(guard) = ctx_arc.lock() {
                guard.runtime.mouse_reporting_active()
            } else {
                // Finding J: log lock poisoning, treat as terminal not active.
                tracing::warn!("ctx lock poisoned while reading mouse_reporting_active");
                return (false, false);
            }
        }
        None => false,
    };

    (true, reporting)
}

/// Execute the action returned by the gesture state machine.
fn execute_gesture_action(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    action: GestureAction,
    mouse_event: &iocraft::FullscreenMouseEvent,
) {
    match action {
        GestureAction::BeginSelection(point) => {
            // Capture the snapshot at gesture start (Finding B): the copy at
            // release will use this same snapshot, not a fresh recapture.
            let snapshot = capture_current_snapshot(ctx, app_state);
            let mut state = app_state.write();
            state.selection = Some(TextSelection::collapsed(point));
            state.selection_snapshot = snapshot;
        }
        GestureAction::BeginSelectionRange { anchor, focus } => {
            // Capture the snapshot at gesture start (Finding B): the copy at
            // release will use this same snapshot, not a fresh recapture.
            let snapshot = capture_current_snapshot(ctx, app_state);
            let mut state = app_state.write();
            state.selection = Some(TextSelection { anchor, focus });
            state.selection_snapshot = snapshot;
        }
        GestureAction::UpdateSelection(point) => {
            let anchor = {
                let state = app_state.read();
                state.selection.map(|s| s.anchor)
            };
            if let Some(anchor) = anchor {
                let mut state = app_state.write();
                state.selection = Some(TextSelection {
                    anchor,
                    focus: point,
                });
            }
        }
        GestureAction::FinalizeAndCopy => {
            finalize_terminal_selection(ctx, app_state, clipboard::write_osc52);
        }
        GestureAction::ForwardToPty(replays) => {
            forward_replays(
                ctx,
                &replays,
                mouse_event,
                app_state.read().shell_overlay_active(),
            );
        }
        GestureAction::Composite { first, second } => {
            execute_gesture_action(ctx, app_state, *first, mouse_event);
            execute_gesture_action(ctx, app_state, *second, mouse_event);
        }
        GestureAction::Noop => {
            // For scroll events that the gesture machine noops (non-reporting
            // child), fall through to app-level scroll handling.
            dispatch_detail_scroll(app_state, mouse_event);
        }
    }
}

/// Detail-pane scroll dispatch for a mouse event, shared by the app-selection
/// path and the gesture-machine Noop path so the two cannot diverge (issue
/// #197 review: single source of truth for wheel granularity / pane resolution).
fn dispatch_detail_scroll(
    app_state: &mut HookState<AppState>,
    mouse_event: &iocraft::FullscreenMouseEvent,
) {
    use crossterm::event::MouseEventKind;
    match mouse_event.kind {
        MouseEventKind::ScrollUp => {
            scroll_detail_pane(
                app_state,
                mouse_event.column,
                mouse_event.row,
                WheelDirection::Up,
            );
        }
        MouseEventKind::ScrollDown => {
            scroll_detail_pane(
                app_state,
                mouse_event.column,
                mouse_event.row,
                WheelDirection::Down,
            );
        }
        _ => {}
    }
}

/// Whether the mouse event is a wheel scroll (issue #198).
fn is_wheel_event(mouse_event: &iocraft::FullscreenMouseEvent) -> bool {
    use crossterm::event::MouseEventKind;
    matches!(
        mouse_event.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    )
}

/// Whether the mouse event coordinates land inside the terminal pane bounds
/// (issue #198).
fn is_event_over_terminal_pane(
    mouse_event: &iocraft::FullscreenMouseEvent,
    overlay_active: bool,
) -> bool {
    let (cols, rows) = terminal_size();
    let layout = active_pty_layout(cols, rows, overlay_active);
    let row_end = layout
        .pane_row0
        .saturating_add(layout.pty_rows.saturating_sub(1));
    let col_end = layout
        .pane_col0
        .saturating_add(layout.pty_cols.saturating_sub(1));

    mouse_event.column >= layout.pane_col0
        && mouse_event.column <= col_end
        && mouse_event.row >= layout.pane_row0
        && mouse_event.row <= row_end
}

/// Whether Jefe's scrollback viewport should own wheel events for the focused
/// terminal agent (issue #198 + #245).
///
/// Returns `true` only when the agent is a kennel (Code Puppy) session AND
/// shift is not held (shift+wheel is host passthrough). Non-kennel agents
/// (llxprt) handle their own scrolling via SGR mouse reporting, so the wheel
/// must forward to the PTY through the gesture state machine (issue #245).
///
/// The wheel-vs-non-wheel and over-pane-vs-not checks are performed by the
/// caller (via [`is_wheel_event`] and [`is_event_over_terminal_pane`], already
/// unit-tested separately) so this helper stays focused on the agent-kind
/// gating decision that #245 introduced.
#[must_use]
pub fn wheel_intercept_active_for_agent(kennel_mode: bool, shift_held: bool) -> bool {
    kennel_mode && !shift_held
}

/// Map a wheel event to a terminal scroll AppEvent (issue #198).
///
/// Returns `None` for non-wheel events.
#[must_use]
fn wheel_to_terminal_scroll_event(
    mouse_event: &iocraft::FullscreenMouseEvent,
) -> Option<jefe::state::AppEvent> {
    use crossterm::event::MouseEventKind;
    match mouse_event.kind {
        MouseEventKind::ScrollUp => Some(jefe::state::AppEvent::TerminalScrollUp),
        MouseEventKind::ScrollDown => Some(jefe::state::AppEvent::TerminalScrollDown),
        _ => None,
    }
}

/// Forward a list of PTY replay events, encoding each as SGR mouse bytes.
fn forward_replays(
    ctx: Option<&CtxArc>,
    replays: &[PtyReplay],
    mouse_event: &iocraft::FullscreenMouseEvent,
    overlay_active: bool,
) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return;
    };

    let (cols, rows) = terminal_size();
    let layout = active_pty_layout(cols, rows, overlay_active);

    for replay in replays {
        let (screen_col, screen_row) = (replay.col, replay.row);

        let local_row = screen_row.saturating_sub(layout.pane_row0);
        let local_col = screen_col.saturating_sub(layout.pane_col0);

        // Left-button replays may be buffered/replayed (e.g. a pending click
        // replays its buffered down + the up), so they are reconstructed from
        // the gesture kind. Non-left replays (wheel/right/middle) are always
        // the live event: forward the ORIGINAL mouse event kind so the real
        // button and phase (right-down vs middle-up, etc.) are preserved
        // (issue #197 review: OtherButton collapsed right/middle, which were
        // then silently dropped).
        let crossterm_kind = match replay.kind {
            GestureEventKind::LeftDown => {
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
            }
            GestureEventKind::LeftDrag => {
                crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left)
            }
            GestureEventKind::LeftUp => {
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left)
            }
            GestureEventKind::ScrollUp
            | GestureEventKind::ScrollDown
            | GestureEventKind::OtherButton => mouse_event.kind,
        };

        let mut local_event =
            iocraft::FullscreenMouseEvent::new(crossterm_kind, local_col, local_row);
        local_event.modifiers = mouse_event.modifiers;

        if let Some(bytes) = mouse_event_to_bytes(&local_event) {
            let _ = ctx_guard.runtime.write_input(&bytes);
        }
    }
}

/// Capture the current terminal snapshot for the selected agent.
///
/// Both agent IDs are derived from a single short read so they describe the
/// same selected agent. `capture_terminal_snapshot` re-validates attachment
/// internally, so a mid-flight agent change yields a benign miss rather than
/// corrupted content.
fn capture_current_snapshot(
    ctx: Option<&CtxArc>,
    app_state: &HookState<AppState>,
) -> Option<jefe::runtime::TerminalSnapshot> {
    let (selected_agent_id, selected_running_agent_id) = {
        let state = app_state.read();
        let selected = state.selected_agent();
        let ids = (
            selected.map(|a| a.id.clone()),
            selected.filter(|a| a.is_running()).map(|a| a.id.clone()),
        );
        drop(state);
        ids
    };
    capture_terminal_snapshot(
        ctx,
        &app_state.read(),
        selected_agent_id.as_ref(),
        selected_running_agent_id.as_ref(),
    )
}

fn resolve_terminal_point(
    app_state: &HookState<AppState>,
    col: u16,
    row: u16,
) -> Option<SelectionPoint> {
    let (cols, rows) = terminal_size();
    let (pane, geometry) = {
        let state = app_state.read();
        if state.shell_overlay_active() {
            let layout = compute_shell_overlay_pty_layout(cols, rows);
            let bottom = layout.pane_row0.saturating_add(layout.pty_rows);
            let right = layout.pane_col0.saturating_add(layout.pty_cols);
            if row < layout.pane_row0 || row >= bottom || col < layout.pane_col0 || col >= right {
                return None;
            }
            let chrome_cols = jefe::layout::TERMINAL_VIEW_CHROME_COLS;
            let chrome_rows = jefe::layout::TERMINAL_VIEW_CHROME_ROWS;
            (
                SelectablePane::TerminalView,
                jefe::selection::PaneGeometry::new(
                    layout.pane_col0.saturating_sub(chrome_cols),
                    layout.pane_row0.saturating_sub(chrome_rows),
                    layout.pty_cols.saturating_add(chrome_cols * 2),
                    layout.pty_rows.saturating_add(chrome_rows + 1),
                    layout.pane_col0,
                    layout.pane_row0,
                ),
            )
        } else {
            resolve_pane(&state, col, row, cols, rows, false)?
        }
    };
    let (line, c) = point_to_content_coords(col, row, 0, &geometry);
    Some(SelectionPoint::new(pane, line, c))
}

/// Resolve a screen `(col, row)` to a selection point for non-terminal panes.
fn begin_app_selection(app_state: &mut HookState<AppState>, col: u16, row: u16) {
    let (cols, rows) = terminal_size();
    let point = {
        let state = app_state.read();
        resolve_app_selection_point(&state, col, row, cols, rows)
    };
    if let Some(p) = point {
        let git_info = {
            let state = app_state.read();
            matches!(p.pane, SelectablePane::AgentList | SelectablePane::Preview)
                .then(|| jefe::dashboard_git_info::resolve_dashboard_git_info(&state))
                .flatten()
        };
        let mut state = app_state.write();
        state.selection = Some(TextSelection::collapsed(p));
        state.selection_snapshot = None;
        state.selection_dashboard_git_info = git_info;
    } else {
        let mut state = app_state.write();
        state.selection = None;
        state.selection_snapshot = None;
        state.selection_dashboard_git_info = None;
    }
}

/// Update the focus (drag) point of the active selection for non-terminal panes.
fn update_app_selection(app_state: &mut HookState<AppState>, col: u16, row: u16) {
    let (cols, rows) = terminal_size();
    let (anchor, pane) = {
        let state = app_state.read();
        let Some(current) = state.selection else {
            return;
        };
        let pair = (current.anchor, current.pane());
        drop(state);
        pair
    };
    let focus_point = {
        let state = app_state.read();
        let Some((resolved_pane, geometry)) = resolve_pane(&state, col, row, cols, rows, false)
        else {
            return;
        };
        if resolved_pane != pane {
            return;
        }
        let scroll_offset = scroll_offset_for_pane(&state, pane);
        let scroll_offset = effective_scroll_for_detail(pane, row, &geometry, scroll_offset);
        let (line, c) = crate::detail_wrap_map::content_coords_for_pane(
            &state,
            pane,
            cols,
            rows,
            &crate::detail_wrap_map::ScreenCoord {
                col,
                row,
                scroll_offset,
                geometry: &geometry,
            },
        );
        let point = SelectionPoint::new(pane, line, c);
        drop(state);
        point
    };
    app_state.write().selection = Some(TextSelection {
        anchor,
        focus: focus_point,
    });
}

/// Finalize a TERMINAL selection and copy using the selection-bound snapshot
/// (Finding B) with wrap-aware text extraction (Finding C+D).
fn finalize_terminal_selection(
    ctx: Option<&CtxArc>,
    app_state: &HookState<AppState>,
    writer: ClipboardWriter,
) {
    let (selection, snapshot) = {
        let state = app_state.read();
        (state.selection, state.selection_snapshot.clone())
    };
    let Some(selection) = selection else {
        return;
    };
    if selection.is_empty() {
        return;
    }

    // For the terminal pane, use the selection-bound snapshot with wrap-aware
    // extraction (Finding B + C + D). For non-terminal panes, fall back to the
    // generic path.
    let text = if selection.pane() == SelectablePane::TerminalView {
        if let Some(snap) = &snapshot {
            terminal_selection_text(snap, &selection)
        } else {
            // No bound snapshot — fall back to generic extraction (issue #198:
            // include retained history lines for the terminal pane). Cache-only
            // read so no multiplexer subprocess runs under a guard (issue #374
            // S3).
            let (cols, rows) = terminal_size();
            let history_lines = crate::app_shell_workers::capture_history_from_cache(ctx).clone();
            let state = app_state.read();
            let content =
                projected_pane_content(selection.pane(), &state, None, &history_lines, cols, rows);
            drop(state);
            selection_text(&selection, &content.lines)
        }
    } else {
        let (cols, rows) = terminal_size();
        let state = app_state.read();
        let content = projected_pane_content(selection.pane(), &state, None, &[], cols, rows);
        drop(state);
        selection_text(&selection, &content.lines)
    };

    if !text.is_empty()
        && let Err(err) = writer(&text)
    {
        tracing::warn!(error = %err, "OSC 52 clipboard write failed");
    }
}

/// Finalize and copy for non-terminal selections (legacy entry point for
/// app-level selection finalization).
fn finalize_and_copy_selection(
    ctx: Option<&CtxArc>,
    app_state: &HookState<AppState>,
    writer: ClipboardWriter,
) {
    // This path is reached from the non-terminal app-selection branch. The
    // terminal pane won't be the active selection here.
    let selection = {
        let state = app_state.read();
        state.selection
    };
    let Some(selection) = selection else {
        return;
    };
    if selection.is_empty() {
        return;
    }
    let text = {
        let state = app_state.read();
        let snapshot = capture_terminal_snapshot(
            ctx,
            &state,
            state.selected_agent().map(|a| &a.id),
            state
                .selected_agent()
                .filter(|a| a.is_running())
                .map(|a| &a.id),
        );
        // Capture history lines for the selection content projection (issue
        // #198). Cache-only read so no multiplexer subprocess runs under a
        // guard (issue #374 S3).
        let history_lines = crate::app_shell_workers::capture_history_from_cache(ctx).clone();
        let (cols, rows) = terminal_size();
        let content = projected_pane_content(
            selection.pane(),
            &state,
            snapshot.as_ref(),
            &history_lines,
            cols,
            rows,
        );
        drop(state);
        selection_text(&selection, &content.lines)
    };
    if !text.is_empty()
        && let Err(err) = writer(&text)
    {
        tracing::warn!(error = %err, "OSC 52 clipboard write failed");
    }
}

/// Resolve the screen `(col, row)` to a content selection point under the pane
/// it lands in for non-terminal panes.
fn resolve_app_selection_point(
    app_state: &AppState,
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
) -> Option<SelectionPoint> {
    let (pane, geometry) = resolve_pane(app_state, col, row, cols, rows, false)?;
    let scroll_offset = scroll_offset_for_pane(app_state, pane);
    let scroll_offset = effective_scroll_for_detail(pane, row, &geometry, scroll_offset);
    let (line, c) = crate::detail_wrap_map::content_coords_for_pane(
        app_state,
        pane,
        cols,
        rows,
        &crate::detail_wrap_map::ScreenCoord {
            col,
            row,
            scroll_offset,
            geometry: &geometry,
        },
    );
    Some(SelectionPoint::new(pane, line, c))
}

fn screen_layout_for(state: &AppState, cols: u16, rows: u16) -> ScreenLayout {
    let (mode_error, filter_open) = match state.screen_mode {
        ScreenMode::DashboardIssues => (
            jefe::layout::issues_banner_visible(
                state.issues_state.error.as_deref(),
                state.issues_state.draft_notice.as_deref(),
            ),
            state.issues_state.filter_ui.controls_open,
        ),
        ScreenMode::DashboardPullRequests => (
            state.prs_state.error.is_some(),
            state.prs_state.filter_ui.controls_open,
        ),
        ScreenMode::DashboardActions => (
            state.actions_state.error.is_some(),
            state.actions_state.ui.filter_ui_open,
        ),
        ScreenMode::DashboardErrors
        | ScreenMode::Dashboard
        | ScreenMode::Split
        | ScreenMode::DashboardTerminals => (false, false),
    };
    let error_visible = (state.error_message.is_some()
        && !matches!(state.screen_mode, ScreenMode::DashboardErrors))
        || mode_error;
    ScreenLayout::new(cols, rows, state.screen_mode, error_visible, filter_open)
        .with_overlay(active_overlay_for(state))
}

/// Whether a blocking modal is open (Finding G).
///
/// Blocking modals intercept mouse input even if they have no selectable
/// content projection. ThemePicker and WorkflowDispatch are full-screen
/// modals without content projection — they must still disable terminal
/// routing so a focused terminal behind them doesn't receive PTY events.
fn is_blocking_modal_open(state: &AppState) -> bool {
    use jefe::state::ModalState;
    matches!(
        state.modal,
        ModalState::Help
            | ModalState::NewAgent { .. }
            | ModalState::EditAgent { .. }
            | ModalState::NewRepository { .. }
            | ModalState::EditRepository { .. }
            | ModalState::ConfirmDeleteRepository { .. }
            | ModalState::ConfirmDeleteAgent { .. }
            | ModalState::ConfirmKillAgent { .. }
            | ModalState::PreflightPrompt { .. }
            | ModalState::ConfirmIssueDirtyCopy { .. }
            | ModalState::ConfirmIssueOriginMismatch { .. }
            | ModalState::ThemePicker { .. }
            | ModalState::WorkflowDispatch { .. }
            | ModalState::Auth { .. }
    )
}

/// Determine which overlay (modal/form/chooser) is currently active, if any.
fn active_overlay_for(state: &AppState) -> jefe::selection::OverlayPane {
    use jefe::selection::OverlayPane;
    match &state.modal {
        jefe::state::ModalState::Help => return OverlayPane::HelpModal,
        jefe::state::ModalState::NewAgent { .. } | jefe::state::ModalState::EditAgent { .. } => {
            return OverlayPane::AgentForm;
        }
        jefe::state::ModalState::NewRepository { .. }
        | jefe::state::ModalState::EditRepository { .. } => return OverlayPane::RepositoryForm,
        jefe::state::ModalState::ConfirmDeleteRepository { .. }
        | jefe::state::ModalState::ConfirmDeleteAgent { .. }
        | jefe::state::ModalState::ConfirmKillAgent { .. }
        | jefe::state::ModalState::PreflightPrompt { .. }
        | jefe::state::ModalState::ConfirmIssueDirtyCopy { .. }
        | jefe::state::ModalState::ConfirmIssueOriginMismatch { .. }
        | jefe::state::ModalState::Auth { .. } => {
            return OverlayPane::ConfirmModal;
        }
        jefe::state::ModalState::None
        | jefe::state::ModalState::Search { .. }
        | jefe::state::ModalState::ThemePicker { .. }
        | jefe::state::ModalState::WorkflowDispatch { .. } => {}
    }
    if state.issues_state.agent_chooser.is_some() || state.prs_state.agent_chooser.is_some() {
        return OverlayPane::AgentChooser;
    }
    if state.prs_state.merge_chooser.is_some() {
        return OverlayPane::MergeChooser;
    }
    if state.issues_state.property_editor.is_some() || state.prs_state.property_editor.is_some() {
        return OverlayPane::PropertyEditor;
    }
    if state.issues_state.close_reason_chooser.is_some() {
        return OverlayPane::CloseReasonChooser;
    }
    if state.issues_state.delete_confirm.is_some() {
        return OverlayPane::IssueDeleteConfirm;
    }
    OverlayPane::None
}

/// Resolve which pane + geometry a screen coordinate maps to, given app state.
///
/// `terminal_input_enabled` (Finding K: renamed from `terminal_selectable`)
/// controls whether the dashboard terminal region resolves to
/// [`SelectablePane::TerminalView`]. When the terminal is receiving PTY input,
/// pass `true` so the terminal pane is excluded from selection (returns None
/// for the dashboard terminal region); otherwise `false` so it is selectable.
fn resolve_pane(
    state: &AppState,
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
    terminal_input_enabled: bool,
) -> Option<(SelectablePane, jefe::selection::PaneGeometry)> {
    let layout = screen_layout_for(state, cols, rows);
    pane_at(col, row, state.screen_mode, terminal_input_enabled, &layout)
}

/// HelpModal title rows (title text + blank): not affected by scroll offset.
const HELP_TITLE_ROWS: usize = 2;

fn effective_scroll_for_detail(
    pane: SelectablePane,
    row: u16,
    geometry: &jefe::selection::PaneGeometry,
    scroll_offset: usize,
) -> usize {
    use jefe::layout::DETAIL_HEADER_ROWS;
    match pane {
        SelectablePane::IssueDetail | SelectablePane::PrDetail | SelectablePane::ActionsDetail => {
            let content_row = usize::from(row.saturating_sub(geometry.content_origin_row));
            if content_row < DETAIL_HEADER_ROWS {
                0
            } else {
                scroll_offset
            }
        }
        SelectablePane::HelpModal => {
            let content_row = usize::from(row.saturating_sub(geometry.content_origin_row));
            if content_row < HELP_TITLE_ROWS {
                0
            } else {
                scroll_offset
            }
        }
        _ => scroll_offset,
    }
}

fn scroll_offset_for_pane(state: &AppState, pane: SelectablePane) -> usize {
    match pane {
        SelectablePane::IssueDetail => state.issues_state.detail_scroll_offset,
        SelectablePane::PrDetail => state.prs_state.detail_scroll_offset,
        SelectablePane::ActionsDetail => state.actions_state.detail_scroll_offset,
        SelectablePane::HelpModal => state.help_scroll_offset,
        // Issue #198: terminal history offset is bottom-relative, but the
        // selection layer expects a top-relative "lines hidden above viewport".
        // Convert via the shared single source of truth so the selection
        // coordinate system agrees with the viewport projection.
        SelectablePane::TerminalView => jefe::state::scrollback_ops::terminal_content_start_line(
            state.terminal_history_offset,
            state.terminal_total_lines,
            state.terminal_viewport_rows,
        ),
        _ => 0,
    }
}

/// Direction of a single mousewheel tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WheelDirection {
    Up,
    Down,
}

#[must_use]
fn next_wheel_scroll_offset(current: usize, max: usize, direction: WheelDirection) -> usize {
    let clamped = current.min(max);
    match direction {
        WheelDirection::Up => clamped.saturating_sub(1),
        WheelDirection::Down => (clamped + 1).min(max),
    }
}

fn max_scroll_offset_for_pane(state: &AppState, pane: SelectablePane) -> usize {
    match pane {
        SelectablePane::IssueDetail => state.issues_state.max_detail_scroll_offset(),
        SelectablePane::PrDetail => state.pr_detail_max_scroll_offset(),
        SelectablePane::ActionsDetail => state.actions_max_detail_scroll_offset(),
        _ => 0,
    }
}

fn scroll_detail_pane(
    app_state: &mut HookState<AppState>,
    col: u16,
    row: u16,
    direction: WheelDirection,
) {
    let (cols, rows) = terminal_size();
    let pane = {
        let state = app_state.read();
        let Some((pane, _geometry)) = resolve_pane(&state, col, row, cols, rows, false) else {
            return;
        };
        pane
    };
    if !matches!(
        pane,
        SelectablePane::IssueDetail | SelectablePane::PrDetail | SelectablePane::ActionsDetail
    ) {
        return;
    }
    {
        let mut state = app_state.write();
        refresh_detail_viewport_rows(&mut state, pane, cols, rows);
    }
    let (current, max) = {
        let state = app_state.read();
        let current = scroll_offset_for_pane(&state, pane);
        let max = max_scroll_offset_for_pane(&state, pane);
        drop(state);
        (current, max)
    };
    let next = next_wheel_scroll_offset(current, max, direction);
    let mut state = app_state.write();
    match pane {
        SelectablePane::IssueDetail => state.issues_state.detail_scroll_offset = next,
        SelectablePane::PrDetail => state.prs_state.detail_scroll_offset = next,
        SelectablePane::ActionsDetail => state.actions_state.detail_scroll_offset = next,
        _ => {}
    }
}

#[cfg(test)]
#[path = "mouse_routing_tests.rs"]
mod mouse_routing_tests;

#[cfg(test)]
#[path = "mouse_routing_wheel_intercept_tests.rs"]
mod mouse_routing_wheel_intercept_tests;
