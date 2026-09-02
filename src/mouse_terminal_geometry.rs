//! Terminal-pane mouse geometry and scrollback refresh.

use crate::app_shell::{CtxArc, HookState};
use jefe::layout::compute_shell_overlay_pty_layout;
use jefe::state::AppState;
use jefe::workbench::Rect;

pub fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((120, 40))
}

/// The on-screen rectangle PTY cells occupy for the active screen.
///
/// Normal routing reads the committed resolved frame so hit-testing and
/// replay translation match the rectangles the renderer drew (issue #706).
/// The shell overlay keeps its legacy rectangle until overlay layers are
/// declared in the descriptor (issue #706 cutover step 2). `None` means no
/// PTY panel is visible: there is nothing to hit-test or forward into.
#[must_use]
pub fn active_pty_content_rect(state: &AppState, overlay_active: bool) -> Option<Rect> {
    if overlay_active {
        let (cols, rows) = terminal_size();
        let layout = compute_shell_overlay_pty_layout(cols, rows);
        return Some(Rect::new(
            layout.pane_col0,
            layout.pane_row0,
            layout.pty_cols,
            layout.pty_rows,
        ));
    }
    jefe::screen_layout::committed_pty_content_rect(state)
}

/// The viewport height of the active screen's PTY panel, in rows.
#[must_use]
pub fn active_pty_viewport_rows(state: &AppState, overlay_active: bool) -> Option<u16> {
    active_pty_content_rect(state, overlay_active).map(|rect| rect.height)
}

pub fn refresh_terminal_scroll_geometry_from_ctx(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    overlay_active: bool,
) {
    let viewport_rows = {
        let state = app_state.read();
        active_pty_viewport_rows(&state, overlay_active)
    };
    let Some(viewport_rows) = viewport_rows else {
        // No visible PTY panel in the committed frame: preserve existing
        // geometry instead of zeroing it (which would clear the scroll
        // offset).
        return;
    };
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
    let viewport_rows = usize::from(viewport_rows);
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
