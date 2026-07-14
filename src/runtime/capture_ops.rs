//! Pane capture and scrollback history operations extracted from
//! `manager.rs` to keep that file under the source-file size hard limit
//! (issue #269).
//!
//! These free functions implement the bodies of `capture_session_output` and
//! `capture_history` for [`super::manager::TmuxRuntimeManager`]. The manager
//! delegates to them, passing its internal fields by reference.

use std::collections::HashMap;

use iocraft::Color;
use tracing::debug;

use super::commands;
use super::history_cache::{HistoryCache, strip_trailing_rows};
use super::session::{RuntimeSession, TerminalCell, TerminalCellStyle, TerminalSnapshot};
use crate::domain::AgentId;

/// Maximum number of scrollback history lines retained for an embedded
/// terminal session (issue #198). Matches the `terminal-scrollback.json`
/// test scenario's `history_limit` (2000), intentionally smaller than the
/// harness default (10000) to bound render/capture cost.
const HISTORY_LINE_CAP: usize = 2000;

/// Build a [`TerminalSnapshot`] from captured pane lines for a local session.
///
/// Remote sessions return `None`. This is the body of
/// `RuntimeManager::capture_session_output` for `TmuxRuntimeManager`.
pub(super) fn capture_session_output(
    sessions: &HashMap<AgentId, RuntimeSession>,
    agent_id: &AgentId,
) -> Option<TerminalSnapshot> {
    let session = sessions.get(agent_id)?;
    if session.launch_signature.remote.enabled {
        return None;
    }

    let lines = commands::capture_pane_lines(&session.session_name)?;

    let rows = lines.len();
    let cols = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    if rows == 0 || cols == 0 {
        return Some(TerminalSnapshot::default());
    }

    let default_style = TerminalCellStyle {
        fg: Color::White,
        bg: Color::Black,
        bold: false,
        dim: false,
        underline: false,
    };

    let mut snapshot = TerminalSnapshot::blank(rows, cols, default_style);
    // `capture_pane_lines` does not preserve soft-wrap metadata, so all
    // rows are treated as hard line breaks (wraps stays all-false, which
    // is the default from `blank` — issue #197).
    snapshot.wraps = vec![false; rows];
    for (r, line) in lines.iter().enumerate() {
        for (c, ch) in line.chars().enumerate() {
            snapshot.cells[r][c] = TerminalCell {
                ch,
                style: default_style,
                wide_spacer: false,
            };
        }
    }

    Some(snapshot)
}

/// Capture scrollback history for the attached session (issue #198).
///
/// This is the body of `RuntimeManager::capture_history` for
/// `TmuxRuntimeManager`. The caller provides the manager's fields so this
/// function does not call back into the manager.
pub(super) fn capture_history(
    attached_agent_id: Option<&AgentId>,
    sessions: &HashMap<AgentId, RuntimeSession>,
    history_cache: &mut HistoryCache,
    output_generation: u64,
    is_currently_dirty: bool,
    live_snapshot_rows: usize,
) -> Option<Vec<String>> {
    let agent_id = attached_agent_id.cloned()?;
    let session = sessions.get(&agent_id)?;

    // Remote sessions do not support local capture-pane history.
    if session.launch_signature.remote.enabled {
        return None;
    }
    let session_name = session.session_name.clone();

    // Cache hit: same agent + generation + not dirty → reuse (fix #2/#10).
    // The generation counter increments on take_dirty(). Also treat a
    // currently-dirty viewer as a cache miss so input-driven refresh does
    // not serve stale lines before take_dirty() bumps the generation.
    if !is_currently_dirty && let Some(cached) = history_cache.get(&agent_id, output_generation) {
        return Some(cached.clone());
    }

    // Cache miss / dirty: re-capture. On transient failure, return prior
    // cache so a momentary tmux hiccup doesn't wipe retained history.
    let Some(raw_lines) = commands::capture_pane_history(&session_name, HISTORY_LINE_CAP) else {
        if let Some(prior) = history_cache.get_fallback(&agent_id) {
            debug!(session_name = %session_name, "capture-pane failed; retaining prior cache");
            return Some(prior.clone());
        }
        return None;
    };

    // Strip the visible pane rows (live snapshot already has them).
    let lines = strip_trailing_rows(raw_lines, live_snapshot_rows);

    // Do NOT strip trailing blank lines — they may be real blank output,
    // not tmux padding. Cache the result (including an empty capture) so
    // we don't shell out every frame. Return `Some(lines)` consistently so
    // the current frame and subsequent cache-hit frames agree (an empty
    // capture returns `Some(vec![])`, not `None` — callers normalize via
    // `map_or(0, Vec::len)`, and `None` is reserved for "no session /
    // capture not applicable").
    history_cache.store(&agent_id, output_generation, Some(lines.clone()));

    Some(lines)
}
