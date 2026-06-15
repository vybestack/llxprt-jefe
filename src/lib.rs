//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-001

pub mod cli;
pub mod domain;
pub mod input;
pub mod layout;
pub mod logging;
pub mod persistence;
pub mod runtime;
pub mod services;
pub mod state;
pub mod theme;
pub mod ui;

/// @plan PLAN-20260329-ISSUES-MODE.P03
pub mod github;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::manual_string_new)]
#[path = "github/tests.rs"]
mod github_tests;

/// Current application version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
