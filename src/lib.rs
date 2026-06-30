//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-001

pub mod cli;
pub mod domain;
pub mod input;
pub mod issue_detail_content;
pub mod layout;
pub mod logging;
pub mod messages;
pub mod persistence;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
pub mod pr_detail_content;
pub mod runtime;
pub mod services;
pub mod startup;
pub mod state;
/// Pure multiline text-box viewport projection (iocraft-free).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
pub mod text_box_view;
pub mod theme;
pub mod ui;

/// @plan PLAN-20260329-ISSUES-MODE.P03
pub mod github;

#[cfg(test)]
#[path = "github/tests.rs"]
mod github_tests;

#[cfg(test)]
#[path = "github/tests_pr.rs"]
mod github_tests_pr;

#[cfg(test)]
#[path = "github/tests_pr_detail.rs"]
mod github_tests_pr_detail;

/// Current application version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub mod harness;
