//! Tmux-backed TUI automation harness — pure scenario layer.
//!
//! This module hosts the future tmux-backed scenario automation harness
//! (parent issue #97). This first phase (#98) delivers only the pure,
//! side-effect-free layer: strongly typed scenario models, serde-based JSON
//! deserialization, validation, and macro expansion.
//!
//! No tmux interaction, process spawning, terminal I/O, or file I/O occurs
//! here. Tests may pass JSON strings, but production code is pure. Later
//! subissues (#99-#102) will add the runtime/execution surface on top of
//! these typed models.
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
pub mod scenario;
pub mod step;

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
pub use scenario::Scenario;
pub use step::Step;

#[cfg(test)]
#[path = "matchers_tests.rs"]
mod matchers_tests;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
