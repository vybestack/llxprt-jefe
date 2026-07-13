//! Runtime session identity model.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 01-06

use crate::domain::{AgentId, LaunchSignature, ProcessIdentity};

/// Runtime session binding for an agent.
///
/// Represents the stable identity of a tmux session bound to an agent.
/// The session may be attached (viewer connected) or detached.
#[derive(Debug, Clone)]
pub struct RuntimeSession {
    /// The agent this session belongs to.
    pub agent_id: AgentId,
    /// The tmux session name (e.g., "jefe-{agent_id}").
    pub session_name: String,
    /// Launch configuration for spawn/relaunch.
    pub launch_signature: LaunchSignature,
    /// Whether a viewer is currently attached to this session.
    pub attached: bool,
    /// OS PID of the worker process (`llxprt`) backing this session, when
    /// known. Captured via tmux `list-panes` (the pane PID *is* the worker
    /// because the worker runs as the pane's direct command). Used as a
    /// liveness fallback when the tmux session is gone but the worker process
    /// is still alive.
    pub pid: Option<u32>,
    /// Stable process-instance identity used to reject PID reuse.
    pub process_identity: Option<ProcessIdentity>,
}

impl RuntimeSession {
    /// Create a new runtime session binding.
    #[must_use]
    pub fn new(agent_id: AgentId, session_name: String, launch_signature: LaunchSignature) -> Self {
        Self {
            agent_id,
            session_name,
            launch_signature,
            attached: false,
            pid: None,
            process_identity: None,
        }
    }

    /// Generate a session name from an agent ID.
    ///
    /// tmux session names may only contain ASCII alphanumerics, hyphens, and
    /// underscores. Any other character in the agent id (spaces, slashes, shell
    /// metacharacters, non-ASCII code points) is replaced with `_` so the
    /// resulting name is always safe to use as a tmux target.
    ///
    /// Because distinct characters collapse to `_`, two different raw ids could
    /// in principle map to the same session name. This is acceptable in
    /// practice: agent ids are unique nanosecond timestamps composed solely of
    /// ASCII digits, which survive sanitization unchanged and therefore never
    /// collide.
    #[must_use]
    pub fn session_name_for(agent_id: &AgentId) -> String {
        let sanitized: String = agent_id
            .0
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("jefe-{sanitized}")
    }
}

/// Style information for one terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCellStyle {
    /// Foreground color.
    pub fg: iocraft::Color,
    /// Background color.
    pub bg: iocraft::Color,
    /// Bold weight.
    pub bold: bool,
    /// Dim/faint intensity (ANSI SGR 2). Tracked separately from the color so
    /// a default-colored (transparent) cell can still render dimmed — the
    /// foreground stays `Color::Reset` and the renderer applies `Weight::Light`.
    pub dim: bool,
    /// Underline decoration.
    pub underline: bool,
}

/// One renderable terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCell {
    /// Display character.
    pub ch: char,
    /// Cell style.
    pub style: TerminalCellStyle,
    /// Whether this cell is the trailing spacer of a wide (width-2) glyph.
    ///
    /// When `true`, the actual glyph lives in the preceding cell's `ch` and
    /// this cell carries a blank `' '` for rendering. Selection extraction
    /// skips wide-spacer cells so copying a selection across a wide glyph
    /// yields just the glyph, not a spurious trailing space (issue #197).
    pub wide_spacer: bool,
}

/// Terminal snapshot data for rendering.
///
/// Represents a frozen, styled view of terminal state at a point in time.
#[derive(Debug, Clone, Default)]
pub struct TerminalSnapshot {
    /// Number of visible rows.
    pub rows: usize,
    /// Number of visible columns.
    pub cols: usize,
    /// Row-major terminal cells.
    pub cells: Vec<Vec<TerminalCell>>,
    /// Per-row soft-wrap metadata for selection text extraction.
    ///
    /// `wraps[row] == true` means row `row` soft-wraps into row `row+1` (the
    /// logical line continues on the next row without a newline). An empty
    /// `Vec` (the default) means no rows wrap — every row ends a logical line.
    /// This lets terminal selection extraction join soft-wrapped rows without
    /// inserting a spurious newline while still inserting one at real line
    /// breaks (issue #197).
    pub wraps: Vec<bool>,
}

impl TerminalSnapshot {
    /// Build an empty snapshot pre-filled with a base style.
    #[must_use]
    pub fn blank(rows: usize, cols: usize, style: TerminalCellStyle) -> Self {
        let cell = TerminalCell {
            ch: ' ',
            style,
            wide_spacer: false,
        };
        Self {
            rows,
            cols,
            cells: vec![vec![cell; cols]; rows],
            wraps: Vec::new(),
        }
    }

    /// Check if the snapshot is empty (no content).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() || self.cells.iter().all(Vec::is_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_name_preserves_plain_alphanumeric() {
        // ASCII letters and digits pass through unchanged.
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId("abc123".into())),
            "jefe-abc123"
        );
    }

    #[test]
    fn session_name_preserves_hyphen_and_underscore() {
        // The existing contract: "my-agent" stays "jefe-my-agent".
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId("my-agent".into())),
            "jefe-my-agent"
        );
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId("my_agent".into())),
            "jefe-my_agent"
        );
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId("a-b_c".into())),
            "jefe-a-b_c"
        );
    }

    #[test]
    fn session_name_replaces_spaces_slashes_and_dots() {
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId("a b/c.d".into())),
            "jefe-a_b_c_d"
        );
    }

    #[test]
    fn session_name_replaces_shell_metacharacters() {
        // Characters that would be dangerous in a tmux session name become '_'.
        // Input "a*b;c`" keeps a/b/c and maps *, ;, $, ` each to '_'.
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId("a*b;c$`".into())),
            "jefe-a_b_c__"
        );
    }

    #[test]
    fn session_name_replaces_non_ascii_unicode() {
        // Non-ASCII alphanumeric chars (here 'é') are NOT ascii_alphanumeric.
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId("café".into())),
            "jefe-caf_"
        );
    }

    #[test]
    fn session_name_empty_id_yields_bare_prefix() {
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId(String::new())),
            "jefe-"
        );
    }

    #[test]
    fn session_name_preserves_nanosecond_timestamp_like_id() {
        // Realistic all-digit agent id (a nanosecond timestamp) is unchanged.
        assert_eq!(
            RuntimeSession::session_name_for(&AgentId("1718000000000000000".into())),
            "jefe-1718000000000000000"
        );
    }
}
