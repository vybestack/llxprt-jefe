//! Mouse event routing: selection start/update/finalize and PTY forwarding.
//!
//! Extracted from [`crate::app_shell`] to keep that file under the 1000-line
//! size limit. All functions here operate on the iocraft hook state and the
//! shared [`crate::app_shell::CtxArc`], translating fullscreen mouse events
//! into either PTY input (when the terminal pane is focused) or text-selection
//! state transitions.

use crate::app_shell::{CtxArc, HookState, capture_terminal_snapshot};
use crate::pty_encoding::mouse_event_to_bytes;
use jefe::clipboard;
use jefe::layout::compute_pty_layout;
use jefe::runtime::RuntimeManager;
use jefe::selection::{
    ScreenLayout, SelectablePane, SelectionPoint, TextSelection, pane_at, pane_content_lines,
    point_to_content_coords, selection_text,
};
use jefe::state::{AppState, PaneFocus};

/// Terminal size fallback for the default 120x40 geometry.
fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((120, 40))
}

/// Clear any active mouse selection.
///
/// Called on every non-mouse terminal event (key, paste, resize) so a
/// selection doesn't linger after the user moves on to keyboard interaction.
pub fn clear_selection(app_state: &mut HookState<AppState>) {
    let needs_clear = app_state.read().selection.is_some();
    if needs_clear {
        app_state.write().selection = None;
    }
}

/// Route a fullscreen mouse event to PTY forwarding or app-level selection.
pub fn handle_fullscreen_mouse(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    mouse_event: iocraft::FullscreenMouseEvent,
) {
    use crossterm::event::{MouseButton, MouseEventKind};

    // Shift bypasses app selection and PTY forwarding: let the host terminal
    // emulator handle native selection/copy (mirrors mouse_event_to_bytes).
    if mouse_event.modifiers.contains(iocraft::KeyModifiers::SHIFT) {
        return;
    }

    let terminal_input_enabled = {
        let state = app_state.read();
        state.terminal_focused && state.pane_focus == PaneFocus::Terminal
    };

    // When the terminal is focused, mouse events within the terminal pane go to
    // the managed PTY (current behavior). Everything else is app selection.
    if terminal_input_enabled && forward_to_pty_if_in_terminal(ctx, &mouse_event) {
        return;
    }

    // Route the event to the app-level selection handler.
    match mouse_event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            begin_selection(app_state, mouse_event.column, mouse_event.row);
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            update_selection(app_state, mouse_event.column, mouse_event.row);
        }
        MouseEventKind::Up(MouseButton::Left) => {
            finalize_and_copy_selection(ctx, app_state);
        }
        _ => {}
    }
}

/// Forward `mouse_event` to the managed PTY if the terminal is focused, mouse
/// reporting is active, and the event lands inside the terminal pane bounds.
///
/// Returns `true` when the event was consumed (forwarded or filtered as
/// out-of-bounds while focused), `false` when the caller should handle it as
/// an app-level selection.
fn forward_to_pty_if_in_terminal(
    ctx: Option<&CtxArc>,
    mouse_event: &iocraft::FullscreenMouseEvent,
) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return false;
    };
    if !ctx_guard.runtime.mouse_reporting_active() {
        return false;
    }

    let (cols, rows) = terminal_size();
    let layout = compute_pty_layout(cols, rows);

    let row_end = layout
        .pane_row0
        .saturating_add(layout.pty_rows.saturating_sub(1));
    let col_end = layout
        .pane_col0
        .saturating_add(layout.pty_cols.saturating_sub(1));

    let screen_row0 = mouse_event.row;
    let screen_col0 = mouse_event.column;

    let in_terminal_bounds = screen_col0 >= layout.pane_col0
        && screen_col0 <= col_end
        && screen_row0 >= layout.pane_row0
        && screen_row0 <= row_end;

    if !in_terminal_bounds {
        // Focused but outside the terminal pane: treat as app selection target.
        return false;
    }

    let local_row = screen_row0.saturating_sub(layout.pane_row0);
    let local_col = screen_col0.saturating_sub(layout.pane_col0);

    let mut local_event =
        iocraft::FullscreenMouseEvent::new(mouse_event.kind, local_col, local_row);
    local_event.modifiers = mouse_event.modifiers;

    if let Some(bytes) = mouse_event_to_bytes(&local_event) {
        let _ = ctx_guard.runtime.write_input(&bytes);
    }
    true
}

/// Build the screen-layout descriptor from the current app state + terminal size.
fn screen_layout_for(state: &AppState, cols: u16, rows: u16) -> ScreenLayout {
    let error_visible = state.error_message.is_some()
        || state.issues_state.error.is_some()
        || state.prs_state.error.is_some();
    let filter_open =
        state.issues_state.filter_ui.controls_open || state.prs_state.filter_ui.controls_open;
    ScreenLayout::new(cols, rows, state.screen_mode, error_visible, filter_open)
}

/// Resolve which pane + geometry a screen coordinate maps to, given app state.
fn resolve_pane(
    state: &AppState,
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
) -> Option<(SelectablePane, jefe::selection::PaneGeometry)> {
    let layout = screen_layout_for(state, cols, rows);
    let terminal_input_enabled = state.terminal_focused && state.pane_focus == PaneFocus::Terminal;
    pane_at(col, row, state.screen_mode, terminal_input_enabled, &layout)
}

/// Begin a new text selection at `(col, row)`.
fn begin_selection(app_state: &mut HookState<AppState>, col: u16, row: u16) {
    let (cols, rows) = terminal_size();
    let point = resolve_selection_point(app_state, col, row, cols, rows);
    match point {
        Some(p) => app_state.write().selection = Some(TextSelection::collapsed(p)),
        None => app_state.write().selection = None,
    }
}

/// Resolve the screen `(col, row)` to a content selection point under the pane
/// it lands in, applying the detail-header scroll suppression. Returns `None`
/// when the coordinate is not over any selectable pane.
fn resolve_selection_point(
    app_state: &HookState<AppState>,
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
) -> Option<SelectionPoint> {
    // Resolve the pane + geometry in one short read, then read the scroll
    // offset in another, so each read guard drops immediately (the guard has a
    // significant Drop and clippy::significant_drop_tightening requires it not
    // be held across unrelated statements).
    let (pane, geometry) = {
        let state = app_state.read();
        resolve_pane(&state, col, row, cols, rows)?
    };
    let raw_scroll = {
        let state = app_state.read();
        scroll_offset_for_pane(&state, pane)
    };
    let scroll_offset = effective_scroll_for_detail(pane, row, &geometry, raw_scroll);
    let (line, c) = point_to_content_coords(col, row, scroll_offset, &geometry);
    Some(SelectionPoint::new(pane, line, c))
}

/// Update the focus (drag) point of the active selection.
fn update_selection(app_state: &mut HookState<AppState>, col: u16, row: u16) {
    let (cols, rows) = terminal_size();
    let (anchor, pane, scroll_offset) = {
        let state = app_state.read();
        let Some(current) = state.selection else {
            return;
        };
        let pane = current.pane();
        let raw_scroll = scroll_offset_for_pane(&state, pane);
        drop(state);
        (current.anchor, pane, raw_scroll)
    };
    let focus_point = {
        let state = app_state.read();
        // Clamp drag to the anchor pane: cross-pane drag would mix coordinate
        // spaces. If the cursor left the anchor pane, keep the last valid focus.
        let Some((resolved_pane, geometry)) = resolve_pane(&state, col, row, cols, rows) else {
            return;
        };
        if resolved_pane != pane {
            return;
        }
        let scroll_offset = effective_scroll_for_detail(pane, row, &geometry, scroll_offset);
        let (line, c) = point_to_content_coords(col, row, scroll_offset, &geometry);
        SelectionPoint::new(pane, line, c)
    };
    app_state.write().selection = Some(TextSelection {
        anchor,
        focus: focus_point,
    });
}

/// Finalize the active selection and copy its text to the clipboard via OSC 52.
fn finalize_and_copy_selection(ctx: Option<&CtxArc>, app_state: &HookState<AppState>) {
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
        let (cols, rows) = terminal_size();
        let content = pane_content_lines(selection.pane(), &state, snapshot.as_ref(), cols, rows);
        drop(state);
        selection_text(&selection, &content.lines)
    };
    if !text.is_empty()
        && let Err(err) = clipboard::write_osc52(&text)
    {
        tracing::warn!(error = %err, "OSC 52 clipboard write failed");
    }
}

/// For detail panes, headers occupy content lines `0..DETAIL_HEADER_ROWS` and
/// are not affected by scroll offset. Scrollable content starts at line
/// `DETAIL_HEADER_ROWS`. Return 0 when the click is in the header area,
/// otherwise the real scroll offset.
fn effective_scroll_for_detail(
    pane: SelectablePane,
    row: u16,
    geometry: &jefe::selection::PaneGeometry,
    scroll_offset: usize,
) -> usize {
    use jefe::layout::DETAIL_HEADER_ROWS;
    match pane {
        SelectablePane::IssueDetail | SelectablePane::PrDetail => {
            let content_row = usize::from(row.saturating_sub(geometry.content_origin_row));
            if content_row < DETAIL_HEADER_ROWS {
                0
            } else {
                scroll_offset
            }
        }
        _ => scroll_offset,
    }
}

/// Scroll offset for a specific pane from app state.
fn scroll_offset_for_pane(state: &AppState, pane: SelectablePane) -> usize {
    match pane {
        SelectablePane::IssueDetail => state.issues_state.detail_scroll_offset,
        SelectablePane::PrDetail => state.prs_state.detail_scroll_offset,
        _ => 0,
    }
}
