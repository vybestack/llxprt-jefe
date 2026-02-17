//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-001

pub mod domain;
pub mod persistence;
pub mod runtime;
pub mod state;
pub mod theme;
pub mod ui;

/// Current application version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
