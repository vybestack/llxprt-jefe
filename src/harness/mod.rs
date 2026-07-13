//! Tmux-backed TUI automation harness.
//!
//! This module hosts the tmux-backed scenario automation harness (parent issue
//! #97). Scenario parsing, macro expansion, capture models, and matchers are
//! pure, side-effect-free layers. The `tmux_driver` module is the explicit
//! side-effecting boundary that shells out to tmux for real-TTY sessions.
//!
//! Runner orchestration and artifacts are added by later subissues on top of
//! these typed models and the driver seam.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

pub mod capture;
pub mod config;
pub mod error;
pub mod expand;
pub mod macro_def;
pub mod matchers;
pub mod parser;
pub mod runner;
pub mod scenario;
pub mod step;
#[cfg(windows)]
#[path = "psmux_driver.rs"]
pub mod tmux_driver;
#[cfg(not(windows))]
pub mod tmux_driver;

pub use capture::{PaneStatus, PaneStatusParseError, ScreenCapture, ScrollbackSample};
pub use config::{AssertMode, ScenarioConfig};
pub use error::ScenarioError;
pub use expand::expand_macros;
pub use macro_def::MacroDef;
pub use matchers::{
    CountOutcome, HistoryDeltaOutcome, MatchPattern, PredicateOutcome, history_delta,
    screen_absent, screen_contains, screen_count, scrollback_absent, scrollback_contains,
    scrollback_count,
};
pub use parser::parse_scenario;
pub use runner::{
    HarnessDriver, RunSummary, RunnerError, RunnerFailure, run_scenario, run_tmux_scenario,
};
pub use scenario::Scenario;
pub use step::Step;
pub use tmux_driver::{TmuxDriver, TmuxDriverError, TmuxPaneSize, TmuxSession, TmuxStartRequest};

#[cfg(test)]
#[path = "matchers_tests.rs"]
mod matchers_tests;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
