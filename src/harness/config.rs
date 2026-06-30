//! Scenario configuration and assertion mode.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::error::ScenarioError;

/// How assertion steps (`expect`, `expectCount`, ...) treat a mismatch.
///
/// `Soft` records the failure and continues; `Strict` aborts the scenario.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssertMode {
    Soft,
    #[default]
    Strict,
}

/// Harness and terminal configuration for a scenario.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioConfig {
    /// Terminal column count (must be >= 1).
    pub cols: u16,
    /// Terminal row count (must be >= 1).
    pub rows: u16,
    /// Maximum scrollback lines retained for history sampling.
    #[serde(default = "default_history_limit")]
    pub history_limit: u32,
    /// Milliseconds to wait before the first step runs.
    #[serde(default)]
    pub initial_wait_ms: u64,
    /// Optional directory for captures and artifacts. Absent means ephemeral.
    #[serde(default)]
    pub out_dir: Option<PathBuf>,
    /// When true, keep the tmux session alive after the scenario completes.
    #[serde(default)]
    pub keep_session: bool,
    /// How assertion mismatches are handled.
    #[serde(default)]
    pub assert_mode: AssertMode,
}

fn default_history_limit() -> u32 {
    10_000
}

impl ScenarioConfig {
    /// Validate config bounds. Returns the first violation, if any.
    ///
    /// Pseudocode:
    ///   if cols == 0 -> InvalidConfig("cols", "must be >= 1")
    ///   if rows == 0 -> InvalidConfig("rows", "must be >= 1")
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P01
    /// @requirement REQ-TMUX-HARNESS-001
    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.cols == 0 {
            return Err(ScenarioError::InvalidConfig {
                field: "cols".to_string(),
                reason: "must be >= 1".to_string(),
            });
        }
        if self.rows == 0 {
            return Err(ScenarioError::InvalidConfig {
                field: "rows".to_string(),
                reason: "must be >= 1".to_string(),
            });
        }
        if self.history_limit == 0 {
            return Err(ScenarioError::InvalidConfig {
                field: "history_limit".to_string(),
                reason: "must be >= 1".to_string(),
            });
        }
        Ok(())
    }
}
