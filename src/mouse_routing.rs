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
        // Suppress PTY forwarding when an overlay (modal/form/chooser) is
        // active so mouse selection targets the top-most overlay instead of
        // the terminal underneath (issue #178 z-order fix).
        state.terminal_focused
            && state.pane_focus == PaneFocus::Terminal
            && active_overlay_for(&state) == jefe::selection::OverlayPane::None
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
    let overlay = active_overlay_for(state);
    ScreenLayout::new(cols, rows, state.screen_mode, error_visible, filter_open)
        .with_overlay(overlay)
}

/// Determine which overlay (modal/form/chooser) is currently active, if any.
///
/// Full-screen modals take priority over positioned choosers. Returns
/// [`OverlayPane::None`] when no overlay is active so normal pane geometry
/// applies.
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
        | jefe::state::ModalState::ConfirmIssueDirtyCopy { .. } => {
            return OverlayPane::ConfirmModal;
        }
        // Explicit match (not wildcard) so new ModalState variants force a
        // conscious overlay-routing decision here (issue #178 z-order).
        jefe::state::ModalState::None | jefe::state::ModalState::Search { .. } => {}
    }
    // Positioned overlays (choosers) — checked only when no full-screen modal.
    if state.issues_state.agent_chooser.is_some() || state.prs_state.agent_chooser.is_some() {
        return OverlayPane::AgentChooser;
    }
    if state.prs_state.merge_chooser.is_some() {
        return OverlayPane::MergeChooser;
    }
    OverlayPane::None
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

/// HelpModal title rows (title text + blank): not affected by scroll offset,
/// mirroring the renderer's title Box(height: 2) above ScrollableText.
const HELP_TITLE_ROWS: usize = 2;

/// For detail panes, headers occupy content lines `0..DETAIL_HEADER_ROWS` and
/// are not affected by scroll offset. Scrollable content starts at line
/// `DETAIL_HEADER_ROWS`. Return 0 when the click is in the header area,
/// otherwise the real scroll offset.
///
/// The HelpModal also has fixed header rows (title + blank = 2 rows) that are
/// not affected by the scroll offset; the scrollable help content starts at
/// line 2.
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
        SelectablePane::HelpModal => {
            let content_row = usize::from(row.saturating_sub(geometry.content_origin_row));
            // Title Box is height 2 (title text + blank) — not scrolled.
            if content_row < HELP_TITLE_ROWS {
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
        SelectablePane::HelpModal => state.help_scroll_offset,
        _ => 0,
    }
}

/// Refresh the cached `detail_viewport_rows` for `pane` from the current
/// terminal size and conditional bands.
///
/// Mirrors `update_detail_viewport_rows` / `update_pr_detail_viewport_rows` in
/// the keyboard dispatch path so the scroll-offset clamp bound
/// (`max_detail_scroll_offset` / `pr_detail_max_scroll_offset`) agrees with the
/// viewport the renderer actually uses. Without this, a stale cache (e.g. left
/// over from before entering the mode, since it is only refreshed on keyboard
/// scroll-down) lets the stored offset drift past the renderer's clamp bound.
fn refresh_detail_viewport_rows(state: &mut AppState, pane: SelectablePane, term_rows: u16) {
    let term_rows = usize::from(term_rows);
    match pane {
        SelectablePane::IssueDetail => {
            state.issues_state.detail_viewport_rows = jefe::layout::issues_detail_viewport_rows(
                term_rows,
                state.issues_state.error.is_some(),
                state.issues_state.filter_ui.controls_open,
            );
        }
        SelectablePane::PrDetail => {
            state.prs_state.detail_viewport_rows = jefe::layout::prs_detail_viewport_rows(
                term_rows,
                state.prs_state.error.is_some(),
                state.prs_state.filter_ui.controls_open,
            );
        }
        _ => {}
    }
}

/// Direction of a single mousewheel tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WheelDirection {
    Up,
    Down,
}

/// Next detail scroll offset after one mousewheel tick, clamped to `[0, max]`.
///
/// Pure and side-effect-free so the clamp invariant is unit-testable without
/// the iocraft runtime: scrolling up never drops below 0 and scrolling down
/// never exceeds the pane's maximum offset. `max` is the detail pane's
/// `max_detail_scroll_offset()` (0 when there is no content to scroll).
///
/// `current` is clamped to `max` *before* the tick is applied, so a stale
/// offset that drifted past `max` (e.g. the cached viewport shrank) snaps back
/// into the valid range on the very next tick instead of stranding the view in
/// a phantom region the renderer clamps but the stored value does not.
#[must_use]
fn next_wheel_scroll_offset(current: usize, max: usize, direction: WheelDirection) -> usize {
    let clamped = current.min(max);
    match direction {
        WheelDirection::Up => clamped.saturating_sub(1),
        WheelDirection::Down => (clamped + 1).min(max),
    }
}

/// Maximum scroll offset for a detail pane from app state.
///
/// Returns 0 for non-scrollable panes (they are never advanced by the wheel).
fn max_scroll_offset_for_pane(state: &AppState, pane: SelectablePane) -> usize {
    match pane {
        SelectablePane::IssueDetail => state.issues_state.max_detail_scroll_offset(),
        SelectablePane::PrDetail => state.pr_detail_max_scroll_offset(),
        _ => 0,
    }
}

/// Scroll the detail pane under `(col, row)` by one wheel tick.
///
/// Resolves the pane under the cursor via [`resolve_pane`] and, when it is a
/// scrollable detail pane (`IssueDetail` / `PrDetail`), advances its scroll
/// offset by one line in the given direction, clamped to the pane's valid
/// bounds via [`next_wheel_scroll_offset`]. The cached viewport row count is
/// refreshed first ([`refresh_detail_viewport_rows`]) so the clamp bound
/// matches the renderer. Non-detail panes are ignored — mousewheel scrolling
/// in list/terminal panes is out of scope (issue #148).
fn scroll_detail_pane(
    app_state: &mut HookState<AppState>,
    col: u16,
    row: u16,
    direction: WheelDirection,
) {
    let (cols, rows) = terminal_size();
    // Resolve the pane in one short read guard, then read current/max offsets
    // in another, so each read guard drops immediately (the guard has a
    // significant Drop and clippy::significant_drop_tightening requires it not
    // be held across unrelated statements).
    let pane = {
        let state = app_state.read();
        let Some((pane, _geometry)) = resolve_pane(&state, col, row, cols, rows) else {
            return;
        };
        pane
    };
    if !matches!(pane, SelectablePane::IssueDetail | SelectablePane::PrDetail) {
        return;
    }
    // Refresh the cached detail-viewport row count from the current terminal
    // layout before computing the clamp bound. The renderer windows content
    // with a *fresh* viewport (derived from the actual layout), but
    // `max_detail_scroll_offset` / `pr_detail_max_scroll_offset` read the cached
    // `detail_viewport_rows` field. The keyboard dispatch path refreshes that
    // cache (`update_*_detail_viewport_rows`) before every scroll; the mouse
    // path must too, otherwise a stale (smaller) cache lets the offset run past
    // the renderer's clamp bound and the wheel appears to overscroll / stick.
    {
        let mut state = app_state.write();
        refresh_detail_viewport_rows(&mut state, pane, rows);
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
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{WheelDirection, next_wheel_scroll_offset};

    #[test]
    fn scroll_down_advances_within_bounds() {
        assert_eq!(
            next_wheel_scroll_offset(2, 5, WheelDirection::Down),
            3,
            "scrolling down from 2 with max 5 should advance to 3"
        );
    }

    #[test]
    fn scroll_up_decrements_within_bounds() {
        assert_eq!(
            next_wheel_scroll_offset(2, 5, WheelDirection::Up),
            1,
            "scrolling up from 2 with max 5 should decrement to 1"
        );
    }

    #[test]
    fn scroll_down_clamps_at_max_offset() {
        assert_eq!(
            next_wheel_scroll_offset(5, 5, WheelDirection::Down),
            5,
            "scrolling down at max offset must not overscroll"
        );
    }

    #[test]
    fn scroll_up_clamps_at_zero() {
        assert_eq!(
            next_wheel_scroll_offset(0, 5, WheelDirection::Up),
            0,
            "scrolling up at offset 0 must not underscroll"
        );
    }

    #[test]
    fn scroll_down_with_zero_max_stays_zero() {
        assert_eq!(
            next_wheel_scroll_offset(0, 0, WheelDirection::Down),
            0,
            "empty detail pane (max 0) cannot scroll down"
        );
    }

    #[test]
    fn scroll_up_with_zero_max_stays_zero() {
        assert_eq!(
            next_wheel_scroll_offset(0, 0, WheelDirection::Up),
            0,
            "empty detail pane (max 0) cannot scroll up"
        );
    }

    #[test]
    fn scroll_down_in_middle_of_large_content() {
        assert_eq!(
            next_wheel_scroll_offset(10, 100, WheelDirection::Down),
            11,
            "scrolling down from 10 within a large pane should advance to 11"
        );
    }

    // ── stale / inflated offset recovery ───────────────────────────────────
    //
    // The stored detail scroll offset can lag behind the renderer's clamp
    // bound when the cached `detail_viewport_rows` is smaller than the
    // renderer's fresh viewport (the keyboard dispatch path refreshes that
    // cache before scrolling; the mouse path now does too). If the offset is
    // ever inflated past `max`, a single wheel tick must snap it back into the
    // valid range instead of leaving it stranded in a phantom region where the
    // renderer clamps the display but the stored value keeps climbing.

    #[test]
    fn scroll_up_from_inflated_offset_snaps_below_max() {
        assert_eq!(
            next_wheel_scroll_offset(8, 5, WheelDirection::Up),
            4,
            "scrolling up from an inflated offset (8) with max 5 must snap to 4, not walk back one-at-a-time through the phantom region"
        );
    }

    #[test]
    fn scroll_down_from_inflated_offset_snaps_to_max() {
        assert_eq!(
            next_wheel_scroll_offset(8, 5, WheelDirection::Down),
            5,
            "scrolling down from an inflated offset (8) with max 5 must snap to 5"
        );
    }

    #[test]
    fn scroll_up_from_inflated_offset_near_zero_snaps_to_zero() {
        assert_eq!(
            next_wheel_scroll_offset(8, 1, WheelDirection::Up),
            0,
            "scrolling up from an inflated offset (8) with max 1 must snap to 0"
        );
    }
}
