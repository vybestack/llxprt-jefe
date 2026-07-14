//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-001

/// Shared finite-width Actions detail body projection.
pub mod actions_detail_projection;
pub mod actions_view;
pub mod agent_detection;
pub mod cli;
/// OSC 52 clipboard writer with tmux / GNU screen passthrough.
pub mod clipboard;
/// Resolved dashboard Git display data shared by rendering and selection copy.
pub mod dashboard_git_info;
pub mod domain;
pub mod input;
pub mod issue_detail_content;
pub mod layout;
/// Pure geometry, windowing, navigation, and row-width primitives for selectable lists.
pub mod list_viewport;
pub mod logging;
/// Single-pass HTML-to-text stripping for untrusted markdown (issue #155).
pub(crate) mod markdown_html_strip;
/// Plain-text markdown rendering for the detail panes (issue #155).
pub(crate) mod markdown_render;
pub mod messages;
/// Boundary-owned display data for mouse-selection content projection.
pub mod pane_content_projection;
pub mod persistence;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
pub mod pr_detail_content;
pub mod runtime;
/// Pure, iocraft-free mouse-selection model (pane geometry + text extraction).
pub mod selection;
pub mod services;
pub mod startup;
pub mod state;
/// Pure multiline text-box viewport projection (iocraft-free).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
pub mod text_box_view;
/// Pure, iocraft-free word-wrap projection shared by the editor and displayer.
///
/// @requirement REQ-TEXT-WRAP
pub mod text_wrap;
pub mod theme;
pub mod ui;

/// Cached git repository info (origin shortform + branch) for agent display.
pub mod git_info;
/// @plan PLAN-20260329-ISSUES-MODE.P03
pub mod github;

#[cfg(test)]
mod list_viewport_tests;

#[cfg(test)]
#[path = "github/tests/mod.rs"]
mod github_tests;

#[cfg(test)]
#[path = "github/tests_filters.rs"]
mod github_tests_filters;

#[cfg(test)]
#[path = "github/tests_pr.rs"]
mod github_tests_pr;

#[cfg(test)]
#[path = "github/tests_pr_detail.rs"]
mod github_tests_pr_detail;

#[cfg(test)]
#[path = "github/tests_pr_threads.rs"]
mod github_tests_pr_threads;

/// Current application version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub mod harness;
