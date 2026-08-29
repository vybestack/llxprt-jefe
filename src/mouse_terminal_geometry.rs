//! Terminal-pane mouse geometry and scrollback refresh.

use crate::app_shell::{CtxArc, HookState};
use jefe::layout::{PtyLayout, compute_pty_layout, compute_shell_overlay_pty_layout};
use jefe::state::AppState;

pub fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((120, 40))
}

pub fn active_pty_layout(cols: u16, rows: u16, overlay_active: bool) -> PtyLayout {
    if overlay_active {
        compute_shell_overlay_pty_layout(cols, rows)
    } else {
        compute_pty_layout(cols, rows)
    }
}

pub fn refresh_terminal_scroll_geometry_from_ctx(
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
