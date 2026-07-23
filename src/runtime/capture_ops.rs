//! Capture operations extracted from `manager.rs` to keep that file under
//! the source-file size hard limit (issue #301).
//!
//! These functions delegate to `TmuxRuntimeManager` fields via public(crate)
//! accessors. They implement the `RuntimeManager` trait methods
//! `capture_session_output` and `capture_history`.

use super::commands;
use super::manager::history_cache::strip_trailing_rows;
use super::manager::{HISTORY_LINE_CAP, TmuxRuntimeManager};
use super::session::{TerminalCell, TerminalCellStyle, TerminalSnapshot};
use crate::domain::AgentId;
use crate::runtime::RuntimeManager;
use tracing::debug;

/// Capture pane output for a known session (used for dead-pane crash text).
pub fn capture_session_output(
    mgr: &TmuxRuntimeManager,
    agent_id: &AgentId,
) -> Option<TerminalSnapshot> {
    let session = mgr.sessions.get(agent_id)?;
    if session.launch_signature.remote.enabled {
        return None;
    }

    let lines = commands::capture_pane_lines(&session.session_name)?;

    Some(snapshot_from_lines(&lines))
}

/// Project plain pane lines into an unstyled terminal snapshot.
#[must_use]
pub fn snapshot_from_lines(lines: &[String]) -> TerminalSnapshot {
    let rows = lines.len();
    let cols = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    if rows == 0 || cols == 0 {
        return TerminalSnapshot::default();
    }

    let default_style = TerminalCellStyle {
        fg: iocraft::Color::White,
        bg: iocraft::Color::Black,
        bold: false,
        dim: false,
        underline: false,
    };
    let mut snapshot = TerminalSnapshot::blank(rows, cols, default_style);
    for (r, line) in lines.iter().enumerate() {
        for (c, ch) in line.chars().enumerate() {
            snapshot.cells[r][c] = TerminalCell {
                ch,
                style: default_style,
                wide_spacer: false,
            };
        }
    }
    snapshot
}

/// Retrieve retained scrollback history lines for the currently attached
/// session (issue #198).
///
/// Returns `Option<Vec<String>>` — plain-text rows (no styles) from the
/// tmux pane's scrollback buffer. Caches so it does not shell out on every
/// render frame: re-capture only when `take_dirty()` returns true (new PTY
/// data) or the attached session changes.
pub fn capture_history(mgr: &mut TmuxRuntimeManager) -> Option<Vec<String>> {
    let agent_id = mgr.attached_agent_id.as_ref()?;
    let session_name = {
        let session = mgr.sessions.get(agent_id)?;
        // Clone is required: session_name outlives the sessions borrow
        // because subsequent calls (output_generation, snapshot,
        // history_cache.store) borrow mgr through other fields.
        let name = session.session_name.clone();
        if session.launch_signature.remote.enabled {
            return None;
        }
        name
    };

    // Cache hit: same agent + generation + not dirty → reuse (fix #2/#10).
    // The generation counter increments on take_dirty(). Also treat a
    // currently-dirty viewer as a cache miss so input-driven refresh does
    // not serve stale lines before take_dirty() bumps the generation.
    let generation = mgr.output_generation();
    let is_currently_dirty = mgr.is_dirty();
    if !is_currently_dirty && let Some(cached) = mgr.history_cache.get(agent_id, generation) {
        return Some(cached.clone());
    }

    // Cache miss / dirty: re-capture. On transient failure, return prior
    // cache so a momentary tmux hiccup doesn't wipe retained history.
    let Some(raw_lines) = commands::capture_pane_history(&session_name, HISTORY_LINE_CAP) else {
        if let Some(prior) = mgr.history_cache.get_fallback(agent_id) {
            debug!(session_name = %session_name, "capture-pane failed; retaining prior cache");
            return Some(prior.clone());
        }
        return None;
    };

    // Strip the visible pane rows (live snapshot already has them).
    let live_rows = mgr.snapshot().map_or(0, |s| s.rows);
    let lines = strip_trailing_rows(raw_lines, live_rows);

    // Do NOT strip trailing blank lines — they may be real blank output,
    // not tmux padding. Cache the result (including an empty capture) so
    // we don't shell out every frame. Return `Some(lines)` consistently so
    // the current frame and subsequent cache-hit frames agree (an empty
    // capture returns `Some(vec![])`, not `None` — callers normalize via
    // `map_or(0, Vec::len)`, and `None` is reserved for "no session /
    // capture not applicable").
    mgr.history_cache
        .store(agent_id, generation, Some(lines.clone()));

    Some(lines)
}
