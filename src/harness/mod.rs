//! Deterministic schema-1 TUI automation harness.
//!
//! Scenario parsing is owned entirely by [`v1`]: one strict schema-1 parser,
//! one key encoder, one closed step grammar, and one real-PTY runner. Native
//! Windows multiplexer behavior is tested at its runtime ownership boundary;
//! schema-1 execution is Unix-only.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

pub mod v1;
