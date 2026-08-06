//! Multiplexer-backed TUI automation harness.
//!
//! Scenario parsing is owned entirely by [`v1`]: one strict schema-1 parser,
//! one key encoder, one closed step grammar. The superseded pre-schema parser,
//! scenario model, macro expander, and adapter were deleted by the no-shim
//! amendment on issue #383 (see `project-plans/issue383-plan.md`, S8).
//!
//! What remains here is the side-effecting multiplexer boundary: `tmux_driver`
//! (psmux on native Windows) plus its signal-cleanup guard. Both schema-1
//! backends — the Unix PTY runner and [`v1::tmux_runner`] — sit on top of it.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

pub mod capture;
pub mod error;
#[cfg(windows)]
mod psmux_process;

/// Environment variables the harness must not let reach the jefe under test.
///
/// The scenario runners isolate jefe with `--config <dir>`, and since #547 that
/// directory is what selects the multiplexer namespace. `JEFE_NAMESPACE`
/// outranks the derived value, so an operator who exported it -- exactly what
/// someone debugging a stranded session would do -- would otherwise have every
/// harness run join their live server instead of a private one, creating and
/// killing sessions next to real agents.
///
/// This is deliberately separate from each driver's `TMUX_ENV_VARS_TO_SCRUB`:
/// that list exists so inner multiplexer calls resolve the right socket
/// (#171/#173), while this one exists so the harness cannot escape its own
/// isolation. They are scrubbed together but they are not the same rule.
pub(crate) const HARNESS_ENV_VARS_TO_SCRUB: &[&str] = &["JEFE_NAMESPACE"];
pub mod signal_cleanup;
#[cfg(windows)]
#[path = "psmux_driver.rs"]
pub mod tmux_driver;
#[cfg(not(windows))]
pub mod tmux_driver;
pub mod v1;

pub use capture::{PaneStatus, PaneStatusParseError, ScreenCapture, ScrollbackSample};
pub use error::ScenarioError;
pub use signal_cleanup::SignalCleanupGuard;
pub use tmux_driver::{TmuxDriver, TmuxDriverError, TmuxPaneSize, TmuxSession, TmuxStartRequest};
