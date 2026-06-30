//! Pure screen, scrollback, and pane status capture models.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P02
//! @requirement REQ-TMUX-HARNESS-002

/// A captured tmux pane screen at a point in time.
///
/// The driver owns terminal I/O; this type only stores already-captured text.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCapture {
    pub rows: u16,
    pub cols: u16,
    pub lines: Vec<String>,
}

impl ScreenCapture {
    /// Build a screen capture from pre-captured lines.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P02
    /// @requirement REQ-TMUX-HARNESS-002
    #[must_use]
    pub fn new(rows: u16, cols: u16, lines: Vec<String>) -> Self {
        Self { rows, cols, lines }
    }

    /// Borrow the captured lines for pure matchers.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

/// A sampled slice of tmux scrollback history.
///
/// `history_size` is the full pane history size reported by tmux when the
/// sample was captured; `lines` is the retained text sample.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollbackSample {
    pub history_size: u64,
    pub lines: Vec<String>,
}

impl ScrollbackSample {
    /// Build a scrollback sample from a tmux history size and captured lines.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P02
    /// @requirement REQ-TMUX-HARNESS-002
    #[must_use]
    pub fn new(history_size: u64, lines: Vec<String>) -> Self {
        Self {
            history_size,
            lines,
        }
    }

    /// Borrow the sampled lines for pure matchers.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

/// Parsed tmux pane liveness status.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneStatus {
    pub dead: bool,
}

impl PaneStatus {
    /// Parse the output of `tmux display-message -p '#{pane_dead}'`.
    ///
    /// # Errors
    ///
    /// Returns [`PaneStatusParseError`] when tmux returns anything other than
    /// `0` or `1` after trimming whitespace.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P02
    /// @requirement REQ-TMUX-HARNESS-002
    pub fn parse_tmux_pane_dead(output: &str) -> Result<Self, PaneStatusParseError> {
        match output.trim() {
            "0" => Ok(Self { dead: false }),
            "1" => Ok(Self { dead: true }),
            other => Err(PaneStatusParseError {
                value: other.to_string(),
            }),
        }
    }
}

/// Typed parse error for tmux pane status output.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneStatusParseError {
    pub value: String,
}

impl std::fmt::Display for PaneStatusParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid tmux pane_dead value: '{}'", self.value)
    }
}

impl std::error::Error for PaneStatusParseError {}
